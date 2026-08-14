//! Per-widget content renderers for the GUI.
//!
//! Pure-move extraction from `app.rs`: stateless associated helpers that
//! render `WindowContent` variants from `AppCore` state.

use super::*;

mod bestiary;
mod boards;
mod command_widget;
mod containers;
mod creature_field;
mod injury;
mod links_bars;
mod map_compass;
mod multiaccount;
mod panels;
mod text;
pub(super) use text::LineInset;
mod vitals;

/// egui data key holding the RAW skin/theme accent for widget painting.
/// `visuals.selection.bg_fill` carries the readability-adjusted menu-row
/// fill (see `readable_selection_fill`), which is deliberately dimmer than
/// the accent — widgets that want the accent itself (map, dialog progress
/// fills, wheel highlight, focus rings, effect bars) read this instead.
fn widget_accent_id() -> egui::Id {
    egui::Id::new("vellum_widget_accent")
}

/// Publish the raw accent for `widget_accent` readers. Called wherever the
/// application visuals are (re)built.
pub(super) fn set_widget_accent(ctx: &egui::Context, accent: Color32) {
    ctx.data_mut(|data| data.insert_temp(widget_accent_id(), accent));
}

/// The raw skin/theme accent. Falls back to `selection.bg_fill` when no
/// accent has been published (plain theme, tests).
pub(super) fn widget_accent(ctx: &egui::Context, visuals: &egui::Visuals) -> Color32 {
    ctx.data(|data| data.get_temp(widget_accent_id()))
        .unwrap_or(visuals.selection.bg_fill)
}

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
                Self::render_text_content_auto_split(
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
            WindowContent::MultiAccount => {
                let data = app_core
                    .layout
                    .windows
                    .iter()
                    .find(|def| def.name() == window.name)
                    .and_then(|def| match def {
                        crate::config::WindowDef::MultiAccount { data, .. } => Some(data.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                Self::render_multiaccount_content(app_core, ui, &settings, &data);
                None
            }
            WindowContent::Containers => Self::render_containers_content(app_core, ui),
            WindowContent::BestiaryView => {
                Self::render_bestiary_content(app_core, ui, &tab.window_name)
            }
            WindowContent::MissingSpells => {
                Self::render_missing_spells_content(app_core, ui);
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
                    if let Some(link) = Self::render_text_content_auto_split(
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
            WindowContent::CreatureField => {
                Self::render_creature_field_content(app_core, ui, &tab.window_name)
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
    /// Extra vertical space a row must reserve beyond its own text, because
    /// a floated inline image overhangs it. Parallel to `heights`.
    ///
    /// This is a SEPARATE column on purpose. The render loop writes the
    /// measured galley height back into `heights` every frame
    /// (`text.rs`, "correct the cached estimate"), so any reservation folded
    /// into `heights` would be erased on the very next frame. Everything
    /// that consumes a row's vertical span — the visible-range scan, the
    /// spacers, the drag hit-test, the trim pre-pass — must use
    /// [`Self::stride`] rather than `heights[i]` directly.
    extra: Vec<f32>,
    /// Per-row wrap width and paint offset, so measurement, painting, and
    /// the drag hit-test all lay a line out identically. Parallel to
    /// `heights`; a row with no float carries the full width and no shift.
    insets: Vec<LineInset>,
    /// Rows that ORIGINATE a float, and how many rows each one spans.
    /// Parallel to `heights`: `spans[i] > 0` means row `i` paints an image
    /// that overhangs the next `spans[i] - 1` rows.
    ///
    /// Virtualization needs this: when the viewport starts partway through a
    /// float the origin row is scrolled out, so without a lookback nothing
    /// would paint the image at all.
    spans: Vec<u16>,
    /// Bumped whenever float geometry changes (an image resolves, `rows`
    /// changes, the window's row count changes). Participates in cache
    /// invalidation: float heights depend on the window's own height, which
    /// the wrap-width test cannot see.
    float_epoch: u64,
}

impl RowHeightCache {
    /// Cached per-line heights, for characterization tests that pin the
    /// virtualization invariants (incremental append, full rebuild on width
    /// or font change, one entry per rendered line).
    #[cfg(test)]
    pub(super) fn heights(&self) -> &[f32] {
        &self.heights
    }

    /// Total vertical space row `i` occupies: its text height plus any
    /// reserved float overhang. The ONE way to ask "how tall is this row".
    pub(super) fn stride(&self, i: usize) -> f32 {
        self.heights.get(i).copied().unwrap_or(0.0)
            + self.extra.get(i).copied().unwrap_or(0.0)
    }

    /// Sum of strides over a range, the shape every offset computation needs
    /// (`spacing_y` is added per row by the caller).
    pub(super) fn stride_sum(&self, range: std::ops::Range<usize>, spacing_y: f32) -> f32 {
        range
            .filter(|i| *i < self.heights.len())
            .map(|i| self.stride(i) + spacing_y)
            .sum()
    }

    /// Reserve `extra` pixels below row `i` for a float that overhangs it.
    pub(super) fn set_extra(&mut self, i: usize, extra: f32) {
        if i < self.extra.len() {
            self.extra[i] = extra;
        }
    }

    #[cfg(test)]
    pub(super) fn extra(&self) -> &[f32] {
        &self.extra
    }

    /// The first row at or before `from` that originates a float covering
    /// `from`, or `from` itself when no float reaches it.
    ///
    /// The scan is bounded by the longest span the cache holds, so it stays
    /// O(max_span) rather than walking to the top of the buffer.
    pub(super) fn float_origin_at(&self, from: usize) -> usize {
        let reach = self.spans.iter().copied().max().unwrap_or(0) as usize;
        let lowest = from.saturating_sub(reach.saturating_sub(1));
        for i in (lowest..=from.min(self.spans.len().saturating_sub(1))).rev() {
            let span = self.spans.get(i).copied().unwrap_or(0) as usize;
            if span > 0 && i + span > from {
                return i;
            }
        }
        from
    }

    /// Record that row `i` originates a float spanning `span` rows.
    pub(super) fn set_span(&mut self, i: usize, span: u16) {
        if i < self.spans.len() {
            self.spans[i] = span;
        }
    }

    #[cfg(test)]
    pub(super) fn spans(&self) -> &[u16] {
        &self.spans
    }

    /// The layout this row was measured with — the ONE source painting and
    /// hit-testing must reuse, or their galleys disagree and selection lands
    /// on the wrong character.
    pub(super) fn inset(&self, i: usize, fallback: f32) -> LineInset {
        self.insets
            .get(i)
            .copied()
            .unwrap_or_else(|| LineInset::full(fallback))
    }

    #[cfg(test)]
    pub(super) fn set_float_epoch(&mut self, epoch: u64) {
        self.float_epoch = epoch;
    }
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
mod tests;
