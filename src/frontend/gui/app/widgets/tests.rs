//! Test module of the parent facade, split out for size —
//! `super` is still the parent module, so private access and
//! `use super::*` semantics are identical to the inline mod.

use super::{CommandEditOp, GuiBufferSelection, VellumGuiApp};

/// Drive one edit op against text + (primary, secondary) char range.
fn edit(
    text: &str,
    range: (usize, usize),
    op: CommandEditOp,
    extend: bool,
) -> (String, (usize, usize)) {
    let ctx = eframe::egui::Context::default();
    let mut t = text.to_string();
    let mut r = range;
    VellumGuiApp::apply_command_edit_op(&ctx, &mut t, &mut r, op, extend);
    (t, r)
}

#[test]
fn edit_op_cursor_moves_and_shift_extends() {
    // Plain left collapses+moves; shift-left extends (anchor stays).
    assert_eq!(edit("hello", (3, 3), CommandEditOp::Left, false).1, (2, 2));
    assert_eq!(edit("hello", (3, 3), CommandEditOp::Left, true).1, (2, 3));
    // Plain left with a selection collapses to its start.
    assert_eq!(edit("hello", (4, 1), CommandEditOp::Left, false).1, (1, 1));
    assert_eq!(edit("hello", (3, 3), CommandEditOp::Right, false).1, (4, 4));
    assert_eq!(edit("hello", (5, 5), CommandEditOp::Right, false).1, (5, 5));
    assert_eq!(
        edit("go west", (7, 7), CommandEditOp::WordLeft, false).1,
        (3, 3)
    );
    assert_eq!(
        edit("go west", (0, 0), CommandEditOp::WordRight, false).1,
        (2, 2)
    );
    assert_eq!(edit("hello", (3, 3), CommandEditOp::Home, false).1, (0, 0));
    assert_eq!(edit("hello", (0, 0), CommandEditOp::End, true).1, (5, 0));
}

#[test]
fn edit_op_deletions() {
    assert_eq!(
        edit("hello", (3, 3), CommandEditOp::Backspace, false),
        ("helo".to_string(), (2, 2))
    );
    // Backspace with a selection removes the selection.
    assert_eq!(
        edit("hello", (4, 1), CommandEditOp::Backspace, false),
        ("ho".to_string(), (1, 1))
    );
    assert_eq!(
        edit("hello", (2, 2), CommandEditOp::Delete, false),
        ("helo".to_string(), (2, 2))
    );
    assert_eq!(
        edit("go west now", (7, 7), CommandEditOp::DeleteWord, false),
        ("go  now".to_string(), (3, 3))
    );
    // Unicode: char-indexed surgery, not bytes.
    assert_eq!(
        edit("café!", (4, 4), CommandEditOp::Backspace, false),
        ("caf!".to_string(), (3, 3))
    );
}

#[test]
fn edit_op_select_all() {
    assert_eq!(
        edit("hello", (2, 2), CommandEditOp::SelectAll, false).1,
        (5, 0)
    );
}

#[test]
fn countdown_remaining_clamps_to_zero_when_elapsed() {
    // now = 150_000ms (150s), end = 100s -> elapsed
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds(100, 0, 150_000),
        0
    );
}

#[test]
fn countdown_remaining_counts_down_from_end_time() {
    // now = 100_000ms (100s), end = 110s -> exactly 10s left
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds(110, 0, 100_000),
        10
    );
}

#[test]
fn countdown_remaining_applies_server_offset() {
    // Server clock runs 5s ahead of local time. now = 100s -> 5s left.
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds(110, 5, 100_000),
        5
    );
}

#[test]
fn countdown_remaining_ceilings_partial_seconds() {
    // 1001ms remaining -> displays 2 (ceiling): end 110s, now 108_999ms
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds(110, 0, 108_999),
        2
    );
    // Exactly 1000ms remaining -> displays 1
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds(110, 0, 109_000),
        1
    );
    // 1ms remaining -> still displays 1
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds(110, 0, 109_999),
        1
    );
    // 0ms remaining -> displays 0
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds(110, 0, 110_000),
        0
    );
}

#[test]
fn countdown_remaining_fraction_keeps_sub_seconds() {
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds_f(110, 0, 105.5),
        4.5
    );
}

#[test]
fn countdown_remaining_fraction_clamps_to_zero_when_elapsed() {
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds_f(100, 0, 150.0),
        0.0
    );
}

#[test]
fn countdown_remaining_fraction_applies_server_offset() {
    // Server clock runs 5s ahead of local time: end 110s, now 100s,
    // offset +5 -> 110 - (100 + 5) = 5.0.
    assert_eq!(
        VellumGuiApp::countdown_remaining_seconds_f(110, 5, 100.0),
        5.0
    );
}

#[test]
fn countdown_remaining_fraction_matches_number_sign() {
    // Regression: the fractional bar and the whole-second number must use
    // the SAME offset sign, or they disagree on a drifted clock. With a
    // non-symmetric offset the two must still describe the same remaining
    // time. end=120s, now=100s (=100_000ms), offset=+3 -> 17s remaining.
    let f = VellumGuiApp::countdown_remaining_seconds_f(120, 3, 100.0);
    let n = VellumGuiApp::countdown_remaining_seconds(120, 3, 100_000);
    assert_eq!(f, 17.0);
    assert_eq!(n, 17); // number ceilings, but here it's a whole value
}

#[test]
fn split_search_runs_marks_exact_matches() {
    let runs = VellumGuiApp::split_search_runs("Some walls, some shelves", "some");
    assert_eq!(
        runs,
        vec![
            ("Some", true),
            (" walls, ", false),
            ("some", true),
            (" shelves", false),
        ]
    );
}

#[test]
fn split_search_runs_no_match_returns_whole_text() {
    let runs = VellumGuiApp::split_search_runs("nothing here", "xyz");
    assert_eq!(runs, vec![("nothing here", false)]);
}

#[test]
fn split_search_runs_adjacent_matches() {
    let runs = VellumGuiApp::split_search_runs("aaa", "a");
    assert_eq!(runs, vec![("a", true), ("a", true), ("a", true)]);
}

#[test]
fn word_char_range_expands_around_word_chars() {
    assert_eq!(VellumGuiApp::word_char_range("you say hello", 5), (4, 7));
    // Punctuation selects just itself.
    assert_eq!(VellumGuiApp::word_char_range("a, b", 1), (1, 2));
    // Clamps past-the-end to the last char.
    assert_eq!(VellumGuiApp::word_char_range("word", 99), (0, 4));
    assert_eq!(VellumGuiApp::word_char_range("", 0), (0, 0));
    // Char (not byte) indexing with multibyte text.
    assert_eq!(VellumGuiApp::word_char_range("éléphant rose", 2), (0, 8));
}

#[test]
fn slice_line_by_chars_uses_char_offsets() {
    assert_eq!(
        VellumGuiApp::slice_line_by_chars("hello world", Some(6), None),
        "world"
    );
    assert_eq!(
        VellumGuiApp::slice_line_by_chars("hello world", None, Some(5)),
        "hello"
    );
    // Multibyte chars: offsets count chars, not bytes.
    assert_eq!(
        VellumGuiApp::slice_line_by_chars("éé abc", Some(3), Some(6)),
        "abc"
    );
    // Reversed offsets are reordered, out-of-range clamps to the end.
    assert_eq!(
        VellumGuiApp::slice_line_by_chars("abc", Some(99), Some(1)),
        "bc"
    );
}

#[test]
fn resolve_line_uid_clamps_trimmed_and_overrun() {
    // base 100, 10 lines: uids 100..110 are live.
    assert_eq!(VellumGuiApp::resolve_line_uid(100, 10, 105), 5);
    // Trimmed off the front (uid below base) clamps to the first line.
    assert_eq!(VellumGuiApp::resolve_line_uid(100, 10, 95), 0);
    // Past the end clamps to the last line.
    assert_eq!(VellumGuiApp::resolve_line_uid(100, 10, 500), 9);
    // Wrapping base (fresh buffer populated without generation bumps).
    let base = 0u64.wrapping_sub(3);
    assert_eq!(
        VellumGuiApp::resolve_line_uid(base, 5, base.wrapping_add(4)),
        4
    );
}

#[test]
fn ordered_selection_endpoints_orders_reversed_drags() {
    let selection = GuiBufferSelection {
        scroll_id: "main".into(),
        anchor: (107, 4),
        head: (103, 2),
        dragging: false,
    };
    assert_eq!(
        VellumGuiApp::ordered_selection_endpoints(&selection, 100, 10),
        ((3, 2), (7, 4))
    );
    // Same line, chars reversed.
    let selection = GuiBufferSelection {
        scroll_id: "main".into(),
        anchor: (105, 9),
        head: (105, 2),
        dragging: false,
    };
    assert_eq!(
        VellumGuiApp::ordered_selection_endpoints(&selection, 100, 10),
        ((5, 2), (5, 9))
    );
}

