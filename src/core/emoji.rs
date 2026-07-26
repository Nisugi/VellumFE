//! Discord-style emoji shortcode expansion for incoming game text.
//!
//! Rewrites `:grin:`-style shortcodes into their emoji (embedded gemoji data
//! via the `emojis` crate) as a pure in-segment text rewrite: segments are
//! never split or merged, styles and link data are untouched, and unknown or
//! malformed codes stay byte-identical. Runs at the same pipeline seam as
//! highlight text replacement (see `MessageProcessor::flush_current_stream`),
//! so every frontend (TUI, GUI, web) sees the expanded text.

use crate::data::TextSegment;

/// Bytes allowed inside a shortcode name (between the colons).
/// Matches gemoji shortcode alphabet: `[A-Za-z0-9_+-]`.
#[inline]
fn is_shortcode_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-'
}

/// Replace every known `:code:` shortcode in `text` with its emoji.
///
/// Returns `None` when nothing was replaced so callers can keep the original
/// string without allocating. Lookup is case-insensitive (codes are
/// lowercased); unknown codes and malformed candidates (empty `::`, unclosed
/// `:grin`, time strings like `12:30:45`) pass through byte-identical.
/// Replaced output is never rescanned.
pub fn replace_shortcodes(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    // Byte index up to which `text` has been flushed into `out`.
    let mut copied = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b':' {
            i += 1;
            continue;
        }

        // Candidate name runs from just past this ':' to the next
        // non-shortcode byte.
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && is_shortcode_byte(bytes[j]) {
            j += 1;
        }

        // A valid candidate is a non-empty name followed by a closing ':'.
        if j > start && j < bytes.len() && bytes[j] == b':' {
            let code = &text[start..j];
            let hit = if code.bytes().any(|b| b.is_ascii_uppercase()) {
                emojis::get_by_shortcode(&code.to_ascii_lowercase())
            } else {
                emojis::get_by_shortcode(code)
            };
            if let Some(emoji) = hit {
                let buf = out.get_or_insert_with(|| String::with_capacity(text.len()));
                buf.push_str(&text[copied..i]);
                buf.push_str(emoji.as_str());
                copied = j + 1;
                i = j + 1; // Never rescan replaced output.
                continue;
            }
        }

        // No replacement. Resume at `j`: if the candidate ended at a ':' it
        // may open the next shortcode (e.g. `:x:grin:`); any other byte is
        // skipped by the loop. `j >= i + 1`, so progress is guaranteed.
        i = j;
    }

    out.map(|mut buf| {
        buf.push_str(&text[copied..]);
        buf
    })
}

/// Expand shortcodes in every segment's text in place.
///
/// Pure text rewrite: styles, span types, and link data are left untouched
/// and segments are never split or merged. Segments without a ':' are
/// skipped without any scanning.
pub fn apply_to_segments(segments: &mut [TextSegment]) {
    for seg in segments.iter_mut() {
        // System spans skip text transforms (same rule as the highlight
        // engine); everything else is display text.
        if seg.span_type == crate::data::widget::SpanType::System {
            continue;
        }
        if !seg.text.contains(':') {
            continue;
        }
        if let Some(replaced) = replace_shortcodes(&seg.text) {
            seg.text = replaced;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::{LinkData, SpanType};

    #[test]
    fn replaces_known_shortcode() {
        assert_eq!(
            replace_shortcodes("You :grin: broadly.").as_deref(),
            Some("You \u{1F601} broadly.")
        );
    }

    #[test]
    fn replaces_multiple_per_line() {
        assert_eq!(
            replace_shortcodes(":grin: and :heart: forever").as_deref(),
            Some("\u{1F601} and \u{2764}\u{FE0F} forever")
        );
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(replace_shortcodes(":GRIN:").as_deref(), Some("\u{1F601}"));
    }

    #[test]
    fn plus_and_minus_codes_work() {
        assert_eq!(replace_shortcodes(":+1:").as_deref(), Some("\u{1F44D}"));
        assert_eq!(replace_shortcodes(":-1:").as_deref(), Some("\u{1F44E}"));
    }

    #[test]
    fn unknown_code_untouched() {
        assert_eq!(replace_shortcodes("a :notarealcode: b"), None);
    }

    #[test]
    fn time_strings_and_adjacent_colons_untouched() {
        assert_eq!(replace_shortcodes("at 12:30:45 sharp"), None);
        assert_eq!(replace_shortcodes("weird :: tokens ::"), None);
        assert_eq!(replace_shortcodes(":unclosed"), None);
        assert_eq!(replace_shortcodes("trailing:"), None);
        assert_eq!(replace_shortcodes(":"), None);
    }

    #[test]
    fn failed_candidate_colon_can_open_next_code() {
        // The ':' closing a failed candidate starts the next one.
        assert_eq!(
            replace_shortcodes(":notarealcode:grin:").as_deref(),
            Some(":notarealcode\u{1F601}")
        );
    }

    #[test]
    fn replaced_output_is_not_rescanned() {
        // :grin: expands once; scanning resumes after the closing colon so
        // the following text is still handled independently.
        assert_eq!(
            replace_shortcodes(":grin::grin:").as_deref(),
            Some("\u{1F601}\u{1F601}")
        );
    }

    #[test]
    fn system_spans_are_skipped() {
        let mut segments = vec![TextSegment {
            text: "[client :grin: notice]".into(),
            fg: None,
            bg: None,
            bold: false,
            mono: false,
            span_type: SpanType::System,
            link_data: None,
        }];
        apply_to_segments(&mut segments);
        assert_eq!(segments[0].text, "[client :grin: notice]");
    }

    #[test]
    fn segments_keep_styles_and_links() {
        let mut segments = vec![
            TextSegment {
                text: "no colon here".into(),
                fg: Some("#ff0000".into()),
                bg: None,
                bold: true,
                mono: false,
                span_type: SpanType::Normal,
                link_data: None,
            },
            TextSegment {
                text: "click :grin: me".into(),
                fg: Some("#00ff00".into()),
                bg: Some("#000000".into()),
                bold: false,
                mono: true,
                span_type: SpanType::Link,
                link_data: Some(LinkData {
                    exist_id: "12345".into(),
                    noun: "smile".into(),
                    text: "smile".into(),
                    coord: None,
                }),
            },
        ];

        apply_to_segments(&mut segments);

        assert_eq!(segments.len(), 2, "segments must never split or merge");
        assert_eq!(segments[0].text, "no colon here");
        assert_eq!(segments[1].text, "click \u{1F601} me");
        // Styles and link data untouched
        assert_eq!(segments[1].fg.as_deref(), Some("#00ff00"));
        assert_eq!(segments[1].bg.as_deref(), Some("#000000"));
        assert!(segments[1].mono);
        assert_eq!(segments[1].span_type, SpanType::Link);
        let link = segments[1].link_data.as_ref().expect("link preserved");
        assert_eq!(link.exist_id, "12345");
        assert_eq!(link.noun, "smile");
    }
}
