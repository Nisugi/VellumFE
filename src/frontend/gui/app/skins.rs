//! Skin and appearance operations: activating/compiling/listing skins,
//! painting skinned title bars, borders and edges, and per-window accent
//! color resolution.

use super::*;

impl VellumGuiApp {
    /// Set the active skin in the layout (its home — checkpoints carry it)
    /// and mirror it into config for the web doll endpoint and the
    /// non-GUI frontends.
    pub(super) fn set_active_skin(&mut self, skin: Option<String>) {
        self.ui_settings.active_skin = skin.clone();
        self.layout_dirty = true;
        if self.app_core.config.active_skin != skin {
            self.app_core.config.active_skin = skin;
            self.save_config_after_skin_change();
        }
    }

    /// Bake the current live appearance — doll, compass set, status icon
    /// art, pool frames in use, per-window backgrounds — into
    /// `global/skins/<name>/skin.toml`, referencing pool paths (the image
    /// resolver falls back to the pool, so nothing is copied). The live
    /// state doesn't change: skins are a publish format, not a
    /// prerequisite. Sheet-cell icon overrides can't be expressed in a
    /// skin manifest and stay as layout overrides.
    pub(super) fn compile_appearance_to_skin(&self, name: &str) -> anyhow::Result<()> {
        use toml_edit::{value, Array, DocumentMut, Item, Table};

        let mut doc = DocumentMut::new();
        let mut meta = Table::new();
        meta.insert("name", value(name));
        meta.insert(
            "description",
            value("Compiled from the live appearance (.saveskin)"),
        );
        doc.insert("meta", Item::Table(meta));

        // Status icons: the active pool set, then Image overrides on top.
        let mut icon_entries: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        if let Some(set) = &self.ui_settings.status_icons.set {
            icon_entries.extend(crate::config::pool::set_members("statusicons", set));
        }
        for (id, icon) in &self.ui_settings.status_icons.overrides {
            if let crate::data::IconRef::Image { path } = icon {
                icon_entries.insert(id.to_ascii_lowercase(), path.clone());
            }
        }
        if !icon_entries.is_empty() {
            let mut icons = Table::new();
            for (id, path) in &icon_entries {
                icons.insert(id, value(path));
            }
            doc.insert("icons", Item::Table(icons));
        }

        // Compass set (only meaningful with a rose).
        if let Some(set) = &self.ui_settings.compass_set {
            let entries: std::collections::BTreeMap<String, String> =
                crate::config::pool::set_members("compass", set)
                    .into_iter()
                    .collect();
            if let Some(rose) = entries.get("rose").cloned() {
                let mut compass = Table::new();
                compass.insert("rose", value(rose));
                for (role, path) in &entries {
                    if role != "rose" {
                        compass.insert(role, value(path));
                    }
                }
                doc.insert("compass", Item::Table(compass));
            }
        }

        // Injury doll: pool image + its sidecar calibration.
        if let Some(image) = &self.ui_settings.doll_image {
            let mut doll = Table::new();
            doll.insert("base", value(image));
            doc.insert("injury_doll", Item::Table(doll));
            let abs = crate::config::Config::global_images_dir()?.join(image);
            if let Some(sidecar) =
                crate::config::pool::read_sidecar::<crate::config::pool::DollSidecar>(&abs)
            {
                let rounded = |v: f32, places: f64| (v as f64 * places).round() / places;
                let mut anchors = Table::new();
                let mut keys: Vec<&String> = sidecar.anchors.keys().collect();
                keys.sort();
                for key in keys {
                    let [x, y] = sidecar.anchors[key];
                    let mut pair = Array::new();
                    pair.push(rounded(x, 10_000.0));
                    pair.push(rounded(y, 10_000.0));
                    anchors.insert(key, value(pair));
                }
                let mut dots = Table::new();
                dots.insert("wound_color", value(sidecar.dots.wound_color.as_str()));
                dots.insert("scar_color", value(sidecar.dots.scar_color.as_str()));
                dots.insert("opacity", value(rounded(sidecar.dots.opacity, 100.0)));
                dots.insert("diameter", value(rounded(sidecar.dots.diameter, 1_000.0)));
                let doll = doc["injury_doll"].as_table_mut().expect("just inserted");
                doll.insert("anchors", Item::Table(anchors));
                doll.insert("dots", Item::Table(dots));
            }
        }

        // Pool frames any window override references -> [frames.<stem>],
        // plus the global default frame (Settings > GUI).
        let mut wanted_frames: Vec<String> = self
            .tab_settings
            .values()
            .filter_map(|settings| settings.skin_frame.clone())
            .chain(self.ui_settings.default_frame.clone())
            .map(|frame| frame.to_ascii_lowercase())
            .filter(|frame| frame != "none")
            .collect();
        wanted_frames.sort();
        wanted_frames.dedup();
        if !wanted_frames.is_empty() {
            let mut frames = Table::new();
            frames.set_implicit(true);
            for image in crate::config::pool::list_category("frames") {
                let stem = image.stem().to_ascii_lowercase();
                if !wanted_frames.contains(&stem) {
                    continue;
                }
                let Some(sidecar) = crate::config::pool::read_sidecar::<
                    crate::config::pool::FrameSidecar,
                >(&image.abs_path) else {
                    continue;
                };
                let mut entry = Table::new();
                entry.insert("image", value(&image.pool_path));
                let mut slice = Array::new();
                for inset in sidecar.slice.insets() {
                    slice.push(inset as f64);
                }
                entry.insert("slice", value(slice));
                entry.insert("scale", value(sidecar.effective_scale() as f64));
                frames.insert(&stem, Item::Table(entry));
            }
            if !frames.is_empty() {
                doc.insert("frames", Item::Table(frames));
            }
        }

        // Per-window backgrounds -> [window.<name>.background]; the global
        // default background bakes as the skin's [window.default] entry
        // (the manifest-wide fallback window_field consults).
        let mut backgrounds: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        if let Some(background) = &self.ui_settings.default_background {
            if !background.eq_ignore_ascii_case("none") {
                backgrounds.insert("default".to_string(), background.clone());
            }
        }
        for (key, settings) in &self.tab_settings {
            let Some(background) = &settings.background_image else {
                continue;
            };
            if background.eq_ignore_ascii_case("none") {
                continue;
            }
            if let Some(tab) = self.available_tabs.get(key) {
                backgrounds.insert(tab.window_name.clone(), background.clone());
            }
        }
        if !backgrounds.is_empty() {
            let mut windows = Table::new();
            windows.set_implicit(true);
            for (window_name, path) in &backgrounds {
                let mut background = Table::new();
                background.insert("image", value(path));
                let mut per_window = Table::new();
                per_window.set_implicit(true);
                per_window.insert("background", Item::Table(background));
                windows.insert(window_name, Item::Table(per_window));
            }
            doc.insert("window", Item::Table(windows));
        }

        let root = crate::config::Config::skins_dir()?.join(name);
        std::fs::create_dir_all(&root)?;
        crate::config::write_atomic(&root.join("skin.toml"), doc.to_string())?;
        Ok(())
    }