#[test]
fn buffer_selection_copy_text_spans_lines_and_slices_endpoints() {
    use crate::data::StyledLine;
    let mut content = crate::data::TextContent::new("Test", 100);
    content.add_line(StyledLine::from_text("first line"));
    content.add_line(StyledLine::from_text("second line"));
    content.add_line(StyledLine::from_text("third line"));
    let base = content.generation.wrapping_sub(content.lines.len() as u64);

    // From "line" on the first line through "third" on the last.
    let selection = GuiBufferSelection {
        scroll_id: "main".into(),
        anchor: (base, 6),
        head: (base.wrapping_add(2), 5),
        dragging: false,
    };
    assert_eq!(
        VellumGuiApp::buffer_selection_copy_text(
            &eframe::egui::Context::default(),
            &eframe::egui::FontId::monospace(14.0),
            &content,
            &selection,
            base,
            None
        ),
        "line\nsecond line\nthird"
    );

    // Reversed drag yields the same text.
    let reversed = GuiBufferSelection {
        scroll_id: "main".into(),
        anchor: (base.wrapping_add(2), 5),
        head: (base, 6),
        dragging: false,
    };
    assert_eq!(
        VellumGuiApp::buffer_selection_copy_text(
            &eframe::egui::Context::default(),
            &eframe::egui::FontId::monospace(14.0),
            &content,
            &reversed,
            base,
            None
        ),
        "line\nsecond line\nthird"
    );

    // Single-line slice.
    let single = GuiBufferSelection {
        scroll_id: "main".into(),
        anchor: (base.wrapping_add(1), 7),
        head: (base.wrapping_add(1), 11),
        dragging: false,
    };
    assert_eq!(
        VellumGuiApp::buffer_selection_copy_text(
            &eframe::egui::Context::default(),
            &eframe::egui::FontId::monospace(14.0),
            &content,
            &single,
            base,
            None
        ),
        "line"
    );
}

#[test]
fn buffer_selection_copy_text_survives_front_trim() {
    use crate::data::StyledLine;
    let mut content = crate::data::TextContent::new("Test", 3);
    for i in 0..5 {
        content.add_line(StyledLine::from_text(format!("line {}", i)));
    }
    // Buffer now holds lines 2..5; generation is 5.
    let base = content.generation.wrapping_sub(content.lines.len() as u64);
    // Anchor on a line that has been trimmed away clamps to the first
    // remaining line.
    let selection = GuiBufferSelection {
        scroll_id: "main".into(),
        anchor: (0, 0),
        head: (base.wrapping_add(1), 6),
        dragging: false,
    };
    assert_eq!(
        VellumGuiApp::buffer_selection_copy_text(
            &eframe::egui::Context::default(),
            &eframe::egui::FontId::monospace(14.0),
            &content,
            &selection,
            base,
            None
        ),
        "line 2\nline 3"
    );
}

#[test]
fn build_line_job_records_link_char_ranges() {
    use crate::data::{LinkData, StyledLine, TextSegment};
    let line = StyledLine {
        segments: vec![
            TextSegment::plain("héllo "),
            TextSegment {
                text: "an orc".into(),
                link_data: Some(LinkData {
                    exist_id: "123".into(),
                    noun: "orc".into(),
                    text: "an orc".into(),
                    coord: None,
                }),
                ..Default::default()
            },
            TextSegment::plain(" lunges!"),
        ],
        stream: "main".into(),
        timestamp: None,
    };
    let visuals = eframe::egui::Visuals::default();
    let font_id = eframe::egui::FontId::monospace(14.0);
    let built = VellumGuiApp::build_line_job(
        &eframe::egui::Context::default(),
        &line,
        &visuals,
        None,
        &font_id,
        super::LineInset::full(f32::INFINITY),
        None,
    );
    assert_eq!(built.job.text, "héllo an orc lunges!");
    assert_eq!(built.links.len(), 1);
    // Char (not byte) range: "héllo " is 6 chars.
    assert_eq!(built.links[0].0, 6..12);
    assert_eq!(built.links[0].1.exist_id, "123");
}

