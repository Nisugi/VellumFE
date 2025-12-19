//! Shared highlight application utilities for TUI widgets
//!
//! This module provides a reusable `HighlightEngine` that can apply highlight patterns
//! to text content in any TUI widget. It supports both Style-based spans (used by TextWindow)
//! and TextSegment-based content (used by Inventory/Spells windows).

use crate::config::HighlightPattern;
use crate::data::{LinkData, SpanType, TextSegment};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use ratatui::style::{Color, Modifier, Style};
use regex::Regex;
use std::collections::HashMap;

/// Character-level style information for highlight processing
#[derive(Clone, Copy)]
struct CharStyle {
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
    span_type: SpanType,
}

/// Match information for a highlight pattern
#[derive(Clone)]
struct MatchInfo {
    start_byte: usize,
    end_byte: usize,
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
    color_entire_line: bool,
    replace: Option<String>,
}

/// Compiled highlight engine that can be reused across multiple highlight applications
pub struct HighlightEngine {
    highlights: Vec<HighlightPattern>,
    highlight_regexes: Vec<Option<Regex>>,
    fast_matcher: Option<AhoCorasick>,
    fast_pattern_map: Vec<usize>,
}

impl HighlightEngine {
    /// Create a new highlight engine from a list of patterns
    ///
    /// This compiles regexes and builds the Aho-Corasick automaton for fast matching.
    pub fn new(highlights: Vec<HighlightPattern>) -> Self {
        // Separate fast_parse patterns from regex patterns
        let mut fast_patterns: Vec<String> = Vec::new();
        let mut fast_map: Vec<usize> = Vec::new();

        // Build regex list and collect fast_parse patterns
        let highlight_regexes = highlights
            .iter()
            .enumerate()
            .map(|(i, h)| {
                if h.fast_parse {
                    // Split pattern on | and add to Aho-Corasick
                    for literal in h.pattern.split('|') {
                        let literal = literal.trim();
                        if !literal.is_empty() {
                            fast_patterns.push(literal.to_string());
                            fast_map.push(i); // Map this pattern back to highlight index
                        }
                    }
                    None // Don't compile as regex
                } else {
                    // Regular regex pattern
                    Regex::new(&h.pattern).ok()
                }
            })
            .collect();

        // Build Aho-Corasick matcher for fast_parse patterns with whole-word matching only
        let (fast_matcher, fast_pattern_map) = if !fast_patterns.is_empty() {
            let matcher = AhoCorasickBuilder::new()
                .match_kind(MatchKind::Standard)
                .build(&fast_patterns)
                .ok();
            (matcher, fast_map)
        } else {
            (None, Vec::new())
        };

        Self {
            highlights,
            highlight_regexes,
            fast_matcher,
            fast_pattern_map,
        }
    }

