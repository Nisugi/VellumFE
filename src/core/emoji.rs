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
                let is_gemoji = if code.bytes().any(|b| b.is_ascii_uppercase()) {
                    emojis::get_by_shortcode(&code.to_ascii_lowercase()).is_some()
                } else {
                    emojis::get_by_shortcode(code).is_some()
                };
                // Also strip installed CUSTOM emoji (:VibeCat:) — they're
                // Vellum-only and meaningless (or embarrassing) to the game.
                let known = is_gemoji || crate::core::custom_emoji::contains(code);
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

/// Expand shortcodes in every segment.
///
/// Gemoji shortcodes (`:grin:`) resolve to a Unicode glyph as a pure in-place
/// text rewrite. CUSTOM emoji (`:VibeCat:`, installed under
/// `~/.vellum-fe/emoji/`) have no Unicode codepoint, so a segment containing
/// one is SPLIT: the custom run becomes its own segment carrying the same
/// style but tagged with [`TextSegment::custom_emoji`] = the name, its `text`
/// left as the literal `:name:` (the universal fallback). Ordinary text and
/// gemoji are never split. System spans are skipped entirely.
///
/// Custom emoji take priority over gemoji on a name collision (the user's
/// file wins), matching the Discord convention.
pub fn apply_to_segments(segments: &mut Vec<TextSegment>) {
    // Only pay the split cost if a custom emoji could possibly appear.
    let have_custom = !crate::core::custom_emoji::all().is_empty();

    // Fast path: no custom emoji installed → pure in-place gemoji rewrite,
    // never splits (identical to the original behavior, zero extra allocation).
    if !have_custom {
        for seg in segments.iter_mut() {
            if seg.span_type == crate::data::widget::SpanType::System
                || !seg.text.contains(':')
            {
                continue;
            }
            if let Some(replaced) = replace_shortcodes(&seg.text) {
                seg.text = replaced;
            }
        }
        return;
    }

    // Custom emoji are installed: a segment may need splitting, so build a
    // fresh vec. Consume the old segments by value to avoid borrow conflicts.
    let old = std::mem::take(segments);
    segments.reserve(old.len());
    for seg in old {
        if seg.span_type == crate::data::widget::SpanType::System || !seg.text.contains(':') {
            segments.push(seg);
            continue;
        }
        if contains_custom_shortcode(&seg.text) {
            split_segment_with_custom(&seg, segments);
        } else {
            let mut seg = seg;
            if let Some(replaced) = replace_shortcodes(&seg.text) {
                seg.text = replaced;
            }
            segments.push(seg);
        }
    }
}

/// Does `text` contain at least one `:name:` that is an installed custom emoji?
fn contains_custom_shortcode(text: &str) -> bool {
    scan_shortcodes(text).any(|(name, _, _)| crate::core::custom_emoji::contains(name))
}

/// Split one segment into a run of segments, turning each custom-emoji
/// shortcode into its own tagged segment and resolving gemoji in the plain-text
/// pieces between them. Pushes the results onto `out`.
fn split_segment_with_custom(seg: &TextSegment, out: &mut Vec<TextSegment>) {
    let text = &seg.text;
    let mut cursor = 0usize;

    let push_text = |out: &mut Vec<TextSegment>, slice: &str| {
        if slice.is_empty() {
            return;
        }
        let mut piece = seg.clone();
        piece.custom_emoji = None;
        // Resolve gemoji within the plain slice.
        piece.text = replace_shortcodes(slice)
            .unwrap_or_else(|| slice.to_string());
        out.push(piece);
    };

    for (name, start, end) in scan_shortcodes(text) {
        if !crate::core::custom_emoji::contains(name) {
            continue;
        }
        // Flush the plain text before this custom emoji.
        push_text(out, &text[cursor..start]);
        // The custom-emoji segment: literal :name: as fallback text, tagged.
        let mut custom = seg.clone();
        custom.text = text[start..end].to_string(); // includes the colons
        custom.custom_emoji = Some(name.to_ascii_lowercase());
        // A custom emoji is an image, not a clickable game object; drop link
        // data so it never mis-registers as a link.
        custom.link_data = None;
        out.push(custom);
        cursor = end;
    }
    // Trailing plain text after the last custom emoji.
    push_text(out, &text[cursor..]);
}

