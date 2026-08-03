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

/// Is `c` a character we treat as "emoji" for OUTBOUND stripping?
///
/// This is deliberately conservative-but-broad: it covers the Unicode blocks
/// that hold pictographic emoji plus the joiner/selector machinery that binds
/// them into sequences (ZWJ, VS16, skin-tone modifiers, regional indicators,
/// keycap combiner). It intentionally does NOT include low-codepoint dual-use
/// symbols like `©`/`®`/`™`/`#`/`*`/digits (which are ordinary text far more
/// often than emoji), matching the U+203C floor the GUI color overlay uses so
/// the two agree on "what is an emoji glyph". Stripping is glyph-based, so a
/// broad block match is correct here even though the GUI overlay is pickier
/// about which sequences it can paint.
pub fn is_emoji_char(c: char) -> bool {
    let u = c as u32;
    matches!(u,
        0x200D                       // ZWJ (binds sequences)
        | 0xFE0F                     // VS16 emoji presentation selector
        | 0x20E3                     // combining enclosing keycap
        | 0x203C | 0x2049            // ‼ ⁉
        | 0x2122 | 0x2139            // ™ ℹ
        | 0x2194..=0x21AA            // arrows used as emoji
        | 0x231A..=0x231B            // ⌚ ⌛
        | 0x2328
        | 0x23CF
        | 0x23E9..=0x23F3
        | 0x23F8..=0x23FA
        | 0x24C2                     // Ⓜ
        | 0x25AA..=0x25AB
        | 0x25B6 | 0x25C0
        | 0x25FB..=0x25FE
        | 0x2600..=0x27BF            // Misc symbols + Dingbats (big emoji range)
        | 0x2934..=0x2935
        | 0x2B00..=0x2BFF            // Misc symbols and arrows
        | 0x3030 | 0x303D
        | 0x3297 | 0x3299            // ㊗ ㊙
        | 0x1F000..=0x1FAFF          // Mahjong … Symbols & Pictographs Extended-A
        | 0x1F1E6..=0x1F1FF          // Regional indicators (flags)
        | 0xE0020..=0xE007F          // Tag characters (subdivision flags)
    )
}