    /// Apply highlights to Style-based spans (used by TextWindow)
    ///
    /// Returns `None` if all spans are System type (which skip highlighting),
    /// otherwise returns the transformed spans with highlights applied.
    pub fn apply_highlights(
        &self,
        spans: &[(String, Style, SpanType, Option<LinkData>)],
        stream: &str,
    ) -> Option<Vec<(String, Style, SpanType, Option<LinkData>)>> {
        // Skip highlight transforms for pure system lines
        if spans
            .iter()
            .all(|(_, _, span_type, _)| matches!(span_type, SpanType::System))
        {
            return None;
        }

        if self.highlights.is_empty() {
            return None;
        }

        // STEP 1: Build character-by-character style and link maps from current spans
        let mut char_styles: Vec<CharStyle> = Vec::new();
        let mut char_links: Vec<Option<LinkData>> = Vec::new();
        for (content, style, span_type, link) in spans {
            for _ in content.chars() {
                char_styles.push(CharStyle {
                    fg: style.fg,
                    bg: style.bg,
                    bold: style.add_modifier.contains(Modifier::BOLD),
                    span_type: *span_type,
                });
                char_links.push(link.clone());
            }
        }

        if char_styles.is_empty() {
            return None;
        }

        // STEP 2: Build full text for pattern matching
        let mut full_text: String = spans.iter().map(|(content, _, _, _)| content.as_str()).collect();

        // STEP 3: Find all highlight matches (both Aho-Corasick and regex)
        let mut matches: Vec<MatchInfo> = Vec::new();

        // Try Aho-Corasick fast patterns (with word boundary checking)
        if let Some(ref matcher) = self.fast_matcher {
            for mat in matcher.find_iter(&full_text) {
                let start = mat.start();
                let end = mat.end();
                let bytes = full_text.as_bytes();

                let is_word_start = start == 0 || {
                    bytes.get(start - 1).is_none_or(|&b| {
                        let c = b as char;
                        !c.is_alphanumeric() && c != '_'
                    })
                };

                let is_word_end = end >= bytes.len() || {
                    bytes.get(end).is_none_or(|&b| {
                        let c = b as char;
                        !c.is_alphanumeric() && c != '_'
                    })
                };

                if is_word_start && is_word_end {
                    if let Some(&highlight_idx) = self.fast_pattern_map.get(mat.pattern().as_usize())
                    {
                        if let Some(highlight) = self.highlights.get(highlight_idx) {
                            // Check stream filter - skip if highlight requires specific stream and doesn't match
                            if let Some(ref required_stream) = highlight.stream {
                                if !stream.eq_ignore_ascii_case(required_stream) {
                                    continue;
                                }
                            }

                            let fg = highlight.fg.as_ref().and_then(|h| Self::parse_hex_color(h));
                            let bg = highlight.bg.as_ref().and_then(|h| Self::parse_hex_color(h));
                            matches.push(MatchInfo {
                                start_byte: start,
                                end_byte: end,
                                fg,
                                bg,
                                bold: highlight.bold,
                                color_entire_line: highlight.color_entire_line,
                                replace: highlight.replace.clone(),
                            });
                        }
                    }
                }
            }
        }

        // Try regex patterns
        for (i, highlight) in self.highlights.iter().enumerate() {
            if highlight.fast_parse {
                continue; // Already handled by Aho-Corasick
            }

            // Check stream filter - skip if highlight requires specific stream and doesn't match
            if let Some(ref required_stream) = highlight.stream {
                if !stream.eq_ignore_ascii_case(required_stream) {
                    continue;
                }
            }

            if let Some(Some(regex)) = self.highlight_regexes.get(i) {
                let fg = highlight.fg.as_ref().and_then(|h| Self::parse_hex_color(h));
                let bg = highlight.bg.as_ref().and_then(|h| Self::parse_hex_color(h));

                // If there's a replace template, use captures_iter to expand $1, $2, etc.
                // Otherwise, use find_iter for simpler/faster matching
                if let Some(ref replace_template) = highlight.replace {
                    for caps in regex.captures_iter(&full_text) {
                        if let Some(m) = caps.get(0) {
                            // Expand capture groups in replacement template
                            let mut expanded = String::new();
                            caps.expand(replace_template, &mut expanded);
                            matches.push(MatchInfo {
                                start_byte: m.start(),
                                end_byte: m.end(),
                                fg,
                                bg,
                                bold: highlight.bold,
                                color_entire_line: highlight.color_entire_line,
                                replace: Some(expanded),
                            });
                        }
                    }
                } else {
                    for m in regex.find_iter(&full_text) {
                        matches.push(MatchInfo {
                            start_byte: m.start(),
                            end_byte: m.end(),
                            fg,
                            bg,
                            bold: highlight.bold,
                            color_entire_line: highlight.color_entire_line,
                            replace: None,
                        });
                    }
                }
            }
        }

        // STEP 4: Process matches and build replacement text
        if !matches.is_empty() {
            // Map byte offsets to char indices
            let mut byte_to_char: HashMap<usize, usize> = HashMap::new();
            for (idx, (byte, _ch)) in full_text.char_indices().enumerate() {
                byte_to_char.insert(byte, idx);
            }

            let full_text_chars: Vec<char> = full_text.chars().collect();
            matches.sort_by_key(|m| m.start_byte);

            let mut new_text = String::new();
            let mut new_styles: Vec<CharStyle> = Vec::new();
            let mut new_links: Vec<Option<LinkData>> = Vec::new();
            let mut new_match_ranges: Vec<(usize, usize, MatchInfo)> = Vec::new();

            let mut last_char_idx = 0usize;
            for m in matches {
                let start_char = *byte_to_char.get(&m.start_byte).unwrap_or(&last_char_idx);
                let end_char = *byte_to_char.get(&m.end_byte).unwrap_or(&full_text_chars.len());
                if start_char < last_char_idx {
                    continue; // overlapping; skip
                }

                // Copy untouched region
                for i in last_char_idx..start_char {
                    new_text.push(full_text_chars[i]);
                    new_styles.push(char_styles.get(i).cloned().unwrap_or(CharStyle {
                        fg: None,
                        bg: None,
                        bold: false,
                        span_type: SpanType::Normal,
                    }));
                    new_links.push(char_links.get(i).cloned().unwrap_or(None));
                }

                let new_start = new_styles.len();

                // Replacement or original segment
                if let Some(ref repl) = m.replace {
                    let base_style = char_styles.get(start_char).cloned().unwrap_or(CharStyle {
                        fg: None,
                        bg: None,
                        bold: false,
                        span_type: SpanType::Normal,
                    });
                    for ch in repl.chars() {
                        new_text.push(ch);
                        new_styles.push(base_style);
                        new_links.push(None);
                    }
                } else {
                    for i in start_char..end_char {
                        new_text.push(full_text_chars[i]);
                        new_styles.push(char_styles.get(i).cloned().unwrap_or(CharStyle {
                            fg: None,
                            bg: None,
                            bold: false,
                            span_type: SpanType::Normal,
                        }));
                        new_links.push(char_links.get(i).cloned().unwrap_or(None));
                    }
                }

                let new_end = new_styles.len();
                new_match_ranges.push((
                    new_start,
                    new_end,
                    MatchInfo {
                        start_byte: m.start_byte,
                        end_byte: m.end_byte,
                        fg: m.fg,
                        bg: m.bg,
                        bold: m.bold,
                        color_entire_line: m.color_entire_line,
                        replace: m.replace,
                    },
                ));

                last_char_idx = end_char;
            }

            // Tail
            for i in last_char_idx..full_text_chars.len() {
                new_text.push(full_text_chars[i]);
                new_styles.push(char_styles.get(i).cloned().unwrap_or(CharStyle {
                    fg: None,
                    bg: None,
                    bold: false,
                    span_type: SpanType::Normal,
                }));
                new_links.push(char_links.get(i).cloned().unwrap_or(None));
            }

            // Apply highlight styling on rewritten text
            for (start, end, info) in &new_match_ranges {
                if info.color_entire_line {
                    for cs in new_styles.iter_mut() {
                        if cs.span_type == SpanType::Link || cs.span_type == SpanType::Monsterbold {
                            if let Some(color) = info.bg {
                                cs.bg = Some(color);
                            }
                        } else {
                            if let Some(color) = info.fg {
                                cs.fg = Some(color);
                            }
                            if let Some(color) = info.bg {
                                cs.bg = Some(color);
                            }
                            if info.bold {
                                cs.bold = true;
                            }
                        }
                    }
                    break; // only first whole-line match applies
                } else {
                    for idx in *start..(*end).min(new_styles.len()) {
                        if let Some(color) = info.fg {
                            new_styles[idx].fg = Some(color);
                        }
                        if let Some(color) = info.bg {
                            new_styles[idx].bg = Some(color);
                        }
                        if info.bold {
                            new_styles[idx].bold = true;
                        }
                    }
                }
            }

            char_styles = new_styles;
            char_links = new_links;
            full_text = new_text;
        }

        // STEP 5: Reconstruct spans from char_styles with proper splitting
        let mut new_spans: Vec<(String, Style, SpanType, Option<LinkData>)> = Vec::new();
        let full_text_chars: Vec<char> = full_text.chars().collect();

        let mut i = 0;
        while i < char_styles.len() {
            let current_style = char_styles[i];
            let current_link = char_links.get(i).cloned().unwrap_or(None);
            let mut content = String::new();
            content.push(full_text_chars[i]);

            // Extend span while style matches
            i += 1;
            while i < char_styles.len() {
                let next_style = char_styles[i];
                let next_link = char_links.get(i).cloned().unwrap_or(None);
                if next_style.fg == current_style.fg
                    && next_style.bg == current_style.bg
                    && next_style.bold == current_style.bold
                    && next_style.span_type == current_style.span_type
                    && next_link == current_link
                {
                    content.push(full_text_chars[i]);
                    i += 1;
                } else {
                    break;
                }
            }

            // Build Style
            let mut style = Style::default();
            if let Some(fg) = current_style.fg {
                style = style.fg(fg);
            }
            if let Some(bg) = current_style.bg {
                style = style.bg(bg);
            }
            if current_style.bold {
                style = style.add_modifier(Modifier::BOLD);
            }

            new_spans.push((content, style, current_style.span_type, current_link));
        }

        Some(new_spans)
    }

