//! Skin and appearance operations: activating/compiling/listing skins,
//! painting skinned title bars, borders and edges, and per-window accent
//! color resolution.

use super::*;

impl VellumGuiApp {
    /// Sync the canonical appearance store from the layout's live look and
    /// persist when anything changed. The explicit setters call this for
    /// immediacy (core reads the in-memory store live) and the layout
    /// save/load funnels call it as the catch-all — every appearance
    /// mutation marks the layout dirty, so nothing escapes.
    pub(super) fn sync_appearance_from_ui_settings(&mut self) {
        let ui = &self.ui_settings;
        let next = crate::config::appearance::AppearanceSettings {
            // Legacy migration input only — presets are the look now, so
            // the store never carries a live skin.
            active_skin: None,
            doll_image: ui.doll_image.clone(),
            compass_set: ui.compass_set.clone(),
            default_frame: ui.default_frame.clone(),
            default_background: ui.default_background.clone(),
            doll_grayscale: ui.doll_grayscale,
            hand_icon_size: ui.hand_icon_size,
            status_icons: ui.status_icons.clone(),
            control_frames: ui.control_frames.clone(),
            edge_set: ui.edge_set.clone(),
        };
        if self.app_core.config.appearance != next {
            self.app_core.config.appearance = next;
            self.save_appearance();
        }
    }

    /// The reverse funnel: adopt the canonical appearance store into the
    /// layout's live look. Used when CORE wrote the store (skin-pack
    /// install/import via `appearance_changed_externally`) — without this
    /// the next layout save would write the stale ui_settings back over
    /// the store's new values.
    pub(super) fn sync_ui_settings_from_appearance(&mut self) {
        let a = self.app_core.config.appearance.clone();
        // a.active_skin is a legacy migration input; never mirrored back.
        self.ui_settings.active_skin = None;
        self.ui_settings.doll_image = a.doll_image;
        self.ui_settings.compass_set = a.compass_set;
        self.ui_settings.default_frame = a.default_frame;
        self.ui_settings.default_background = a.default_background;
        self.ui_settings.doll_grayscale = a.doll_grayscale;
        self.ui_settings.hand_icon_size = a.hand_icon_size;
        self.ui_settings.status_icons = a.status_icons;
        self.ui_settings.control_frames = a.control_frames;
        self.ui_settings.edge_set = a.edge_set;
        self.layout_dirty = true;
    }

    /// Persist the appearance store after a change. Core reads the
    /// in-memory copy live; the file is what survives a restart. The base
    /// (characterless) copy is written too, so characterless consumers —
    /// the web doll endpoint loads config with no character — follow the
    /// most recently set look instead of a stale global mirror.
    pub(super) fn save_appearance(&mut self) {
        let character = self.app_core.config.character.clone();
        let appearance = self.app_core.config.appearance.clone();
        if let Err(err) = appearance.save(character.as_deref()) {
            self.app_core
                .add_system_message(&format!("Appearance not saved: {err:#}"));
        } else if character.is_some() {
            if let Err(err) = appearance.save(None) {
                tracing::warn!("base appearance.toml not updated: {err:#}");
            }
        }
    }

    /// `.saveskin <name>`: snapshot the current appearance as an inert
    /// preset under `global/skins/<name>/` — re-appliable with .setskin,
    /// shareable with .exportskin. Per-window overrides live in the
    /// layout, not the appearance store, so they are not part of a preset.
    pub(super) fn compile_appearance_to_skin(&self, name: &str) -> anyhow::Result<()> {
        let manifest = crate::config::skin_pack::SkinPackManifest {
            format: crate::config::skin_pack::FORMAT_VERSION,
            meta: crate::config::skin_pack::PackMeta {
                name: name.to_string(),
                description: Some("Saved from the live appearance (.saveskin)".to_string()),
                author: None,
            },
            assignments: crate::config::skin_pack::Assignments::from_appearance(
                &self.app_core.config.appearance,
            ),
        };
        crate::config::skin_pack::write_preset(name, &manifest)
    }

