//! Core-level highlight engine for applying highlight patterns to text
//!
//! This module applies highlights ONCE during message processing (in MessageProcessor),
//! before text reaches any frontend. Text arrives at widgets pre-colored.
//!
//! NO frontend imports - works directly with TextSegment from the data layer.

use crate::config::HighlightPattern;
use crate::data::{LinkData, SpanType, TextSegment};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use regex::Regex;
use std::collections::HashMap;

/// Sound trigger from a highlight match
#[derive(Clone, Debug)]
pub struct SoundTrigger {
    pub file: String,
    pub volume: Option<f32>,
}

/// Result of applying highlights to text segments
#[derive(Clone, Debug)]
pub struct HighlightResult {
    /// The segments with highlight colors applied
    pub segments: Vec<TextSegment>,
    /// Any sounds that should be triggered
    pub sounds: Vec<SoundTrigger>,
}

/// Character-level style information for highlight processing
#[derive(Clone)]
struct CharStyle {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
    span_type: SpanType,
}

/// Match information for a highlight pattern
#[derive(Clone)]
struct MatchInfo {
    start_byte: usize,
    end_byte: usize,
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
    color_entire_line: bool,
    replace: Option<String>,
    sound: Option<String>,
    sound_volume: Option<f32>,
}

/// Core highlight engine that applies highlights during message processing
///
/// This is compiled once at startup (and on .reload) and reused for all messages.
pub struct CoreHighlightEngine {
    highlights: Vec<HighlightPattern>,
    highlight_regexes: Vec<Option<Regex>>,
    fast_matcher: Option<AhoCorasick>,
    fast_pattern_map: Vec<usize>,
    replace_enabled: bool,
}

impl CoreHighlightEngine {
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