    /// Apply highlights to TextSegment-based content (used by Inventory/Spells windows)
    ///
    /// This converts TextSegments to spans, applies highlights, then converts back.
    pub fn apply_highlights_to_segments(
        &self,
        segments: &[TextSegment],
        stream: &str,
    ) -> Option<Vec<TextSegment>> {
        // Convert TextSegment → span tuples
        let spans: Vec<_> = segments.iter().map(segment_to_span_tuple).collect();

        // Apply highlights
        let highlighted = self.apply_highlights(&spans, stream)?;

        // Convert back to TextSegment
        Some(
            highlighted
                .into_iter()
                .map(|span| span_tuple_to_segment(&span))
                .collect(),
        )
    }

    /// Parse hex color string to ratatui Color
    fn parse_hex_color(hex: &str) -> Option<Color> {
        super::colors::parse_color_to_ratatui(hex)
    }
}

/// Convert TextSegment to span tuple format
fn segment_to_span_tuple(
    segment: &TextSegment,
) -> (String, Style, SpanType, Option<LinkData>) {
    let mut style = Style::default();

    if let Some(ref fg_hex) = segment.fg {
        if let Some(color) = super::colors::parse_color_to_ratatui(fg_hex) {
            style = style.fg(color);
        }
    }

    if let Some(ref bg_hex) = segment.bg {
        if let Some(color) = super::colors::parse_color_to_ratatui(bg_hex) {
            style = style.bg(color);
        }
    }

    if segment.bold {
        style = style.add_modifier(Modifier::BOLD);
    }

    (
        segment.text.clone(),
        style,
        segment.span_type,
        segment.link_data.clone(),
    )
}

/// Convert span tuple to TextSegment format
fn span_tuple_to_segment(
    span: &(String, Style, SpanType, Option<LinkData>),
) -> TextSegment {
    let (text, style, span_type, link_data) = span;

    let fg = style.fg.and_then(color_to_hex);
    let bg = style.bg.and_then(color_to_hex);
    let bold = style.add_modifier.contains(Modifier::BOLD);

    TextSegment {
        text: text.clone(),
        fg,
        bg,
        bold,
        span_type: *span_type,
        link_data: link_data.clone(),
    }
}

/// Convert ratatui Color to hex string
fn color_to_hex(color: Color) -> Option<String> {
    match color {
        Color::Rgb(r, g, b) => Some(format!("#{:02x}{:02x}{:02x}", r, g, b)),
        Color::Indexed(idx) => Some(format!("@{}", idx)), // Special format for indexed colors
        Color::Reset => None,
        // Handle other color variants as needed
        _ => None,
    }
}
