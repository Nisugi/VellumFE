//! GUI skin rendering: user-supplied graphics layered on top of themes.
//!
//! The manifest format, loading, and the canonical injury doll part table
//! live in `crate::config::skins` (shared with the web frontend, which
//! compiles without egui). This module owns everything egui: texture
//! loading, the per-skin runtime state, widget sprite lookups, the paint
//! helpers, and the calibrator's comment-preserving skin.toml save.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::skins::{
    self, BackgroundFit, DollDotSpec, InjuryDollSkin, SheetSpec, SkinManifest,
};

/// Everything a renderer needs to paint one window background. Resolved
/// once per frame from the loaded skin, then handed to render paths (some
/// of which run in detached viewports without access to the app).
#[derive(Debug, Clone)]
pub struct ResolvedBackground {
    pub texture: egui::TextureId,
    pub tex_size: egui::Vec2,
    pub fit: BackgroundFit,
    /// Multiply tint with opacity premixed into alpha.
    pub tint: egui::Color32,
    /// Scrim opacity as 0..=255 alpha; the paint call supplies the color.
    pub scrim_alpha: u8,
}

/// One loaded skin texture: id plus native size.
#[derive(Debug, Clone, Copy)]
pub struct SkinTexture {
    pub texture: egui::TextureId,
    pub size: egui::Vec2,
}

/// One icon lookup as rendering needs it: a texture region (full image or
/// a sheet cell) plus its source-pixel size for aspect fitting.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedIcon {
    pub texture: egui::TextureId,
    /// Source-pixel size of the drawn region (aspect fitting).
    pub size: egui::Vec2,
    pub uv: egui::Rect,
}

/// How one indicator id's icon resolves: a standalone sprite, or a sheet
/// cell looked up at call time (so it tracks sheet hot-reloads).
#[derive(Debug, Clone)]
enum IconSlot {
    Sprite(SkinTexture),
    Sheet { sheet: String, cell: u32 },
}

/// Widget sprite art resolved from the active skin. Shared into
/// `WidgetRenderSettings` behind an Arc so every render path (including
/// detached viewports) reads the same lookup tables.
#[derive(Debug, Default)]
pub struct SkinWidgetArt {
    /// Indicator id (stored UPPERCASE) -> icon slot (skin `[icons]`, pool
    /// set art, and per-indicator overrides, pre-merged at build).
    icons: HashMap<String, IconSlot>,
    /// Grayscale icon twins; populated only while "gray when inactive" is
    /// on (lazy — no setting, no twins).
    icons_gray: HashMap<String, IconSlot>,
    /// Pool images referenced by hand-widget icon states, keyed by
    /// lowercase pool-relative path.
    pool_icons: HashMap<String, SkinTexture>,
    /// Grayscale doll art; populated only while "grayscale doll" is on.
    pub doll_base_gray: Option<SkinTexture>,
    doll_parts_gray: HashMap<String, HashMap<u8, SkinTexture>>,
    pub compass_rose: Option<SkinTexture>,
    /// Direction key (lowercase "n".."nw", "up", ...) -> lit overlay.
    compass_dirs: HashMap<String, SkinTexture>,
    pub doll_base: Option<SkinTexture>,
    /// Body part (lowercase) -> severity level (1-6) -> overlay.
    doll_parts: HashMap<String, HashMap<u8, SkinTexture>>,
    /// Body part (lowercase) -> calibrated dot anchor as fractions (0-1)
    /// of the doll image.
    doll_anchors: HashMap<String, egui::Vec2>,
    /// Generated-dot styling resolved from the manifest.
    pub doll_dots: ResolvedDotStyle,
    /// Hotbar icon sprite sheets keyed by lowercased sheet name.
    sheets: HashMap<String, SheetArt>,
    /// Nine-slice art for interactive dialog-panel controls, keyed by
    /// lowercase `"<control>"` or `"<control>.<state>"` (e.g. "button",
    /// "button.hover", "dropdown").
    controls: HashMap<String, ResolvedBorder>,
}

/// One loaded hotbar sprite sheet: the texture, its lazy-built grayscale
/// twin, and the cell edge for UV slicing.
#[derive(Debug, Clone, Copy)]
struct SheetArt {
    texture: SkinTexture,
    gray: Option<SkinTexture>,
    cell: u32,
}

/// Dot styling with colors parsed, ready for the painter.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedDotStyle {
    pub wound: egui::Color32,
    pub scar: egui::Color32,
    pub opacity: f32,
    /// Diameter as a fraction of the drawn doll height.
    pub diameter: f32,
}

impl Default for ResolvedDotStyle {
    fn default() -> Self {
        Self::from_spec(&DollDotSpec::default())
    }
}

impl ResolvedDotStyle {
    pub fn from_spec(spec: &DollDotSpec) -> Self {
        Self {
            wound: parse_hex_rgb(&spec.wound_color)
                .unwrap_or(egui::Color32::from_rgb(0xe0, 0x20, 0x20)),
            scar: parse_hex_rgb(&spec.scar_color)
                .unwrap_or(egui::Color32::from_rgb(0xb8, 0xb8, 0xb8)),
            opacity: spec.opacity.clamp(0.0, 1.0),
            diameter: spec.diameter.clamp(0.01, 0.5),
        }
    }
}

impl SkinWidgetArt {
    pub fn icon(&self, id: &str) -> Option<ResolvedIcon> {
        match self.icons.get(&id.to_ascii_uppercase())? {
            IconSlot::Sprite(texture) => Some(ResolvedIcon {
                texture: texture.texture,
                size: texture.size,
                uv: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            }),
            IconSlot::Sheet { sheet, cell } => {
                let (texture, uv) = self.sheet_cell(sheet, *cell, false)?;
                Some(ResolvedIcon {
                    texture: texture.texture,
                    size: egui::vec2(
                        texture.size.x * uv.width(),
                        texture.size.y * uv.height(),
                    ),
                    uv,
                })
            }
        }
    }

    /// Resolve an arbitrary `IconRef` (hand-widget icon states): `Default`
    /// follows the widget's own icon id through the normal precedence,
    /// `None` is explicitly artless, images come from the pre-declared
    /// hand-state pool loads, sheet cells from the shared sheets.
    pub fn resolve_icon_ref(
        &self,
        icon: &crate::data::IconRef,
        own_id: &str,
    ) -> Option<ResolvedIcon> {
        match icon {
            crate::data::IconRef::Default => self.icon(own_id),
            crate::data::IconRef::None => None,
            crate::data::IconRef::Image { path } => self
                .pool_icons
                .get(&path.to_ascii_lowercase())
                .map(|texture| ResolvedIcon {
                    texture: texture.texture,
                    size: texture.size,
                    uv: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                }),
            crate::data::IconRef::SheetCell { sheet, cell } => {
                let (texture, uv) =
                    self.sheet_cell(&sheet.to_ascii_lowercase(), *cell, false)?;
                Some(ResolvedIcon {
                    texture: texture.texture,
                    size: egui::vec2(
                        texture.size.x * uv.width(),
                        texture.size.y * uv.height(),
                    ),
                    uv,
                })
            }
        }
    }

    /// Texture + uv for an `IconRef`, `sheet_cell`-style, for button-face
    /// painting (hotbar icons). Sheet cells honor `gray`; pool images fall
    /// back to color. `Default`/`None` resolve to nothing here — button
    /// faces have no "own id" to follow.
    pub fn icon_ref_texture(
        &self,
        icon: &crate::data::IconRef,
        gray: bool,
    ) -> Option<(SkinTexture, egui::Rect)> {
        match icon {
            crate::data::IconRef::SheetCell { sheet, cell } => {
                self.sheet_cell(&sheet.to_ascii_lowercase(), *cell, gray)
            }
            crate::data::IconRef::Image { path } => self
                .pool_icons
                .get(&path.to_ascii_lowercase())
                .map(|texture| {
                    (
                        *texture,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    )
                }),
            crate::data::IconRef::Default | crate::data::IconRef::None => None,
        }
    }

    pub fn compass_dir(&self, direction: &str) -> Option<SkinTexture> {
        self.compass_dirs.get(direction).copied()
    }

    /// Nine-slice art for a dialog-panel control in the given state, falling
    /// back to the control's normal art when the state isn't authored
    /// (e.g. `control_border("button", "hover")` → "button.hover", else
    /// "button"). None when the skin provides no art for this control.
    pub fn control_border(&self, control: &str, state: &str) -> Option<&ResolvedBorder> {
        let control = control.to_ascii_lowercase();
        self.controls
            .get(&format!("{control}.{state}"))
            .or_else(|| self.controls.get(&control))
    }

    pub fn doll_overlay(&self, part: &str, level: u8) -> Option<SkinTexture> {
        self.doll_parts
            .get(&part.to_ascii_lowercase())
            .and_then(|levels| levels.get(&level))
            .copied()
    }

    /// Grayscale icon twin; None unless "gray when inactive" is enabled
    /// (callers fall back to the color icon).
    pub fn icon_gray(&self, id: &str) -> Option<ResolvedIcon> {
        match self.icons_gray.get(&id.to_ascii_uppercase())? {
            IconSlot::Sprite(texture) => Some(ResolvedIcon {
                texture: texture.texture,
                size: texture.size,
                uv: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            }),
            IconSlot::Sheet { sheet, cell } => {
                let (texture, uv) = self.sheet_cell(sheet, *cell, true)?;
                Some(ResolvedIcon {
                    texture: texture.texture,
                    size: egui::vec2(
                        texture.size.x * uv.width(),
                        texture.size.y * uv.height(),
                    ),
                    uv,
                })
            }
        }
    }

    /// Grayscale doll overlay twin; None unless "grayscale doll" is on.
    pub fn doll_overlay_gray(&self, part: &str, level: u8) -> Option<SkinTexture> {
        self.doll_parts_gray
            .get(&part.to_ascii_lowercase())
            .and_then(|levels| levels.get(&level))
            .copied()
    }