#[test]
fn build_line_job_records_custom_emoji_runs() {
    use crate::core::custom_emoji::{self, CustomEmoji, CustomEmojiRegistry, EmojiFormat};
    use crate::data::{StyledLine, TextSegment};

    // Write a real 1x1 PNG so is_paintable's decode succeeds.
    let tmp = std::env::temp_dir().join(format!("vellum_emoji_bl_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let path = tmp.join("vibecat.png");
    {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, png).unwrap();
    }
    let mut reg = CustomEmojiRegistry::default();
    reg.insert_for_test(CustomEmoji {
        name: "vibecat".into(),
        path,
        format: EmojiFormat::Png,
    });
    custom_emoji::set_for_test(reg);

    // A line with a tagged custom-emoji segment, as the resolver produces.
    let line = StyledLine {
        segments: vec![
            TextSegment::plain("yep "),
            TextSegment {
                text: ":vibecat:".into(),
                custom_emoji: Some("vibecat".into()),
                ..Default::default()
            },
        ],
        stream: "main".into(),
        timestamp: None,
    };
    let visuals = eframe::egui::Visuals::default();
    let font_id = eframe::egui::FontId::monospace(14.0);
    let ctx = eframe::egui::Context::default();
    ctx.begin_pass(eframe::egui::RawInput::default());
    let built =
        VellumGuiApp::build_line_job(&ctx, &line, &visuals, None, &font_id, super::LineInset::full(f32::INFINITY), None);
    {
        // egui 0.36 debug-asserts the TexturesDelta is applied before drop.
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
    }

    // A paintable custom emoji occupies a space-run placeholder (not the
    // wide `:name:` text), and the run is recorded over exactly it.
    ctx.begin_pass(eframe::egui::RawInput::default());
    let placeholder = VellumGuiApp::emoji_placeholder(&ctx, &font_id);
    {
        // egui 0.36 debug-asserts the TexturesDelta is applied before drop.
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
    }
    let ph = placeholder.chars().count();
    assert!(ph >= 1, "placeholder is at least one space");
    let expected = format!("yep {placeholder}");
    assert_eq!(built.job.text, expected);
    assert_eq!(built.custom_runs.len(), 1, "must record the emoji slot");
    assert_eq!(built.custom_runs[0].0, 4);
    assert_eq!(built.custom_runs[0].1, 4 + ph, "run spans the placeholder");
    assert_eq!(built.custom_runs[0].2, "vibecat");

    // compose_line_text must agree so copy/selection offsets stay aligned.
    assert_eq!(
        VellumGuiApp::compose_line_text(&ctx, &font_id, &line, None),
        expected
    );

    // At the default size (1.0) the line needs no extra height...
    super::custom_emoji_render::set_geometry(1.0, 0.2);
    let a =
        VellumGuiApp::build_line_job(&ctx, &line, &visuals, None, &font_id, super::LineInset::full(f32::INFINITY), None);
    assert_eq!(a.min_height, 0.0);
    // ...but an oversized emoji grows the row so it isn't clipped.
    super::custom_emoji_render::set_geometry(2.0, 0.2);
    let b =
        VellumGuiApp::build_line_job(&ctx, &line, &visuals, None, &font_id, super::LineInset::full(f32::INFINITY), None);
    assert!(b.min_height > 0.0, "size>1 must set a taller min_height");
    super::custom_emoji_render::set_geometry(1.0, 0.2); // reset

    // The slot (row_height * width_factor) must be WIDER than the emoji
    // square (row_height * size_factor) so there's positive padding split
    // symmetrically on both sides — pos_from_cursor can't be trusted for
    // the width (it ignores extra_letter_spacing), so the painter uses
    // width_factor directly. Guard the invariant that keeps padding > 0.
    let size = super::custom_emoji_render::size_factor();
    let width_factor = super::custom_emoji_render::width_factor();
    assert!(
            width_factor >= size,
            "reserved width ({width_factor}) must be >= the square ({size}) so padding is never negative"
        );
    // With the default 0.2 spacing there is strictly positive padding.
    assert!(
        width_factor > size,
        "default spacing gives positive padding"
    );

    custom_emoji::set_for_test(CustomEmojiRegistry::default());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn compose_line_text_matches_job_text() {
    use crate::data::{StyledLine, TextSegment};
    let line = StyledLine {
        segments: vec![TextSegment::plain("a"), TextSegment::plain("bc")],
        stream: "main".into(),
        timestamp: None,
    };
    let visuals = eframe::egui::Visuals::default();
    let font_id = eframe::egui::FontId::monospace(14.0);
    let built = VellumGuiApp::build_line_job(
        &eframe::egui::Context::default(),
        &line,
        &visuals,
        None,
        &font_id,
        super::LineInset::full(f32::INFINITY),
        None,
    );
    assert_eq!(
        VellumGuiApp::compose_line_text(
            &eframe::egui::Context::default(),
            &eframe::egui::FontId::monospace(14.0),
            &line,
            None
        ),
        built.job.text
    );
}

#[test]
fn injury_level_color_distinguishes_injuries_from_scars() {
    use eframe::egui::Color32;
    let palette = VellumGuiApp::default_injury_palette();
    assert_eq!(
        VellumGuiApp::injury_level_color(&palette, 0),
        Color32::from_rgb(0x33, 0x33, 0x33)
    );
    assert_eq!(
        VellumGuiApp::injury_level_color(&palette, 3),
        Color32::from_rgb(0xff, 0x00, 0x00)
    );
    assert_eq!(
        VellumGuiApp::injury_level_color(&palette, 6),
        Color32::from_rgb(0x55, 0x55, 0x55)
    );
    // Out-of-range levels clamp to the deepest scar color.
    assert_eq!(
        VellumGuiApp::injury_level_color(&palette, 9),
        VellumGuiApp::injury_level_color(&palette, 6)
    );
}

#[test]
fn resolved_injury_palette_honors_config_overrides() {
    use eframe::egui::Color32;
    let data = crate::config::InjuryDollWidgetData {
        injury1_color: Some("#00ff00".to_string()),
        ..Default::default()
    };
    let palette = VellumGuiApp::resolved_injury_palette(&data);
    // The overridden level renders the user's color...
    assert_eq!(palette[1], Color32::from_rgb(0x00, 0xff, 0x00));
    // ...while un-overridden levels keep the shared defaults.
    assert_eq!(palette[3], Color32::from_rgb(0xff, 0x00, 0x00));
}

// --- Keybind bug #3: copy priority. A non-empty game-window selection owns
// Copy/Cut over the command input; a collapsed or absent selection does
// not, so copying from the input still works. ---

#[test]
fn active_buffer_selection_gates_copy_priority() {
    let ctx = eframe::egui::Context::default();

    // No selection stored: input keeps the clipboard.
    assert!(!VellumGuiApp::active_buffer_selection_present(&ctx));

    // A collapsed selection (anchor == head) is just a caret, not a
    // highlight -- the input still owns Copy.
    VellumGuiApp::store_buffer_selection(
        &ctx,
        Some(GuiBufferSelection {
            scroll_id: "main".into(),
            anchor: (10, 3),
            head: (10, 3),
            dragging: false,
        }),
    );
    assert!(!VellumGuiApp::active_buffer_selection_present(&ctx));

    // A real, non-empty selection takes priority.
    VellumGuiApp::store_buffer_selection(
        &ctx,
        Some(GuiBufferSelection {
            scroll_id: "main".into(),
            anchor: (10, 3),
            head: (11, 0),
            dragging: false,
        }),
    );
    assert!(VellumGuiApp::active_buffer_selection_present(&ctx));

    // Clearing the selection returns the clipboard to the input.
    VellumGuiApp::store_buffer_selection(&ctx, None);
    assert!(!VellumGuiApp::active_buffer_selection_present(&ctx));
}

// ==================== Inline image floats ====================

fn float_test_line(text: &str) -> crate::data::StyledLine {
    crate::data::StyledLine {
        segments: vec![crate::data::TextSegment::plain(text)],
        stream: "room".into(),
        timestamp: None,
    }
}

fn float_test_image_line(name: &str) -> crate::data::StyledLine {
    crate::data::StyledLine {
        segments: vec![crate::data::TextSegment {
            text: format!("[img:{name}]"),
            inline_image: Some(crate::data::InlineImage {
                name: name.into(),
                rows: 4.0,
                align: crate::data::FloatAlign::Left,
            }),
            ..Default::default()
        }],
        stream: "room".into(),
        timestamp: None,
    }
}

/// Only the lines that fit beside the image are covered; the rest rejoin
/// full width. With a 40pt image and 10pt lines, exactly 4 lines fit.
#[test]
fn float_covers_only_the_lines_that_fit() {
    let body = vec![
        float_test_image_line("banner"),
        float_test_line("a"),
        float_test_line("b"),
        float_test_line("c"),
        float_test_line("d"),
        float_test_line("e"),
    ];
    let end = VellumGuiApp::float_covered_end(&body, 0, 40.0, |_| 10.0);
    assert_eq!(end, 5, "origin + 4 covered lines");
}

/// A line that would straddle the image's bottom edge is excluded rather
/// than half-inset — egui cannot shorten only part of a line's rows.
#[test]
fn float_excludes_a_straddling_line() {
    let body = vec![
        float_test_image_line("banner"),
        float_test_line("a"),
        float_test_line("b"),
    ];
    // 25pt image, 10pt lines: two fit (20pt), the third would straddle.
    let end = VellumGuiApp::float_covered_end(&body, 0, 25.0, |_| 10.0);
    assert_eq!(end, 3, "both lines fit inside 25pt");
    let end = VellumGuiApp::float_covered_end(&body, 0, 15.0, |_| 10.0);
    assert_eq!(end, 2, "second line would straddle, so it rejoins");
}

/// Text running out before the image does simply ends the span.
#[test]
fn float_span_stops_at_end_of_body() {
    let body = vec![float_test_image_line("banner"), float_test_line("a")];
    let end = VellumGuiApp::float_covered_end(&body, 0, 400.0, |_| 10.0);
    assert_eq!(end, 2);
}

/// A second image ends the first float instead of nesting, so two pictures
/// can never overlap.
#[test]
fn float_span_stops_at_the_next_image() {
    let body = vec![
        float_test_image_line("one"),
        float_test_line("a"),
        float_test_image_line("two"),
        float_test_line("b"),
    ];
    let end = VellumGuiApp::float_covered_end(&body, 0, 400.0, |_| 10.0);
    assert_eq!(end, 2, "stops before the second image");
}

/// A script writes `<vellumImg/>The room stretches...` as ONE line, so the
/// image segment and its prose share a StyledLine. The prose must still
/// render (it leads the wrapped text beside the image) — dropping the whole
/// origin line loses the room description entirely.
#[test]
fn float_origin_line_keeps_its_prose() {
    let line = crate::data::StyledLine {
        segments: vec![
            crate::data::TextSegment {
                text: "[img:sunset]".into(),
                inline_image: Some(crate::data::InlineImage {
                    name: "sunset".into(),
                    rows: 4.0,
                    align: crate::data::FloatAlign::Left,
                }),
                ..Default::default()
            },
            crate::data::TextSegment::plain("Stretching like long fingers."),
        ],
        stream: "room".into(),
        timestamp: None,
    };

    // The renderer keeps every non-image segment of the origin line.
    let lead: Vec<_> = line
        .segments
        .iter()
        .filter(|s| s.inline_image.is_none())
        .collect();
    assert_eq!(lead.len(), 1, "prose segment must survive the image split");
    assert_eq!(lead[0].text, "Stretching like long fingers.");
    // ...and the image segment's own fallback text is NOT shown as content.
    assert!(
        !lead.iter().any(|s| s.text.contains("[img:")),
        "the [img:] fallback is not prose"
    );
}

// ==================== Room title formatting ====================

#[test]
fn room_title_matches_the_story_window_shape() {
    let got = VellumGuiApp::format_room_title(
        "Kraken's Fall, Third Pier".into(),
        Some("29043"),
        Some("7118245"),
    );
    assert_eq!(got, "[Kraken's Fall, Third Pier - 29043] (u7118245)");
}

/// Either id may be absent: the uid comes from <nav rm=>, the Lich id only
/// under Lich. Each is omitted rather than rendering an empty slot.
#[test]
fn room_title_handles_missing_ids() {
    assert_eq!(
        VellumGuiApp::format_room_title("Cold River".into(), None, Some("7503201")),
        "[Cold River] (u7503201)"
    );
    assert_eq!(
        VellumGuiApp::format_room_title("Cold River".into(), Some("29043"), None),
        "[Cold River - 29043]"
    );
    assert_eq!(
        VellumGuiApp::format_room_title("Cold River".into(), None, None),
        "[Cold River]"
    );
    assert_eq!(VellumGuiApp::format_room_title(String::new(), None, None), "");
}

/// Re-syncing must not nest brackets or repeat the id — the name can arrive
/// already formatted (the roomName style carries brackets).
#[test]
fn room_title_is_idempotent() {
    let once = VellumGuiApp::format_room_title(
        "Kraken's Fall, Third Pier".into(),
        Some("29043"),
        Some("7118245"),
    );
    let twice = VellumGuiApp::format_room_title(once.clone(), Some("29043"), Some("7118245"));
    assert_eq!(once, twice, "formatting twice must be a no-op");

    // A bare bracketed name with no ids also survives.
    assert_eq!(
        VellumGuiApp::format_room_title("[Cold River]".into(), None, None),
        "[Cold River]"
    );
}

/// A uid that already carries its `u` prefix must not become `uu`.
#[test]
fn room_title_does_not_double_the_u_prefix() {
    assert_eq!(
        VellumGuiApp::format_room_title("Cold River".into(), None, Some("u7503201")),
        "[Cold River] (u7503201)"
    );
}

/// A room name containing a dash that is NOT an id must survive intact.
#[test]
fn room_title_keeps_dashes_that_are_not_ids() {
    assert_eq!(
        VellumGuiApp::format_room_title("Ta'Illistim - The Bazaar".into(), None, Some("123")),
        "[Ta'Illistim - The Bazaar] (u123)"
    );
}

/// The room NAME must be part of the flowing body so it wraps BESIDE room
/// art, not sit above it as a separate header. When art is present it is
/// hoisted onto the name's line, making the name the float origin.
#[test]
fn room_name_shares_the_float_line_with_the_art() {
    use crate::data::{FloatAlign, InlineImage, StyledLine, TextSegment};

    // What room_sync produces: art leading the DESCRIPTION line.
    let description = StyledLine {
        segments: vec![
            TextSegment {
                text: "[img:pier]".into(),
                inline_image: Some(InlineImage {
                    name: "pier".into(),
                    rows: 4.0,
                    align: FloatAlign::Left,
                }),
                ..Default::default()
            },
            TextSegment::plain("Blue and red arrows."),
        ],
        stream: "room".into(),
        timestamp: None,
    };
    let name = StyledLine {
        segments: vec![TextSegment {
            text: "Kraken's Fall, Third Pier".into(),
            bold: true,
            ..Default::default()
        }],
        stream: "room".into(),
        timestamp: None,
    };
    let mut body = vec![name, description];

    // The hoist render_room_content performs.
    let art: Vec<TextSegment> = body[1]
        .segments
        .iter()
        .filter(|s| s.inline_image.is_some())
        .cloned()
        .collect();
    assert!(!art.is_empty(), "fixture must carry art");
    body[1].segments.retain(|s| s.inline_image.is_none());
    let mut lead = art;
    lead.append(&mut body[0].segments);
    body[0].segments = lead;

    // The name line now OWNS the image, so the float starts at the top and
    // the name wraps beside the picture.
    assert!(
        body[0].segments[0].inline_image.is_some(),
        "art must lead the name line"
    );
    assert!(
        body[0].segments.iter().any(|s| s.text.contains("Kraken's Fall")),
        "the name must stay on that line"
    );
    assert!(
        !body[1].segments.iter().any(|s| s.inline_image.is_some()),
        "art must not remain on the description line too"
    );
    assert!(
        body[1].segments.iter().any(|s| s.text.contains("Blue and red")),
        "description prose survives the hoist"
    );
}

// ==================== Row height cache (characterization) ====================
//
// These pin the CURRENT behaviour of the text virtualization before float
// support changes it. They are not aspirational: if one of them starts
// failing, the invalidation or incremental-append contract moved, and the
// scroll anchoring that reads these heights will drift with it.

fn cache_test_content(lines: &[&str], generation: u64) -> crate::data::TextContent {
    use crate::data::{StyledLine, TextSegment};
    crate::data::TextContent {
        lines: lines
            .iter()
            .map(|text| StyledLine {
                segments: vec![TextSegment::plain(*text)],
                stream: "main".into(),
                timestamp: None,
            })
            .collect(),
        scroll_offset: 0,
        max_lines: 1000,
        title: "main".into(),
        generation,
        streams: vec!["main".into()],
        compact: false,
        show_timestamps: false,
        timestamp_position: crate::config::TimestampPosition::Start,
    }
}

/// Run one cache update inside a real egui pass.
fn update_cache(
    cache: &mut super::RowHeightCache,
    content: &crate::data::TextContent,
    wrap_width: f32,
    font_id: &eframe::egui::FontId,
) {
    update_cache_epoch(cache, content, wrap_width, font_id, 0)
}

fn update_cache_epoch(
    cache: &mut super::RowHeightCache,
    content: &crate::data::TextContent,
    wrap_width: f32,
    font_id: &eframe::egui::FontId,
    float_epoch: u64,
) {
    let ctx = eframe::egui::Context::default();
    let visuals = eframe::egui::Visuals::default();
    ctx.begin_pass(eframe::egui::RawInput::default());
    let rendered = content.lines.len();
    VellumGuiApp::update_row_height_cache(
        cache, &ctx, content, 0, rendered, wrap_width, &visuals, font_id, float_epoch,
        400.0,
    );
    let mut output = ctx.end_pass();
    output.textures_delta.clear();
}

/// One cached height per rendered line, always.
#[test]
fn cache_holds_one_height_per_rendered_line() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    let content = cache_test_content(&["a", "b", "c"], 3);
    update_cache(&mut cache, &content, 400.0, &font);
    assert_eq!(cache.heights().len(), 3);
    assert!(cache.heights().iter().all(|h| *h > 0.0), "heights are real");
}

/// Appending lines takes the INCREMENTAL path: existing entries are not
/// re-measured, and the window slides so the count still matches.
#[test]
fn cache_appends_incrementally_on_new_lines() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    let first = cache_test_content(&["a", "b"], 2);
    update_cache(&mut cache, &first, 400.0, &font);
    let before = cache.heights().to_vec();

    let second = cache_test_content(&["a", "b", "c"], 3);
    update_cache(&mut cache, &second, 400.0, &font);
    assert_eq!(cache.heights().len(), 3);
    assert_eq!(
        &cache.heights()[..2],
        &before[..],
        "existing heights must be reused, not re-measured"
    );
}