        // Build Aho-Corasick matcher for fast_parse patterns
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
            replace_enabled: true,
        }
    }

    /// Create an empty engine (no highlights)
    pub fn empty() -> Self {
        Self {
            highlights: Vec::new(),
            highlight_regexes: Vec::new(),
            fast_matcher: None,
            fast_pattern_map: Vec::new(),
            replace_enabled: true,
        }
    }

    /// Update the engine with new patterns (rebuilds everything)
    pub fn update_patterns(&mut self, highlights: Vec<HighlightPattern>) {
        *self = Self::new(highlights);
    }

    /// Set whether text replacement is enabled
    pub fn set_replace_enabled(&mut self, enabled: bool) {
        self.replace_enabled = enabled;
    }

    /// Apply highlights to TextSegments
    ///
    /// This is the main entry point called from MessageProcessor.
    /// Returns the segments with colors applied and any sounds to trigger.
    pub fn apply_highlights(
        &self,
        segments: &[TextSegment],
        stream: &str,
    ) -> HighlightResult {
        // Skip if no highlights or empty input
        if self.highlights.is_empty() || segments.is_empty() {
            return HighlightResult {
                segments: segments.to_vec(),
                sounds: Vec::new(),
            };
        }

        // Skip highlight transforms for pure system lines
        if segments
            .iter()
            .all(|seg| matches!(seg.span_type, SpanType::System))
        {
            return HighlightResult {
                segments: segments.to_vec(),
                sounds: Vec::new(),
            };
        }

        // STEP 1: Build character-by-character style and link maps
        let mut char_styles: Vec<CharStyle> = Vec::new();
        let mut char_links: Vec<Option<LinkData>> = Vec::new();
        for segment in segments {
            for _ in segment.text.chars() {
                char_styles.push(CharStyle {
                    fg: segment.fg.clone(),
                    bg: segment.bg.clone(),
                    bold: segment.bold,
                    span_type: segment.span_type,
                });
                char_links.push(segment.link_data.clone());
            }
        }

        if char_styles.is_empty() {
            return HighlightResult {
                segments: segments.to_vec(),
                sounds: Vec::new(),
            };
        }

        // STEP 2: Build full text for pattern matching
        let mut full_text: String = segments.iter().map(|s| s.text.as_str()).collect();

        // STEP 3: Find all highlight matches
        let mut matches: Vec<MatchInfo> = Vec::new();
        let mut sounds: Vec<SoundTrigger> = Vec::new();

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
                            // Check stream filter
                            if let Some(ref required_stream) = highlight.stream {
                                if !stream.eq_ignore_ascii_case(required_stream) {
                                    continue;
                                }
                            }

                            // Collect sound trigger
                            if let Some(ref sound_file) = highlight.sound {
                                sounds.push(SoundTrigger {
                                    file: sound_file.clone(),
                                    volume: highlight.sound_volume,
                                });
                            }

                            matches.push(MatchInfo {
                                start_byte: start,
                                end_byte: end,
                                fg: highlight.fg.clone(),
                                bg: highlight.bg.clone(),
                                bold: highlight.bold,
                                color_entire_line: highlight.color_entire_line,
                                replace: highlight.replace.clone(),
                                sound: highlight.sound.clone(),
                                sound_volume: highlight.sound_volume,
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

            // Check stream filter
            if let Some(ref required_stream) = highlight.stream {
                if !stream.eq_ignore_ascii_case(required_stream) {
                    continue;
                }
            }

            if let Some(Some(regex)) = self.highlight_regexes.get(i) {
                let use_replacement = self.replace_enabled && highlight.replace.is_some();

                if use_replacement {
                    if let Some(ref replace_template) = highlight.replace {
                        for caps in regex.captures_iter(&full_text) {
                            if let Some(m) = caps.get(0) {
                                // Collect sound trigger
                                if let Some(ref sound_file) = highlight.sound {
                                    sounds.push(SoundTrigger {
                                        file: sound_file.clone(),
                                        volume: highlight.sound_volume,
                                    });
                                }

                                // Expand capture groups
                                let mut expanded = String::new();
                                caps.expand(replace_template, &mut expanded);
                                matches.push(MatchInfo {
                                    start_byte: m.start(),
                                    end_byte: m.end(),
                                    fg: highlight.fg.clone(),
                                    bg: highlight.bg.clone(),
                                    bold: highlight.bold,
                                    color_entire_line: highlight.color_entire_line,
                                    replace: Some(expanded),
                                    sound: highlight.sound.clone(),
                                    sound_volume: highlight.sound_volume,
                                });
                            }
                        }
                    }
                } else {
                    for m in regex.find_iter(&full_text) {
                        // Collect sound trigger
                        if let Some(ref sound_file) = highlight.sound {
                            sounds.push(SoundTrigger {
                                file: sound_file.clone(),
                                volume: highlight.sound_volume,
                            });
                        }

                        matches.push(MatchInfo {
                            start_byte: m.start(),
                            end_byte: m.end(),
                            fg: highlight.fg.clone(),
                            bg: highlight.bg.clone(),
                            bold: highlight.bold,
                            color_entire_line: highlight.color_entire_line,
                            replace: None,
                            sound: highlight.sound.clone(),
                            sound_volume: highlight.sound_volume,
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
                        new_styles.push(base_style.clone());
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
                new_match_ranges.push((new_start, new_end, m));

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

            // Apply highlight styling
            for (start, end, info) in &new_match_ranges {
                if info.color_entire_line {
                    for cs in new_styles.iter_mut() {
                        if cs.span_type == SpanType::Link || cs.span_type == SpanType::Monsterbold {
                            if let Some(ref color) = info.bg {
                                cs.bg = Some(color.clone());
                            }
                        } else {
                            if let Some(ref color) = info.fg {
                                cs.fg = Some(color.clone());
                            }
                            if let Some(ref color) = info.bg {
                                cs.bg = Some(color.clone());
                            }
                            if info.bold {
                                cs.bold = true;
                            }
                        }
                    }
                    break; // only first whole-line match applies
                } else {
                    for idx in *start..(*end).min(new_styles.len()) {
                        if let Some(ref color) = info.fg {
                            new_styles[idx].fg = Some(color.clone());
                        }
                        if let Some(ref color) = info.bg {
                            new_styles[idx].bg = Some(color.clone());
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

        // STEP 5: Reconstruct TextSegments from char_styles
        let mut result_segments: Vec<TextSegment> = Vec::new();
        let full_text_chars: Vec<char> = full_text.chars().collect();

        let mut i = 0;
        while i < char_styles.len() {
            let current_style = &char_styles[i];
            let current_link = char_links.get(i).cloned().unwrap_or(None);
            let mut content = String::new();
            content.push(full_text_chars[i]);

            // Extend span while style matches
            i += 1;
            while i < char_styles.len() {
                let next_style = &char_styles[i];
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

            result_segments.push(TextSegment {
                text: content,
                fg: current_style.fg.clone(),
                bg: current_style.bg.clone(),
                bold: current_style.bold,
                span_type: current_style.span_type,
                link_data: current_link,
            });
        }

        HighlightResult {
            segments: result_segments,
            sounds,
        }
    }

    /// Get the foreground color of the first matching highlight pattern for the given text.
    ///
    /// This is useful for simple widgets that display single-colored rows.
    pub fn get_first_match_color(&self, text: &str) -> Option<String> {
        if self.highlights.is_empty() {
            return None;
        }

        // Try Aho-Corasick fast patterns first
        if let Some(ref matcher) = self.fast_matcher {
            for mat in matcher.find_iter(text) {
                let start = mat.start();
                let end = mat.end();
                let bytes = text.as_bytes();

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
                            if let Some(ref fg) = highlight.fg {
                                return Some(fg.clone());
                            }
                        }
                    }
                }
            }
        }

        // Try regex patterns
        for (i, highlight) in self.highlights.iter().enumerate() {
            if highlight.fast_parse {
                continue;
            }

            if let Some(Some(regex)) = self.highlight_regexes.get(i) {
                if regex.is_match(text) {
                    if let Some(ref fg) = highlight.fg {
                        return Some(fg.clone());
                    }
                }
            }
        }

        None
    }
}

impl Default for CoreHighlightEngine {
    fn default() -> Self {
        Self::empty()
    }
}