    /// Dot anchor for a body part: the skin's calibrated point, else the
    /// built-in default, else dead center (unknown part).
    pub fn doll_anchor(&self, part: &str) -> egui::Vec2 {
        let key = part.to_ascii_lowercase();
        self.doll_anchors
            .get(&key)
            .copied()
            .or_else(|| {
                skins::default_doll_anchor(&key).map(|[x, y]| egui::vec2(x, y))
            })
            .unwrap_or(egui::vec2(0.5, 0.5))
    }

    fn is_empty(&self) -> bool {
        self.icons.is_empty()
            && self.compass_rose.is_none()
            && self.compass_dirs.is_empty()
            && self.doll_base.is_none()
            && self.doll_parts.is_empty()
            && self.sheets.is_empty()
            && self.pool_icons.is_empty()
    }

    /// Registered hotbar sheet names (lowercased), sorted for editor lists.
    pub fn sheet_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.sheets.keys().cloned().collect();
        names.sort();
        names
    }

    /// Number of cells a sheet holds (full rows × columns), for pickers.
    pub fn sheet_cell_count(&self, sheet: &str) -> Option<u32> {
        let art = self.sheets.get(&sheet.to_ascii_lowercase())?;
        let cols = (art.texture.size.x as u32) / art.cell;
        let rows = (art.texture.size.y as u32) / art.cell;
        Some(cols * rows)
    }

    /// Texture + UV rect for a sheet cell (1-based, left→right then
    /// top→bottom, barbar-style). `grayscale` picks the desaturated twin
    /// when available. None for unknown sheets or out-of-bounds cells.
    pub fn sheet_cell(
        &self,
        sheet: &str,
        cell: u32,
        grayscale: bool,
    ) -> Option<(SkinTexture, egui::Rect)> {
        let art = self.sheets.get(&sheet.to_ascii_lowercase())?;
        if cell == 0 {
            return None;
        }
        let size = art.texture.size;
        let cell_px = art.cell as f32;
        let cols = (size.x / cell_px).floor() as u32;
        let rows = (size.y / cell_px).floor() as u32;
        if cols == 0 || cell > cols * rows {
            return None;
        }
        let idx = cell - 1;
        let (col, row) = (idx % cols, idx / cols);
        let uv = egui::Rect::from_min_max(
            egui::pos2(
                col as f32 * cell_px / size.x,
                row as f32 * cell_px / size.y,
            ),
            egui::pos2(
                (col + 1) as f32 * cell_px / size.x,
                (row + 1) as f32 * cell_px / size.y,
            ),
        );
        let texture = if grayscale {
            art.gray.unwrap_or(art.texture)
        } else {
            art.texture
        };
        Some((texture, uv))
    }
}

/// Everything needed to paint one window's nine-slice border.
#[derive(Debug, Clone)]
pub struct ResolvedBorder {
    pub texture: egui::TextureId,
    pub tex_size: egui::Vec2,
    /// Slice insets in source pixels: [top, right, bottom, left].
    pub slice: [f32; 4],
    pub scale: f32,
}

/// Runtime skin state owned by the GUI app: the active manifest plus its
/// loaded textures. Textures live for as long as the skin stays active.
#[derive(Default)]
pub struct SkinState {
    /// Directory name of the loaded skin; None = no skin active.
    loaded_id: Option<String>,
    manifest: SkinManifest,
    root: PathBuf,
    /// Loaded textures keyed by manifest image path. `None` records a load
    /// failure so a bad path warns once instead of retrying every frame.
    textures: HashMap<String, Option<egui::TextureHandle>>,
    /// Widget sprite lookups built once per skin load.
    widget_art: Option<std::sync::Arc<SkinWidgetArt>>,
    applied: bool,
    /// skin.toml mtime at load, for hot-reload detection.
    manifest_mtime: Option<std::time::SystemTime>,
    /// Injury doll override as a pool-relative path (from
    /// ui_settings.doll_image); replaces the skin's `[injury_doll]` when
    /// set — its calibration comes from the image's sidecar toml.
    doll_override: Option<String>,
    /// Pool frames referenced by window overrides (lowercase stems). Only
    /// these load textures — pool frame art can be megabytes, so the
    /// picker lists names without loading (`frame_names`).
    needed_pool_frames: Vec<String>,
    /// Pool background images referenced by window overrides (pool-relative
    /// paths); like frames, only referenced ones load.
    needed_pool_backgrounds: Vec<String>,
    /// Pool images referenced by hand-widget icon states (pool-relative
    /// paths); like backgrounds, only referenced ones load.
    needed_pool_icons: Vec<String>,
    /// Active statusicons pool set (lowercase `<set>_` prefix).
    statusicon_set: Option<String>,
    /// Compass pool set override (lowercase prefix); replaces the skin's
    /// `[compass]` when its rose is present.
    compass_set: Option<String>,
    /// Build grayscale twins for status icons ("gray when inactive").
    gray_status_icons: bool,
    /// Build grayscale twins for doll art ("grayscale doll").
    gray_doll: bool,
    /// Resolved compass set: lowercase role ("rose", "n", ...) -> pool path.
    pool_compass: HashMap<String, String>,
    /// Per-indicator icon overrides (UPPERCASE id; `Default` never stored).
    statusicon_overrides: HashMap<String, crate::data::IconRef>,
    /// Resolved pool set art: UPPERCASE glyph id -> pool-relative path.
    pool_status_icons: HashMap<String, String>,
    /// Loaded pool frames: lowercase stem -> spec whose `image` is the
    /// pool-relative texture key.
    pool_frames: HashMap<String, skins::BorderSpec>,
    /// Lowercased names of sheets that came from the shared icon store
    /// (global/icons) rather than the skin itself.
    shared_sheet_names: std::collections::HashSet<String>,
    /// Shared icons.toml mtime at load, for hot-reload detection.
    shared_manifest_mtime: Option<std::time::SystemTime>,
    /// Last hot-reload poll, so the mtime stat runs at most once a second.
    last_mtime_check: Option<std::time::Instant>,
    /// Appearance-picker preview textures (pool-relative path → ≤48px
    /// thumb). Never cleared: thumbs are tiny (~16KB VRAM each) and pool
    /// paths are skin-independent. `None` records a decode failure.
    thumbnails: HashMap<String, Option<egui::TextureHandle>>,
    /// New thumbnail decodes still allowed this frame (reset by
    /// `apply_if_changed`); menus fill in over a few frames instead of
    /// hitching once on a big pool.
    thumb_budget: u32,
}

impl SkinState {
    /// Load or unload to match `active` (from config) and the doll override
    /// (from the layout's appearance settings). Call once per frame; does
    /// nothing when neither changed and skin.toml is untouched (edits to
    /// the manifest hot-reload within a second).
    pub fn apply_if_changed(
        &mut self,
        ctx: &egui::Context,
        active: Option<&str>,
        doll_override: Option<&str>,
    ) {
        // Per-frame decode allowance for picker thumbnails (this runs
        // once per frame regardless of skin changes).
        self.thumb_budget = 3;
        if self.applied
            && self.loaded_id.as_deref() == active
            && self.doll_override.as_deref() == doll_override
        {
            if !self.manifest_changed_on_disk() {
                return;
            }
            tracing::info!("skin.toml changed on disk; reloading skin");
        }
        self.applied = true;
        self.loaded_id = active.map(str::to_owned);
        self.doll_override = doll_override.map(str::to_owned);
        self.manifest = SkinManifest::default();
        self.pool_frames = load_pool_frames(&self.needed_pool_frames);
        self.pool_status_icons = load_pool_set("statusicons", self.statusicon_set.as_deref());
        self.pool_compass = load_pool_set("compass", self.compass_set.as_deref());
        self.textures.clear();
        self.widget_art = None;
        self.manifest_mtime = None;
        self.shared_sheet_names.clear();
        self.shared_manifest_mtime = None;

        if let Some(name) = active {
            match skins::load_manifest(name) {
                Ok((manifest, root)) => {
                    self.manifest = manifest;
                    self.root = root;
                    self.manifest_mtime = skins::manifest_mtime(&self.root);
                }
                Err(err) => {
                    // Remember the root so a skin.toml appearing later (e.g. a
                    // scaffold being written) still hot-loads.
                    if let Ok(dir) = crate::config::Config::skins_dir() {
                        self.root = dir.join(name);
                    }
                    tracing::warn!("Failed to load skin '{}': {:#}", name, err);
                }
            }
        }

        // Shared sheets load with or without a skin, so hotbar icons don't
        // require one.
        self.merge_shared_sheets();

        self.load_textures(ctx, active.unwrap_or("shared-icons"));
        self.widget_art = self.build_widget_art();
    }

    /// Fold the shared icon store's sheets into the loaded manifest.
    /// Skin-local sheets win name collisions; shared paths are absolutized
    /// against the shared directory so they resolve from any skin root.
    fn merge_shared_sheets(&mut self) {
        // Record the mtime before parsing so a broken icons.toml warns once
        // instead of re-loading (and re-warning) every poll.
        self.shared_manifest_mtime = shared_icons_mtime();
        let (shared, shared_root) = match skins::load_global_sheets() {
            Ok(loaded) => loaded,
            Err(err) => {
                tracing::warn!("Failed to load shared icon sheets: {:#}", err);
                return;
            }
        };
        self.shared_sheet_names =
            merge_shared_sheets_into(&mut self.manifest.sheets, shared, &shared_root);
    }

    /// Force a full reload on the next frame (`.reloadskin`). Unlike the
    /// mtime poll this also picks up edited *images*, which don't touch
    /// skin.toml.
    pub fn force_reload(&mut self) {
        self.applied = false;
    }