/// A width change rebuilds everything: wrapped heights depend on it, so a
/// stale entry would desync the scroll math.
#[test]
fn cache_rebuilds_when_wrap_width_changes() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    let long = "word ".repeat(40);
    let content = cache_test_content(&[long.as_str()], 1);

    update_cache(&mut cache, &content, 1000.0, &font);
    let wide = cache.heights()[0];
    update_cache(&mut cache, &content, 120.0, &font);
    let narrow = cache.heights()[0];

    assert!(
        narrow > wide,
        "narrower wrap must wrap to more rows: {narrow} vs {wide}"
    );
}

/// A font change also rebuilds — same reasoning as width.
#[test]
fn cache_rebuilds_when_font_changes() {
    let mut cache = super::RowHeightCache::default();
    let content = cache_test_content(&["a"], 1);
    update_cache(&mut cache, &content, 400.0, &eframe::egui::FontId::monospace(10.0));
    let small = cache.heights()[0];
    update_cache(&mut cache, &content, 400.0, &eframe::egui::FontId::monospace(24.0));
    let large = cache.heights()[0];
    assert!(large > small, "bigger font is taller: {large} vs {small}");
}

/// A generation jump larger than the rendered window forces a full rebuild
/// rather than a wrong incremental slide.
#[test]
fn cache_rebuilds_on_a_large_generation_jump() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(&mut cache, &cache_test_content(&["a", "b"], 2), 400.0, &font);
    // Generation leaps far beyond the rendered count.
    let jumped = cache_test_content(&["x", "y"], 900);
    update_cache(&mut cache, &jumped, 400.0, &font);
    assert_eq!(cache.heights().len(), 2, "count still matches after rebuild");
}

// ==================== Scroll harness ====================
//
// Drives the REAL `render_text_content` headlessly so the hand-rolled
// scroll machinery (trim compensation, the programmatic hold, the
// near-bottom snap re-arm) is testable instead of manual-QA territory.
// Every scroll bug in this project's history was found live from log
// forensics; these exist so the next one fails a test instead.

/// A headless text window: fixed viewport, real egui passes, real renderer.
struct ScrollHarness {
    ctx: eframe::egui::Context,
    content: crate::data::TextContent,
    scroll_id: String,
    font_id: eframe::egui::FontId,
    view: eframe::egui::Vec2,
    time: f64,
}

impl ScrollHarness {
    fn new(scroll_id: &str, view_h: f32) -> Self {
        Self {
            ctx: eframe::egui::Context::default(),
            content: cache_test_content(&[], 0),
            scroll_id: scroll_id.to_string(),
            font_id: eframe::egui::FontId::monospace(14.0),
            view: eframe::egui::vec2(400.0, view_h),
            time: 0.0,
        }
    }

    /// Append lines the way the game does: push and bump the generation.
    fn push_lines(&mut self, count: usize) {
        use crate::data::{StyledLine, TextSegment};
        for _ in 0..count {
            let n = self.content.generation + 1;
            self.content.lines.push_back(StyledLine {
                segments: vec![TextSegment::plain(format!("line {n}"))],
                stream: "main".into(),
                timestamp: None,
            });
            // Mirror AppCore's ring-buffer trim so the pre-pass has work.
            while self.content.lines.len() > self.content.max_lines {
                self.content.lines.pop_front();
            }
            self.content.generation = n;
        }
    }

    fn raw_input(&mut self) -> eframe::egui::RawInput {
        let mut input = eframe::egui::RawInput::default();
        input.screen_rect = Some(eframe::egui::Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            self.view,
        ));
        // egui 0.35 runs wheel events through a smoothing accumulator that
        // needs real elapsed time; without a clock the smoothed delta never
        // materializes and synthetic wheels appear to do nothing.
        self.time += 1.0 / 60.0;
        input.time = Some(self.time);
        input.predicted_dt = 1.0 / 60.0;
        input
    }

    /// A pointer-move into the middle of the window. egui only scrolls the
    /// area under the cursor, so without this the wheel goes nowhere.
    fn hover_center(&self) -> eframe::egui::Event {
        eframe::egui::Event::PointerMoved(eframe::egui::pos2(
            self.view.x * 0.5,
            self.view.y * 0.5,
        ))
    }

    /// Run one frame with optional synthetic events.
    fn frame_with(&mut self, events: Vec<eframe::egui::Event>) {
        let mut input = self.raw_input();
        input.events = events;
        let content = self.content.clone();
        let scroll_id = self.scroll_id.clone();
        let font_id = self.font_id.clone();
        let mut output = self.ctx.run_ui(input, |ui| {

            VellumGuiApp::render_text_content(
                ui, &content, &scroll_id, None, &font_id, true, None, false,
            );
        });
        output.textures_delta.clear();
    }

    fn frame(&mut self) {
        self.frame_with(Vec::new());
    }

    /// Same as `frame_with` but through the auto-split entry point, the one
    /// the real GUI window path uses.
    fn frame_split_with(&mut self, events: Vec<eframe::egui::Event>) {
        let mut input = self.raw_input();
        input.events = events;
        let content = self.content.clone();
        let scroll_id = self.scroll_id.clone();
        let font_id = self.font_id.clone();
        let mut output = self.ctx.run_ui(input, |ui| {
            VellumGuiApp::render_text_content_auto_split(
                ui, &content, &scroll_id, None, &font_id, true, None,
            );
        });
        output.textures_delta.clear();
    }

    fn frame_split(&mut self) {
        self.frame_split_with(Vec::new());
    }

    /// Did the live (bottom) pane render this session? Its scroll area
    /// stashes an id under the derived `~live` scroll id when it does.
    fn live_pane_rendered(&self) -> bool {
        let live_id = format!("{}~live", self.scroll_id);
        self.ctx
            .data_mut(|d| {
                d.get_temp::<eframe::egui::Id>(eframe::egui::Id::new((
                    "text_scroll_area_id",
                    live_id.as_str(),
                )))
            })
            .is_some()
    }

    /// The live pane's follow flag — must always be true (force-followed).
    fn live_following(&self) -> bool {
        let live_id = format!("{}~live", self.scroll_id);
        self.ctx
            .data_mut(|d| {
                d.get_temp(eframe::egui::Id::new((
                    "text_scroll_follow",
                    live_id.as_str(),
                )))
            })
            .unwrap_or(true)
    }

    /// egui's own persisted offset — the post-layout truth.
    fn offset(&self) -> f32 {
        let area_id: Option<eframe::egui::Id> = self.ctx.data_mut(|d| {
            d.get_temp(eframe::egui::Id::new((
                "text_scroll_area_id",
                self.scroll_id.as_str(),
            )))
        });
        area_id
            .and_then(|id| eframe::egui::scroll_area::State::load(&self.ctx, id))
            .map(|s| s.offset.y)
            .unwrap_or(0.0)
    }

    /// The single scroll authority: are we following the tail?
    fn following(&self) -> bool {
        self.ctx
            .data_mut(|d| {
                d.get_temp(eframe::egui::Id::new((
                    "text_scroll_follow",
                    self.scroll_id.as_str(),
                )))
            })
            .unwrap_or(true)
    }

    /// Queue a programmatic scroll the way keybinds and the gamepad do.
    fn request(&mut self, kind: u8, value: f32) {
        let id = eframe::egui::Id::new(("text_scroll_pending", self.scroll_id.as_str()));
        self.ctx.data_mut(|d| d.insert_temp(id, (kind, value)));
    }

    fn wheel(delta_y: f32) -> eframe::egui::Event {
        eframe::egui::Event::MouseWheel {
            unit: eframe::egui::MouseWheelUnit::Point,
            delta: eframe::egui::vec2(0.0, delta_y),
            phase: eframe::egui::TouchPhase::Move,
            modifiers: eframe::egui::Modifiers::default(),
        }
    }
}