/// Iterate `:name:` candidates in `text`, yielding `(name, start, end)` where
/// `start..end` spans the whole `:name:` (colons included). Only well-formed
/// candidates are yielded; time strings and bare colons are skipped. Mirrors
/// the tokenizer in [`replace_shortcodes`].
fn scan_shortcodes(text: &str) -> impl Iterator<Item = (&str, usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    std::iter::from_fn(move || {
        while i < bytes.len() {
            if bytes[i] != b':' {
                i += 1;
                continue;
            }
            let start = i;
            let name_start = i + 1;
            let mut j = name_start;
            while j < bytes.len() && is_shortcode_byte(bytes[j]) {
                j += 1;
            }
            if j > name_start && j < bytes.len() && bytes[j] == b':' {
                let name = &text[name_start..j];
                let end = j + 1;
                // Resume at the closing colon so `:a:b:` can match `:b:` too.
                i = j;
                return Some((name, start, end));
            }
            i = j.max(i + 1);
        }
        None
    })
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

    // ============ Custom emoji splitting (global registry) ============
    // These mutate the process-wide custom-emoji registry, so they share one
    // lock and reset it when done.
    use std::sync::Mutex;
    static CUSTOM_LOCK: Mutex<()> = Mutex::new(());

    fn seg(text: &str) -> TextSegment {
        TextSegment {
            text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn custom_emoji_splits_into_tagged_segment() {
        let _g = CUSTOM_LOCK.lock().unwrap();
        crate::core::custom_emoji::set_for_test(
            crate::core::custom_emoji::registry_from_names(&[(
                "vibecat",
                crate::core::custom_emoji::EmojiFormat::Png,
            )]),
        );

        let mut segments = vec![seg("hey :VibeCat: there")];
        apply_to_segments(&mut segments);

        assert_eq!(segments.len(), 3, "split into before / emoji / after");
        assert_eq!(segments[0].text, "hey ");
        assert_eq!(segments[0].custom_emoji, None);
        assert_eq!(segments[1].text, ":VibeCat:", "literal fallback kept");
        assert_eq!(segments[1].custom_emoji.as_deref(), Some("vibecat"));
        assert_eq!(segments[2].text, " there");
        assert_eq!(segments[2].custom_emoji, None);

        crate::core::custom_emoji::set_for_test(Default::default());
    }

    #[test]
    fn custom_wins_over_gemoji_on_collision_and_gemoji_still_resolves() {
        let _g = CUSTOM_LOCK.lock().unwrap();
        // Register a custom ":grin:" so it shadows the gemoji, plus a normal
        // gemoji ":heart:" that must still resolve to Unicode in the same line.
        crate::core::custom_emoji::set_for_test(
            crate::core::custom_emoji::registry_from_names(&[(
                "grin",
                crate::core::custom_emoji::EmojiFormat::Gif,
            )]),
        );

        let mut segments = vec![seg(":grin: and :heart:")];
        apply_to_segments(&mut segments);

        // :grin: → tagged custom (kept as text), :heart: → Unicode.
        let tagged: Vec<_> = segments.iter().filter(|s| s.custom_emoji.is_some()).collect();
        assert_eq!(tagged.len(), 1);
        assert_eq!(tagged[0].custom_emoji.as_deref(), Some("grin"));
        assert_eq!(tagged[0].text, ":grin:");
        let joined: String = segments.iter().map(|s| s.text.as_str()).collect();
        assert!(joined.contains('\u{2764}'), "gemoji heart still resolves: {joined:?}");

        crate::core::custom_emoji::set_for_test(Default::default());
    }

    #[test]
    fn unknown_custom_name_is_left_as_gemoji_only() {
        let _g = CUSTOM_LOCK.lock().unwrap();
        crate::core::custom_emoji::set_for_test(
            crate::core::custom_emoji::registry_from_names(&[(
                "vibecat",
                crate::core::custom_emoji::EmojiFormat::Png,
            )]),
        );

        // :notinstalled: isn't custom and isn't gemoji → untouched, no split.
        let mut segments = vec![seg("a :notinstalled: b")];
        apply_to_segments(&mut segments);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "a :notinstalled: b");
        assert_eq!(segments[0].custom_emoji, None);

        crate::core::custom_emoji::set_for_test(Default::default());
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
            custom_emoji: None,
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
                custom_emoji: None,
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
                custom_emoji: None,
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