    /// Declare the status-icon config (pool set + per-indicator overrides,
    /// from ui_settings). Call before `apply_if_changed`; changes trigger a
    /// reload so the needed textures come in.
    pub fn set_status_icon_config(
        &mut self,
        set: Option<&str>,
        overrides: &HashMap<String, crate::data::IconRef>,
    ) {
        let set = set.map(|s| s.to_ascii_lowercase());
        let overrides: HashMap<String, crate::data::IconRef> = overrides
            .iter()
            .filter(|(_, icon)| **icon != crate::data::IconRef::Default)
            .map(|(id, icon)| (id.to_ascii_uppercase(), icon.clone()))
            .collect();
        if set != self.statusicon_set || overrides != self.statusicon_overrides {
            self.statusicon_set = set;
            self.statusicon_overrides = overrides;
            self.applied = false;
        }
    }

    /// Declare which pool backgrounds window overrides reference. Call
    /// before `apply_if_changed`; a change triggers a reload.
    pub fn set_needed_pool_backgrounds(&mut self, paths: impl IntoIterator<Item = String>) {
        let mut paths: Vec<String> = paths
            .into_iter()
            .filter(|path| !path.eq_ignore_ascii_case("none"))
            .collect();
        paths.sort();
        paths.dedup();
        if paths != self.needed_pool_backgrounds {
            self.needed_pool_backgrounds = paths;
            self.applied = false;
        }
    }

    /// Declare which pool images hand-widget icon states reference. Call
    /// before `apply_if_changed`; a change triggers a reload.
    pub fn set_needed_pool_icons(&mut self, paths: impl IntoIterator<Item = String>) {
        let mut paths: Vec<String> = paths.into_iter().collect();
        paths.sort();
        paths.dedup();
        if paths != self.needed_pool_icons {
            self.needed_pool_icons = paths;
            self.applied = false;
        }
    }

    /// Declare which grayscale twins settings demand. Twins are built only
    /// while a checkbox asks for them (checked + saved -> next frame) and
    /// dropped when it clears — nobody pays for gray they don't use.
    pub fn set_grayscale(&mut self, status_icons: bool, doll: bool) {
        if status_icons != self.gray_status_icons || doll != self.gray_doll {
            self.gray_status_icons = status_icons;
            self.gray_doll = doll;
            self.applied = false;
        }
    }

    /// Declare the compass pool set (from ui_settings.compass_set). Call
    /// before `apply_if_changed`; a change triggers a reload.
    pub fn set_compass_set(&mut self, set: Option<&str>) {
        let set = set.map(|s| s.to_ascii_lowercase());
        if set != self.compass_set {
            self.compass_set = set;
            self.applied = false;
        }
    }


    /// Declare which pool frames window overrides reference (any case).
    /// Call before `apply_if_changed`; a changed set triggers a reload so
    /// the newly-needed textures come in (and dropped ones free up).
    pub fn set_needed_pool_frames(&mut self, names: impl IntoIterator<Item = String>) {
        let mut names: Vec<String> = names
            .into_iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        names.sort();
        names.dedup();
        if names != self.needed_pool_frames {
            self.needed_pool_frames = names;
            self.applied = false;
        }
    }

    /// True when the active skin's manifest or the shared icons.toml mtime
    /// differs from what was loaded. Rate-limited to one stat per second.
    fn manifest_changed_on_disk(&mut self) -> bool {
        let now = std::time::Instant::now();
        if self
            .last_mtime_check
            .is_some_and(|last| now.duration_since(last) < std::time::Duration::from_secs(1))
        {
            return false;
        }
        self.last_mtime_check = Some(now);
        let skin_changed = self.loaded_id.is_some() && {
            let current = skins::manifest_mtime(&self.root);
            current.is_some() && current != self.manifest_mtime
        };
        // != (not is_some &&) so deleting icons.toml also unloads its sheets.
        skin_changed || shared_icons_mtime() != self.shared_manifest_mtime
    }

    /// Sprite lookups for widget renderers; None when the skin defines no
    /// widget art (renderers then use their vector drawings).
    pub fn widget_art(&self) -> Option<std::sync::Arc<SkinWidgetArt>> {
        self.widget_art.clone()
    }

    /// Directory name of the loaded skin, if one is active.
    pub fn loaded_skin(&self) -> Option<&str> {
        self.loaded_id.as_deref()
    }

    /// True when `sheet` (any case) came from the shared icon store rather
    /// than the active skin.
    pub fn sheet_is_shared(&self, sheet: &str) -> bool {
        self.shared_sheet_names
            .contains(&sheet.to_ascii_lowercase())
    }

    /// The loaded manifest's injury doll section (for seeding the
    /// calibrator with the current anchors and dot styling).
    pub fn doll_manifest(&self) -> &InjuryDollSkin {
        &self.manifest.injury_doll
    }

    /// The active doll override (pool-relative path), if one is set.
    pub fn doll_override(&self) -> Option<&str> {
        self.doll_override.as_deref()
    }

    /// Absolute path of the active doll override's image, for sidecar
    /// reads/writes.
    pub fn doll_override_abs_path(&self) -> Option<std::path::PathBuf> {
        self.doll_override
            .as_deref()
            .map(|path| skins::resolve_image_path(&self.root, path))
    }

    fn build_widget_art(&self) -> Option<std::sync::Arc<SkinWidgetArt>> {
        let tex = |path: &String| {
            self.textures
                .get(path)
                .and_then(|handle| handle.as_ref())
                .map(|handle| SkinTexture {
                    texture: handle.id(),
                    size: handle.size_vec2(),
                })
        };

        let mut art = SkinWidgetArt::default();
        for (id, path) in &self.manifest.icons {
            if let Some(texture) = tex(path) {
                art.icons
                    .insert(id.to_ascii_uppercase(), IconSlot::Sprite(texture));
            }
        }
        // Pool set art fills ids the skin doesn't define (skin wins).
        for (id, path) in &self.pool_status_icons {
            if let Some(texture) = tex(path) {
                art.icons
                    .entry(id.to_ascii_uppercase())
                    .or_insert(IconSlot::Sprite(texture));
            }
        }
        // Per-indicator overrides beat both.
        for (id, icon) in &self.statusicon_overrides {
            match icon {
                crate::data::IconRef::Default => {}
                // Explicit "no art": drop whatever the skin/pool resolved so
                // the widget falls back to its artless rendering. Gray twins
                // mirror art.icons below, so the removal propagates.
                crate::data::IconRef::None => {
                    art.icons.remove(id);
                }
                crate::data::IconRef::Image { path } => {
                    if let Some(texture) = tex(path) {
                        art.icons.insert(id.clone(), IconSlot::Sprite(texture));
                    }
                }
                crate::data::IconRef::SheetCell { sheet, cell } => {
                    art.icons.insert(
                        id.clone(),
                        IconSlot::Sheet {
                            sheet: sheet.to_ascii_lowercase(),
                            cell: *cell,
                        },
                    );
                }
            }
        }
        // Hand-widget icon-state images (pre-declared pool loads).
        for path in &self.needed_pool_icons {
            if let Some(texture) = tex(path) {
                art.pool_icons
                    .insert(path.to_ascii_lowercase(), texture);
            }
        }
        for (name, spec) in &self.manifest.sheets {
            if spec.cell == 0 {
                tracing::warn!("Skin sheet '{}': cell size must be > 0", name);
                continue;
            }
            if let Some(texture) = tex(&spec.path) {
                art.sheets.insert(
                    name.to_ascii_lowercase(),
                    SheetArt {
                        texture,
                        gray: tex(&format!("{}#gray", spec.path)),
                        cell: spec.cell,
                    },
                );
            }
        }
        art.compass_rose = self.manifest.compass.rose.as_ref().and_then(tex);
        for (direction, path) in &self.manifest.compass.directions {
            if let Some(texture) = tex(path) {
                art.compass_dirs
                    .insert(direction.to_ascii_lowercase(), texture);
            }
        }
        // Interactive dialog-panel control art (button/dropdown/... states),
        // each a nine-slice that stretches to the control's rect.
        for (key, spec) in &self.manifest.controls {
            if let Some(border) = self.resolve_border(spec) {
                art.controls.insert(key.to_ascii_lowercase(), border);
            }
        }
        // A compass pool set with a rose replaces the skin's compass
        // wholesale (rose + direction overlays are same-canvas art; mixing
        // sources would misalign them). The "none" sentinel (picker "None")
        // strips compass art entirely so the widget draws its vector rose.
        if self
            .compass_set
            .as_deref()
            .is_some_and(|set| set.eq_ignore_ascii_case("none"))
        {
            art.compass_rose = None;
            art.compass_dirs.clear();
        } else if let Some(rose) = self.pool_compass.get("rose").and_then(tex) {
            art.compass_rose = Some(rose);
            art.compass_dirs.clear();
            for (role, path) in &self.pool_compass {
                if role == "rose" {
                    continue;
                }
                if let Some(texture) = tex(path) {
                    art.compass_dirs.insert(role.clone(), texture);
                }
            }
        }
        art.doll_base = self.manifest.injury_doll.base.as_ref().and_then(tex);
        art.doll_dots = ResolvedDotStyle::from_spec(&self.manifest.injury_doll.dots);
        for (part, anchor) in &self.manifest.injury_doll.anchors {
            art.doll_anchors.insert(
                part.to_ascii_lowercase(),
                egui::vec2(anchor[0].clamp(0.0, 1.0), anchor[1].clamp(0.0, 1.0)),
            );
        }
        for (part, levels) in &self.manifest.injury_doll.parts {
            for (key, path) in levels {
                let Some(level) = skins::severity_level_from_key(key) else {
                    tracing::warn!(
                        "Skin injury_doll.{}: unknown severity key '{}' (expected injury1-3/scar1-3)",
                        part,
                        key
                    );
                    continue;
                };
                if let Some(texture) = tex(path) {
                    art.doll_parts
                        .entry(part.to_ascii_lowercase())
                        .or_default()
                        .insert(level, texture);
                }
            }
        }

        // A doll override replaces the skin's `[injury_doll]` wholesale:
        // base from the pool image, anchors/dots from its sidecar, severity
        // rendered as generated dots (pool dolls carry no overlay art).
        // The "none" sentinel (picker "None") strips doll art entirely so
        // the widget draws its built-in vector body.
        if let Some(path) = &self.doll_override {
            if path.eq_ignore_ascii_case("none") {
                art.doll_base = None;
                art.doll_parts.clear();
                art.doll_anchors.clear();
            } else if let Some(texture) = tex(path) {
                art.doll_base = Some(texture);
                art.doll_parts.clear();
                art.doll_anchors.clear();
                let abs = skins::resolve_image_path(&self.root, path);
                match crate::config::pool::read_sidecar::<crate::config::pool::DollSidecar>(&abs)
                {
                    Some(sidecar) => {
                        for (part, anchor) in &sidecar.anchors {
                            art.doll_anchors.insert(
                                part.to_ascii_lowercase(),
                                egui::vec2(anchor[0].clamp(0.0, 1.0), anchor[1].clamp(0.0, 1.0)),
                            );
                        }
                        art.doll_dots = ResolvedDotStyle::from_spec(&sidecar.dots);
                    }
                    None => art.doll_dots = ResolvedDotStyle::default(),
                }
            }
        }

        // Grayscale twins mirror whatever resolved above, keyed by the same
        // ids; sheet-cell slots resolve their gray at lookup (sheets keep
        // their own twins).
        if self.gray_status_icons {
            for (id, slot) in art.icons.clone() {
                match slot {
                    IconSlot::Sprite(_) => {
                        // Find the color slot's source path back through the
                        // same precedence and fetch its twin.
                        let path = self
                            .statusicon_overrides
                            .get(&id)
                            .and_then(|icon| match icon {
                                crate::data::IconRef::Image { path } => Some(path.clone()),
                                _ => None,
                            })
                            .or_else(|| {
                                self.manifest
                                    .icons
                                    .iter()
                                    .find(|(icon_id, _)| icon_id.eq_ignore_ascii_case(&id))
                                    .map(|(_, path)| path.clone())
                            })
                            .or_else(|| {
                                self.pool_status_icons
                                    .iter()
                                    .find(|(glyph, _)| glyph.eq_ignore_ascii_case(&id))
                                    .map(|(_, path)| path.clone())
                            });
                        if let Some(texture) = path.and_then(|p| tex(&format!("{p}#gray"))) {
                            art.icons_gray.insert(id, IconSlot::Sprite(texture));
                        }
                    }
                    IconSlot::Sheet { .. } => {
                        art.icons_gray.insert(id, slot);
                    }
                }
            }
        }
        if self.gray_doll {
            let base_path = self
                .doll_override
                .clone()
                .or_else(|| self.manifest.injury_doll.base.clone());
            art.doll_base_gray = base_path.and_then(|p| tex(&format!("{p}#gray")));
            if self.doll_override.is_none() {
                for (part, levels) in &self.manifest.injury_doll.parts {
                    for (key, path) in levels {
                        let Some(level) = skins::severity_level_from_key(key) else {
                            continue;
                        };
                        if let Some(texture) = tex(&format!("{path}#gray")) {
                            art.doll_parts_gray
                                .entry(part.to_ascii_lowercase())
                                .or_default()
                                .insert(level, texture);
                        }
                    }
                }
            }
        }

        if art.is_empty() {
            None
        } else {
            Some(std::sync::Arc::new(art))
        }
    }