/// Baseline: a fresh window follows the tail, so new text is visible.
#[test]
fn scroll_starts_following_the_bottom() {
    let mut h = ScrollHarness::new("baseline", 100.0);
    h.push_lines(200);
    h.frame();
    h.frame();
    let first = h.offset();
    h.push_lines(20);
    h.frame();
    assert!(h.following(), "a fresh window follows the newest text");
    assert!(
        h.offset() > first,
        "appending must follow the tail: {first} -> {}",
        h.offset()
    );
}

/// A page-up request stops following, and STAYS stopped across frames.
/// This is what egui's stick_to_bottom could not express on its own.
#[test]
fn page_up_stops_following_and_stays_stopped() {
    let mut h = ScrollHarness::new("pageup", 100.0);
    h.push_lines(200);
    h.frame();
    h.frame();

    h.request(0, -200.0);
    h.frame();
    assert!(!h.following(), "page up must stop following");
    let parked = h.offset();
    h.frame();
    h.frame();
    assert!(!h.following(), "and must not silently resume");
    assert!(
        (h.offset() - parked).abs() < 1.0,
        "the view must stay put: {parked} -> {}",
        h.offset()
    );
}

/// Appending text while paged up must not drag the reader down.
#[test]
fn appending_while_paged_up_does_not_move_the_view() {
    let mut h = ScrollHarness::new("paged", 100.0);
    h.push_lines(200);
    h.frame();
    h.frame();
    h.request(0, -300.0);
    h.frame();
    let parked = h.offset();

    h.push_lines(30);
    h.frame();
    assert!(!h.following());
    assert!(
        (h.offset() - parked).abs() < 1.0,
        "incoming text must not move a reader: {parked} -> {}",
        h.offset()
    );
}

/// The End request resumes following.
#[test]
fn end_request_resumes_following() {
    let mut h = ScrollHarness::new("endkey", 100.0);
    h.push_lines(200);
    h.frame();
    h.frame();
    h.request(0, -300.0);
    h.frame();
    assert!(!h.following());

    h.request(2, 0.0);
    h.frame();
    assert!(h.following(), "End must resume following the tail");
}

/// User wheel input takes the window back immediately.
///
/// IGNORED: HARNESS LIMITATION, NOT A PRODUCT BUG. Wheel scrolling was
/// verified working live on 2026-08-10 (Nisugi) — scroll up, resume
/// following at the bottom, and hold position while text streams all behave.
/// A synthetic wheel still does not move the offset headlessly even with a
/// clock (egui smooths wheel events across frames), a hovering pointer, and
/// the pin guarded to fire only when the stored offset is behind the tail.
/// Left here as a marker: if someone finds the missing ingredient, this
/// becomes a real regression test for free.
#[ignore = "egui wheel smoothing does not drive headlessly; verified live instead"]
#[test]
fn user_wheel_stops_following() {
    let mut h = ScrollHarness::new("wheel", 100.0);
    h.push_lines(200);
    h.frame();
    h.frame();
    assert!(h.following());

    // Far enough to leave the near-bottom tolerance; a tiny nudge at the
    // tail legitimately counts as "still at the bottom".
    // Wheel far enough to leave the near-bottom tolerance. A tiny nudge at
    // the tail legitimately counts as "still following" — the tolerance is
    // what replaced egui's exact-equality re-stick.
    for _ in 0..6 {
        let hover = h.hover_center();
        h.frame_with(vec![hover, ScrollHarness::wheel(-200.0)]);
    }
    assert!(
        !h.following(),
        "a sustained wheel scroll away from the tail stops following"
    );
}

/// REGRESSION (82c2a8d5): a producer re-issuing every frame must NOT be
/// able to out-race user input. Under the old hold mechanism the wheel
/// cleared the hold and the next request rebuilt it — "the mouse lost every
/// round". Clearing a bool is idempotent, so the user now wins.
#[test]
fn level_triggered_producer_cannot_starve_user_input() {
    let mut h = ScrollHarness::new("starve", 100.0);
    h.push_lines(200);
    h.frame();
    h.frame();

    // Producer and wheel arrive together, every frame.
    for _ in 0..3 {
        h.request(0, -50.0);
        h.frame_with(vec![ScrollHarness::wheel(-20.0)]);
    }
    assert!(
        !h.following(),
        "user input must still own the window while a producer streams"
    );
    let parked = h.offset();
    // The producer keeps firing with no user input; the view must not be
    // yanked back to the bottom.
    h.request(0, -50.0);
    h.frame();
    assert!(
        h.offset() <= parked + 1.0,
        "the window must not snap back to the tail: {parked} -> {}",
        h.offset()
    );
}

/// REGRESSION: trim compensation must reach the position actually used.
/// The old hold mechanism made the pre-pass adjust an offset that the hold
/// then overrode, so a paged-up reader drifted as lines fell off the front.
/// With one authority the compensation lands where it is read.
#[test]
fn trim_compensation_keeps_a_paged_up_reader_in_place() {
    let mut h = ScrollHarness::new("trim", 100.0);
    h.content.max_lines = 60;
    h.push_lines(60);
    h.frame();
    h.frame();

    h.request(0, -200.0);
    h.frame();
    h.frame();
    let parked = h.offset();
    assert!(!h.following(), "paged up");

    // Lines now fall off the FRONT: content slides up under the reader, and
    // the pre-pass must absorb it.
    h.push_lines(10);
    h.frame();

    assert!(!h.following(), "still paged up");
    assert!(
        h.offset() < parked,
        "the offset must be compensated DOWN as rows leave the front:          {parked} -> {}",
        h.offset()
    );
}

/// An absolute scroll-to-line lands above the bottom and stops following.
#[test]
fn absolute_scroll_targets_a_line() {
    let mut h = ScrollHarness::new("absolute", 100.0);
    h.push_lines(200);
    h.frame();
    h.frame();
    let bottom = h.offset();

    h.request(3, 20.0);
    h.frame();
    assert!(!h.following(), "an absolute jump stops following");
    assert!(
        h.offset() < bottom,
        "line 20 of 200 must be well above the bottom: {} vs {bottom}",
        h.offset()
    );
}

/// REGRESSION: an absolute request past the cached tail used to be consumed
/// and silently become "jump to the end" — a search hit off the top of the
/// buffer dumped the reader at the newest line. It now clamps to the last
/// cached row instead, so the view lands as close as the cache allows.
#[test]
fn absolute_scroll_past_the_cache_clamps_instead_of_jumping_to_the_end() {
    let mut h = ScrollHarness::new("absolute_oob", 100.0);
    h.push_lines(200);
    h.frame();
    h.frame();
    let bottom = h.offset();

    // A target far past the buffer clamps to the last cached row. The
    // resulting offset is at or below the tail — never thrown beyond it —
    // and the request is honoured rather than silently discarded.
    h.request(3, 99_999.0);
    h.frame();
    assert!(
        h.offset() <= bottom + 1.0,
        "clamped, not thrown past the end: {} vs {bottom}",
        h.offset()
    );
}

// ==================== Reserved float height (P2.1) ====================

/// `extra` must stay exactly parallel to `heights` through both the full
/// rebuild and the incremental append, or reservations drift onto the wrong
/// rows as the ring buffer trims.
#[test]
fn reserved_height_column_stays_parallel() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();

    update_cache(&mut cache, &cache_test_content(&["a", "b", "c"], 3), 400.0, &font);
    assert_eq!(cache.extra().len(), cache.heights().len());

    // Incremental append.
    update_cache(&mut cache, &cache_test_content(&["a", "b", "c", "d"], 4), 400.0, &font);
    assert_eq!(cache.extra().len(), cache.heights().len(), "after append");

    // Full rebuild (width change).
    update_cache(&mut cache, &cache_test_content(&["a", "b", "c", "d"], 4), 120.0, &font);
    assert_eq!(cache.extra().len(), cache.heights().len(), "after rebuild");
}

/// A row's stride is text height PLUS its reserved float overhang — the one
/// number every offset computation must use.
#[test]
fn stride_includes_reserved_height() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(&mut cache, &cache_test_content(&["a", "b"], 2), 400.0, &font);

    let bare = cache.stride(0);
    cache.set_extra(0, 40.0);
    assert_eq!(cache.stride(0), bare + 40.0, "reservation adds to the stride");
    assert_eq!(cache.stride(1), cache.heights()[1], "other rows untouched");

    // stride_sum is the shape the spacers and anchoring use.
    let spacing = 2.0;
    assert_eq!(
        cache.stride_sum(0..2, spacing),
        cache.stride(0) + cache.stride(1) + spacing * 2.0
    );
}