    /// Set the injury doll override (pool-relative path): layout copy for
    /// checkpoints, appearance store for core/web. The doll switches next
    /// frame via `SkinState::apply_if_changed`.
    pub(super) fn set_doll_image(&mut self, image: Option<String>) {
        self.ui_settings.doll_image = image;
        self.layout_dirty = true;
        self.sync_appearance_from_ui_settings();
    }

    /// Handle `action:setskin:<name>` from dot-commands or menus. "none"
    /// (or "off") disables the active skin. The switch itself happens next
    /// frame via `SkinState::apply_if_changed`.
    pub(super) fn apply_skin_by_name(&mut self, name: &str) {
        if name.eq_ignore_ascii_case("none") || name.eq_ignore_ascii_case("off") {
            // Clear every appearance assignment slot back to defaults.
            self.ui_settings.active_skin = None;
            self.ui_settings.doll_image = None;
            self.ui_settings.compass_set = None;
            self.ui_settings.default_frame = None;
            self.ui_settings.default_background = None;
            self.ui_settings.edge_set = None;
            self.ui_settings.doll_grayscale = false;
            self.ui_settings.status_icons.set = None;
            self.ui_settings.control_frames.clear();
            self.layout_dirty = true;
            self.sync_appearance_from_ui_settings();
            self.app_core
                .add_system_message("Appearance reset to plain theme.");
            return;
        }
        // New-format presets (jinx skin packs, .importskin): the manifest
        // holds appearance assignments, not live art references — apply
        // them to the store and stay skinless.
        if let Some(preset) = crate::config::skin_pack::load_preset(name) {
            self.apply_skin_preset(name, &preset.assignments);
            return;
        }
        // A legacy live-manifest skin: there is no live runtime anymore.
        // One-shot conversion — migrate the manifest + its art into the
        // pool, persist the preset over the legacy dir's name, apply it.
        match self.migrate_legacy_skin_to_preset(name) {
            Ok(assignments) => {
                self.apply_skin_preset(name, &assignments);
            }
            Err(err) => {
                let available = crate::config::skins::list_skins();
                if available.is_empty() {
                    self.app_core.add_system_message(&format!(
                        "Cannot load skin '{}': {:#}. No skins installed; install one with .jinx or .importskin",
                        name, err
                    ));
                } else {
                    self.app_core.add_system_message(&format!(
                        "Cannot load skin '{}': {:#}. Available: {}",
                        name,
                        err,
                        available.join(", ")
                    ));
                }
            }
        }
    }

    /// Convert a legacy skin into a preset in place: art to the pool
    /// (collision-safe), the rename-adjusted manifest written over the
    /// legacy skin.toml (the art files in the skin dir stay — they're the
    /// user's, and now inert). Reports what could not be expressed.
    fn migrate_legacy_skin_to_preset(
        &mut self,
        name: &str,
    ) -> anyhow::Result<crate::config::skin_pack::Assignments> {
        let (pack, warnings) = crate::config::skin_pack::migrate_legacy(name)?;
        let report = crate::config::skin_pack::install_files(&pack)?;
        let mut manifest = pack.manifest.clone();
        manifest.assignments = report.assignments.clone();
        // The preset overwrites the legacy skin.toml (same path, so the
        // name keeps working everywhere); the authored original is kept
        // beside it in case the user wants to salvage unmapped sections.
        if let Ok(dir) = crate::config::Config::skins_dir() {
            let original = dir.join(name).join("skin.toml");
            let backup = dir.join(name).join("skin-legacy.toml.bak");
            if original.is_file() && !backup.exists() {
                let _ = std::fs::copy(&original, &backup);
            }
        }
        crate::config::skin_pack::write_preset(name, &manifest)?;
        self.app_core.add_system_message(&format!(
            "Legacy skin '{}' converted to a preset ({} file(s) moved to the image pool).",
            name,
            report.installed.len() + report.renamed.len()
        ));
        for warning in warnings.iter().chain(&report.warnings) {
            self.app_core
                .add_system_message(&format!("  note: {warning}"));
        }
        Ok(report.assignments)
    }