    fn load_textures(&mut self, ctx: &egui::Context, skin_name: &str) {
        let mut images: Vec<String> = self
            .manifest
            .windows
            .values()
            .flat_map(|window| {
                window
                    .background
                    .as_ref()
                    .map(|bg| bg.image.clone())
                    .into_iter()
                    .chain(window.border.as_ref().map(|border| border.image.clone()))
            })
            .collect();
        images.extend(self.manifest.frames.values().map(|frame| frame.image.clone()));
        images.extend(self.manifest.controls.values().map(|ctrl| ctrl.image.clone()));
        images.extend(self.pool_frames.values().map(|frame| frame.image.clone()));
        images.extend(self.needed_pool_backgrounds.iter().cloned());
        images.extend(self.needed_pool_icons.iter().cloned());
        images.extend(self.doll_override.iter().cloned());
        images.extend(self.pool_status_icons.values().cloned());
        images.extend(self.pool_compass.values().cloned());
        images.extend(
            self.statusicon_overrides
                .values()
                .filter_map(|icon| match icon {
                    crate::data::IconRef::Image { path } => Some(path.clone()),
                    _ => None,
                }),
        );
        images.extend(self.manifest.icons.values().cloned());
        images.extend(self.manifest.sheets.values().map(|s| s.path.clone()));
        images.extend(self.manifest.compass.rose.iter().cloned());
        images.extend(self.manifest.compass.directions.values().cloned());
        images.extend(self.manifest.injury_doll.base.iter().cloned());
        images.extend(
            self.manifest
                .injury_doll
                .parts
                .values()
                .flat_map(|levels| levels.values().cloned()),
        );
        for image in images {
            if self.textures.contains_key(&image) {
                continue;
            }
            let handle = load_texture(ctx, &self.root, &image, skin_name);
            self.textures.insert(image, handle);
        }
        // Grayscale twins for hotbar sheets (barbar's gs variant), cached
        // under a synthetic "<path>#gray" key.
        for spec in self.manifest.sheets.values() {
            let key = format!("{}#gray", spec.path);
            if self.textures.contains_key(&key) {
                continue;
            }
            // Skip the twin when the base image itself failed (one warning
            // is enough).
            let handle = if matches!(self.textures.get(&spec.path), Some(Some(_))) {
                load_texture_desaturated(ctx, &self.root, &spec.path, skin_name)
            } else {
                None
            };
            self.textures.insert(key, handle);
        }
        // Lazy grayscale twins: built only for what the checkboxes demand
        // (status icons when "gray inactive" is on, doll art when
        // "grayscale doll" is on). Unchecking rebuilds without them.
        let mut gray_paths: Vec<String> = Vec::new();
        if self.gray_status_icons {
            gray_paths.extend(self.manifest.icons.values().cloned());
            gray_paths.extend(self.pool_status_icons.values().cloned());
            gray_paths.extend(
                self.statusicon_overrides
                    .values()
                    .filter_map(|icon| match icon {
                        crate::data::IconRef::Image { path } => Some(path.clone()),
                        _ => None,
                    }),
            );
        }
        if self.gray_doll {
            match &self.doll_override {
                Some(path) => gray_paths.push(path.clone()),
                None => {
                    gray_paths.extend(self.manifest.injury_doll.base.iter().cloned());
                    gray_paths.extend(
                        self.manifest
                            .injury_doll
                            .parts
                            .values()
                            .flat_map(|levels| levels.values().cloned()),
                    );
                }
            }
        }
        for path in gray_paths {
            let key = format!("{path}#gray");
            if self.textures.contains_key(&key) {
                continue;
            }
            let handle = if matches!(self.textures.get(&path), Some(Some(_))) {
                load_texture_desaturated(ctx, &self.root, &path, skin_name)
            } else {
                None
            };
            self.textures.insert(key, handle);
        }
    }

    /// Resolve the background for a window, falling back to the manifest's
    /// "default" entry. None when no skin is active, the window has no
    /// background, or its image failed to load.
    pub fn background_for(&self, window_name: &str) -> Option<ResolvedBackground> {
        let spec = skins::window_background(&self.manifest, window_name)?;
        let texture = self.textures.get(&spec.image)?.as_ref()?;
        let opacity = spec.opacity.clamp(0.0, 1.0);
        let tint = spec
            .tint
            .as_deref()
            .and_then(parse_hex_rgb)
            .unwrap_or(egui::Color32::WHITE)
            .gamma_multiply(opacity);
        Some(ResolvedBackground {
            texture: texture.id(),
            tex_size: texture.size_vec2(),
            fit: spec.fit,
            tint,
            scrim_alpha: (spec.scrim.clamp(0.0, 1.0) * 255.0).round() as u8,
        })
    }

    /// Background resolution honoring a per-window override: "none" kills
    /// the background, a pool-relative path renders that image with
    /// readable defaults (cover fit, a light theme scrim), anything else
    /// falls back to the skin's own per-window mapping.
    pub fn background_for_with_override(
        &self,
        window_name: &str,
        background_override: Option<&str>,
    ) -> Option<ResolvedBackground> {
        match background_override {
            Some(path) if path.eq_ignore_ascii_case("none") => None,
            Some(path) => {
                let texture = self.textures.get(path)?.as_ref()?;
                Some(ResolvedBackground {
                    texture: texture.id(),
                    tex_size: texture.size_vec2(),
                    fit: BackgroundFit::Cover,
                    tint: egui::Color32::WHITE,
                    // Text stays readable over arbitrary pool art; skins
                    // that want exact control keep using their manifest.
                    scrim_alpha: (0.25 * 255.0) as u8,
                })
            }
            None => self.background_for(window_name),
        }
    }

    /// Resolve the nine-slice border for a window, falling back to the
    /// manifest's "default" entry (independently of the background, so a
    /// window can override one without losing the other).
    pub fn border_for(&self, window_name: &str) -> Option<ResolvedBorder> {
        self.resolve_border(skins::window_field(&self.manifest, window_name, |window| {
            window.border.as_ref()
        })?)
    }

    /// Border resolution honoring a per-window user override: "none" kills
    /// the frame, a named `[frames.*]` entry replaces it, and an unknown
    /// name (stale layout, switched skin) falls back to the skin's own
    /// per-window mapping — as does no override at all.
    pub fn border_for_with_override(
        &self,
        window_name: &str,
        frame_override: Option<&str>,
    ) -> Option<ResolvedBorder> {
        match frame_override {
            Some(name) if name.eq_ignore_ascii_case(skins::NO_FRAME) => None,
            Some(name) => skins::named_frame(&self.manifest, name)
                .and_then(|spec| self.resolve_border(spec))
                .or_else(|| {
                    // Pool frame (skinless or supplementing the skin).
                    self.pool_frames
                        .get(&name.to_ascii_lowercase())
                        .and_then(|spec| self.resolve_border(spec))
                })
                .or_else(|| self.border_for(window_name)),
            None => self.border_for(window_name),
        }
    }