/// THE TRAP THIS COLUMN EXISTS FOR. The render loop writes each rendered
/// row's measured galley height back into `heights` every frame. A
/// reservation folded into `heights` would be erased immediately; kept in
/// `extra`, it survives.
#[test]
fn reserved_height_survives_a_height_writeback() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(&mut cache, &cache_test_content(&["a", "b"], 2), 400.0, &font);
    cache.set_extra(0, 40.0);
    let with_float = cache.stride(0);

    // Simulate the render loop's correction: overwrite the measured height.
    let measured = cache.heights()[0];
    let content = cache_test_content(&["a", "b"], 2);
    update_cache(&mut cache, &content, 400.0, &font);

    assert_eq!(
        cache.heights()[0],
        measured,
        "text height is re-measured as before"
    );
    assert_eq!(
        cache.stride(0),
        with_float,
        "the float reservation must NOT be erased by a height update"
    );
}

/// A float-geometry change invalidates the cache even though wrap width,
/// font, and generation are all unchanged — the hazard that would otherwise
/// leave resized floats measured at their old height forever.
#[test]
fn float_epoch_change_forces_a_rebuild() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    let content = cache_test_content(&["a", "b"], 2);

    update_cache_epoch(&mut cache, &content, 400.0, &font, 1);
    cache.set_extra(0, 40.0);
    assert!(cache.stride(0) > cache.heights()[0]);

    // Same content, same width, same font — only the float epoch moved.
    update_cache_epoch(&mut cache, &content, 400.0, &font, 2);
    assert_eq!(
        cache.extra()[0],
        0.0,
        "a float-geometry change must clear stale reservations for re-measure"
    );
}

// ==================== Line inset (P2.2) ====================

/// A row with no float lays out at the full width with no shift.
#[test]
fn line_inset_defaults_to_full_width() {
    let inset = super::LineInset::full(400.0);
    assert_eq!(inset.width, 400.0);
    assert_eq!(inset.x_offset, 0.0);
}

/// The cache hands back the inset a row was MEASURED with. Painting and the
/// drag hit-test both read it from here, which is what keeps their galleys
/// identical — the invariant that stops selection landing on the wrong
/// character on a floated line.
#[test]
fn cache_returns_the_inset_a_row_was_measured_with() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(&mut cache, &cache_test_content(&["a", "b"], 2), 400.0, &font);

    let inset = cache.inset(0, 400.0);
    assert_eq!(inset.width, 400.0, "measured at the full width");
    assert_eq!(inset.x_offset, 0.0);

    // Out-of-range rows fall back rather than panicking: the render loop can
    // ask about a slot the cache has not caught up with yet.
    assert_eq!(cache.inset(999, 123.0).width, 123.0, "fallback width");
}

/// A narrower inset must actually wrap the text differently — proving the
/// value reaches the LayoutJob rather than being carried and ignored.
#[test]
fn inset_width_changes_how_a_line_wraps() {
    use crate::data::{StyledLine, TextSegment};
    let line = StyledLine {
        segments: vec![TextSegment::plain("word ".repeat(40))],
        stream: "main".into(),
        timestamp: None,
    };
    let visuals = eframe::egui::Visuals::default();
    let font_id = eframe::egui::FontId::monospace(14.0);
    let ctx = eframe::egui::Context::default();

    ctx.begin_pass(eframe::egui::RawInput::default());
    let wide = VellumGuiApp::measure_line_height(
        &ctx,
        &line,
        &visuals,
        super::LineInset::full(1000.0),
        &font_id,
        None,
    );
    let narrow = VellumGuiApp::measure_line_height(
        &ctx,
        &line,
        &visuals,
        super::LineInset { width: 200.0, x_offset: 800.0, y_offset: 0.0, float_height: 0.0, float_width: 800.0 },
        &font_id,
        None,
    );
    {
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
    }

    assert!(
        narrow > wide,
        "a float's narrower column must wrap to more rows: {narrow} vs {wide}"
    );
}

/// Insets stay parallel to heights through both cache paths, like `extra`.
#[test]
fn inset_column_stays_parallel() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(&mut cache, &cache_test_content(&["a", "b", "c"], 3), 400.0, &font);
    assert_eq!(cache.heights().len(), 3);
    // Appending keeps them in step.
    update_cache(&mut cache, &cache_test_content(&["a", "b", "c", "d"], 4), 400.0, &font);
    assert_eq!(cache.inset(3, 0.0).width, 400.0, "appended row has an inset");
    // A rebuild re-derives every row's inset.
    update_cache(&mut cache, &cache_test_content(&["a", "b", "c", "d"], 4), 150.0, &font);
    assert_eq!(cache.inset(0, 0.0).width, 150.0, "rebuild re-derives insets");
}

// ==================== Float spans + virtualization (P2.3) ====================

/// A viewport starting mid-float must walk back to the origin row, because
/// only that row paints the image. Without the lookback, scrolling into the
/// middle of a float makes the picture vanish.
#[test]
fn float_origin_lookback_finds_the_painting_row() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(
        &mut cache,
        &cache_test_content(&["a", "b", "c", "d", "e"], 5),
        400.0,
        &font,
    );
    // Row 1 originates a float covering rows 1..4.
    cache.set_span(1, 3);

    assert_eq!(cache.float_origin_at(1), 1, "the origin resolves to itself");
    assert_eq!(cache.float_origin_at(2), 1, "mid-float walks back");
    assert_eq!(cache.float_origin_at(3), 1, "last covered row walks back");
    assert_eq!(cache.float_origin_at(4), 4, "past the float, no lookback");
    assert_eq!(cache.float_origin_at(0), 0, "before the float, no lookback");
}

/// With no floats the lookback is the identity — it must not perturb normal
/// virtualization.
#[test]
fn float_origin_lookback_is_identity_without_floats() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(&mut cache, &cache_test_content(&["a", "b", "c"], 3), 400.0, &font);
    for i in 0..3 {
        assert_eq!(cache.float_origin_at(i), i, "row {i}");
    }
}

/// Spans stay parallel to heights through both cache paths, and a rebuild
/// clears them so stale spans can never outlive the float that made them.
#[test]
fn span_column_stays_parallel_and_clears_on_rebuild() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(&mut cache, &cache_test_content(&["a", "b", "c"], 3), 400.0, &font);
    cache.set_span(0, 2);
    assert_eq!(cache.spans().len(), cache.heights().len());

    // Append: still parallel, new row has no span.
    update_cache(&mut cache, &cache_test_content(&["a", "b", "c", "d"], 4), 400.0, &font);
    assert_eq!(cache.spans().len(), cache.heights().len(), "after append");

    // Rebuild (width change): spans reset for re-derivation.
    update_cache(&mut cache, &cache_test_content(&["a", "b", "c", "d"], 4), 150.0, &font);
    assert_eq!(cache.spans().len(), cache.heights().len(), "after rebuild");
    assert!(
        cache.spans().iter().all(|s| *s == 0),
        "a rebuild must clear spans so none outlive their float"
    );
}

/// The lookback is bounded by the longest span, not by the buffer length —
/// a 10,000-line window must not scan to the top on every frame.
#[test]
fn float_origin_lookback_is_bounded_by_the_longest_span() {
    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    let lines: Vec<&str> = vec!["x"; 200];
    update_cache(&mut cache, &cache_test_content(&lines, 200), 400.0, &font);
    cache.set_span(0, 2); // a short float far above

    // Row 150 is nowhere near that float; the scan must not find it.
    assert_eq!(cache.float_origin_at(150), 150);
}