    /// Apply a preset's assignments to the live look: ui_settings first
    /// (the layout copy), then the one-way funnel persists the store.
    fn apply_skin_preset(&mut self, name: &str, a: &crate::config::skin_pack::Assignments) {
        self.ui_settings.active_skin = None;
        self.ui_settings.doll_image = a.doll_image.clone();
        self.ui_settings.compass_set = a.compass_set.clone();
        self.ui_settings.default_frame = a.default_frame.clone();
        self.ui_settings.default_background = a.default_background.clone();
        self.ui_settings.edge_set = a.edge_set.clone();
        self.ui_settings.doll_grayscale = a.doll_grayscale;
        if let Some(size) = a.hand_icon_size {
            self.ui_settings.hand_icon_size = size;
        }
        self.ui_settings.status_icons.set = a.status_icon_set.clone();
        self.ui_settings.status_icons.gray_inactive = a.status_gray_inactive;
        self.ui_settings.control_frames = a.control_frames.clone();
        self.layout_dirty = true;
        self.sync_appearance_from_ui_settings();
        self.app_core
            .add_system_message(&format!("Skin preset '{name}' applied."));
    }

    /// Handle `action:skins`: list installed skins in the main window.
    pub(super) fn list_skins_to_window(&mut self) {
        let available = crate::config::skins::list_skins();
        if available.is_empty() {
            self.app_core.add_system_message(
                "No skins installed. Create one under ~/.vellum-fe/global/skins/<name>/skin.toml",
            );
            return;
        }
        self.app_core.add_system_message("Installed skins:");
        for name in available {
            let marker = if crate::config::skin_pack::load_preset(&name).is_some() {
                " (preset)"
            } else {
                " (legacy - migrates on .setskin)"
            };
            self.app_core
                .add_system_message(&format!("  {}{}", name, marker));
        }
        self.app_core
            .add_system_message("Use .setskin <name> to activate, .setskin none to disable.");
    }