    fn resolve_border(&self, spec: &skins::BorderSpec) -> Option<ResolvedBorder> {
        let texture = self.textures.get(&spec.image)?.as_ref()?;
        Some(ResolvedBorder {
            texture: texture.id(),
            tex_size: texture.size_vec2(),
            slice: spec.slice,
            scale: spec.scale.max(0.05),
        })
    }

    /// A small preview texture for Appearance pickers (aspect kept,
    /// longest edge ≤ 48px). Budgeted: a handful of new decodes per
    /// frame — callers get None until a later frame fills the cache, and
    /// a repaint is requested so open menus fill in on their own.
    pub fn thumbnail(
        &mut self,
        ctx: &egui::Context,
        image_path: &str,
    ) -> Option<(egui::TextureId, egui::Vec2)> {
        if let Some(entry) = self.thumbnails.get(image_path) {
            return entry.as_ref().map(|t| (t.id(), t.size_vec2()));
        }
        if self.thumb_budget == 0 {
            ctx.request_repaint();
            return None;
        }
        self.thumb_budget -= 1;
        let handle = load_thumbnail_impl(ctx, &self.root, image_path);
        let out = handle.as_ref().map(|t| (t.id(), t.size_vec2()));
        self.thumbnails.insert(image_path.to_string(), handle);
        ctx.request_repaint();
        out
    }

    /// Frames the Appearance picker offers: the active skin's `[frames.*]`
    /// plus every pool frame with a sidecar (names only — textures load
    /// lazily for frames actually assigned). Skin names win collisions.
    pub fn frame_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .manifest
            .frames
            .keys()
            .filter(|name| !name.eq_ignore_ascii_case(skins::NO_FRAME))
            .cloned()
            .collect();
        for image in crate::config::pool::list_category("frames") {
            let stem = image.stem();
            if stem.eq_ignore_ascii_case(skins::NO_FRAME) {
                continue;
            }
            // Without a slice/scale sidecar the frame can't nine-slice;
            // leave it out rather than offering a dead entry.
            if !image.sidecar_path().is_file() {
                continue;
            }
            if !names.iter().any(|name| name.eq_ignore_ascii_case(stem)) {
                names.push(stem.to_owned());
            }
        }
        names.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
        names
    }
}

/// Resolve one pool set: `<set>_<suffix>.png` -> lowercase suffix -> pool
/// path (glyph ids for statusicons, roles for compass). No set = empty.
fn load_pool_set(category: &str, set: Option<&str>) -> HashMap<String, String> {
    let mut entries = HashMap::new();
    let Some(set) = set else {
        return entries;
    };
    for image in crate::config::pool::list_category(category) {
        let Some((prefix, suffix)) = image.stem().split_once('_') else {
            continue;
        };
        if prefix.eq_ignore_ascii_case(set) && !suffix.is_empty() {
            entries.insert(suffix.to_ascii_lowercase(), image.pool_path.clone());
        }
    }
    entries
}

/// Load the specs (not textures) for the needed pool frames: match stems
/// case-insensitively, take slice/scale from each image's sidecar. Frames
/// without a usable sidecar are skipped with a warning.
fn load_pool_frames(needed: &[String]) -> HashMap<String, skins::BorderSpec> {
    let mut frames = HashMap::new();
    if needed.is_empty() {
        return frames;
    }
    for image in crate::config::pool::list_category("frames") {
        let stem = image.stem().to_ascii_lowercase();
        if !needed.contains(&stem) {
            continue;
        }
        let Some(sidecar) =
            crate::config::pool::read_sidecar::<crate::config::pool::FrameSidecar>(&image.abs_path)
        else {
            tracing::warn!(
                "pool frame '{}' has no sidecar with slice/scale; skipping",
                image.file_name
            );
            continue;
        };
        frames.insert(
            stem,
            skins::BorderSpec {
                image: image.pool_path.clone(),
                slice: sidecar.slice.insets(),
                scale: sidecar.effective_scale(),
            },
        );
    }
    frames
}

/// mtime of the shared icon store's manifest, if it exists.
fn shared_icons_mtime() -> Option<std::time::SystemTime> {
    let root = crate::config::Config::global_icons_dir().ok()?;
    std::fs::metadata(root.join("icons.toml"))
        .and_then(|meta| meta.modified())
        .ok()
}

/// Fold shared sheets into a manifest's sheet table: skin entries win name
/// collisions (case-insensitive), and relative shared paths become absolute
/// against the shared directory so they load regardless of the skin root.
/// Returns the lowercased names of the sheets actually added.
fn merge_shared_sheets_into(
    sheets: &mut HashMap<String, SheetSpec>,
    shared: HashMap<String, SheetSpec>,
    shared_root: &Path,
) -> std::collections::HashSet<String> {
    let mut added = std::collections::HashSet::new();
    for (name, mut spec) in shared {
        if sheets.keys().any(|k| k.eq_ignore_ascii_case(&name)) {
            continue;
        }
        if Path::new(&spec.path).is_relative() {
            spec.path = shared_root.join(&spec.path).to_string_lossy().into_owned();
        }
        added.insert(name.to_ascii_lowercase());
        sheets.insert(name, spec);
    }
    added
}

fn load_texture(
    ctx: &egui::Context,
    root: &Path,
    image_path: &str,
    skin_name: &str,
) -> Option<egui::TextureHandle> {
    load_texture_impl(ctx, root, image_path, skin_name, false)
}

/// Desaturated twin of a texture (hotbar sheet grayscale variants);
/// registered under a distinct texture name so both coexist.
fn load_texture_desaturated(
    ctx: &egui::Context,
    root: &Path,
    image_path: &str,
    skin_name: &str,
) -> Option<egui::TextureHandle> {
    load_texture_impl(ctx, root, image_path, skin_name, true)
}