/// Strip emoji from a string that is about to be sent TO THE GAME.
///
/// The game is a roleplaying MUD; sending emoji (or the `:grin:` shortcodes
/// that render as emoji) as speech/thoughts/whispers can get a player warned
/// or banned. Emoji are a Vellum-side display convenience only and must never
/// reach the server. This runs at the single network write seam so it covers
/// every frontend and every internally-generated command.
///
/// What it removes:
///  - runs of emoji characters (`is_emoji_char`), including multi-codepoint
///    sequences (ZWJ families, skin tones, flags);
///  - `:shortcode:` runs that resolve to a KNOWN emoji (gemoji, and — once
///    wired — custom emoji). Unknown `:foo:` and bare colons (Lich script
///    syntax, time strings) are left byte-identical so scripts don't break.
///
/// Adjacent spaces left behind by a removed emoji are collapsed so
/// `"hi :grin: there"` sends as `"hi there"`, not `"hi  there"`. Returns
/// `None` when nothing was stripped so the caller avoids allocating.
pub fn strip_outbound_emoji(text: &str) -> Option<String> {
    // Fast path: no emoji glyph and no ':' means nothing to do.
    if !text.contains(':') && !text.chars().any(is_emoji_char) {
        return None;
    }

    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    let mut i = 0usize;

    // Track whether the last char we PUSHED was a space, and whether we just
    // removed something, so we can collapse the space a removed emoji leaves.
    while i < bytes.len() {
        let b = bytes[i];

        // Candidate shortcode :name:
        if b == b':' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && is_shortcode_byte(bytes[j]) {
                j += 1;
            }
            if j > start && j < bytes.len() && bytes[j] == b':' {
                let code = &text[start..j];
                let known = if code.bytes().any(|b| b.is_ascii_uppercase()) {
                    emojis::get_by_shortcode(&code.to_ascii_lowercase()).is_some()
                } else {
                    emojis::get_by_shortcode(code).is_some()
                };
                if known {
                    // Drop the whole :code:. Collapse a following space if the
                    // char we already emitted ended with a space (or output is
                    // empty), to avoid a double space / leading space.
                    changed = true;
                    i = j + 1;
                    if (out.is_empty() || out.ends_with(' '))
                        && i < bytes.len()
                        && bytes[i] == b' '
                    {
                        i += 1;
                    }
                    continue;
                }
            }
            // Not a known shortcode: emit the ':' literally.
            out.push(':');
            i += 1;
            continue;
        }

        // Decode one char to test for emoji.
        let ch = text[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        if is_emoji_char(ch) {
            changed = true;
            i += ch_len;
            // Collapse a trailing space after an emoji run if the output
            // already ends with a space (or is empty).
            if (out.is_empty() || out.ends_with(' ')) && i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            continue;
        }

        out.push(ch);
        i += ch_len;
    }

    if !changed {
        return None;
    }
    // A removed trailing emoji can leave one trailing space.
    while out.ends_with(' ') && text.ends_with(|c: char| is_emoji_char(c) || c == ':') {
        out.pop();
        break;
    }
    Some(out)
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

    // ==================== Outbound emoji stripping ====================

    #[test]
    fn strip_removes_unicode_emoji_glyph() {
        // A raw emoji glyph typed/pasted into a command must not reach the game.
        assert_eq!(
            strip_outbound_emoji("say hello \u{1F601}").as_deref(),
            Some("say hello")
        );
    }

    #[test]
    fn strip_removes_known_shortcode() {
        assert_eq!(
            strip_outbound_emoji("say :grin: hello").as_deref(),
            Some("say hello")
        );
    }

    #[test]
    fn strip_collapses_the_space_left_behind() {
        assert_eq!(
            strip_outbound_emoji("hi :grin: there").as_deref(),
            Some("hi there")
        );
        assert_eq!(
            strip_outbound_emoji("hi \u{1F601} there").as_deref(),
            Some("hi there")
        );
    }

    #[test]
    fn strip_leaves_unknown_shortcodes_alone() {
        // :VibeCat: isn't a known gemoji yet — must pass through untouched so
        // we don't corrupt arbitrary text/script tokens. (Once custom emoji
        // are wired, the resolver will recognize it and it WILL be stripped.)
        assert_eq!(strip_outbound_emoji("emote :VibeCat:"), None);
        assert_eq!(strip_outbound_emoji("a :notacode: b"), None);
    }

    #[test]
    fn strip_preserves_lich_script_colons_and_times() {
        // Lich script syntax and time strings must survive verbatim.
        assert_eq!(strip_outbound_emoji(";send at 12:30:45"), None);
        assert_eq!(strip_outbound_emoji(";e echo done"), None);
        assert_eq!(strip_outbound_emoji("ratio 3:1 odds"), None);
    }

    #[test]
    fn strip_returns_none_when_clean() {
        assert_eq!(strip_outbound_emoji("say hello there"), None);
        assert_eq!(strip_outbound_emoji(";script arg1 arg2"), None);
    }

    #[test]
    fn strip_handles_multiple_and_mixed() {
        assert_eq!(
            strip_outbound_emoji(":grin: hi \u{1F44D} :heart:").as_deref(),
            Some("hi")
        );
    }

    #[test]
    fn strip_handles_multi_codepoint_sequence() {
        // Thumbs-up with a skin-tone modifier (two codepoints) removed whole.
        assert_eq!(
            strip_outbound_emoji("nice \u{1F44D}\u{1F3FB}").as_deref(),
            Some("nice")
        );
        // A flag (two regional indicators).
        assert_eq!(
            strip_outbound_emoji("go \u{1F1FA}\u{1F1F8} team").as_deref(),
            Some("go team")
        );
    }

    #[test]
    fn strip_shortcode_at_boundaries() {
        assert_eq!(strip_outbound_emoji(":grin:").as_deref(), Some(""));
        assert_eq!(strip_outbound_emoji(":grin: hi").as_deref(), Some("hi"));
        assert_eq!(strip_outbound_emoji("hi :grin:").as_deref(), Some("hi"));
    }

    #[test]
    fn strip_does_not_touch_low_codepoint_symbols() {
        // ©/®/™/# and digits are ordinary text far more than emoji.
        assert_eq!(strip_outbound_emoji("copyright (c) 2026"), None);
        assert_eq!(strip_outbound_emoji("channel #trade"), None);
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