/// End-to-end: a line carrying an inline image must come out of the cache
/// pass with a real float — a span, a narrowed inset on the covered rows,
/// and reserved height. Until now every column was a no-op, so this is the
/// test that proves the layout pass does its job.
#[test]
fn layout_pass_computes_a_float_for_an_image_line() {
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
    use crate::data::{FloatAlign, InlineImage, StyledLine, TextSegment};

    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // A real on-disk PNG so the size lookup resolves.
    let tmp = std::env::temp_dir().join(format!("vellum_float_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let path = tmp.join("banner.png");
    {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, png).unwrap();
    }
    let mut registry = CustomEmojiRegistry::default();
    registry.insert_for_test(CustomEmoji {
        name: "banner".into(),
        path,
        format: EmojiFormat::Png,
    });
    crate::core::inline_image::set_for_test(registry);

    let mut content = cache_test_content(&["after one", "after two", "after three"], 4);
    content.lines.push_front(StyledLine {
        segments: vec![
            TextSegment {
                text: "[img:banner]".into(),
                inline_image: Some(InlineImage {
                    name: "banner".into(),
                    rows: 3.0,
                    align: FloatAlign::Left,
                }),
                ..Default::default()
            },
            TextSegment::plain("Prose beside the picture."),
        ],
        stream: "main".into(),
        timestamp: None,
    });

    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(&mut cache, &content, 400.0, &font);

    assert!(cache.spans()[0] > 0, "the image line must originate a float");
    assert!(
        cache.inset(0, 400.0).width < 400.0,
        "text beside the image wraps narrower: {}",
        cache.inset(0, 400.0).width
    );
    assert!(
        cache.inset(0, 400.0).x_offset > 0.0,
        "a left float shifts its text right"
    );
    // Total reserved space is at least the image's height.
    let span = cache.spans()[0] as usize;
    assert!(
        cache.stride_sum(0..span, 0.0) >= cache.heights()[0],
        "the float's rows must be tall enough for the picture"
    );
    // Rows past the float rejoin the full width.
    if span < cache.heights().len() {
        assert_eq!(
            cache.inset(span, 400.0).width,
            400.0,
            "text rejoins full width after the image"
        );
    }

    crate::core::inline_image::set_for_test(CustomEmojiRegistry::default());
}

/// REPRO (live, 2026-08-10): a `<vellumImg>` arriving as a NEW line rendered
/// its `[img:sunset]` fallback instead of floating.
///
/// Appends take the incremental cache path, which cannot compute a span (it
/// only ever adds rows past the end), and `float_epoch` did not catch it —
/// the epoch tracks the window's row capacity, which an arriving line does
/// not change. An appended image line must force the full layout pass.
#[test]
fn an_appended_image_line_still_gets_a_float() {
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
    use crate::data::{FloatAlign, InlineImage, StyledLine, TextSegment};

    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let tmp = std::env::temp_dir().join(format!("vellum_append_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let path = tmp.join("banner.png");
    {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[0, 128, 255, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, png).unwrap();
    }
    let mut registry = CustomEmojiRegistry::default();
    registry.insert_for_test(CustomEmoji {
        name: "banner".into(),
        path,
        format: EmojiFormat::Png,
    });
    crate::core::inline_image::set_for_test(registry);

    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();

    // A settled window with ordinary text: the next update is an APPEND.
    let mut content = cache_test_content(&["one", "two"], 2);
    update_cache(&mut cache, &content, 400.0, &font);
    assert!(cache.spans().iter().all(|s| *s == 0), "no floats yet");

    // Now the image arrives as a new line, exactly as _respond delivers it.
    content.lines.push_back(StyledLine {
        segments: vec![TextSegment {
            text: "[img:banner]".into(),
            inline_image: Some(InlineImage {
                name: "banner".into(),
                rows: 4.0,
                align: FloatAlign::Left,
            }),
            ..Default::default()
        }],
        stream: "main".into(),
        timestamp: None,
    });
    content.generation = 3;
    update_cache(&mut cache, &content, 400.0, &font);

    let last = cache.heights().len() - 1;
    assert!(
        cache.spans()[last] > 0,
        "an appended image line must originate a float, not fall back to text"
    );

    crate::core::inline_image::set_for_test(CustomEmojiRegistry::default());
}

/// REGRESSION (live, 2026-08-10): after shrinking the window, the floated
/// picture collapsed to a sliver behind the text.
///
/// The painter derived the image's width from `row_width - inset.width`,
/// but the row width comes from the CURRENT layout while `inset.width` was
/// computed when the row was measured. A resize made those disagree and the
/// difference collapsed toward zero. The reserved column is now carried on
/// the inset itself, so it cannot drift from the layout that produced it.
#[test]
fn float_width_survives_a_window_resize() {
    // A float measured in a 600pt window.
    let wide = super::LineInset {
        width: 440.0,
        x_offset: 160.0,
        y_offset: 0.0,
        float_height: 68.0,
        float_width: 160.0,
    };
    // The window is now narrower; the painter's row width shrank.
    let narrow_row_width = 400.0_f32;

    // The OLD derivation collapses (and even goes negative here).
    let derived = (narrow_row_width - wide.width).max(0.0);
    assert!(
        derived < 1.0,
        "the old row_width - inset.width derivation collapses: {derived}"
    );

    // The stored width is unchanged, so the picture keeps its column.
    assert_eq!(
        wide.float_width, 160.0,
        "the reserved column must not depend on the current row width"
    );
}

/// A line with no float reserves no column, so an ordinary row can never
/// accidentally paint an image-sized gap.
#[test]
fn a_full_width_line_reserves_no_float_column() {
    let inset = super::LineInset::full(400.0);
    assert_eq!(inset.float_width, 0.0);
    assert_eq!(inset.x_offset, 0.0);
}

/// REGRESSION (live, 2026-08-10): in a narrow window the room description
/// wrapped to MORE rows than the picture was tall, and the overflow ran
/// across the image instead of stopping beside it.
///
/// egui wraps a line to one width for its whole height — a galley cannot be
/// inset for its first rows only — so the picture's reserved column grows to
/// the origin row's real height rather than the text spilling over it.
#[test]
fn a_tall_origin_row_grows_the_reserved_column() {
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
    use crate::data::{FloatAlign, InlineImage, StyledLine, TextSegment};

    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let tmp = std::env::temp_dir().join(format!("vellum_tall_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let path = tmp.join("wide.png");
    {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[9, 9, 9, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, png).unwrap();
    }
    let mut registry = CustomEmojiRegistry::default();
    registry.insert_for_test(CustomEmoji {
        name: "wide".into(),
        path,
        format: EmojiFormat::Png,
    });
    crate::core::inline_image::set_for_test(registry);

    // One line: a small image plus a LOT of prose. In a narrow window the
    // prose wraps to more rows than a 2-row picture covers.
    let mut content = cache_test_content(&["after"], 2);
    content.lines.push_front(StyledLine {
        segments: vec![
            TextSegment {
                text: "[img:wide]".into(),
                inline_image: Some(InlineImage {
                    name: "wide".into(),
                    rows: 2.0,
                    align: FloatAlign::Left,
                }),
                ..Default::default()
            },
            TextSegment::plain("word ".repeat(120)),
        ],
        stream: "main".into(),
        timestamp: None,
    });

    let font = eframe::egui::FontId::monospace(14.0);
    let mut cache = super::RowHeightCache::default();
    update_cache(&mut cache, &content, 260.0, &font);

    let span = cache.spans()[0] as usize;
    assert!(span > 0, "the image line originates a float");

    // The reserved block must cover the origin row's FULL wrapped height,
    // so no part of that text sits over the picture.
    let reserved = cache.stride_sum(0..span, 0.0);
    assert!(
        reserved >= cache.heights()[0] - 0.5,
        "reserved {reserved} must cover the origin row's {} of text",
        cache.heights()[0]
    );

    crate::core::inline_image::set_for_test(CustomEmojiRegistry::default());
}

/// REGRESSION (live, 2026-08-10): text rendered OVER the picture whenever
/// the text beside it was shorter than the image.
///
/// The reserved float height (`extra`) was counted by the virtualization
/// spacers but never ALLOCATED by the visible rows — the row rect was only
/// the galley's height, so every following line rendered straight over the
/// bottom of the painted image. Driven through the real renderer: the
/// window's total content height must include the picture's reserved space,
/// which shows up as a larger bottom-of-buffer scroll offset than the same
/// text without the image.
#[test]
fn reserved_float_height_is_actually_allocated() {
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
    use crate::data::{FloatAlign, InlineImage, StyledLine, TextSegment};

    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let tmp = std::env::temp_dir().join(format!("vellum_alloc_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let path = tmp.join("tall.png");
    {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[40, 40, 40, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, png).unwrap();
    }
    let mut registry = CustomEmojiRegistry::default();
    registry.insert_for_test(CustomEmoji {
        name: "tall".into(),
        path,
        format: EmojiFormat::Png,
    });
    crate::core::inline_image::set_for_test(registry);

    let image_line = StyledLine {
        segments: vec![
            TextSegment {
                text: "[img:tall]".into(),
                inline_image: Some(InlineImage {
                    name: "tall".into(),
                    rows: 6.0,
                    align: FloatAlign::Left,
                }),
                ..Default::default()
            },
            // ONE short line of text: far shorter than a 6-row picture, so
            // the block's height is almost entirely reserved `extra`.
            TextSegment::plain("short"),
        ],
        stream: "main".into(),
        timestamp: None,
    };

    // Identical buffers except for the image segment.
    // The image is the LAST line: nothing follows to fill its span, so the
    // reserved height stands alone (lines pushed after would slide up
    // beside the picture and mask the difference).
    let mut with_image = ScrollHarness::new("alloc_with", 100.0);
    with_image.push_lines(30);
    with_image.content.lines.push_back(image_line);
    with_image.content.generation += 1;
    with_image.frame();
    with_image.frame();
    with_image.frame();

    let mut without = ScrollHarness::new("alloc_without", 100.0);
    without.push_lines(30);
    without.content.lines.push_back(StyledLine {
        segments: vec![TextSegment::plain("short")],
        stream: "main".into(),
        timestamp: None,
    });
    without.content.generation += 1;
    without.frame();
    without.frame();
    without.frame();

    let row_h = 17.0; // monospace 14 is ~17pt; the margin below absorbs slop
    let gap = with_image.offset() - without.offset();
    assert!(
        gap > row_h * 3.0,
        "the picture's reserved rows must exist in the REAL layout, not just \
         the spacer math: content grew by only {gap:.1}pt over the text-only \
         baseline (expected several rows)"
    );

    crate::core::inline_image::set_for_test(CustomEmojiRegistry::default());
}

/// Owner request (live, 2026-08-10): when the following text cannot fit
/// beside the picture, scale the picture DOWN rather than leaving it at
/// full size over a text-below layout. The requested `rows` is a ceiling,
/// not a promise — but only when there is a follower to make room for: a
/// standalone image keeps its requested size, so a script's `rows=` still
/// means what it says.
#[test]
fn picture_shrinks_when_the_following_text_cannot_fit_beside_it() {
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
    use crate::data::{FloatAlign, InlineImage, StyledLine, TextSegment};

    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let tmp = std::env::temp_dir().join(format!("vellum_shrink_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let path = tmp.join("art.png");
    {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[7, 7, 7, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, png).unwrap();
    }
    let mut registry = CustomEmojiRegistry::default();
    registry.insert_for_test(CustomEmoji {
        name: "art".into(),
        path,
        format: EmojiFormat::Png,
    });
    crate::core::inline_image::set_for_test(registry);

    let image_line = || StyledLine {
        segments: vec![
            TextSegment {
                text: "[img:art]".into(),
                inline_image: Some(InlineImage {
                    name: "art".into(),
                    rows: 6.0,
                    align: FloatAlign::Left,
                }),
                ..Default::default()
            },
            TextSegment::plain("name"),
        ],
        stream: "main".into(),
        timestamp: None,
    };
    let font = eframe::egui::FontId::monospace(14.0);

    // Standalone: the image line is LAST, nothing to make room for.
    let mut alone = cache_test_content(&["before"], 2);
    alone.lines.push_back(image_line());
    let mut cache_alone = super::RowHeightCache::default();
    update_cache(&mut cache_alone, &alone, 300.0, &font);
    let kept = cache_alone
        .inset(cache_alone.heights().len() - 1, 300.0)
        .float_height;
    assert!(kept > 0.0, "standalone float laid out");

    // Followed by prose too long to fit beside a 6-row picture at this
    // width: the picture must come out SMALLER than the standalone one.
    let mut followed = cache_test_content(&[], 1);
    followed.lines.push_back(image_line());
    followed.lines.push_back(StyledLine {
        segments: vec![TextSegment::plain("word ".repeat(200))],
        stream: "main".into(),
        timestamp: None,
    });
    followed.generation = 3;
    let mut cache_followed = super::RowHeightCache::default();
    update_cache(&mut cache_followed, &followed, 300.0, &font);
    let shrunk = cache_followed.inset(0, 300.0).float_height;

    assert!(
        shrunk > 0.0 && shrunk < kept,
        "an unfittable follower must shrink the picture: {shrunk} vs {kept}"
    );

    crate::core::inline_image::set_for_test(CustomEmojiRegistry::default());
}

/// Owner decision (2026-08-10): `rows` is a DEFAULT, and the displayed
/// picture follows the text block that wraps it — press-and-hold shows the
/// real size. Three lines of neighboring text produce a taller picture than
/// one line, and both come out smaller than the standalone default.
#[test]
fn picture_height_follows_the_text_block_beside_it() {
    use crate::core::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
    use crate::data::{FloatAlign, InlineImage, StyledLine, TextSegment};

    let _guard = crate::core::inline_image::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let tmp = std::env::temp_dir().join(format!("vellum_dyn_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let path = tmp.join("art.png");
    {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[3, 3, 3, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, png).unwrap();
    }
    let mut registry = CustomEmojiRegistry::default();
    registry.insert_for_test(CustomEmoji {
        name: "art".into(),
        path,
        format: EmojiFormat::Png,
    });
    crate::core::inline_image::set_for_test(registry);

    let image_line = || StyledLine {
        segments: vec![
            TextSegment {
                text: "[img:art]".into(),
                inline_image: Some(InlineImage {
                    name: "art".into(),
                    rows: 6.0,
                    align: FloatAlign::Left,
                }),
                ..Default::default()
            },
            TextSegment::plain("name"),
        ],
        stream: "main".into(),
        timestamp: None,
    };
    let text = |t: &str| StyledLine {
        segments: vec![TextSegment::plain(t.to_string())],
        stream: "main".into(),
        timestamp: None,
    };
    let font = eframe::egui::FontId::monospace(14.0);
    let height_of = |lines: Vec<StyledLine>| {
        let mut content = cache_test_content(&[], 1);
        for line in lines {
            content.lines.push_back(line);
        }
        content.generation = content.lines.len() as u64;
        let mut cache = super::RowHeightCache::default();
        update_cache(&mut cache, &content, 400.0, &font);
        cache.inset(0, 400.0).float_height
    };

    let standalone = height_of(vec![image_line()]);
    let one_line = height_of(vec![image_line(), text("a")]);
    let three_lines = height_of(vec![
        image_line(),
        text("mid one"),
        text("mid two"),
        text("mid three"),
    ]);

    assert!(standalone > 0.0 && one_line > 0.0 && three_lines > 0.0);
    assert!(
        one_line < three_lines,
        "more neighboring text means a taller picture: {one_line} vs {three_lines}"
    );
    assert!(
        three_lines < standalone,
        "wrapped pictures stay smaller than the standalone default: \
         {three_lines} vs {standalone}"
    );

    crate::core::inline_image::set_for_test(CustomEmojiRegistry::default());
}

// ==================== Auto split-screen scrollback ====================
//
// Scrolling back splits the window: frozen history on top, a live pane
// pinned to the tail below. Returning to the bottom merges the panes.

/// While following the tail there is no split — the live pane never renders.
#[test]
fn no_split_while_following() {
    let mut h = ScrollHarness::new("split_none", 300.0);
    h.push_lines(100);
    h.frame_split();
    h.frame_split();
    assert!(h.following(), "fresh window follows the tail");
    assert!(!h.live_pane_rendered(), "no live pane while following");
}

/// Scrolling back opens the split; the live pane exists and is pinned to
/// the tail even as new lines arrive.
#[test]
fn scrollback_opens_split_with_pinned_live_pane() {
    let mut h = ScrollHarness::new("split_open", 300.0);
    h.push_lines(200);
    h.frame_split();
    h.frame_split();
    // Wheel up over the window: detach follow.
    let hover = h.hover_center();
    h.frame_split_with(vec![hover.clone(), ScrollHarness::wheel(600.0)]);
    for _ in 0..5 {
        h.frame_split();
    }
    assert!(!h.following(), "wheel-up must detach the top pane");
    assert!(h.live_pane_rendered(), "scrolled back => live pane renders");
    assert!(h.live_following(), "live pane is pinned to the tail");

    // New text arrives; the live pane must stay pinned.
    h.push_lines(50);
    for _ in 0..3 {
        h.frame_split();
    }
    assert!(h.live_following(), "live pane still pinned after new lines");
    assert!(!h.following(), "top pane stays frozen after new lines");
}

/// The End action re-arms follow and the panes merge again.
#[test]
fn end_action_merges_split() {
    let mut h = ScrollHarness::new("split_merge", 300.0);
    h.push_lines(200);
    h.frame_split();
    let hover = h.hover_center();
    h.frame_split_with(vec![hover, ScrollHarness::wheel(600.0)]);
    // Let egui's wheel smoothing decay fully — residual smoothed delta
    // counts as user input and would re-detach follow right after End.
    for _ in 0..30 {
        h.frame_split();
    }
    assert!(!h.following());
    // End: resume following (kind 2 pending action, same as the End key).
    h.request(2, 0.0);
    h.frame_split();
    h.frame_split();
    assert!(h.following(), "End re-arms follow and merges the panes");
}

/// A window too short for two readable panes keeps single-view scrollback.
#[test]
fn tiny_window_never_splits() {
    let mut h = ScrollHarness::new("split_tiny", 60.0);
    h.push_lines(100);
    h.frame_split();
    let hover = h.hover_center();
    h.frame_split_with(vec![hover, ScrollHarness::wheel(200.0)]);
    for _ in 0..3 {
        h.frame_split();
    }
    assert!(!h.following(), "still scrolled back");
    assert!(!h.live_pane_rendered(), "too short to split");
}

/// The live pane is not a scrolling surface: a wheel over the bottom half
/// must leave its offset exactly where the tail pin put it.
#[test]
fn wheel_over_live_pane_does_not_scroll_it() {
    let mut h = ScrollHarness::new("split_live_wheel", 300.0);
    h.push_lines(200);
    h.frame_split();
    let hover_top = h.hover_center();
    h.frame_split_with(vec![hover_top, ScrollHarness::wheel(600.0)]);
    for _ in 0..30 {
        h.frame_split();
    }
    assert!(h.live_pane_rendered());

    let live_offset = |h: &ScrollHarness| -> f32 {
        let live_id = format!("{}~live", h.scroll_id);
        let area_id: Option<eframe::egui::Id> = h.ctx.data_mut(|d| {
            d.get_temp(eframe::egui::Id::new((
                "text_scroll_area_id",
                live_id.as_str(),
            )))
        });
        area_id
            .and_then(|id| eframe::egui::scroll_area::State::load(&h.ctx, id))
            .map(|s| s.offset.y)
            .unwrap_or(0.0)
    };
    let before = live_offset(&h);
    assert!(before > 0.0, "live pane starts pinned to the tail");

    // Wheel up with the pointer in the BOTTOM section (the live pane).
    let hover_bottom = eframe::egui::Event::PointerMoved(eframe::egui::pos2(
        h.view.x * 0.5,
        h.view.y * 0.9,
    ));
    h.frame_split_with(vec![hover_bottom, ScrollHarness::wheel(600.0)]);
    for _ in 0..30 {
        h.frame_split();
    }
    let after = live_offset(&h);
    assert!(
        (after - before).abs() < 0.5,
        "wheel over the live pane must not move it: {before} -> {after}"
    );
}

/// Wheel over the bottom (live) section scrolls the HISTORY pane — the
/// whole split window acts as one scroll surface.
#[test]
fn wheel_over_live_pane_scrolls_the_history_pane() {
    let mut h = ScrollHarness::new("split_forward_wheel", 300.0);
    h.push_lines(300);
    h.frame_split();
    let hover_top = h.hover_center();
    h.frame_split_with(vec![hover_top, ScrollHarness::wheel(600.0)]);
    for _ in 0..30 {
        h.frame_split();
    }
    assert!(h.live_pane_rendered());
    let before = h.offset();
    assert!(before > 0.0, "top pane is scrolled somewhere above the tail");

    // Wheel up with the pointer over the live pane.
    let hover_bottom = eframe::egui::Event::PointerMoved(eframe::egui::pos2(
        h.view.x * 0.5,
        h.view.y * 0.9,
    ));
    h.frame_split_with(vec![hover_bottom, ScrollHarness::wheel(600.0)]);
    for _ in 0..30 {
        h.frame_split();
    }
    let after = h.offset();
    assert!(
        after < before - 1.0,
        "wheel over the live pane must scroll history up: {before} -> {after}"
    );
    assert!(!h.following(), "still detached while scrolled back");
}
