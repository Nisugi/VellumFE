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
    self, BackgroundFit, DollDotSpec, InjuryDollSkin, SkinManifest,
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

/// Widget sprite art resolved from the active skin. Shared into
/// `WidgetRenderSettings` behind an Arc so every render path (including
/// detached viewports) reads the same lookup tables.
#[derive(Debug, Default)]
pub struct SkinWidgetArt {
    /// Indicator id (stored UPPERCASE) -> icon sprite.
    icons: HashMap<String, SkinTexture>,
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
    pub fn icon(&self, id: &str) -> Option<SkinTexture> {
        self.icons.get(&id.to_ascii_uppercase()).copied()
    }

    pub fn compass_dir(&self, direction: &str) -> Option<SkinTexture> {
        self.compass_dirs.get(direction).copied()
    }

    pub fn doll_overlay(&self, part: &str, level: u8) -> Option<SkinTexture> {
        self.doll_parts
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
    /// Last hot-reload poll, so the mtime stat runs at most once a second.
    last_mtime_check: Option<std::time::Instant>,
}

impl SkinState {
    /// Load or unload to match `active` (from config). Call once per frame;
    /// does nothing when the active skin hasn't changed and its skin.toml
    /// is untouched (edits to the manifest hot-reload within a second).
    pub fn apply_if_changed(&mut self, ctx: &egui::Context, active: Option<&str>) {
        if self.applied && self.loaded_id.as_deref() == active {
            if !self.manifest_changed_on_disk() {
                return;
            }
            tracing::info!("skin.toml changed on disk; reloading skin");
        }
        self.applied = true;
        self.loaded_id = active.map(str::to_owned);
        self.manifest = SkinManifest::default();
        self.textures.clear();
        self.widget_art = None;
        self.manifest_mtime = None;

        let Some(name) = active else {
            return;
        };
        match skins::load_manifest(name) {
            Ok((manifest, root)) => {
                self.manifest = manifest;
                self.root = root;
                self.manifest_mtime = skins::manifest_mtime(&self.root);
                self.load_textures(ctx, name);
                self.widget_art = self.build_widget_art();
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

    /// Force a full reload on the next frame (`.reloadskin`). Unlike the
    /// mtime poll this also picks up edited *images*, which don't touch
    /// skin.toml.
    pub fn force_reload(&mut self) {
        self.applied = false;
    }

    /// True when the active skin's manifest mtime differs from what was
    /// loaded. Rate-limited to one stat per second.
    fn manifest_changed_on_disk(&mut self) -> bool {
        if self.loaded_id.is_none() {
            return false;
        }
        let now = std::time::Instant::now();
        if self
            .last_mtime_check
            .is_some_and(|last| now.duration_since(last) < std::time::Duration::from_secs(1))
        {
            return false;
        }
        self.last_mtime_check = Some(now);
        let current = skins::manifest_mtime(&self.root);
        current.is_some() && current != self.manifest_mtime
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

    /// The loaded manifest's injury doll section (for seeding the
    /// calibrator with the current anchors and dot styling).
    pub fn doll_manifest(&self) -> &InjuryDollSkin {
        &self.manifest.injury_doll
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
                art.icons.insert(id.to_ascii_uppercase(), texture);
            }
        }
        art.compass_rose = self.manifest.compass.rose.as_ref().and_then(tex);
        for (direction, path) in &self.manifest.compass.directions {
            if let Some(texture) = tex(path) {
                art.compass_dirs
                    .insert(direction.to_ascii_lowercase(), texture);
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
        images.extend(self.manifest.icons.values().cloned());
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

    /// Resolve the nine-slice border for a window, falling back to the
    /// manifest's "default" entry (independently of the background, so a
    /// window can override one without losing the other).
    pub fn border_for(&self, window_name: &str) -> Option<ResolvedBorder> {
        let spec =
            skins::window_field(&self.manifest, window_name, |window| window.border.as_ref())?;
        let texture = self.textures.get(&spec.image)?.as_ref()?;
        Some(ResolvedBorder {
            texture: texture.id(),
            tex_size: texture.size_vec2(),
            slice: spec.slice,
            scale: spec.scale.max(0.05),
        })
    }
}

fn load_texture(
    ctx: &egui::Context,
    root: &Path,
    image_path: &str,
    skin_name: &str,
) -> Option<egui::TextureHandle> {
    let path = {
        let raw = Path::new(image_path);
        if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            root.join(raw)
        }
    };
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
    let rgba = decoded.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture(
        format!("skin:{}:{}", skin_name, image_path),
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

/// Paint a window background into `rect`, clipped to it. `scrim_color`
/// supplies the scrim's RGB (normally the theme's window fill) so the
/// overlay darkens/lightens toward the theme rather than plain black.
pub fn paint_background(
    painter: &egui::Painter,
    rect: egui::Rect,
    bg: &ResolvedBackground,
    scrim_color: egui::Color32,
) {
    if !rect.is_positive() || bg.tex_size.x <= 0.0 || bg.tex_size.y <= 0.0 {
        return;
    }
    let painter = painter.with_clip_rect(rect);
    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    match bg.fit {
        BackgroundFit::Stretch => {
            painter.image(bg.texture, rect, full_uv, bg.tint);
        }
        BackgroundFit::Cover => {
            let uv = cover_uv(bg.tex_size, rect.size());
            painter.image(bg.texture, rect, uv, bg.tint);
        }
        BackgroundFit::Contain => {
            let dest = contain_dest(bg.tex_size, rect);
            painter.image(bg.texture, dest, full_uv, bg.tint);
        }
        BackgroundFit::Center => {
            let dest = egui::Rect::from_center_size(rect.center(), bg.tex_size);
            painter.image(bg.texture, dest, full_uv, bg.tint);
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
                    let dest = egui::Rect::from_min_size(min, bg.tex_size);
                    painter.image(bg.texture, dest, full_uv, bg.tint);
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
        painter.rect_filled(rect, 0.0, scrim);
    }
}

/// Largest rect with the sprite's aspect ratio centered inside `rect`.
/// Layered sprites (compass rose + overlays, doll base + overlays) should
/// all be painted into the dest computed from the *base* sprite so
/// same-canvas art stays aligned.
pub fn sprite_dest(sprite: &SkinTexture, rect: egui::Rect) -> egui::Rect {
    contain_dest(sprite.size, rect)
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
    std::fs::write(&manifest_path, updated)
        .map_err(|err| anyhow::anyhow!("cannot write {}: {}", manifest_path.display(), err))?;
    Ok(())
}

/// Paint a nine-slice border into `rect`: corners at fixed size, edges
/// stretched along their axis, center left empty so the window fill or
/// background image shows through.
pub fn paint_nine_slice(painter: &egui::Painter, rect: egui::Rect, border: &ResolvedBorder) {
    let full_alpha = egui::Color32::WHITE;
    for (dest, uv) in nine_slice_patches(border.tex_size, border.slice, border.scale, rect) {
        painter.image(border.texture, dest, uv, full_alpha);
    }
}

/// The eight border patches as (destination rect, UV rect) pairs. Slice
/// insets larger than the destination shrink proportionally so opposite
/// borders never overlap. Degenerate patches (zero-size) are skipped.
fn nine_slice_patches(
    tex: egui::Vec2,
    slice: [f32; 4],
    scale: f32,
    rect: egui::Rect,
) -> Vec<(egui::Rect, egui::Rect)> {
    if tex.x <= 0.0 || tex.y <= 0.0 || !rect.is_positive() {
        return Vec::new();
    }
    let [top, right, bottom, left] = slice.map(|inset| inset.max(0.0));

    // On-screen border thicknesses, shrunk if the rect is too small.
    let mut dt = top * scale;
    let mut db = bottom * scale;
    if dt + db > rect.height() {
        let shrink = rect.height() / (dt + db);
        dt *= shrink;
        db *= shrink;
    }
    let mut dl = left * scale;
    let mut dr = right * scale;
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
fn parse_hex_rgb(input: &str) -> Option<egui::Color32> {
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
        let patches = nine_slice_patches(egui::vec2(32.0, 32.0), [8.0, 8.0, 8.0, 8.0], 1.0, rect);
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
        let patches = nine_slice_patches(egui::vec2(32.0, 32.0), [8.0, 8.0, 8.0, 8.0], 1.0, rect);
        let max_bottom_of_top_row = patches
            .iter()
            .filter(|(dest, _)| dest.min.y == 0.0)
            .map(|(dest, _)| dest.max.y)
            .fold(0.0f32, f32::max);
        assert!((max_bottom_of_top_row - 5.0).abs() < 1e-4);
    }

    #[test]
    fn nine_slice_patches_empty_on_degenerate_input() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 80.0));
        assert!(nine_slice_patches(egui::vec2(0.0, 32.0), [8.0; 4], 1.0, rect).is_empty());
        let empty_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(0.0, 0.0));
        assert!(nine_slice_patches(egui::vec2(32.0, 32.0), [8.0; 4], 1.0, empty_rect).is_empty());
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
        art.icons.insert("KNEELING".to_string(), texture);
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
