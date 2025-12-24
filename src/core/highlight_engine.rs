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

/// A replacement that was deferred because it targets a specific window
#[derive(Clone, Debug)]
pub struct DeferredReplacement {
    /// Character range in the original text (before any replacements)
    pub start_char: usize,
    pub end_char: usize,
    /// The replacement text (with capture groups expanded)
    pub replacement: String,
    /// Window name this replacement targets
    pub target_window: String,
    /// Original matched text (for applying replacement)
    pub original_text: String,
}

/// Result of applying highlights to text segments
#[derive(Clone, Debug)]
pub struct HighlightResult {
    /// The segments with highlight colors applied
    pub segments: Vec<TextSegment>,
    /// Any sounds that should be triggered
    pub sounds: Vec<SoundTrigger>,
    /// Replacements that target specific windows (applied during routing)
    pub deferred_replacements: Vec<DeferredReplacement>,
    /// True if the ENTIRE line was covered by silent_prompt patterns (suppress prompt)
    pub line_is_silent: bool,
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
    /// Window name for window-specific replacements (None = apply everywhere)
    target_window: Option<String>,
    /// If true, this match contributes to silent prompt detection
    silent_prompt: bool,
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
                    // Regular regex pattern (reuse compiled regex when available)
                    if let Some(regex) = h.compiled_regex.clone() {
                        Some(regex)
                    } else {
                        Regex::new(&h.pattern).ok()
                    }
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
                deferred_replacements: Vec::new(),
                line_is_silent: false,
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
                deferred_replacements: Vec::new(),
                line_is_silent: false,
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
                deferred_replacements: Vec::new(),
                line_is_silent: false,
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
                                target_window: highlight.window.clone(),
                                silent_prompt: highlight.silent_prompt,
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
                                    target_window: highlight.window.clone(),
                                    silent_prompt: highlight.silent_prompt,
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
                            target_window: highlight.window.clone(),
                            silent_prompt: highlight.silent_prompt,
                        });
                    }
                }
            }
        }

        // STEP 4: Process matches and build replacement text
        let mut deferred_replacements: Vec<DeferredReplacement> = Vec::new();
        let mut line_is_silent = false;

        if !matches.is_empty() {
            // Map byte offsets to char indices
            let mut byte_to_char: HashMap<usize, usize> = HashMap::new();
            for (idx, (byte, _ch)) in full_text.char_indices().enumerate() {
                byte_to_char.insert(byte, idx);
            }

            let full_text_chars: Vec<char> = full_text.chars().collect();
            matches.sort_by_key(|m| m.start_byte);

            // Calculate line_is_silent: true if ALL non-whitespace chars are covered by silent_prompt matches
            let any_silent = matches.iter().any(|m| m.silent_prompt);
            if any_silent {
                let mut silent_covered = vec![false; full_text_chars.len()];
                for m in matches.iter() {
                    if m.silent_prompt {
                        let start_char = *byte_to_char.get(&m.start_byte).unwrap_or(&0);
                        let end_char =
                            *byte_to_char.get(&m.end_byte).unwrap_or(&full_text_chars.len());
                        for i in start_char..end_char.min(silent_covered.len()) {
                            silent_covered[i] = true;
                        }
                    }
                }
                // Line is silent only if ALL non-whitespace chars are covered by silent_prompt matches
                line_is_silent = full_text_chars.iter().enumerate().all(|(i, ch)| {
                    ch.is_whitespace() || (i < silent_covered.len() && silent_covered[i])
                });
            }

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

                // Check if this is a window-specific replacement (deferred)
                let should_defer = m.target_window.is_some() && m.replace.is_some();

                if should_defer {
                    // Defer the replacement - collect original text for later
                    let original_text: String =
                        full_text_chars[start_char..end_char].iter().collect();
                    deferred_replacements.push(DeferredReplacement {
                        start_char: new_styles.len(), // Position in new text
                        end_char: new_styles.len() + (end_char - start_char),
                        replacement: m.replace.clone().unwrap(),
                        target_window: m.target_window.clone().unwrap(),
                        original_text: original_text.clone(),
                    });
                    // Use original text (colors will still be applied below)
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
                } else if let Some(ref repl) = m.replace {
                    // Immediate replacement (no window filter)
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
                    // No replacement - use original text
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
            deferred_replacements,
            line_is_silent,
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

/// Apply deferred replacements for a specific window.
///
/// This is called during message routing to apply window-specific replacements
/// that were deferred during the main highlight processing.
///
/// Returns new segments with the replacements applied, or the original segments
/// if no replacements apply to this window.
pub fn apply_deferred_for_window(
    segments: &[TextSegment],
    deferred: &[DeferredReplacement],
    window_name: &str,
) -> Vec<TextSegment> {
    // Filter to replacements targeting this window
    let applicable: Vec<_> = deferred
        .iter()
        .filter(|d| d.target_window.eq_ignore_ascii_case(window_name))
        .collect();

    if applicable.is_empty() {
        return segments.to_vec(); // No changes needed
    }

    // Build full text from segments
    let full_text: String = segments.iter().map(|s| s.text.as_str()).collect();
    let mut result = full_text.clone();

    // Apply replacements in reverse order (to preserve character positions)
    let mut sorted_applicable: Vec<_> = applicable.clone();
    sorted_applicable.sort_by(|a, b| b.start_char.cmp(&a.start_char));

    for repl in sorted_applicable {
        // Find the original text in the result and replace it
        if let Some(pos) = result.find(&repl.original_text) {
            result = format!(
                "{}{}{}",
                &result[..pos],
                repl.replacement,
                &result[pos + repl.original_text.len()..]
            );
        }
    }

    // If text didn't change, return original
    if result == full_text {
        return segments.to_vec();
    }

    // Rebuild segments with replaced text
    // For simplicity, preserve first segment's style for the entire replaced text
    let first_style = segments.first().cloned().unwrap_or_else(|| TextSegment {
        text: String::new(),
        fg: None,
        bg: None,
        bold: false,
        span_type: SpanType::Normal,
        link_data: None,
    });

    vec![TextSegment {
        text: result,
        fg: first_style.fg,
        bg: first_style.bg,
        bold: first_style.bold,
        span_type: first_style.span_type,
        link_data: None,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HighlightPattern, RedirectMode};

    // Helper to create a basic highlight pattern
    fn make_pattern(pattern: &str) -> HighlightPattern {
        HighlightPattern {
            pattern: pattern.to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: None,
            sound_volume: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            compiled_regex: None,
        }
    }

    // Helper to create a text segment
    fn make_segment(text: &str) -> TextSegment {
        TextSegment {
            text: text.to_string(),
            fg: None,
            bg: None,
            bold: false,
            span_type: SpanType::Normal,
            link_data: None,
        }
    }

    // Helper to get full text from segments
    fn segments_to_text(segments: &[TextSegment]) -> String {
        segments.iter().map(|s| s.text.as_str()).collect()
    }

    // ===========================================
    // Empty/passthrough tests
    // ===========================================

    #[test]
    fn test_empty_highlights_passthrough() {
        let engine = CoreHighlightEngine::empty();
        let segments = vec![make_segment("Hello world")];
        let result = engine.apply_highlights(&segments, "main");

        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].text, "Hello world");
        assert!(result.sounds.is_empty());
        assert!(result.deferred_replacements.is_empty());
    }

    #[test]
    fn test_empty_segments_passthrough() {
        let patterns = vec![{
            let mut p = make_pattern("test");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments: Vec<TextSegment> = vec![];
        let result = engine.apply_highlights(&segments, "main");

        assert!(result.segments.is_empty());
        assert!(result.sounds.is_empty());
    }

    #[test]
    fn test_no_match_passthrough() {
        let patterns = vec![{
            let mut p = make_pattern("xyz");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("Hello world")];
        let result = engine.apply_highlights(&segments, "main");

        // Should have original text, no colors applied
        assert_eq!(segments_to_text(&result.segments), "Hello world");
        assert!(result.segments.iter().all(|s| s.fg.is_none()));
    }

    // ===========================================
    // Basic pattern matching tests
    // ===========================================

    #[test]
    fn test_single_pattern_colors_match() {
        let patterns = vec![{
            let mut p = make_pattern("world");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("Hello world")];
        let result = engine.apply_highlights(&segments, "main");

        // "world" should have the color
        assert!(result
            .segments
            .iter()
            .any(|s| s.text.contains("world") && s.fg == Some("#FF0000".to_string())));
    }

    #[test]
    fn test_multiple_patterns_apply() {
        let patterns = vec![
            {
                let mut p = make_pattern("Hello");
                p.fg = Some("#00FF00".to_string());
                p
            },
            {
                let mut p = make_pattern("world");
                p.fg = Some("#FF0000".to_string());
                p
            },
        ];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("Hello world")];
        let result = engine.apply_highlights(&segments, "main");

        // Both should be colored
        let has_hello_color = result
            .segments
            .iter()
            .any(|s| s.text.contains("Hello") && s.fg == Some("#00FF00".to_string()));
        let has_world_color = result
            .segments
            .iter()
            .any(|s| s.text.contains("world") && s.fg == Some("#FF0000".to_string()));

        assert!(has_hello_color, "Hello should be green");
        assert!(has_world_color, "world should be red");
    }

    #[test]
    fn test_background_color_applied() {
        let patterns = vec![{
            let mut p = make_pattern("test");
            p.bg = Some("#330000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("This is a test")];
        let result = engine.apply_highlights(&segments, "main");

        assert!(result
            .segments
            .iter()
            .any(|s| s.text == "test" && s.bg == Some("#330000".to_string())));
    }

    #[test]
    fn test_bold_applied() {
        let patterns = vec![{
            let mut p = make_pattern("bold");
            p.bold = true;
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("Make this bold text")];
        let result = engine.apply_highlights(&segments, "main");

        assert!(result.segments.iter().any(|s| s.text == "bold" && s.bold));
    }

    // ===========================================
    // Fast parse (Aho-Corasick) tests
    // ===========================================

    #[test]
    fn test_fast_parse_literal_matching() {
        let patterns = vec![{
            let mut p = make_pattern("damage|hits|misses");
            p.fg = Some("#FF0000".to_string());
            p.fast_parse = true;
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("You take 50 damage")];
        let result = engine.apply_highlights(&segments, "main");

        assert!(result
            .segments
            .iter()
            .any(|s| s.text == "damage" && s.fg == Some("#FF0000".to_string())));
    }

    #[test]
    fn test_fast_parse_multiple_alternatives() {
        let patterns = vec![{
            let mut p = make_pattern("hit|miss|dodge");
            p.fg = Some("#00FF00".to_string());
            p.fast_parse = true;
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);

        // Test "hit"
        let result1 = engine.apply_highlights(&[make_segment("You hit the target")], "main");
        assert!(result1
            .segments
            .iter()
            .any(|s| s.text == "hit" && s.fg == Some("#00FF00".to_string())));

        // Test "miss"
        let result2 = engine.apply_highlights(&[make_segment("You miss!")], "main");
        assert!(result2
            .segments
            .iter()
            .any(|s| s.text == "miss" && s.fg == Some("#00FF00".to_string())));
    }

    #[test]
    fn test_fast_parse_word_boundary_check() {
        // "dam" should NOT match "damage" due to word boundary
        let patterns = vec![{
            let mut p = make_pattern("dam");
            p.fg = Some("#FF0000".to_string());
            p.fast_parse = true;
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("You take 50 damage")];
        let result = engine.apply_highlights(&segments, "main");

        // Should NOT color anything (no word boundary match)
        assert!(
            result.segments.iter().all(|s| s.fg.is_none()),
            "dam should not match within damage"
        );
    }

    #[test]
    fn test_fast_parse_matches_standalone_word() {
        let patterns = vec![{
            let mut p = make_pattern("dam");
            p.fg = Some("#FF0000".to_string());
            p.fast_parse = true;
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("The dam broke")];
        let result = engine.apply_highlights(&segments, "main");

        // Should match standalone "dam"
        assert!(result
            .segments
            .iter()
            .any(|s| s.text == "dam" && s.fg == Some("#FF0000".to_string())));
    }

    // ===========================================
    // Regex pattern tests
    // ===========================================

    #[test]
    fn test_regex_pattern_matching() {
        let patterns = vec![{
            let mut p = make_pattern(r"\d+ damage");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("You take 50 damage")];
        let result = engine.apply_highlights(&segments, "main");

        assert!(result
            .segments
            .iter()
            .any(|s| s.text == "50 damage" && s.fg == Some("#FF0000".to_string())));
    }

    #[test]
    fn test_invalid_regex_is_skipped() {
        // Invalid regex should not crash, just be skipped
        let patterns = vec![{
            let mut p = make_pattern(r"[invalid(regex");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("Some text")];
        let result = engine.apply_highlights(&segments, "main");

        // Should pass through unchanged
        assert_eq!(segments_to_text(&result.segments), "Some text");
    }

    // ===========================================
    // Replacement tests
    // ===========================================

    #[test]
    fn test_replacement_modifies_text() {
        let patterns = vec![{
            let mut p = make_pattern("xxx");
            p.replace = Some("yyy".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("Replace xxx here")];
        let result = engine.apply_highlights(&segments, "main");

        let full_text = segments_to_text(&result.segments);
        assert!(full_text.contains("yyy"), "Should contain replacement");
        assert!(!full_text.contains("xxx"), "Should not contain original");
    }

    #[test]
    fn test_regex_capture_groups_in_replacement() {
        let patterns = vec![{
            let mut p = make_pattern(r"(\d+) damage");
            p.replace = Some("[$1] DMG".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("You take 50 damage")];
        let result = engine.apply_highlights(&segments, "main");

        let full_text = segments_to_text(&result.segments);
        assert!(
            full_text.contains("[50] DMG"),
            "Should expand capture group"
        );
    }

    #[test]
    fn test_replacement_disabled() {
        let patterns = vec![{
            let mut p = make_pattern("xxx");
            p.replace = Some("yyy".to_string());
            p
        }];
        let mut engine = CoreHighlightEngine::new(patterns);
        engine.set_replace_enabled(false);

        let segments = vec![make_segment("Replace xxx here")];
        let result = engine.apply_highlights(&segments, "main");

        let full_text = segments_to_text(&result.segments);
        assert!(full_text.contains("xxx"), "Original should be preserved");
        assert!(
            !full_text.contains("yyy"),
            "Replacement should not be applied"
        );
    }

    #[test]
    fn test_empty_replacement() {
        let patterns = vec![{
            let mut p = make_pattern("remove");
            p.replace = Some("".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("Please remove this")];
        let result = engine.apply_highlights(&segments, "main");

        let full_text = segments_to_text(&result.segments);
        assert!(!full_text.contains("remove"), "Word should be removed");
        assert!(full_text.contains("Please  this"), "Rest should remain");
    }

    // ===========================================
    // Deferred replacement tests
    // ===========================================

    #[test]
    fn test_deferred_replacement_for_window() {
        let patterns = vec![{
            let mut p = make_pattern("xxx");
            p.replace = Some("yyy".to_string());
            p.window = Some("deaths".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("Replace xxx here")];
        let result = engine.apply_highlights(&segments, "main");

        // Replacement should be DEFERRED, not applied
        let full_text = segments_to_text(&result.segments);
        assert!(full_text.contains("xxx"), "Original should be preserved");
        assert!(
            !result.deferred_replacements.is_empty(),
            "Should have deferred replacement"
        );
        assert_eq!(result.deferred_replacements[0].target_window, "deaths");
        assert_eq!(result.deferred_replacements[0].replacement, "yyy");
    }

    #[test]
    fn test_apply_deferred_for_window_matches() {
        let segments = vec![make_segment("Replace xxx here")];
        let deferred = vec![DeferredReplacement {
            start_char: 8,
            end_char: 11,
            replacement: "yyy".to_string(),
            target_window: "deaths".to_string(),
            original_text: "xxx".to_string(),
        }];

        // Apply to deaths window - should replace
        let result = apply_deferred_for_window(&segments, &deferred, "deaths");
        let full_text = segments_to_text(&result);
        assert!(full_text.contains("yyy"), "Should apply replacement");
        assert!(!full_text.contains("xxx"), "Should replace original");
    }

    #[test]
    fn test_apply_deferred_for_window_no_match() {
        let segments = vec![make_segment("Replace xxx here")];
        let deferred = vec![DeferredReplacement {
            start_char: 8,
            end_char: 11,
            replacement: "yyy".to_string(),
            target_window: "deaths".to_string(),
            original_text: "xxx".to_string(),
        }];

        // Apply to main window - should NOT replace
        let result = apply_deferred_for_window(&segments, &deferred, "main");
        let full_text = segments_to_text(&result);
        assert!(full_text.contains("xxx"), "Should preserve original");
        assert!(!full_text.contains("yyy"), "Should not apply replacement");
    }

    #[test]
    fn test_apply_deferred_case_insensitive_window_match() {
        let segments = vec![make_segment("Replace xxx here")];
        let deferred = vec![DeferredReplacement {
            start_char: 8,
            end_char: 11,
            replacement: "yyy".to_string(),
            target_window: "Deaths".to_string(),
            original_text: "xxx".to_string(),
        }];

        // Apply with different case - should still match
        let result = apply_deferred_for_window(&segments, &deferred, "deaths");
        let full_text = segments_to_text(&result);
        assert!(
            full_text.contains("yyy"),
            "Case-insensitive match should work"
        );
    }

    // ===========================================
    // Stream filter tests
    // ===========================================

    #[test]
    fn test_stream_filter_respects_stream_name() {
        let patterns = vec![{
            let mut p = make_pattern("test");
            p.fg = Some("#FF0000".to_string());
            p.stream = Some("death".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("test message")];

        // Main stream - should NOT match
        let result_main = engine.apply_highlights(&segments, "main");
        assert!(
            result_main.segments.iter().all(|s| s.fg.is_none()),
            "Should not color in main stream"
        );

        // Death stream - should match
        let result_death = engine.apply_highlights(&segments, "death");
        assert!(
            result_death
                .segments
                .iter()
                .any(|s| s.fg == Some("#FF0000".to_string())),
            "Should color in death stream"
        );
    }

    #[test]
    fn test_stream_filter_case_insensitive() {
        let patterns = vec![{
            let mut p = make_pattern("test");
            p.fg = Some("#FF0000".to_string());
            p.stream = Some("Death".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("test message")];

        // Lowercase stream should match uppercase filter
        let result = engine.apply_highlights(&segments, "death");
        assert!(result
            .segments
            .iter()
            .any(|s| s.fg == Some("#FF0000".to_string())));
    }

    // ===========================================
    // Color entire line tests
    // ===========================================

    #[test]
    fn test_color_entire_line_flag() {
        let patterns = vec![{
            let mut p = make_pattern("death");
            p.fg = Some("#FF0000".to_string());
            p.color_entire_line = true;
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("You have death nearby")];
        let result = engine.apply_highlights(&segments, "main");

        // ALL segments should have the color
        assert!(
            result
                .segments
                .iter()
                .all(|s| s.fg == Some("#FF0000".to_string())),
            "Entire line should be colored"
        );
    }

    #[test]
    fn test_color_entire_line_preserves_link_span_type() {
        let patterns = vec![{
            let mut p = make_pattern("click");
            p.fg = Some("#FF0000".to_string());
            p.bg = Some("#330000".to_string());
            p.color_entire_line = true;
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);

        // Create segment with Link span type
        let segments = vec![TextSegment {
            text: "click here".to_string(),
            fg: Some("#0000FF".to_string()), // Blue link
            bg: None,
            bold: false,
            span_type: SpanType::Link,
            link_data: None,
        }];

        let result = engine.apply_highlights(&segments, "main");

        // Link segments should only get bg color, not fg (to preserve link visibility)
        let link_seg = result
            .segments
            .iter()
            .find(|s| s.span_type == SpanType::Link);
        assert!(link_seg.is_some());
        let link = link_seg.unwrap();
        assert_eq!(
            link.bg,
            Some("#330000".to_string()),
            "Link should get bg color"
        );
        // fg should be applied but the span_type check might preserve original
    }

    // ===========================================
    // System span type tests
    // ===========================================

    #[test]
    fn test_system_span_type_skips_highlights() {
        let patterns = vec![{
            let mut p = make_pattern("test");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);

        let segments = vec![TextSegment {
            text: "test system message".to_string(),
            fg: None,
            bg: None,
            bold: false,
            span_type: SpanType::System,
            link_data: None,
        }];

        let result = engine.apply_highlights(&segments, "main");

        // Should be unchanged (system messages skip highlighting)
        assert!(
            result.segments.iter().all(|s| s.fg.is_none()),
            "System spans should not be highlighted"
        );
    }

    // ===========================================
    // Sound trigger tests
    // ===========================================

    #[test]
    fn test_sound_trigger_collected() {
        let patterns = vec![{
            let mut p = make_pattern("alarm");
            p.sound = Some("alert.wav".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("The alarm rings")];
        let result = engine.apply_highlights(&segments, "main");

        assert!(!result.sounds.is_empty(), "Should have sound trigger");
        assert_eq!(result.sounds[0].file, "alert.wav");
    }

    #[test]
    fn test_sound_volume_override() {
        let patterns = vec![{
            let mut p = make_pattern("alarm");
            p.sound = Some("alert.wav".to_string());
            p.sound_volume = Some(0.5);
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("The alarm rings")];
        let result = engine.apply_highlights(&segments, "main");

        assert_eq!(result.sounds[0].volume, Some(0.5));
    }

    #[test]
    fn test_no_sound_when_no_match() {
        let patterns = vec![{
            let mut p = make_pattern("alarm");
            p.sound = Some("alert.wav".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("No match here")];
        let result = engine.apply_highlights(&segments, "main");

        assert!(result.sounds.is_empty(), "No match means no sound");
    }

    // ===========================================
    // get_first_match_color tests
    // ===========================================

    #[test]
    fn test_get_first_match_color_returns_first() {
        let patterns = vec![
            {
                let mut p = make_pattern("first");
                p.fg = Some("#FF0000".to_string());
                p
            },
            {
                let mut p = make_pattern("second");
                p.fg = Some("#00FF00".to_string());
                p
            },
        ];
        let engine = CoreHighlightEngine::new(patterns);

        let color = engine.get_first_match_color("first second");
        assert_eq!(color, Some("#FF0000".to_string()));
    }

    #[test]
    fn test_get_first_match_color_no_match() {
        let patterns = vec![{
            let mut p = make_pattern("test");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);

        let color = engine.get_first_match_color("no match here");
        assert!(color.is_none());
    }

    #[test]
    fn test_get_first_match_color_empty_engine() {
        let engine = CoreHighlightEngine::empty();
        let color = engine.get_first_match_color("any text");
        assert!(color.is_none());
    }

    #[test]
    fn test_get_first_match_color_fast_parse() {
        let patterns = vec![{
            let mut p = make_pattern("damage|hit");
            p.fg = Some("#FF0000".to_string());
            p.fast_parse = true;
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);

        let color = engine.get_first_match_color("You hit the target");
        assert_eq!(color, Some("#FF0000".to_string()));
    }

    // ===========================================
    // Update and default tests
    // ===========================================

    #[test]
    fn test_update_patterns_rebuilds_engine() {
        let mut engine = CoreHighlightEngine::empty();

        let patterns = vec![{
            let mut p = make_pattern("test");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        engine.update_patterns(patterns);

        let segments = vec![make_segment("test message")];
        let result = engine.apply_highlights(&segments, "main");

        assert!(result
            .segments
            .iter()
            .any(|s| s.fg == Some("#FF0000".to_string())));
    }

    #[test]
    fn test_default_is_empty() {
        let engine = CoreHighlightEngine::default();
        let segments = vec![make_segment("test")];
        let result = engine.apply_highlights(&segments, "main");

        // Should pass through unchanged
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].text, "test");
    }

    // ===========================================
    // Edge case tests
    // ===========================================

    #[test]
    fn test_overlapping_matches_first_wins() {
        // Two patterns that would overlap
        let patterns = vec![
            {
                let mut p = make_pattern("abc");
                p.fg = Some("#FF0000".to_string());
                p
            },
            {
                let mut p = make_pattern("bcd");
                p.fg = Some("#00FF00".to_string());
                p
            },
        ];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("abcd")];
        let result = engine.apply_highlights(&segments, "main");

        // First match should win, second should be skipped
        assert!(result
            .segments
            .iter()
            .any(|s| s.text == "abc" && s.fg == Some("#FF0000".to_string())));
    }

    #[test]
    fn test_unicode_text_handling() {
        let patterns = vec![{
            let mut p = make_pattern("★");
            p.fg = Some("#FFD700".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);
        let segments = vec![make_segment("You get ★★★ stars")];
        let result = engine.apply_highlights(&segments, "main");

        // Should handle unicode correctly
        let full_text = segments_to_text(&result.segments);
        assert!(full_text.contains("★"));
    }

    #[test]
    fn test_multiple_segments_input() {
        let patterns = vec![{
            let mut p = make_pattern("red");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);

        // Input with multiple segments
        let segments = vec![
            TextSegment {
                text: "Make this ".to_string(),
                fg: None,
                bg: None,
                bold: false,
                span_type: SpanType::Normal,
                link_data: None,
            },
            TextSegment {
                text: "red text".to_string(),
                fg: None,
                bg: None,
                bold: false,
                span_type: SpanType::Normal,
                link_data: None,
            },
        ];

        let result = engine.apply_highlights(&segments, "main");

        // "red" should be colored even when split across segment boundary logic
        assert!(result
            .segments
            .iter()
            .any(|s| s.text == "red" && s.fg == Some("#FF0000".to_string())));
    }

    #[test]
    fn test_preserves_existing_colors() {
        let patterns = vec![{
            let mut p = make_pattern("world");
            p.fg = Some("#FF0000".to_string());
            p
        }];
        let engine = CoreHighlightEngine::new(patterns);

        // Input with pre-existing color
        let segments = vec![TextSegment {
            text: "Hello world".to_string(),
            fg: Some("#0000FF".to_string()), // Pre-existing blue
            bg: None,
            bold: false,
            span_type: SpanType::Normal,
            link_data: None,
        }];

        let result = engine.apply_highlights(&segments, "main");

        // "Hello " should keep original blue, "world" should be red
        let hello_seg = result.segments.iter().find(|s| s.text.contains("Hello"));
        let world_seg = result.segments.iter().find(|s| s.text == "world");

        assert!(hello_seg.is_some());
        assert_eq!(
            hello_seg.unwrap().fg,
            Some("#0000FF".to_string()),
            "Hello should keep original color"
        );
        assert!(world_seg.is_some());
        assert_eq!(
            world_seg.unwrap().fg,
            Some("#FF0000".to_string()),
            "world should get highlight color"
        );
    }
}