    /// Set the injury doll override (pool-relative path), persisted in the
    /// layout and mirrored to config for the web doll endpoint. The doll
    /// switches next frame via `SkinState::apply_if_changed`.
    pub(super) fn set_doll_image(&mut self, image: Option<String>) {
        self.ui_settings.doll_image = image.clone();
        self.layout_dirty = true;
        if self.app_core.config.doll_image != image {
            self.app_core.config.doll_image = image;
            self.save_config_after_skin_change();
        }
    }

    /// Handle `action:setskin:<name>` from dot-commands or menus. "none"
    /// (or "off") disables the active skin. The switch itself happens next
    /// frame via `SkinState::apply_if_changed`.
    pub(super) fn apply_skin_by_name(&mut self, name: &str) {
        if name.eq_ignore_ascii_case("none") || name.eq_ignore_ascii_case("off") {
            self.set_active_skin(None);
            self.app_core.add_system_message("Skin disabled.");
            return;
        }
        match crate::config::skins::load_manifest(name) {
            Ok(_) => {
                self.set_active_skin(Some(name.to_string()));
                self.app_core
                    .add_system_message(&format!("Skin switched to: {}", name));
            }
            Err(err) => {
                let available = crate::config::skins::list_skins();
                if available.is_empty() {
                    self.app_core.add_system_message(&format!(
                        "Cannot load skin '{}': {}. No skins installed; create one under ~/.vellum-fe/global/skins/<name>/skin.toml",
                        name, err
                    ));
                } else {
                    self.app_core.add_system_message(&format!(
                        "Cannot load skin '{}': {}. Available: {}",
                        name,
                        err,
                        available.join(", ")
                    ));
                }
            }
        }
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
        let active = self.ui_settings.active_skin.clone();
        self.app_core.add_system_message("Installed skins:");
        for name in available {
            let marker = if active.as_deref() == Some(name.as_str()) {
                " (active)"
            } else {
                ""
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
                    "Edit skin.toml (sections are commented out), add images, then .setskin to activate.",
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
                    "Activate with .setskin {} (frames 'harmony' and 'harmony-accent' \
                     are also assignable per window).",
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
        // Caption color = the skin's titlebar_text (defaults to accent). A skin
        // whose title-bar art needs a specific text color (StormFront's silver
        // bar wants dark text, not the steel-blue accent) pins it via
        // [ui].titlebar_text; falls back to the theme text color when no palette.
        let caption_color = self
            .skin_state
            .widget_art()
            .and_then(|art| art.ui_palette.as_ref().map(|pal| pal.titlebar_text))
            .unwrap_or_else(|| visuals.text_color());
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
    /// accent (context menu), else the shared layout definition's
    /// border_color — so a border color set in the window editor or the
    /// TUI finally shows here too — else the theme frame (None).
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
        if let Some(color) = match base.border_color.as_deref() {
            None | Some("-") | Some("") => None,
            Some(color) => widgets::parse_hex_color(color),
        } {
            return Some(color);
        }
        // colors.toml ui.border_color, only when actually changed from the
        // built-in default (extracted defaults fall through to the theme).
        self.app_core
            .config
            .colors
            .ui
            .user_border_color()
            .and_then(widgets::parse_hex_color)
    }
}