/// Decode + downscale one image into a picker thumbnail texture. Quieter
/// than the full loader (a broken pool image just shows no preview).
fn load_thumbnail_impl(
    ctx: &egui::Context,
    root: &Path,
    image_path: &str,
) -> Option<egui::TextureHandle> {
    const THUMB_EDGE: u32 = 48;
    let path = skins::resolve_image_path(root, image_path);
    let bytes = std::fs::read(&path).ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?;
    // `thumbnail` is the image crate's fast aspect-preserving resize.
    let rgba = decoded.thumbnail(THUMB_EDGE, THUMB_EDGE).to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture(
        format!("thumb:{image_path}"),
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

fn load_texture_impl(
    ctx: &egui::Context,
    root: &Path,
    image_path: &str,
    skin_name: &str,
    desaturate: bool,
) -> Option<egui::TextureHandle> {
    // Skin folder first, then the shared image pool (global/images/).
    let path = skins::resolve_image_path(root, image_path);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!("Skin '{}': cannot read {}: {}", skin_name, path.display(), err);
            return None;
        }
    };
    let decoded = match image::load_from_memory(&bytes) {
        Ok(decoded) => decoded,
        Err(err) => {
            tracing::warn!("Skin '{}': cannot decode {}: {}", skin_name, path.display(), err);
            return None;
        }
    };
    let mut rgba = decoded.to_rgba8();
    if desaturate {
        // barbar's gs variant: luminance recolor, alpha preserved.
        for px in rgba.pixels_mut() {
            let [r, g, b, a] = px.0;
            let luma =
                (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32).round() as u8;
            px.0 = [luma, luma, luma, a];
        }
    }
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let suffix = if desaturate { "#gray" } else { "" };
    Some(ctx.load_texture(
        format!("skin:{}:{}{}", skin_name, image_path, suffix),
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

/// Build the shapes that paint a window background into `rect`. The caller
/// paints them through a painter clipped to `rect` — normally deferred via
/// a reserved shape slot (`Painter::add(Noop)` + `Painter::set`) so the
/// art lands behind the window's content yet is sized from the content's
/// final extent, not the pre-layout available rect (which can overshoot an
/// auto-sized window's frame). `scrim_color` supplies the scrim's RGB
/// (normally the theme's window fill) so the overlay darkens/lightens
/// toward the theme rather than plain black.
pub fn background_shapes(
    rect: egui::Rect,
    bg: &ResolvedBackground,
    scrim_color: egui::Color32,
) -> Vec<egui::Shape> {
    let mut shapes = Vec::new();
    if !rect.is_positive() || bg.tex_size.x <= 0.0 || bg.tex_size.y <= 0.0 {
        return shapes;
    }
    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let image = |dest: egui::Rect, uv: egui::Rect| {
        let mut mesh = egui::Mesh::with_texture(bg.texture);
        mesh.add_rect_with_uv(dest, uv, bg.tint);
        egui::Shape::mesh(mesh)
    };
    match bg.fit {
        BackgroundFit::Stretch => {
            shapes.push(image(rect, full_uv));
        }
        BackgroundFit::Cover => {
            shapes.push(image(rect, cover_uv(bg.tex_size, rect.size())));
        }
        BackgroundFit::Contain => {
            shapes.push(image(contain_dest(bg.tex_size, rect), full_uv));
        }
        BackgroundFit::Center => {
            let dest = egui::Rect::from_center_size(rect.center(), bg.tex_size);
            shapes.push(image(dest, full_uv));
        }
        BackgroundFit::Tile => {
            // Cap the grid so a tiny tile in a huge window can't explode the
            // frame's mesh; past the cap the remainder just stays theme fill.
            const MAX_TILES_PER_AXIS: usize = 64;
            let cols = ((rect.width() / bg.tex_size.x).ceil() as usize).min(MAX_TILES_PER_AXIS);
            let rows = ((rect.height() / bg.tex_size.y).ceil() as usize).min(MAX_TILES_PER_AXIS);
            for row in 0..rows {
                for col in 0..cols {
                    let min = rect.min
                        + egui::vec2(col as f32 * bg.tex_size.x, row as f32 * bg.tex_size.y);
                    shapes.push(image(egui::Rect::from_min_size(min, bg.tex_size), full_uv));
                }
            }
        }
    }
    if bg.scrim_alpha > 0 {
        let scrim = egui::Color32::from_rgba_unmultiplied(
            scrim_color.r(),
            scrim_color.g(),
            scrim_color.b(),
            bg.scrim_alpha,
        );
        shapes.push(egui::Shape::rect_filled(rect, 0.0, scrim));
    }
    shapes
}

/// Largest rect with the sprite's aspect ratio centered inside `rect`.
/// Layered sprites (compass rose + overlays, doll base + overlays) should
/// all be painted into the dest computed from the *base* sprite so
/// same-canvas art stays aligned.
pub fn sprite_dest(sprite: &SkinTexture, rect: egui::Rect) -> egui::Rect {
    contain_dest(sprite.size, rect)
}

/// Largest rect with the icon's aspect ratio centered inside `rect`.
pub fn icon_dest(icon: &ResolvedIcon, rect: egui::Rect) -> egui::Rect {
    contain_dest(icon.size, rect)
}

/// Paint a resolved icon (full image or sheet cell) into `dest`.
pub fn paint_icon(
    painter: &egui::Painter,
    dest: egui::Rect,
    icon: &ResolvedIcon,
    tint: egui::Color32,
) {
    painter.image(icon.texture, dest, icon.uv, tint);
}

/// Paint a sprite stretched into `dest` (use `sprite_dest` for aspect fit).
pub fn paint_sprite(
    painter: &egui::Painter,
    dest: egui::Rect,
    sprite: &SkinTexture,
    tint: egui::Color32,
) {
    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    painter.image(sprite.texture, dest, full_uv, tint);
}

/// Paint one generated injury dot: wounds (levels 1-3) are a solid circle
/// with the severity numeral inside, scars (levels 4-6) a ring with the
/// numeral in the ring color. The numeral is skipped when the dot is too
/// small to render it legibly (the doll tooltip still carries the detail).
pub fn paint_severity_dot(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    level: u8,
    style: &ResolvedDotStyle,
) {
    if level == 0 || level > 6 {
        return;
    }
    let radius = radius.max(3.0);
    let numeral_font = egui::FontId::proportional((radius * 1.3).max(9.0));
    let show_numeral = radius >= 5.5;
    if level <= 3 {
        let fill = style.wound.gamma_multiply(style.opacity);
        painter.circle_filled(center, radius, fill);
        if show_numeral {
            let numeral_color = contrast_color(style.wound).gamma_multiply(style.opacity);
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                level.to_string(),
                numeral_font,
                numeral_color,
            );
        }
    } else {
        let color = style.scar.gamma_multiply(style.opacity);
        let stroke_width = (radius * 0.28).max(1.5);
        painter.circle_stroke(center, radius, egui::Stroke::new(stroke_width, color));
        if show_numeral {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                (level - 3).to_string(),
                numeral_font,
                color,
            );
        }
    }
}

/// Black or white, whichever contrasts more against `fill` (for the wound
/// numeral painted on the solid dot).
fn contrast_color(fill: egui::Color32) -> egui::Color32 {
    let luminance =
        0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if luminance > 140.0 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

/// Rewrite the `[injury_doll.anchors]` and `[injury_doll.dots]` tables in a
/// skin.toml, preserving everything else byte-for-byte (comments included).
/// Pure string -> string so it's testable without touching the filesystem.
pub fn calibration_toml(
    contents: &str,
    anchors: &HashMap<String, [f32; 2]>,
    dots: &DollDotSpec,
) -> anyhow::Result<String> {
    use toml_edit::{value, Array, DocumentMut, Item, Table};

    let mut doc: DocumentMut = contents
        .parse()
        .map_err(|err| anyhow::anyhow!("skin.toml is not valid TOML: {}", err))?;

    let existed = doc.contains_key("injury_doll");
    let doll = doc
        .entry("injury_doll")
        .or_insert(Item::Table(Table::new()));
    let doll = doll
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("[injury_doll] is not a table"))?;
    // A freshly created parent shouldn't emit its own empty [injury_doll]
    // header; one that already existed keeps whatever shape it had.
    if !existed {
        doll.set_implicit(true);
    }

    // Round in f64: the f32 -> f64 cast would otherwise smear 0.09 into
    // 0.09000000357... in the written file. Four decimals is sub-pixel on
    // any realistic doll image and keeps the file readable.
    let rounded = |v: f32, places: f64| (v as f64 * places).round() / places;

    let mut anchors_table = Table::new();
    let mut keys: Vec<&String> = anchors.keys().collect();
    keys.sort();
    for key in keys {
        let [x, y] = anchors[key];
        let mut pair = Array::new();
        pair.push(rounded(x, 10_000.0));
        pair.push(rounded(y, 10_000.0));
        anchors_table.insert(key, value(pair));
    }
    doll.insert("anchors", Item::Table(anchors_table));

    let mut dots_table = Table::new();
    dots_table.insert("wound_color", value(dots.wound_color.as_str()));
    dots_table.insert("scar_color", value(dots.scar_color.as_str()));
    dots_table.insert("opacity", value(rounded(dots.opacity, 100.0)));
    dots_table.insert("diameter", value(rounded(dots.diameter, 1_000.0)));
    doll.insert("dots", Item::Table(dots_table));

    Ok(doc.to_string())
}

/// Write calibrated anchors + dot styling into `skins/<name>/skin.toml`.
/// The skin hot-reload poll picks the change up within a second.
pub fn save_calibration(
    name: &str,
    anchors: &HashMap<String, [f32; 2]>,
    dots: &DollDotSpec,
) -> anyhow::Result<()> {
    let root = crate::config::Config::skins_dir()?.join(name);
    let manifest_path = root.join("skin.toml");
    let contents = std::fs::read_to_string(&manifest_path)
        .map_err(|err| anyhow::anyhow!("cannot read {}: {}", manifest_path.display(), err))?;
    let updated = calibration_toml(&contents, anchors, dots)?;
    crate::config::write_atomic(&manifest_path, updated)
        .map_err(|err| anyhow::anyhow!("cannot write {}: {}", manifest_path.display(), err))?;
    Ok(())
}

/// Insert (or replace) one `[sheets.<name>]` entry in skin.toml, preserving
/// comments and the author's formatting elsewhere (toml_edit, same approach
/// as `calibration_toml`).
pub fn sheet_registration_toml(
    contents: &str,
    name: &str,
    path: &str,
    cell: u32,
) -> anyhow::Result<String> {
    use toml_edit::{value, DocumentMut, Item, Table};

    let mut doc: DocumentMut = contents
        .parse()
        .map_err(|err| anyhow::anyhow!("sheet manifest is not valid TOML: {}", err))?;

    let existed = doc.contains_key("sheets");
    let sheets = doc.entry("sheets").or_insert(Item::Table(Table::new()));
    let sheets = sheets
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("[sheets] is not a table"))?;
    if !existed {
        // Don't emit a bare [sheets] header for a freshly created parent.
        sheets.set_implicit(true);
    }

    let mut entry = Table::new();
    entry.insert("path", value(path));
    entry.insert("cell", value(cell as i64));
    sheets.insert(name, Item::Table(entry));

    Ok(doc.to_string())
}

/// Register a hotbar icon sprite sheet into `skins/<skin>/`: copies the
/// source image under `icons/` and records `[sheets.<name>]` in skin.toml.
/// The skin hot-reload poll picks the change up within a second.
pub fn register_sheet(
    skin: &str,
    sheet_name: &str,
    source: &Path,
    cell: u32,
) -> anyhow::Result<()> {
    let root = crate::config::Config::skins_dir()?.join(skin);
    let manifest_path = root.join("skin.toml");
    let contents = std::fs::read_to_string(&manifest_path)
        .map_err(|err| anyhow::anyhow!("cannot read {}: {}", manifest_path.display(), err))?;
    register_sheet_impl(&root, &manifest_path, &contents, "icons", sheet_name, source, cell)
}

/// Register a hotbar icon sprite sheet into the shared store
/// (`global/images/icons/`), where every skin — and a skinless setup —
/// can use it. Creates the store and its icons.toml on first use.
pub fn register_sheet_shared(
    sheet_name: &str,
    source: &Path,
    cell: u32,
) -> anyhow::Result<()> {
    let root = crate::config::Config::global_icons_dir()?;
    std::fs::create_dir_all(&root)
        .map_err(|err| anyhow::anyhow!("cannot create {}: {}", root.display(), err))?;
    let manifest_path = root.join("icons.toml");
    let contents = match std::fs::read_to_string(&manifest_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            "# VellumFE shared hotbar icon sheets - available to every skin (and\n\
             # with no skin active). Skin sheets with the same name win.\n\
             # Managed by the .hotbars editor; image paths are relative to\n\
             # this folder.\n"
                .to_string()
        }
        Err(err) => {
            return Err(anyhow::anyhow!(
                "cannot read {}: {}",
                manifest_path.display(),
                err
            ));
        }
    };
    // Images sit beside icons.toml, so no subdirectory prefix.
    register_sheet_impl(&root, &manifest_path, &contents, "", sheet_name, source, cell)
}

