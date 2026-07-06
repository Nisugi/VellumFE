//! GUI skin system: user-supplied graphics layered on top of themes.
//!
//! A skin is a directory under `~/.vellum-fe/skins/<name>/` containing a
//! `skin.toml` manifest plus image assets. Themes own colors and fonts;
//! skins own graphics. This module currently covers window background
//! images; border nine-slices and icon sets build on the same manifest in
//! later phases. Everything falls back to plain theme rendering when no
//! skin is active or an asset fails to load.
//!
//! Manifest format:
//!
//! ```toml
//! [meta]
//! name = "Parchment"
//! description = "Warm paper backgrounds for text windows"
//!
//! # Applies to every window without its own [window.<name>] entry.
//! [window.default.background]
//! image = "bg/paper.png"   # relative to the skin directory (absolute paths allowed)
//! fit = "cover"            # stretch | cover | contain | tile | center
//! opacity = 0.85           # 0.0..=1.0
//! tint = "#c0a878"         # optional multiply tint
//! scrim = 0.3              # 0.0..=1.0 theme-colored overlay for text readability
//!
//! # Windows are matched by their layout window name ("main", "thoughts", ...).
//! [window.main.background]
//! image = "bg/vellum.png"
//! scrim = 0.5
//! ```
//!
//! Image paths are usually relative to the skin directory; absolute paths
//! are allowed on purpose so a skin can reference assets from another
//! install (e.g. a user's local Wrayth art) without copying them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Parsed skin.toml.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkinManifest {
    #[serde(default)]
    pub meta: SkinMeta,
    /// Per-window graphics keyed by layout window name; the "default" entry
    /// applies to windows without their own entry.
    #[serde(default, rename = "window")]
    pub windows: HashMap<String, WindowSkin>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkinMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WindowSkin {
    #[serde(default)]
    pub background: Option<BackgroundSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackgroundSpec {
    /// Image path, relative to the skin directory (absolute allowed).
    pub image: String,
    #[serde(default)]
    pub fit: BackgroundFit,
    /// Image opacity, 0.0..=1.0.
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Optional multiply tint as "#rrggbb".
    #[serde(default)]
    pub tint: Option<String>,
    /// Strength (0.0..=1.0) of a theme-colored overlay painted over the
    /// image so window text stays readable. 0 disables it.
    #[serde(default)]
    pub scrim: f32,
}

fn default_opacity() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundFit {
    /// Fill the window, distorting aspect ratio.
    Stretch,
    /// Fill the window, cropping whatever overflows.
    #[default]
    Cover,
    /// Show the whole image, letterboxed and centered.
    Contain,
    /// Repeat the image at its native size from the top-left.
    Tile,
    /// Native size, centered, no scaling.
    Center,
}

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
    applied: bool,
}

impl SkinState {
    /// Load or unload to match `active` (from config). Call once per frame;
    /// does nothing when the active skin hasn't changed.
    pub fn apply_if_changed(&mut self, ctx: &egui::Context, active: Option<&str>) {
        if self.applied && self.loaded_id.as_deref() == active {
            return;
        }
        self.applied = true;
        self.loaded_id = active.map(str::to_owned);
        self.manifest = SkinManifest::default();
        self.textures.clear();

        let Some(name) = active else {
            return;
        };
        match load_manifest(name) {
            Ok((manifest, root)) => {
                self.manifest = manifest;
                self.root = root;
                self.load_textures(ctx, name);
            }
            Err(err) => {
                tracing::warn!("Failed to load skin '{}': {:#}", name, err);
            }
        }
    }

    fn load_textures(&mut self, ctx: &egui::Context, skin_name: &str) {
        let images: Vec<String> = self
            .manifest
            .windows
            .values()
            .filter_map(|window| window.background.as_ref())
            .map(|bg| bg.image.clone())
            .collect();
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
        let spec = window_background(&self.manifest, window_name)?;
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
}

/// Manifest lookup for a window: exact name, then case-insensitive, then
/// the "default" entry.
fn window_background<'a>(
    manifest: &'a SkinManifest,
    window_name: &str,
) -> Option<&'a BackgroundSpec> {
    let entry = manifest.windows.get(window_name).or_else(|| {
        manifest
            .windows
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(window_name))
            .map(|(_, window)| window)
    });
    entry
        .and_then(|window| window.background.as_ref())
        .or_else(|| {
            manifest
                .windows
                .get("default")
                .and_then(|window| window.background.as_ref())
        })
}

/// Read and parse `skins/<name>/skin.toml`. Returns the manifest and the
/// skin directory (for resolving relative image paths).
pub fn load_manifest(name: &str) -> anyhow::Result<(SkinManifest, PathBuf)> {
    let root = crate::config::Config::skins_dir()?.join(name);
    let manifest_path = root.join("skin.toml");
    let contents = std::fs::read_to_string(&manifest_path)
        .map_err(|err| anyhow::anyhow!("cannot read {}: {}", manifest_path.display(), err))?;
    let manifest: SkinManifest = toml::from_str(&contents)
        .map_err(|err| anyhow::anyhow!("invalid {}: {}", manifest_path.display(), err))?;
    Ok((manifest, root))
}

/// Skin directory names that contain a skin.toml, sorted.
pub fn list_skins() -> Vec<String> {
    let Ok(dir) = crate::config::Config::skins_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut skins: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join("skin.toml").is_file())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    skins.sort();
    skins
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

    fn manifest(toml_src: &str) -> SkinManifest {
        toml::from_str(toml_src).expect("manifest should parse")
    }

    #[test]
    fn manifest_parses_defaults_and_per_window_entries() {
        let manifest = manifest(
            r##"
            [meta]
            name = "Test"

            [window.default.background]
            image = "bg/paper.png"

            [window.main.background]
            image = "bg/vellum.png"
            fit = "tile"
            opacity = 0.5
            tint = "#ff8800"
            scrim = 0.25
            "##,
        );
        assert_eq!(manifest.meta.name, "Test");

        let default_bg = manifest.windows["default"].background.as_ref().unwrap();
        assert_eq!(default_bg.image, "bg/paper.png");
        assert_eq!(default_bg.fit, BackgroundFit::Cover);
        assert_eq!(default_bg.opacity, 1.0);
        assert_eq!(default_bg.scrim, 0.0);
        assert!(default_bg.tint.is_none());

        let main_bg = manifest.windows["main"].background.as_ref().unwrap();
        assert_eq!(main_bg.fit, BackgroundFit::Tile);
        assert_eq!(main_bg.opacity, 0.5);
        assert_eq!(main_bg.tint.as_deref(), Some("#ff8800"));
        assert_eq!(main_bg.scrim, 0.25);
    }

    #[test]
    fn window_lookup_falls_back_to_default() {
        let manifest = manifest(
            r#"
            [window.default.background]
            image = "default.png"

            [window.main.background]
            image = "main.png"
            "#,
        );
        assert_eq!(window_background(&manifest, "main").unwrap().image, "main.png");
        assert_eq!(window_background(&manifest, "Main").unwrap().image, "main.png");
        assert_eq!(
            window_background(&manifest, "thoughts").unwrap().image,
            "default.png"
        );
    }

    #[test]
    fn window_lookup_without_default_is_none() {
        let manifest = manifest(
            r#"
            [window.main.background]
            image = "main.png"
            "#,
        );
        assert!(window_background(&manifest, "thoughts").is_none());
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
