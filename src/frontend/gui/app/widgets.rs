//! Per-widget content renderers for the GUI.
//!
//! Pure-move extraction from `app.rs`: stateless associated helpers that
//! render `WindowContent` variants from `AppCore` state.

use super::*;

mod boards;
mod command_widget;
mod injury;
mod links_bars;
mod map_compass;
mod panels;
mod text;
mod vitals;

/// Seconds for a value-driven bar to glide to a new target value.
const BAR_ANIMATION_SECONDS: f32 = 0.2;

/// Height (points) of the band at the bottom of a positioned dialog canvas
/// whose links render in the footer row instead of the canvas. The canvas
/// skip test and the footer's membership test MUST use the same value, or a
/// bottom-anchored link draws twice — or not at all.
const PANEL_FOOTER_BAND: f32 = 40.0;

/// Editing operations the command input applies for BOUND key combos (see
/// `render_command_input_widget`) — the GUI mirror of the TUI's
/// `apply_command_input_action`. Bound combos are consumed before the
/// TextEdit sees them; egui built-ins keep handling unbound keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CommandEditOp {
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
    Backspace,
    Delete,
    DeleteWord,
    SelectAll,
    Copy,
    Paste,
}

impl VellumGuiApp {
    /// Estimated height of one line at the given wrap width, from a single
    /// LayoutJob over all segments. Exact for link-free lines (they render as
    /// one galley); link-bearing lines wrap as separate widgets and may
    /// differ slightly — the renderer self-corrects those once visible.
    pub(super) fn render_window_content(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        tab: &GuiTab,
        settings: WidgetRenderSettings,
    ) -> Option<GuiLinkClick> {
        let Some(window) = app_core.ui_state.windows.get(&tab.window_name) else {
            ui.label("This tab's source window is no longer available.");
            return None;
        };

        // Skin background: reserve a paint slot now (so the art stays behind
        // the content), fill it after layout from the content's real extent.
        // Compact one-row widgets live in auto-sized windows whose pre-layout
        // available rect can be taller than the final frame; painting that
        // rect up front spilled the art below the window.
        let background_slot = settings.background.clone().map(|background| {
            (
                ui.painter().add(egui::Shape::Noop),
                ui.available_rect_before_wrap(),
                background,
            )
        });

        // Scale the label-driven text styles so list/grid widgets (targets,
        // players, dashboards, ...) follow the window's text size and font,
        // not just the segment-based text renderers below.
        let text_size = settings.text_size;
        let font_id = settings.font_id();
        {
            let styles = &mut ui.style_mut().text_styles;
            if let Some(font) = styles.get_mut(&egui::TextStyle::Body) {
                font.size = text_size;
                font.family = font_id.family.clone();
            }
            if let Some(font) = styles.get_mut(&egui::TextStyle::Monospace) {
                font.size = text_size;
            }
            if let Some(font) = styles.get_mut(&egui::TextStyle::Small) {
                font.size = (text_size - 4.0).max(8.0);
            }
        }

        let clicked_link = match &window.content {
            WindowContent::Text(content)
            | WindowContent::Inventory(content)
            | WindowContent::Reserve(content)
            | WindowContent::Spells(content) => {
                let query = Self::active_search_query(app_core);
                Self::render_text_content(
                    ui,
                    content,
                    &tab.window_name,
                    query.as_deref(),
                    &font_id,
                    settings.wrap_text,
                    window.content_align.as_deref(),
                )
            }
            WindowContent::MiniVitals => {
                Self::render_vitals_content(app_core, ui, &settings);
                None
            }
            WindowContent::Progress(data) => {
                Self::render_single_progress_content(ui, data, &settings);
                None
            }
            WindowContent::Compass(compass) => {
                Self::render_compass_content(app_core, ui, compass, settings.skin_art.as_deref())
            }
            WindowContent::Map(map_data) => {
                Self::render_map_content(app_core, ui, map_data, settings.map_zoom)
            }
            WindowContent::Hand { item, link } => {
                let hand_prefix = if window.name.to_ascii_lowercase().contains("left") {
                    "L"
                } else if window.name.to_ascii_lowercase().contains("right") {
                    "R"
                } else {
                    "S"
                };
                // Status-driven icon states from the window's layout def.
                let resolved = app_core
                    .layout
                    .windows
                    .iter()
                    .find(|def| def.name() == window.name)
                    .and_then(|def| match def {
                        crate::config::WindowDef::Hand { data, .. } => Some(data),
                        _ => None,
                    })
                    .filter(|data| !data.states.is_empty())
                    .map(|data| {
                        let now_server = chrono::Utc::now().timestamp()
                            + app_core.message_processor.server_time_offset;
                        crate::core::conditions::resolve_hand(
                            data,
                            &app_core.game_state,
                            now_server,
                            app_core.gameobj_data_cached(),
                        )
                    })
                    .unwrap_or_default();
                Self::render_hand_content(
                    ui,
                    hand_prefix,
                    item,
                    link,
                    settings.skin_art.as_deref(),
                    &resolved,
                    settings.hand_icon_size,
                )
            }
            WindowContent::TabbedText(tabbed) => {
                let mut clicked_link = Self::render_tabbed_text_tab_strip(
                    ui,
                    &tab.window_name,
                    tabbed,
                    settings.skin_art.as_deref(),
                );
                if let Some(active) = tabbed.tabs.get(tabbed.active_tab_index) {
                    let query = Self::active_search_query(app_core);
                    // Per-tab scroll id: each tab keeps its own scroll
                    // position and height cache (tabs have independent
                    // buffers and generations).
                    let scroll_id =
                        format!("{}::tab{}", tab.window_name, tabbed.active_tab_index);
                    if let Some(link) = Self::render_text_content(
                        ui,
                        &active.content,
                        &scroll_id,
                        query.as_deref(),
                        &font_id,
                        settings.wrap_text,
                        window.content_align.as_deref(),
                    ) {
                        clicked_link.get_or_insert(link);
                    }
                } else {
                    ui.label("No active tab content.");
                }
                clicked_link
            }
            WindowContent::Room(room) => {
                // Per-window section toggles from the layout def (set in the
                // window editor, shared with the TUI). The room-name heading
                // is always shown: the def's show_name flag drives the TUI
                // border title, which has no GUI equivalent.
                let show = match app_core
                    .layout
                    .windows
                    .iter()
                    .find(|w| w.name() == tab.window_name)
                {
                    Some(crate::config::WindowDef::Room { data, .. }) => (
                        data.show_desc,
                        data.show_objs,
                        data.show_players,
                        data.show_exits,
                    ),
                    _ => (true, true, true, true),
                };
                let interact_focus = app_core.interact_focus_exist_id();
                Self::render_room_content(
                    ui,
                    room,
                    show,
                    &tab.window_name,
                    text_size,
                    &font_id,
                    interact_focus.as_deref(),
                )
            }
            WindowContent::ActiveEffects(content) => {
                Self::render_active_effects_content(
                    ui,
                    content,
                    settings,
                    window.content_align.as_deref(),
                );
                None
            }
            WindowContent::WebUi(content) => {
                Self::render_webui_content(ui, content);
                None
            }
            WindowContent::Targets => {
                Self::render_targets_content(app_core, ui, &tab.window_name)
            }
            WindowContent::Players => Self::render_players_content(app_core, ui),
            WindowContent::Countdown(countdown) => {
                Self::render_countdown_content(app_core, ui, countdown, &settings);
                None
            }
            WindowContent::Indicator(indicator) => {
                // Per-indicator gray override wins over the global toggle.
                let gray = settings
                    .gray_icon_overrides
                    .get(&indicator.indicator_id)
                    .or_else(|| {
                        settings
                            .gray_icon_overrides
                            .get(&indicator.indicator_id.to_ascii_uppercase())
                    })
                    .copied()
                    .unwrap_or(settings.gray_inactive_icons);
                // Resolve the status template's condition-driven art (state
                // icon/color) from the cached templates; empty when the id has
                // no template or no states (falls back to id-keyed art).
                let resolved = app_core
                    .indicator_template(&indicator.indicator_id)
                    .filter(|t| !t.states.is_empty() || t.icon_ref.is_some())
                    .map(|template| {
                        let now_server = chrono::Utc::now().timestamp()
                            + app_core.message_processor.server_time_offset;
                        crate::core::conditions::resolve_status(
                            template,
                            indicator.active,
                            &app_core.game_state,
                            now_server,
                            app_core.gameobj_data_cached(),
                        )
                    })
                    .unwrap_or_default();
                Self::render_indicator_content(
                    ui,
                    &tab.id.title,
                    indicator,
                    settings.skin_art.as_deref(),
                    gray,
                    &resolved,
                );
                None
            }
            WindowContent::InjuryDoll(doll) => {
                // Resolve the palette from this doll's config (per-level
                // injury*_color/scar*_color overrides), matching the TUI —
                // the GUI used to ignore these and hardcode the palette.
                let palette = app_core
                    .layout
                    .windows
                    .iter()
                    .find(|def| def.name() == window.name)
                    .and_then(|def| match def {
                        crate::config::WindowDef::InjuryDoll { data, .. } => {
                            Some(Self::resolved_injury_palette(data))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(Self::default_injury_palette);
                let (doll_variant, doll_hidden) =
                    Self::resolve_doll_render(app_core, settings.skin_art.as_deref());
                Self::render_injury_doll(
                    ui,
                    &doll.injuries,
                    settings.skin_art.as_deref(),
                    doll_variant,
                    &doll_hidden,
                    settings.doll_grayscale,
                    &palette,
                );
                None
            }
            WindowContent::Dashboard { indicators } => {
                // Read this dashboard's config (layout/spacing/hide_inactive +
                // per-id icon/colors via the status templates), matching the
                // TUI. Missing config falls back to flow + hide-inactive.
                let data = app_core
                    .layout
                    .windows
                    .iter()
                    .find(|def| def.name() == window.name)
                    .and_then(|def| match def {
                        crate::config::WindowDef::Dashboard { data, .. } => Some(data.clone()),
                        _ => None,
                    });
                Self::render_dashboard_content(
                    app_core,
                    ui,
                    indicators,
                    data.as_ref(),
                    settings.skin_art.as_deref(),
                );
                None
            }
            WindowContent::GS4Experience => {
                Self::render_gs4_experience_content(app_core, ui, &tab.window_name, &settings);
                None
            }
            WindowContent::Experience => {
                Self::render_dr_experience_content(app_core, ui);
                None
            }
            WindowContent::Encumbrance => {
                Self::render_encumbrance_content(app_core, ui, &tab.window_name, &settings);
                None
            }
            WindowContent::Betrayer => {
                Self::render_betrayer_content(app_core, ui, &settings);
                None
            }
            WindowContent::Perception(perception) => {
                Self::render_perception_content(ui, perception)
            }
            WindowContent::Items => Self::render_items_content(app_core, ui),
            WindowContent::Container { container_title } => {
                Self::render_container_content(app_core, ui, container_title, settings.wrap_text)
            }
            WindowContent::DialogPanel { dialog_id } => {
                Self::render_dialog_panel_content(
                    app_core,
                    ui,
                    dialog_id,
                    settings.skin_art.as_deref(),
                );
                None
            }
            WindowContent::Quickbar => Self::render_quickbar_content(app_core, ui),
            WindowContent::Hotkeybar { bar } => Self::render_hotkeybar_content(
                app_core,
                ui,
                &window.name,
                bar,
                settings.skin_art.as_deref(),
            ),
            WindowContent::Performance => {
                Self::render_performance_content(app_core, ui);
                None
            }
            WindowContent::CommandInput { .. } => {
                Self::render_command_input_widget(
                    ui,
                    settings.command_input_seed.as_deref().unwrap_or(""),
                    settings.command_input_completion.as_deref(),
                    settings.command_input_drag_gutter,
                );
                None
            }
            WindowContent::Empty => {
                // Spacers reserve their area and draw nothing.
                ui.allocate_space(ui.available_size());
                None
            }
        };

        if let Some((slot, avail, background)) = background_slot {
            // Same tightest-of-three confinement as the group mesh: the
            // pre-layout available rect can exceed the actual window while a
            // gesture or clamp is in flight, and the mesh must never paint
            // past the frame it belongs to.
            let mut rect = avail.intersect(ui.max_rect()).intersect(ui.clip_rect());
            if Self::is_compact_center_widget(&window.widget_type) {
                // One-row widgets: hug the rendered content so the art can't
                // run past an auto-shrunk frame.
                rect.max.y = rect.max.y.min(ui.min_rect().max.y);
            }
            let shapes = crate::frontend::gui::skin::background_shapes(
                rect,
                &background,
                ui.visuals().window_fill(),
            );
            ui.painter()
                .with_clip_rect(rect)
                .set(slot, egui::Shape::Vec(shapes));
        }

        clicked_link
    }
}

/// One rendered line: its composed layout job plus the char ranges of its
/// clickable links within that composed text.
pub(super) struct GuiLineJob {
    job: egui::text::LayoutJob,
    links: Vec<(std::ops::Range<usize>, LinkData)>,
    /// Custom-emoji image slots as `(char_start, char_end, name)` over the
    /// `:name:` fallback text kept in the job. The caller paints the image over
    /// this run after the galley is drawn (see `paint_custom_emoji_runs`).
    custom_runs: Vec<(usize, usize, String)>,
    /// Minimum row height this line needs so an oversized custom emoji (size
    /// knob > 1) isn't clipped by the line above/below. 0.0 when the line has
    /// no emoji taller than the text.
    min_height: f32,
}

/// Buffer-anchored text selection for virtualized text windows. Endpoints
/// address (line uid, char index) in the stream itself — uid resolves back
/// to a buffer index through the window's generation counter — so the
/// selection survives scrolling, stick-to-bottom shifts, and buffer trims,
/// and Ctrl+C can copy lines that are no longer on screen.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct GuiBufferSelection {
    scroll_id: String,
    /// Where the selection started (press point).
    anchor: (u64, usize),
    /// The moving end; follows the pointer while dragging.
    head: (u64, usize),
    dragging: bool,
}

/// Per-window cache of estimated line heights driving text virtualization.
/// Keyed in egui temp data by scroll id; tracks the rendered slice (the last
/// `MAX_RENDERED_LINES` of the buffer) at a specific wrap width/generation.
#[derive(Default)]
pub(super) struct RowHeightCache {
    wrap_width: f32,
    font_id: egui::FontId,
    generation: u64,
    heights: Vec<f32>,
}

pub(super) fn parse_hex_color(input: &str) -> Option<Color32> {
    let hex = input.strip_prefix('#').unwrap_or(input);
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::{CommandEditOp, GuiBufferSelection, VellumGuiApp};

    /// Drive one edit op against text + (primary, secondary) char range.
    fn edit(text: &str, range: (usize, usize), op: CommandEditOp, extend: bool) -> (String, (usize, usize)) {
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
        assert_eq!(edit("go west", (7, 7), CommandEditOp::WordLeft, false).1, (3, 3));
        assert_eq!(edit("go west", (0, 0), CommandEditOp::WordRight, false).1, (2, 2));
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
        assert_eq!(edit("hello", (2, 2), CommandEditOp::SelectAll, false).1, (5, 0));
    }

    #[test]
    fn countdown_remaining_clamps_to_zero_when_elapsed() {
        // now = 150_000ms (150s), end = 100s -> elapsed
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(100, 0, 150_000), 0);
    }

    #[test]
    fn countdown_remaining_counts_down_from_end_time() {
        // now = 100_000ms (100s), end = 110s -> exactly 10s left
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 100_000), 10);
    }

    #[test]
    fn countdown_remaining_applies_server_offset() {
        // Server clock runs 5s ahead of local time. now = 100s -> 5s left.
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 5, 100_000), 5);
    }

    #[test]
    fn countdown_remaining_ceilings_partial_seconds() {
        // 1001ms remaining -> displays 2 (ceiling): end 110s, now 108_999ms
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 108_999), 2);
        // Exactly 1000ms remaining -> displays 1
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 109_000), 1);
        // 1ms remaining -> still displays 1
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 109_999), 1);
        // 0ms remaining -> displays 0
        assert_eq!(VellumGuiApp::countdown_remaining_seconds(110, 0, 110_000), 0);
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
        assert_eq!(VellumGuiApp::resolve_line_uid(base, 5, base.wrapping_add(4)), 4);
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
            VellumGuiApp::buffer_selection_copy_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &content, &selection, base, None),
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
            VellumGuiApp::buffer_selection_copy_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &content, &reversed, base, None),
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
            VellumGuiApp::buffer_selection_copy_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &content, &single, base, None),
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
            VellumGuiApp::buffer_selection_copy_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &content, &selection, base, None),
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
        let built =
            VellumGuiApp::build_line_job(&eframe::egui::Context::default(), &line, &visuals, None, &font_id, f32::INFINITY, None);
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
            VellumGuiApp::build_line_job(&ctx, &line, &visuals, None, &font_id, f32::INFINITY, None);
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
        let a = VellumGuiApp::build_line_job(&ctx, &line, &visuals, None, &font_id, f32::INFINITY, None);
        assert_eq!(a.min_height, 0.0);
        // ...but an oversized emoji grows the row so it isn't clipped.
        super::custom_emoji_render::set_geometry(2.0, 0.2);
        let b = VellumGuiApp::build_line_job(&ctx, &line, &visuals, None, &font_id, f32::INFINITY, None);
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
        assert!(width_factor > size, "default spacing gives positive padding");

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
        let built =
            VellumGuiApp::build_line_job(&eframe::egui::Context::default(), &line, &visuals, None, &font_id, f32::INFINITY, None);
        assert_eq!(VellumGuiApp::compose_line_text(&eframe::egui::Context::default(), &eframe::egui::FontId::monospace(14.0), &line, None), built.job.text);
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
}