fn register_sheet_impl(
    root: &Path,
    manifest_path: &Path,
    manifest_contents: &str,
    image_dir: &str,
    sheet_name: &str,
    source: &Path,
    cell: u32,
) -> anyhow::Result<()> {
    let name = sheet_name.trim();
    anyhow::ensure!(!name.is_empty(), "sheet name is required");
    anyhow::ensure!(
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "sheet name may only use letters, digits, '_' and '-'"
    );
    anyhow::ensure!(cell > 0, "cell size must be > 0");
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    anyhow::ensure!(
        matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp"),
        "source must be a png/jpg/webp/bmp image"
    );
    anyhow::ensure!(
        source.is_file(),
        "source image not found: {}",
        source.display()
    );

    let file_name = source
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("source has no file name"))?;
    let rel = if image_dir.is_empty() {
        file_name.to_string_lossy().into_owned()
    } else {
        format!("{}/{}", image_dir, file_name.to_string_lossy())
    };
    let dest = root.join(&rel);
    // Refuse to clobber different existing art; re-registering the exact
    // same file path is fine (the copy is skipped).
    if dest.exists() && dest.canonicalize().ok() != source.canonicalize().ok() {
        anyhow::bail!(
            "{} already exists in the store - rename the source file",
            rel
        );
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| anyhow::anyhow!("cannot create {}: {}", parent.display(), err))?;
    }
    if !dest.exists() {
        std::fs::copy(source, &dest)
            .map_err(|err| anyhow::anyhow!("cannot copy the image in: {}", err))?;
    }

    let updated = sheet_registration_toml(manifest_contents, name, &rel, cell)?;
    crate::config::write_atomic(manifest_path, updated)
        .map_err(|err| anyhow::anyhow!("cannot write {}: {}", manifest_path.display(), err))?;
    Ok(())
}

/// Paint a nine-slice border into `rect`: corners at fixed size, edges
/// stretched along their axis, center left empty so the window fill or
/// background image shows through. `sides` is [top, right, bottom, left]
/// (matching the slice order): hidden sides draw nothing — their corners
/// vanish with them, and the surviving perpendicular rails extend to the
/// window edge.
pub fn paint_nine_slice(
    painter: &egui::Painter,
    rect: egui::Rect,
    border: &ResolvedBorder,
    sides: [bool; 4],
) {
    let full_alpha = egui::Color32::WHITE;
    for (dest, uv) in nine_slice_patches(border.tex_size, border.slice, border.scale, rect, sides)
    {
        painter.image(border.texture, dest, uv, full_alpha);
    }
}

/// The eight border patches as (destination rect, UV rect) pairs. Slice
/// insets larger than the destination shrink proportionally so opposite
/// borders never overlap. Degenerate patches (zero-size) are skipped —
/// which is also how hidden sides work: zeroing a side's on-screen inset
/// collapses its edge and both its corners to zero-size rects, while the
/// perpendicular edges (which span between the insets) automatically
/// stretch into the freed space.
fn nine_slice_patches(
    tex: egui::Vec2,
    slice: [f32; 4],
    scale: f32,
    rect: egui::Rect,
    sides: [bool; 4],
) -> Vec<(egui::Rect, egui::Rect)> {
    if tex.x <= 0.0 || tex.y <= 0.0 || !rect.is_positive() {
        return Vec::new();
    }
    let [top, right, bottom, left] = slice.map(|inset| inset.max(0.0));

    // On-screen border thicknesses, shrunk if the rect is too small.
    let mut dt = if sides[0] { top * scale } else { 0.0 };
    let mut db = if sides[2] { bottom * scale } else { 0.0 };
    if dt + db > rect.height() {
        let shrink = rect.height() / (dt + db);
        dt *= shrink;
        db *= shrink;
    }
    let mut dl = if sides[3] { left * scale } else { 0.0 };
    let mut dr = if sides[1] { right * scale } else { 0.0 };
    if dl + dr > rect.width() {
        let shrink = rect.width() / (dl + dr);
        dl *= shrink;
        dr *= shrink;
    }

    // Column/row boundaries in destination space and UV space.
    let dx = [rect.min.x, rect.min.x + dl, rect.max.x - dr, rect.max.x];
    let dy = [rect.min.y, rect.min.y + dt, rect.max.y - db, rect.max.y];
    let ux = [0.0, (left / tex.x).min(1.0), 1.0 - (right / tex.x).min(1.0), 1.0];
    let uy = [0.0, (top / tex.y).min(1.0), 1.0 - (bottom / tex.y).min(1.0), 1.0];

    let mut patches = Vec::with_capacity(8);
    for row in 0..3 {
        for col in 0..3 {
            if row == 1 && col == 1 {
                continue; // center stays empty
            }
            let dest = egui::Rect::from_min_max(
                egui::pos2(dx[col], dy[row]),
                egui::pos2(dx[col + 1], dy[row + 1]),
            );
            let uv = egui::Rect::from_min_max(
                egui::pos2(ux[col], uy[row]),
                egui::pos2(ux[col + 1], uy[row + 1]),
            );
            if dest.width() > 0.0 && dest.height() > 0.0 && uv.width() > 0.0 && uv.height() > 0.0
            {
                patches.push((dest, uv));
            }
        }
    }
    patches
}

/// UV rect that crops the texture to the destination's aspect ratio so the
/// image covers it completely (centered crop).
fn cover_uv(tex: egui::Vec2, dest: egui::Vec2) -> egui::Rect {
    let tex_aspect = tex.x / tex.y;
    let dest_aspect = dest.x / dest.y;
    if dest_aspect > tex_aspect {
        // Destination is wider: use full width, crop top/bottom.
        let visible = tex_aspect / dest_aspect;
        let margin = (1.0 - visible) / 2.0;
        egui::Rect::from_min_max(egui::pos2(0.0, margin), egui::pos2(1.0, 1.0 - margin))
    } else {
        // Destination is taller: use full height, crop left/right.
        let visible = dest_aspect / tex_aspect;
        let margin = (1.0 - visible) / 2.0;
        egui::Rect::from_min_max(egui::pos2(margin, 0.0), egui::pos2(1.0 - margin, 1.0))
    }
}

/// Largest rect with the texture's aspect ratio that fits inside `rect`,
/// centered (letterbox).
fn contain_dest(tex: egui::Vec2, rect: egui::Rect) -> egui::Rect {
    let scale = (rect.width() / tex.x).min(rect.height() / tex.y);
    egui::Rect::from_center_size(rect.center(), tex * scale)
}