    /// Handle `action:makeskin:<name>`: write the starter skin and tell the
    /// user how to proceed. Does not activate it — a fresh scaffold is all
    /// comments, so activating it would visibly do nothing.
    pub(super) fn make_skin_scaffold(&mut self, name: &str) {
        match crate::config::skins::write_scaffold(name) {
            Ok(path) => {
                self.app_core.add_system_message(&format!(
                    "Created skin '{}' at {}",
                    name,
                    path.display()
                ));
                self.app_core.add_system_message(
                    "Edit skin.toml (sections are commented out), add images, then .setskin to \
                     apply it — the skin is converted to a preset (art moves to the image pool).",
                );
            }
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Cannot create skin '{}': {}", name, err));
            }
        }
    }

    /// Handle `action:harmonyskin:<name>` (`.harmony skin <name>`): render
    /// the panel + frame images from the current harmony recipe and write
    /// the skin. Uses default texture/frame settings; the Colors editor's
    /// Generate tab offers the tunable version.
    pub(super) fn write_harmony_skin_default(&mut self, name: &str) {
        use crate::core::harmony_skin::{FrameSpec, PanelSpec, SkinColors};
        let params = self.app_core.harmony_params();
        let panel = PanelSpec::default();
        let frame = FrameSpec::default();
        let colors = SkinColors::derive(&params.background, &params.seed, panel.fade_depth);
        self.write_harmony_skin_files(name, &params, &colors, &panel, &frame);
    }

    /// Shared writer for both the action handler and the Generate tab:
    /// renders the four images, builds the manifest, writes the skin
    /// directory, and reports.
    pub(in crate::frontend::gui) fn write_harmony_skin_files(
        &mut self,
        name: &str,
        params: &crate::core::harmony::HarmonyParams,
        colors: &crate::core::harmony_skin::SkinColors,
        panel: &crate::core::harmony_skin::PanelSpec,
        frame: &crate::core::harmony_skin::FrameSpec,
    ) {
        let images = crate::core::harmony_skin::render_skin_assets(colors, panel, frame);
        let manifest = crate::config::skins::harmony_skin_manifest(
            name.trim(),
            params.scheme.name(),
            &params.seed,
            &colors.panel_top,
            &colors.panel_bottom,
            &colors.line,
            &colors.accent,
            frame.slice,
        );
        match crate::config::skins::write_harmony_skin(name, &manifest, &images) {
            Ok(path) => {
                self.app_core.add_system_message(&format!(
                    "Harmony skin '{}' written to {}",
                    name.trim(),
                    path.display()
                ));
                self.app_core.add_system_message(&format!(
                    "Apply with .setskin {} — it converts to a preset (frames 'harmony' and \
                     'harmony-accent' are also assignable per window).",
                    name.trim()
                ));
            }
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Cannot write harmony skin: {}", err));
            }
        }
    }

    pub(super) fn save_config_after_skin_change(&mut self) {
        if let Err(err) = self
            .app_core
            .config
            .save(self.app_core.config.character.as_deref())
        {
            tracing::warn!("Failed to save config after skin switch: {}", err);
        }
    }

    /// Adjust a docked window's frame when the active skin draws this
    /// window's border: drop the stroke (the nine-slice replaces it) and
    /// widen the inner margin so content clears the border art.
    /// Which sides of the skin's nine-slice frame draw for this window,
    /// as [top, right, bottom, left]. The layout def's border settings
    /// drive it — Border off (or style "none") hides the whole frame,
    /// per-side toggles hide individual rails (their corners collapse and
    /// the surviving rails extend to the window edge). Windows without a
    /// layout def draw all four.
    pub(super) fn skin_border_sides_for_tab(&self, key: &TabKey) -> [bool; 4] {
        let Some(def) = self.layout_def_for_tab(key) else {
            return [true; 4];
        };
        let base = def.base();
        if !base.show_border || base.border_style.eq_ignore_ascii_case("none") {
            return [false; 4];
        }
        let sides = &base.border_sides;
        [sides.top, sides.right, sides.bottom, sides.left]
    }

    /// The skin border this tab draws, honoring the per-window frame
    /// override (Appearance > Skin frame) stored in its tab settings,
    /// then the global default frame (Settings > GUI).
    pub(super) fn skin_border_for_tab(&self, key: &TabKey) -> Option<skin::ResolvedBorder> {
        let tab = self.available_tabs.get(key)?;
        let frame_override = self
            .tab_settings
            .get(key)
            .and_then(|settings| settings.skin_frame.as_deref())
            .or(self.ui_settings.default_frame.as_deref());
        let mut border = self
            .skin_state
            .border_for_with_override(&tab.window_name, frame_override)?;
        // Per-window live scale multiplier (window editor slider). Both the
        // painted thickness and the content inset derive from border.scale,
        // so scaling it here keeps the frame and its padding in lockstep.
        if let Some(mult) = self
            .tab_settings
            .get(key)
            .and_then(|settings| settings.frame_scale)
        {
            border.scale = (border.scale * mult.max(0.05)).clamp(0.05, 8.0);
        }
        Some(border)
    }

    /// The skin's title-bar nine-slice for a window, if it authored one
    /// (`[controls.titlebar]`). When present the window takes over its own
    /// title bar: egui's is hidden and we paint the sprite band + caption +
    /// close button ourselves.
    pub(super) fn skin_titlebar_for_tab(&self, key: &TabKey) -> Option<skin::ResolvedBorder> {
        // Only game windows with a caption get a skinned title bar.
        self.available_tabs.get(key)?;
        self.skin_state
            .widget_art()
            .and_then(|art| art.control_border("titlebar", "normal").cloned())
    }

    pub(super) fn apply_skin_border_to_frame(
        &self,
        key: &TabKey,
        sides: [bool; 4],
        frame: &mut egui::Frame,
    ) {
        let Some(border) = self.skin_border_for_tab(key) else {
            return;
        };
        if sides == [false; 4] {
            return;
        }
        frame.stroke = egui::Stroke::NONE;
        // Square corners whenever skin art frames the window: a rounded
        // background fill would show through (or clip) the art's corners.
        frame.corner_radius = egui::CornerRadius::ZERO;
        let side = |inset: f32| (inset * border.scale).ceil().clamp(0.0, 127.0) as i8;
        let margin = &mut frame.inner_margin;
        if sides[0] {
            margin.top = margin.top.max(side(border.slice[0]));
        }
        if sides[1] {
            margin.right = margin.right.max(side(border.slice[1]));
        }
        if sides[2] {
            margin.bottom = margin.bottom.max(side(border.slice[2]));
        }
        if sides[3] {
            margin.left = margin.left.max(side(border.slice[3]));
        }
    }

    /// Height of a window's skinned title band: the title-bar art's own
    /// height (scaled) so the sprite renders at its authored thickness, unless
    /// the user set an explicit per-window title_bar_height override. Shared
    /// by the content-inset reservation and the paint so they can't drift.
    pub(super) fn skin_titlebar_height(
        &self,
        key: &TabKey,
        titlebar: &skin::ResolvedBorder,
    ) -> f32 {
        let art_height = (titlebar.tex_size.y * titlebar.scale).max(TITLE_BAR_MIN_HEIGHT);
        self.tab_settings
            .get(key)
            .and_then(|s| s.title_bar_height)
            .filter(|h| *h > 0.0)
            .unwrap_or(art_height)
            .clamp(TITLE_BAR_MIN_HEIGHT, TITLE_BAR_MAX_HEIGHT)
    }

    /// Paint a skin's title bar over the top of a rendered window: the
    /// nine-slice band and the caption. There is no skinned close button —
    /// windows hide via the Windows menu / right-click, like the layout
    /// widgets. Runs on the window's own layer so it moves and stacks with
    /// the window.
    pub(super) fn paint_skin_titlebar(
        &self,
        ctx: &egui::Context,
        key: &TabKey,
        window_name: &str,
        titlebar: &skin::ResolvedBorder,
        response: &egui::Response,
    ) {
        let height = self.skin_titlebar_height(key, titlebar);
        let layout = zones::titlebar_layout(response.rect, height);
        let painter = ctx.layer_painter(response.layer_id);

        // Fill the band region with the window's own mesh BEFORE the sprite, so
        // the sprite's transparent cutout notch reveals mesh (not bare panel
        // fill) — the Wrayth look. The content mesh only reaches the content
        // rect (below the reserved title inset), so without this the band area
        // above it has no mesh to show through. Painted at the sprite's own UV
        // scale via background_shapes, clipped to the band.
        if let Some(background) = self.widget_render_settings(key).background {
            let scrim = ctx.global_style().visuals.window_fill();
            let shapes = skin::background_shapes(layout.bar, &background, scrim);
            painter
                .with_clip_rect(layout.bar)
                .add(egui::Shape::Vec(shapes));
        }

        // Band sprite over the mesh: its transparent notch lets the mesh show
        // through, and its opaque edge carries the grey line under the bar.
        //
        // A custom title-bar height rescales the band art UNIFORMLY (the
        // same idea as the Frame size slider): the sprite's scale is derived
        // so its authored height lands exactly on the band height, which
        // keeps the notch and end caps in proportion at any height. Without
        // this the nine-slice stretched (or squashed) only the sprite's
        // vertical middle, distorting the notch — and cropping the sprite
        // instead (the earlier attempt) cut art off. With no custom height
        // the derived scale equals the authored scale, so default bars are
        // untouched.
        let band = skin::ResolvedBorder {
            scale: if titlebar.tex_size.y > 0.0 {
                height / titlebar.tex_size.y
            } else {
                titlebar.scale
            },
            ..titlebar.clone()
        };
        skin::paint_nine_slice(&painter, layout.bar, &band, [true; 4]);

        // Caption, left-aligned within its area, vertically centered.
        let caption = match self.available_tabs.get(key).cloned() {
            Some(tab) => self.window_display_title(&tab),
            None => window_name.to_string(),
        };
        let visuals = ctx.global_style().visuals.clone();
        // Caption color follows the theme text color.
        let caption_color = visuals.text_color();
        if !caption.is_empty() {
            // Sit the caption in the SOLID top band of the title-bar art, not
            // the full-bar center — these sprites carry a mesh-notch / stretch
            // region in their lower half, so a vertically-centered caption lands
            // over it. Anchor near the top; a smaller font fits the solid strip.
            let font_size = (height * 0.42).clamp(8.0, 13.0);
            let baseline_y = layout.caption.top() + font_size * 0.5 + 2.0;
            painter.text(
                egui::pos2(layout.caption.left() + 4.0, baseline_y),
                egui::Align2::LEFT_CENTER,
                caption,
                egui::FontId::proportional(font_size),
                caption_color,
            );
        }
    }

    /// Paint the skin's nine-slice border over a rendered window, on the
    /// window's own layer so it moves and stacks with the window.
    pub(super) fn paint_skin_border(
        &self,
        ctx: &egui::Context,
        key: &TabKey,
        sides: [bool; 4],
        response: &egui::Response,
    ) {
        if sides == [false; 4] {
            return;
        }
        if let Some(border) = self.skin_border_for_tab(key) {
            skin::paint_nine_slice(
                &ctx.layer_painter(response.layer_id),
                response.rect,
                &border,
                sides,
            );
        }
    }

    /// Paint the active skin's decorative edge overlays (strip + corner
    /// ornament per edge) over the window's border, on the window's layer.
    pub(super) fn paint_skin_edges(
        &self,
        ctx: &egui::Context,
        response: &egui::Response,
        top_inset: f32,
        show_ornament: bool,
    ) {
        let Some(art) = self.skin_state.widget_art() else {
            return;
        };
        if !art.has_edges() {
            return;
        }
        let painter = ctx.layer_painter(response.layer_id);
        for edge_name in ["top", "right", "bottom", "left"] {
            if let Some(edge) = art.edge(edge_name) {
                // Suppress the corner ornament when there's no title bar — its
                // whole point is to bridge the title bar into the body, and with
                // no bar it just floats at the top corner over content.
                let edge = if show_ornament {
                    *edge
                } else {
                    skin::ResolvedEdge {
                        ornament: None,
                        ..*edge
                    }
                };
                skin::paint_edge_overlay(&painter, response.rect, edge_name, &edge, top_inset);
            }
        }
    }

    /// Per-window position/size lock: locked windows ignore drag and
    /// resize gestures in every zone; the deliberate Arrange ▸ Move Window
    /// menu action still works. THE flag is the shared layout's
    /// `WindowBase::locked` — the same one `.lockwindows`,
    /// `.lockwindow <name>`, and the TUI write — so global and per-window
    /// locks are one system across both frontends.
    pub(super) fn window_locked(&self, key: &TabKey) -> bool {
        self.available_tabs.get(key).is_some_and(|tab| {
            self.app_core
                .layout
                .windows
                .iter()
                .find(|window| window.name() == tab.window_name)
                .is_some_and(|window| window.base().locked)
        })
    }

    /// Per-window frame corner radius override (context menu); None follows
    /// the global `ui_settings.window_corner_radius` already baked into the
    /// window frame style.
    pub(super) fn corner_radius_override_for_tab(&self, key: &TabKey) -> Option<f32> {
        self.tab_settings
            .get(key)
            .and_then(|settings| settings.corner_radius)
    }

    /// Effective title bar height for a game window: per-window override,
    /// else the global setting. 0 means "auto" in both layers; None =
    /// derive from the title font (egui's default behavior).
    ///
    /// A skin frame paints its top slice over the window top, which includes
    /// the title band — so when a frame is active the title bar is grown to
    /// at least that top thickness, keeping the caption clear of the art
    /// instead of tucked underneath it.
    pub(super) fn title_bar_height_for_tab(&self, key: &TabKey) -> Option<f32> {
        let configured = self
            .tab_settings
            .get(key)
            .and_then(|settings| settings.title_bar_height)
            .unwrap_or(self.ui_settings.title_bar_height);
        let frame_top = self
            .skin_border_for_tab(key)
            .map(|border| border.slice[0] * border.scale)
            .unwrap_or(0.0);
        // Auto (0) with a frame present: derive from the frame top. Explicit
        // height: never let it fall below the frame's top thickness.
        let height = if configured > 0.0 {
            configured.max(frame_top)
        } else {
            frame_top
        };
        (height > 0.0).then(|| height.clamp(TITLE_BAR_MIN_HEIGHT, TITLE_BAR_MAX_HEIGHT))
    }

    /// Effective title text alignment for a game window.
    pub(super) fn title_align_for_tab(&self, key: &TabKey) -> egui::Align {
        let align = self
            .tab_settings
            .get(key)
            .and_then(|settings| settings.title_bar_align.as_deref())
            .unwrap_or(&self.ui_settings.title_bar_align);
        match align {
            "left" => egui::Align::Min,
            "right" => egui::Align::Max,
            _ => egui::Align::Center,
        }
    }

    /// Apply the resolved title bar height and alignment to a game-window
    /// builder. Editor and dialog windows keep egui's standard chrome.
    pub(super) fn style_window_title_bar<'a>(
        &self,
        key: &TabKey,
        mut window: egui::Window<'a>,
    ) -> egui::Window<'a> {
        window = window.title_align(self.title_align_for_tab(key));
        if let Some(height) = self.title_bar_height_for_tab(key) {
            window = window.title_bar_height(height);
        }
        window
    }

    /// Accent (border) color for a window. Precedence: the per-window GUI
    /// accent (context menu, a GUI-only layer the shared resolver doesn't
    /// know about), else the def/user-ui layers of the shared
    /// `window_style::resolve_window_colors` precedence (window def ->
    /// colors.toml ui.border_color the user actually changed) — so a border
    /// color OR NAME set in the window editor or the TUI finally shows here
    /// too — else the theme frame (None; left to the skin/theme's own frame
    /// stroke instead of a computed fallback, since the border-plan/skin
    /// layers painted after this one already handle "nothing set").
    pub(super) fn accent_color_for_tab(&self, key: &TabKey) -> Option<Color32> {
        if let Some(accent) = self
            .tab_settings
            .get(key)
            .and_then(|settings| settings.accent_color.as_deref())
            .and_then(widgets::parse_hex_color)
        {
            return Some(accent);
        }
        let window_name = &self.available_tabs.get(key)?.window_name;
        let base = self
            .app_core
            .layout
            .windows
            .iter()
            .find(|window| window.name() == *window_name)?
            .base();
        if let Some(color) = crate::core::window_style::normalize_color(
            &base.border_color,
            crate::frontend::common::color::parse_color_flexible,
        )
        .as_deref()
        .and_then(widgets::parse_hex_color)
        {
            return Some(color);
        }
        // colors.toml ui.border_color, only when actually changed from the
        // built-in default (extracted defaults fall through to the theme).
        self.app_core
            .config
            .colors
            .ui
            .user_border_color()
            .and_then(crate::frontend::common::color::parse_color_flexible)
            .as_deref()
            .and_then(widgets::parse_hex_color)
    }
}