/// Parse "#rrggbb" (or "rrggbb") into an opaque color.
pub fn parse_hex_rgb(input: &str) -> Option<egui::Color32> {
    let hex = input.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Art with one 4x2-cell sheet (256x128 @ 64px cells) named "rogue".
    fn art_with_sheet() -> SkinWidgetArt {
        let texture = SkinTexture {
            texture: egui::TextureId::default(),
            size: egui::vec2(256.0, 128.0),
        };
        let mut art = SkinWidgetArt::default();
        art.sheets.insert(
            "rogue".to_string(),
            SheetArt {
                texture,
                gray: None,
                cell: 64,
            },
        );
        art
    }

    #[test]
    fn sheet_cell_uv_is_one_based_row_major() {
        let art = art_with_sheet();
        // Cell 1: top-left quarter-cell of a 4-wide sheet.
        let (_, uv) = art.sheet_cell("rogue", 1, false).unwrap();
        assert_eq!((uv.min.x, uv.min.y), (0.0, 0.0));
        assert!((uv.max.x - 0.25).abs() < 1e-5);
        assert!((uv.max.y - 0.5).abs() < 1e-5);
        // Cell 6: second row, second column (idx 5 -> col 1, row 1).
        let (_, uv) = art.sheet_cell("rogue", 6, false).unwrap();
        assert!((uv.min.x - 0.25).abs() < 1e-5);
        assert!((uv.min.y - 0.5).abs() < 1e-5);
        // Lookup is case-insensitive like the icon table.
        assert!(art.sheet_cell("ROGUE", 1, false).is_some());
    }

    #[test]
    fn sheet_cell_rejects_zero_out_of_bounds_and_unknown() {
        let art = art_with_sheet();
        assert!(art.sheet_cell("rogue", 0, false).is_none());
        assert!(art.sheet_cell("rogue", 9, false).is_none()); // 4x2 = 8 cells
        assert!(art.sheet_cell("mage", 1, false).is_none());
        assert_eq!(art.sheet_cell_count("rogue"), Some(8));
    }

    #[test]
    fn sheet_cell_grayscale_falls_back_to_base_texture() {
        // gray: None -> grayscale request still returns the base texture.
        let art = art_with_sheet();
        assert!(art.sheet_cell("rogue", 1, true).is_some());
    }

    #[test]
    fn cover_uv_crops_the_longer_axis() {
        // Wide texture (2:1) into a square: crop left/right.
        let uv = cover_uv(egui::vec2(200.0, 100.0), egui::vec2(100.0, 100.0));
        assert!((uv.min.x - 0.25).abs() < 1e-5);
        assert!((uv.max.x - 0.75).abs() < 1e-5);
        assert_eq!(uv.min.y, 0.0);
        assert_eq!(uv.max.y, 1.0);

        // Tall texture (1:2) into a square: crop top/bottom.
        let uv = cover_uv(egui::vec2(100.0, 200.0), egui::vec2(100.0, 100.0));
        assert_eq!(uv.min.x, 0.0);
        assert!((uv.min.y - 0.25).abs() < 1e-5);
    }

    #[test]
    fn contain_dest_letterboxes_and_centers() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        // Wide texture: full width, half height, vertically centered.
        let dest = contain_dest(egui::vec2(200.0, 100.0), rect);
        assert!((dest.width() - 100.0).abs() < 1e-4);
        assert!((dest.height() - 50.0).abs() < 1e-4);
        assert!((dest.min.y - 25.0).abs() < 1e-4);
    }

    #[test]
    fn nine_slice_patches_cover_border_not_center() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 80.0));
        let patches =
            nine_slice_patches(egui::vec2(32.0, 32.0), [8.0, 8.0, 8.0, 8.0], 1.0, rect, [true; 4]);
        assert_eq!(patches.len(), 8);

        // Top-left corner: fixed 8x8 at the origin, UV = top-left quarter.
        let (dest, uv) = patches[0];
        assert_eq!(dest, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(8.0, 8.0)));
        assert_eq!(uv, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(0.25, 0.25)));

        // No patch covers the center point.
        let center = rect.center();
        assert!(patches.iter().all(|(dest, _)| !dest.contains(center)));
    }

    #[test]
    fn nine_slice_patches_shrink_when_rect_is_small() {
        // 8px insets at scale 1 into a 10px-tall rect: top+bottom shrink to
        // 5px each instead of overlapping.
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 10.0));
        let patches =
            nine_slice_patches(egui::vec2(32.0, 32.0), [8.0, 8.0, 8.0, 8.0], 1.0, rect, [true; 4]);
        let max_bottom_of_top_row = patches
            .iter()
            .filter(|(dest, _)| dest.min.y == 0.0)
            .map(|(dest, _)| dest.max.y)
            .fold(0.0f32, f32::max);
        assert!((max_bottom_of_top_row - 5.0).abs() < 1e-4);
    }

    #[test]
    fn nine_slice_patches_hidden_side_drops_edge_and_corners_and_extends_rails() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 80.0));
        // Hide the top: [top, right, bottom, left].
        let patches = nine_slice_patches(
            egui::vec2(32.0, 32.0),
            [8.0, 8.0, 8.0, 8.0],
            1.0,
            rect,
            [false, true, true, true],
        );
        // Top edge + both top corners gone.
        assert_eq!(patches.len(), 5);
        assert!(patches.iter().all(|(dest, _)| dest.min.y == 0.0 || dest.min.y >= 72.0));
        // The left rail now runs from the very top of the window.
        let left_rail = patches
            .iter()
            .find(|(dest, _)| dest.min.x == 0.0 && dest.min.y == 0.0 && dest.height() > 8.0)
            .expect("left rail present");
        assert_eq!(left_rail.0.height(), 72.0);
        // All sides hidden = nothing drawn.
        assert!(nine_slice_patches(
            egui::vec2(32.0, 32.0),
            [8.0; 4],
            1.0,
            rect,
            [false; 4]
        )
        .is_empty());
    }

    #[test]
    fn nine_slice_patches_empty_on_degenerate_input() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 80.0));
        assert!(nine_slice_patches(egui::vec2(0.0, 32.0), [8.0; 4], 1.0, rect, [true; 4]).is_empty());
        let empty_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(0.0, 0.0));
        assert!(
            nine_slice_patches(egui::vec2(32.0, 32.0), [8.0; 4], 1.0, empty_rect, [true; 4])
                .is_empty()
        );
    }

    #[test]
    fn doll_anchor_prefers_skin_then_default_then_center() {
        let mut art = SkinWidgetArt::default();
        art.doll_anchors
            .insert("head".to_string(), egui::vec2(0.4, 0.2));
        // Calibrated part; lookup is case-insensitive on the protocol key.
        assert_eq!(art.doll_anchor("Head"), egui::vec2(0.4, 0.2));
        // Uncalibrated known part falls back to the built-in default.
        let [dx, dy] = skins::default_doll_anchor("leftarm").unwrap();
        assert_eq!(art.doll_anchor("leftArm"), egui::vec2(dx, dy));
        // Unknown part lands dead center rather than vanishing.
        assert_eq!(art.doll_anchor("tail"), egui::vec2(0.5, 0.5));
    }

    #[test]
    fn calibration_toml_preserves_comments_and_replaces_tables() {
        let original = r##"# My hand-written skin.
[meta]
name = "Test" # keep me

[injury_doll]
base = "doll/base.png"

# stale calibration to be replaced
[injury_doll.anchors]
head = [0.1, 0.1]

[injury_doll.nsys]
injury1 = "doll/nerves.png"
"##;
        let mut anchors = HashMap::new();
        anchors.insert("head".to_string(), [0.5, 0.09]);
        anchors.insert("neck".to_string(), [0.5, 0.2]);
        let dots = DollDotSpec {
            wound_color: "#aa0000".to_string(),
            ..DollDotSpec::default()
        };
        let updated = calibration_toml(original, &anchors, &dots).unwrap();

        // Hand-written content survives byte-for-byte.
        assert!(updated.contains("# My hand-written skin."));
        assert!(updated.contains(r#"name = "Test" # keep me"#));
        assert!(updated.contains(r#"base = "doll/base.png""#));
        assert!(updated.contains(r#"injury1 = "doll/nerves.png""#));

        // The stale anchor is gone; the round-trip parses to the new values.
        let manifest: SkinManifest = toml::from_str(&updated).unwrap();
        assert_eq!(manifest.injury_doll.anchors.len(), 2);
        assert_eq!(manifest.injury_doll.anchors["head"], [0.5, 0.09]);
        assert_eq!(manifest.injury_doll.anchors["neck"], [0.5, 0.2]);
        assert_eq!(manifest.injury_doll.dots.wound_color, "#aa0000");
        assert_eq!(manifest.injury_doll.parts["nsys"]["injury1"], "doll/nerves.png");
    }

    #[test]
    fn sheet_registration_toml_preserves_comments_and_upserts() {
        let original = r##"# My hand-written skin.
[meta]
name = "Test" # keep me

[sheets.old]
path = "icons/old.png"
cell = 32
"##;
        // New sheet appends; existing content survives byte-for-byte.
        let updated =
            sheet_registration_toml(original, "rogue", "icons/rogue.png", 64).unwrap();
        assert!(updated.contains("# My hand-written skin."));
        assert!(updated.contains(r#"name = "Test" # keep me"#));
        assert!(updated.contains(r#"path = "icons/old.png""#));
        let manifest: SkinManifest = toml::from_str(&updated).unwrap();
        assert_eq!(manifest.sheets["rogue"].path, "icons/rogue.png");
        assert_eq!(manifest.sheets["rogue"].cell, 64);
        assert_eq!(manifest.sheets["old"].cell, 32);

        // Re-registering the same name replaces its entry.
        let updated =
            sheet_registration_toml(&updated, "rogue", "icons/rogue2.png", 48).unwrap();
        let manifest: SkinManifest = toml::from_str(&updated).unwrap();
        assert_eq!(manifest.sheets["rogue"].path, "icons/rogue2.png");
        assert_eq!(manifest.sheets["rogue"].cell, 48);
    }

    #[test]
    fn shared_sheets_merge_respects_skin_precedence_and_absolutizes() {
        let mut sheets = HashMap::new();
        sheets.insert(
            "combat".to_string(),
            SheetSpec {
                path: "icons/combat.png".to_string(),
                cell: 64,
            },
        );

        let mut shared = HashMap::new();
        // Same name (different case): the skin's entry must win.
        shared.insert(
            "Combat".to_string(),
            SheetSpec {
                path: "combat.png".to_string(),
                cell: 32,
            },
        );
        // New name: merged in, with its relative path absolutized.
        shared.insert(
            "spells".to_string(),
            SheetSpec {
                path: "spells.png".to_string(),
                cell: 48,
            },
        );

        let shared_root = if cfg!(windows) {
            Path::new(r"C:\vellum\global\icons")
        } else {
            Path::new("/vellum/global/icons")
        };
        let added = merge_shared_sheets_into(&mut sheets, shared, shared_root);

        assert_eq!(added.len(), 1);
        assert!(added.contains("spells"));
        assert_eq!(sheets["combat"].path, "icons/combat.png");
        assert_eq!(sheets["combat"].cell, 64);
        assert!(!sheets.contains_key("Combat"));
        let spells = &sheets["spells"];
        assert_eq!(spells.cell, 48);
        assert_eq!(
            Path::new(&spells.path),
            shared_root.join("spells.png").as_path()
        );
        assert!(Path::new(&spells.path).is_absolute());
    }

    #[test]
    fn sheet_registration_toml_creates_section_when_absent() {
        let original = "[meta]\nname = \"Bare\"\n";
        let updated =
            sheet_registration_toml(original, "combat", "icons/combat.png", 64).unwrap();
        let manifest: SkinManifest = toml::from_str(&updated).unwrap();
        assert_eq!(manifest.sheets["combat"].path, "icons/combat.png");
        // No stray bare [sheets] header for the implicit parent.
        assert!(!updated.contains("[sheets]\n[sheets."));
    }

    #[test]
    fn calibration_toml_creates_section_when_absent() {
        let original = "[meta]\nname = \"Bare\"\n";
        let mut anchors = HashMap::new();
        anchors.insert("chest".to_string(), [0.5, 0.3]);
        let updated =
            calibration_toml(original, &anchors, &DollDotSpec::default()).unwrap();
        let manifest: SkinManifest = toml::from_str(&updated).unwrap();
        assert_eq!(manifest.meta.name, "Bare");
        assert_eq!(manifest.injury_doll.anchors["chest"], [0.5, 0.3]);
        // No spurious [injury_doll] header for the implicit parent table.
        assert!(!updated.contains("[injury_doll]\n"));
    }

    #[test]
    fn widget_art_lookups_normalize_case() {
        let mut art = SkinWidgetArt::default();
        let texture = SkinTexture {
            texture: egui::TextureId::default(),
            size: egui::vec2(16.0, 16.0),
        };
        art.icons
            .insert("KNEELING".to_string(), IconSlot::Sprite(texture));
        art.compass_dirs.insert("ne".to_string(), texture);
        art.doll_parts
            .entry("leftarm".to_string())
            .or_default()
            .insert(2, texture);

        assert!(art.icon("kneeling").is_some());
        assert!(art.icon("Kneeling").is_some());
        assert!(art.icon("HIDDEN").is_none());
        assert!(art.compass_dir("ne").is_some());
        assert!(art.doll_overlay("leftArm", 2).is_some());
        assert!(art.doll_overlay("leftArm", 3).is_none());
        assert!(!art.is_empty());
        assert!(SkinWidgetArt::default().is_empty());
    }

    #[test]
    fn parse_hex_rgb_accepts_with_and_without_hash() {
        assert_eq!(
            parse_hex_rgb("#ff8800"),
            Some(egui::Color32::from_rgb(0xff, 0x88, 0x00))
        );
        assert_eq!(
            parse_hex_rgb("102030"),
            Some(egui::Color32::from_rgb(0x10, 0x20, 0x30))
        );
        assert_eq!(parse_hex_rgb("#fff"), None);
        assert_eq!(parse_hex_rgb("nothex"), None);
    }
}
