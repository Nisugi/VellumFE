//! GUI pool-art rendering: user-supplied graphics layered on top of themes.
//!
//! The image-pool conventions and the canonical injury doll part table
//! live in `crate::config::skins`/`crate::config::pool` (shared with the
//! web frontend, which compiles without egui). This module owns everything
//! egui: texture loading, the appearance-driven runtime state, widget
//! sprite lookups, and the paint helpers. The legacy live-manifest skin
//! runtime is gone — skins are inert presets applied to the appearance
//! store (`config::skin_pack`); everything here resolves from the pool.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::skins::{self, BackgroundFit, DollDotSpec, SheetSpec};

/// Everything a renderer needs to paint one window background. Resolved
/// once per frame from the appearance assignments, then handed to render paths (some
/// of which run in detached viewports without access to the app).
#[derive(Debug, Clone)]
pub struct ResolvedBackground {
    pub texture: egui::TextureId,
    pub tex_size: egui::Vec2,
    pub fit: BackgroundFit,
    /// Tile-mode multiplier over the image's native size (1.0 = native).
    /// Fits other than Tile ignore it.
    pub tile_scale: f32,
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
    /// Per-part suppression conditions (lowercase part -> condition):
    /// while one holds, that part draws nothing at all — no overlay, no
    /// dot. Encodes anatomical dependencies (hand under a severed arm)
    /// in the skin instead of a client-side anatomy tree.
    doll_hidden_when: HashMap<String, crate::config::Condition>,
    /// Conditional doll variants in declaration order; when one's
    /// condition matches, its set replaces the default doll_* fields
    /// wholesale (full replace). Empty when a doll override is active
    /// (pool dolls carry no variants).
    doll_variants: Vec<LoadedDollVariant>,
    /// Named standalone doll sets (`[injury_doll.sets.<name>]`), bound by
    /// name from a window's `doll_set`. Loaded even while a doll override
    /// is active — the override replaces only the default doll.
    doll_sets: HashMap<String, LoadedDollSet>,
    /// Hotbar icon sprite sheets keyed by lowercased sheet name.
    sheets: HashMap<String, SheetArt>,
    /// Nine-slice art for interactive dialog-panel controls, keyed by
    /// lowercase `"<control>"` or `"<control>.<state>"` (e.g. "button",
    /// "button.hover", "dropdown").
    controls: HashMap<String, ResolvedBorder>,
    /// Decorative edge overlays keyed by edge ("top"/"right"/"bottom"/"left"),
    /// painted over the nine-slice border along that window edge.
    edges: HashMap<String, ResolvedEdge>,
}

/// A loaded edge overlay: an optional tiling/stretched strip along the edge
/// plus an optional corner ornament, both as textures with their scale.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedEdge {
    pub strip: Option<SkinTexture>,
    pub tile: bool,
    pub ornament: Option<SkinTexture>,
    /// true = anchor ornament to the END (bottom/right); false = START.
    pub anchor_end: bool,
    /// Inward reach from the edge in on-screen points (already scaled), or
    /// None to use the strip's cross-axis size.
    pub thickness: Option<f32>,
    pub scale: f32,
}

/// One loaded conditional doll variant: the activation condition plus a
/// complete doll set (textures resolved at skin load, so activation is
/// just a lookup swap at render time).
#[derive(Debug)]
struct LoadedDollVariant {
    name: String,
    when: crate::config::Condition,
    set: LoadedDollSet,
}

/// One fully loaded doll set's art and metadata — the shape shared by
/// condition variants and named standalone sets.
#[derive(Debug, Default)]
struct LoadedDollSet {
    base: Option<SkinTexture>,
    base_gray: Option<SkinTexture>,
    parts: HashMap<String, HashMap<u8, SkinTexture>>,
    parts_gray: HashMap<String, HashMap<u8, SkinTexture>>,
    anchors: HashMap<String, egui::Vec2>,
    hidden_when: HashMap<String, crate::config::Condition>,
    dots: ResolvedDotStyle,
}

impl LoadedDollSet {
    fn view(&self) -> DollSetView<'_> {
        DollSetView {
            base: self.base,
            base_gray: self.base_gray,
            parts: &self.parts,
            parts_gray: &self.parts_gray,
            anchors: &self.anchors,
            hidden_when: &self.hidden_when,
            dots: self.dots,
        }
    }
}

/// Borrowed view of one doll set — the default `[injury_doll]` art or an
/// active variant's — so the renderer draws either through one interface.
#[derive(Clone, Copy)]
pub struct DollSetView<'a> {
    pub base: Option<SkinTexture>,
    pub base_gray: Option<SkinTexture>,
    parts: &'a HashMap<String, HashMap<u8, SkinTexture>>,
    parts_gray: &'a HashMap<String, HashMap<u8, SkinTexture>>,
    anchors: &'a HashMap<String, egui::Vec2>,
    hidden_when: &'a HashMap<String, crate::config::Condition>,
    pub dots: ResolvedDotStyle,
}

impl DollSetView<'_> {
    /// Hand-drawn overlay for a part at a severity level (0 = healthy).
    pub fn overlay(&self, part: &str, level: u8) -> Option<SkinTexture> {
        self.parts
            .get(&part.to_ascii_lowercase())
            .and_then(|levels| levels.get(&level))
            .copied()
    }

    /// Grayscale overlay twin; None unless "grayscale doll" is on.
    pub fn overlay_gray(&self, part: &str, level: u8) -> Option<SkinTexture> {
        self.parts_gray
            .get(&part.to_ascii_lowercase())
            .and_then(|levels| levels.get(&level))
            .copied()
    }

    /// True when the part ships any overlay art (healthy or severity).
    /// Such a part is fully hand-drawn: levels without art let the base
    /// show through instead of falling back to a generated dot.
    pub fn has_overlays(&self, part: &str) -> bool {
        self.parts
            .get(&part.to_ascii_lowercase())
            .is_some_and(|levels| !levels.is_empty())
    }

    /// Dot anchor for a body part: this set's calibrated point, else the
    /// built-in default, else dead center (unknown part).
    pub fn anchor(&self, part: &str) -> egui::Vec2 {
        let key = part.to_ascii_lowercase();
        self.anchors
            .get(&key)
            .copied()
            .or_else(|| skins::default_doll_anchor(&key).map(|[x, y]| egui::vec2(x, y)))
            .unwrap_or_else(|| egui::vec2(0.5, 0.5))
    }

    /// Parts this set suppresses right now: each `hidden_when` condition
    /// evaluated against the character's state. A hidden part draws
    /// nothing — no overlay, no dot — at any severity. Lowercase part
    /// keys, matching the set's internal maps.
    pub fn hidden_parts(
        &self,
        gs: &crate::core::state::GameState,
        now_server: i64,
        gameobj: Option<&crate::core::gameobj_data::GameObjData>,
    ) -> std::collections::HashSet<String> {
        self.hidden_when
            .iter()
            .filter(|(_, condition)| {
                crate::core::conditions::eval_condition(condition, gs, now_server, gameobj)
            })
            .map(|(part, _)| part.clone())
            .collect()
    }
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
                    size: egui::vec2(texture.size.x * uv.width(), texture.size.y * uv.height()),
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
                let (texture, uv) = self.sheet_cell(&sheet.to_ascii_lowercase(), *cell, false)?;
                Some(ResolvedIcon {
                    texture: texture.texture,
                    size: egui::vec2(texture.size.x * uv.width(), texture.size.y * uv.height()),
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

    /// Decorative edge overlay for one window edge ("top"/"right"/"bottom"/
    /// "left"), or None when the skin authored none for it.
    pub fn edge(&self, edge: &str) -> Option<&ResolvedEdge> {
        self.edges.get(&edge.to_ascii_lowercase())
    }

    /// Whether any edge overlay exists (cheap gate for the paint pass).
    pub fn has_edges(&self) -> bool {
        !self.edges.is_empty()
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
                    size: egui::vec2(texture.size.x * uv.width(), texture.size.y * uv.height()),
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
            .or_else(|| skins::default_doll_anchor(&key).map(|[x, y]| egui::vec2(x, y)))
            .unwrap_or(egui::vec2(0.5, 0.5))
    }

    /// View of one doll set: `Some(index)` = that variant's set (index
    /// from `resolve_doll_variant`), `None` or out-of-range = the default
    /// `[injury_doll]` set. Callers that must never variant-swap (another
    /// player's doll) simply pass `None`.
    pub fn doll_set(&self, variant: Option<usize>) -> DollSetView<'_> {
        match variant.and_then(|index| self.doll_variants.get(index)) {
            Some(v) => v.set.view(),
            None => DollSetView {
                base: self.doll_base,
                base_gray: self.doll_base_gray,
                parts: &self.doll_parts,
                parts_gray: &self.doll_parts_gray,
                anchors: &self.doll_anchors,
                hidden_when: &self.doll_hidden_when,
                dots: self.doll_dots,
            },
        }
    }

    /// Evaluate variant conditions against the character's state, in
    /// declaration order; first match wins, None = use the default set.
    /// Conditions read SELF state, so this is only meaningful for the
    /// character's own doll — never resolve for another player's.
    pub fn resolve_doll_variant(
        &self,
        gs: &crate::core::state::GameState,
        now_server: i64,
        gameobj: Option<&crate::core::gameobj_data::GameObjData>,
    ) -> Option<usize> {
        self.doll_variants.iter().position(|variant| {
            crate::core::conditions::eval_condition(&variant.when, gs, now_server, gameobj)
        })
    }

    /// View of a NAMED doll set (a window's `doll_set` binding): a
    /// `[injury_doll.sets.<name>]` entry first, else a condition variant
    /// of that name (pinned — its condition is ignored). Case-insensitive.
    /// None when the skin defines neither (callers fall back to the
    /// default resolution so a stale binding degrades gracefully).
    pub fn doll_set_named(&self, name: &str) -> Option<DollSetView<'_>> {
        self.doll_sets
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, set)| set.view())
            .or_else(|| {
                self.doll_variants
                    .iter()
                    .find(|v| v.name.eq_ignore_ascii_case(name))
                    .map(|v| v.set.view())
            })
    }

    /// Names a window's `doll_set` binding can resolve: the named sets
    /// (sorted) followed by the condition variants (declaration order),
    /// deduped case-insensitively. For the per-window picker.
    pub fn doll_set_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.doll_sets.keys().cloned().collect();
        names.sort_by_cached_key(|name| name.to_ascii_lowercase());
        for variant in &self.doll_variants {
            if !names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&variant.name))
            {
                names.push(variant.name.clone());
            }
        }
        names
    }

    /// Names of the loaded doll variants, in declaration order (for
    /// editors and diagnostics).
    pub fn doll_variant_names(&self) -> Vec<&str> {
        self.doll_variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect()
    }

    fn is_empty(&self) -> bool {
        self.icons.is_empty()
            && self.compass_rose.is_none()
            && self.compass_dirs.is_empty()
            && self.doll_base.is_none()
            && self.doll_parts.is_empty()
            && self.doll_variants.is_empty()
            && self.doll_sets.is_empty()
            && self.sheets.is_empty()
            && self.pool_icons.is_empty()
            // Skinless assignments can be the only art in the bundle.
            && self.controls.is_empty()
            && self.edges.is_empty()
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
            egui::pos2(col as f32 * cell_px / size.x, row as f32 * cell_px / size.y),
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

/// Loaded art for one creature-card base image.
#[derive(Clone)]
pub struct CreatureArt {
    pub texture: egui::TextureHandle,
    /// Head anchor as fractions of the image: the sidecar's authored point
    /// when present, else derived (top-centre of the alpha bbox). A
    /// manifest-calibrated anchor wins over this.
    pub head: [f32; 2],
    /// Foot / ground-contact anchor: sidecar-authored, else derived
    /// (bottom-centre of the alpha bbox). The sprite hangs off the floor
    /// position by this point, so per-pose art grounds itself.
    pub feet: [f32; 2],
    /// Alpha bbox as fractions [x0, y0, x1, y1] — body-wrap overlays scale
    /// to this, not the full canvas, so padding in the art costs nothing.
    pub bbox: [f32; 4],
    /// All sidecar-authored anchors (feet/head/mouth/saddle + doll parts),
    /// image fractions. Empty when no sidecar.
    pub anchors: HashMap<String, [f32; 2]>,
    /// Sidecar-authored floor footprint (contact-shadow ellipse), if any.
    pub footprint: Option<crate::config::pool::CreatureFootprint>,
    /// Sidecar-authored world-unit height for THIS image, overriding the
    /// per-family size the field otherwise applies.
    pub size: Option<f32>,
    /// Sidecar-authored ground clearance for a floating neutral pose,
    /// fraction of the drawn sprite height.
    pub lift: Option<f32>,
    /// Opaque pixel count (alpha >= 32): the sprite's body mass, for
    /// pose-ratio sanity warnings.
    pub alpha_px: u64,
    /// Tier extras beside the base: `{token}_<suffix>.png` files keyed by
    /// lowercased suffix ("prone", "chest2", "leftarm1", ...). The locked
    /// tier owns them all — pose swaps and per-wound overlays never mix
    /// across tiers.
    pub extras: HashMap<String, PathBuf>,
}

impl CreatureArt {
    /// Sidecar anchor by name, case-insensitive.
    pub fn anchor(&self, name: &str) -> Option<[f32; 2]> {
        self.anchors
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, a)| *a)
    }

    /// Tier extra by suffix ("prone", "chest2"), case-insensitive.
    pub fn extra(&self, suffix: &str) -> Option<&PathBuf> {
        self.extras.get(&suffix.to_ascii_lowercase())
    }

    /// Whether the tier ships any per-wound overlay art (tier locking:
    /// if so, the manifest's part tables never mix in).
    pub fn has_wound_extras(&self) -> bool {
        self.extras
            .keys()
            .any(|key| key.ends_with(|c: char| c.is_ascii_digit()))
    }
}

/// Lazily loaded creature-card art, shared with the creaturefield renderer.
/// Bases resolve per creature through the `[creature_card]` cascade and are
/// negative-cached (a noun with no art is one lookup, ever); the shared
/// status-overlay textures load once per skin.
#[derive(Default)]
pub struct CreatureArtCache {
    /// noun -> resolved art (None = nothing on disk). A noun maps to one
    /// family, so the noun alone keys the cascade's result.
    pub bases: HashMap<String, Option<CreatureArt>>,
    /// Variant base-override path -> full art (texture + anchors + bbox +
    /// footprint), so pose art carries its own grounding metadata instead
    /// of inheriting the standing base's.
    pub variant_bases: HashMap<String, Option<CreatureArt>>,
    /// Overlay manifest path -> texture (None = load failed).
    pub overlays: HashMap<String, Option<egui::TextureHandle>>,
    /// Shared overlay path -> calibrated `overlay_scale` (fraction of the
    /// wearing creature's drawn width; sidecar-authored). Only paths whose
    /// sidecar set one appear — renderers fall back to per-role defaults.
    pub overlay_scales: HashMap<String, f32>,
    /// Scenery-prop pool path ("scenery/rock.png") -> full art. Props load
    /// through the creature loader so each carries its own feet anchor,
    /// footprint, and world size (sidecar or alpha-derived).
    pub scenery: HashMap<String, Option<CreatureArt>>,
    /// Scene background pool path ("scenes/desert.png") -> texture.
    pub scene_backgrounds: HashMap<String, Option<egui::TextureHandle>>,
    /// The `[creature_card]` template: the built-in default (pool resolve
    /// cascade + convention status overlays) — creature art never requires
    /// a skin.
    pub card: skins::CreatureCardSkin,
    /// Resolve root: the image pool.
    root: PathBuf,
    skin_name: String,
}

// TextureHandle has no Debug; summarize (WidgetRenderSettings derives it).
impl std::fmt::Debug for CreatureArt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatureArt")
            .field("head", &self.head)
            .field("feet", &self.feet)
            .field("bbox", &self.bbox)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for CreatureArtCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatureArtCache")
            .field("bases", &self.bases.len())
            .field("overlays", &self.overlays.len())
            .finish_non_exhaustive()
    }
}

impl CreatureArtCache {
    /// Base art for one creature, if prepared. Key mirrors `prepare`.
    pub fn base(&self, noun: &str) -> Option<&CreatureArt> {
        self.bases.get(noun).and_then(|art| art.as_ref())
    }

    /// Variant pose art for a base-override path, if prepared.
    pub fn variant_base(&self, path: &str) -> Option<&CreatureArt> {
        self.variant_bases.get(path).and_then(|art| art.as_ref())
    }

    /// Scenery-prop art for a pool path, if prepared.
    pub fn scenery(&self, pool_path: &str) -> Option<&CreatureArt> {
        self.scenery.get(pool_path).and_then(|art| art.as_ref())
    }

    /// Calibrated overlay scale for a shared overlay path, if authored.
    pub fn overlay_scale(&self, pool_path: &str) -> Option<f32> {
        self.overlay_scales.get(pool_path).copied()
    }

    /// Scene background texture for a pool path, if prepared.
    pub fn scene_background(&self, pool_path: &str) -> Option<&egui::TextureHandle> {
        self.scene_backgrounds
            .get(pool_path)
            .and_then(|tex| tex.as_ref())
    }
}

/// Content aspect (width/height) of decoded art from its alpha bounds:
/// transparent padding changes `bbox` and the canvas together, so the
/// content aspect — and every geometry derived from it — is unaffected;
/// the same art at a different resolution yields the same value.
pub(crate) fn content_aspect(bbox: [f32; 4], tex_w: f32, tex_h: f32) -> f32 {
    let w = ((bbox[2] - bbox[0]) * tex_w).max(1.0);
    let h = ((bbox[3] - bbox[1]) * tex_h).max(1.0);
    w / h
}

/// Content pixel height of decoded art from its alpha bounds. Transparent
/// padding grows the canvas, not the content, so padding is neutral here.
pub(crate) fn content_height_px(bbox: [f32; 4], tex_h: f32) -> f32 {
    ((bbox[3] - bbox[1]) * tex_h).max(1.0)
}

/// One pose image's calibration for the core geometry contract: the
/// sidecar's absolute `size`, the alpha-derived content aspect, the
/// footprint's contact span, and — for pose art measured against a
/// standing base — the pose-to-standing content ratio (finding 7): the
/// exact factor the renderer's inherited common pixel scale draws the
/// pose at, so core bounds match the drawn sprite. Pass `base: None` for
/// the standing pose itself.
fn pose_calibration(
    art: &CreatureArt,
    base: Option<&CreatureArt>,
) -> crate::core::creature_cards::geometry::PoseCalibration {
    let ts = art.texture.size_vec2();
    crate::core::creature_cards::geometry::PoseCalibration {
        size: art.size.filter(|s| s.is_finite() && *s > 0.0),
        aspect: Some(content_aspect(art.bbox, ts.x, ts.y)),
        span: art.footprint.map(|fp| (fp.rx * 2.0).clamp(0.2, 1.0)),
        content_ratio: base.map(|b| {
            let bts = b.texture.size_vec2();
            content_height_px(art.bbox, ts.y) / content_height_px(b.bbox, bts.y)
        }),
    }
}

/// One creature the field wants art for this frame: the identity keys
/// for tier resolution plus the current state that decides which extras
/// (pose, wound overlays) must be loaded.
#[derive(Debug, Clone)]
pub struct WantedCreature {
    /// Live display name (boon adjectives still on; normalization is the
    /// cache's job so every caller keys identically).
    pub name: String,
    pub noun: Option<String>,
    pub family: Option<String>,
    /// Per-part wound ranks — loads `{token}_{loc}{rank}` overlays.
    pub injuries: Vec<(String, u8)>,
}

/// Per-skin, per-frame handle to `CreatureArtCache`: loading happens in the
/// frame update (`prepare_creature_art`, &mut self), renderers only read.
/// The mutex is uncontended — everything runs on the UI thread.
pub type SharedCreatureArt = std::sync::Arc<std::sync::Mutex<CreatureArtCache>>;

/// Runtime pool-art state owned by the GUI app: the appearance-driven
/// declarations plus their loaded textures.
#[derive(Default)]
pub struct SkinState {
    /// Resolve root for pool-relative paths (the shared image pool).
    root: PathBuf,
    /// Loaded textures keyed by pool-relative image path (+ "#gray"
    /// twins), synced incrementally — unchanged files keep their textures
    /// across appearance changes.
    store: super::image_store::ImageStore,
    /// Widget sprite lookups built once per reload.
    widget_art: Option<std::sync::Arc<SkinWidgetArt>>,
    applied: bool,
    /// Shared hotbar icon sheets (global/images/icons), keyed by name.
    sheets: HashMap<String, SheetSpec>,
    /// Injury doll override as a pool-relative path (from
    /// ui_settings.doll_image); base from the pool image, calibration from
    /// the image's sidecar toml.
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
    /// Pool dolls referenced by per-window `doll_set` bindings (pool
    /// paths, "dolls/x.png"): each loads as a named doll set keyed by its
    /// path, so two doll windows can show two different pool dolls with
    /// no skin at all.
    needed_pool_dolls: Vec<String>,
    /// Active statusicons pool set (lowercase `<set>_` prefix).
    statusicon_set: Option<String>,
    /// Compass pool set (lowercase prefix); None = no compass art set.
    compass_set: Option<String>,
    /// Control-face assignments (lowercase control key -> pool frame
    /// stem).
    control_frames: HashMap<String, String>,
    /// Edge-overlay pool set. "none" strips edge art entirely.
    edge_set: Option<String>,
    /// Resolved edge set: lowercase role ("top", "top-ornament", ...) ->
    /// pool path.
    pool_edges: HashMap<String, String>,
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
    /// Parsed background sidecars for the needed pool backgrounds, keyed
    /// by pool path. Cached here because background resolution runs per
    /// frame — the sidecar files are read only on the (appearance-change)
    /// reload pass, never during render.
    pool_background_meta: HashMap<String, PoolBackgroundMeta>,
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
    /// Creature-card art, lazily resolved per creature (see
    /// `prepare_creature_art`).
    creature_art: SharedCreatureArt,
}

impl SkinState {
    /// Load or unload to match the doll override (from the layout's
    /// appearance settings) and the declared pool assignments. Call once
    /// per frame; does nothing when nothing changed and the shared
    /// icons.toml is untouched (edits hot-reload within a second).
    pub fn apply_if_changed(&mut self, ctx: &egui::Context, doll_override: Option<&str>) {
        // Per-frame decode allowance for picker thumbnails (this runs
        // once per frame regardless of art changes).
        self.thumb_budget = 3;
        if self.applied && self.doll_override.as_deref() == doll_override {
            if !self.manifest_changed_on_disk() {
                return;
            }
            tracing::info!("shared icons.toml changed on disk; reloading pool art");
        }
        self.applied = true;
        self.doll_override = doll_override.map(str::to_owned);
        self.root = crate::config::Config::global_images_dir().unwrap_or_default();
        self.pool_frames = load_pool_frames(&self.needed_pool_frames);
        self.pool_background_meta =
            load_pool_background_meta(&self.root, &self.needed_pool_backgrounds);
        self.pool_status_icons = load_pool_set("statusicons", self.statusicon_set.as_deref());
        self.pool_compass = load_pool_set("compass", self.compass_set.as_deref());
        self.pool_edges = load_pool_set("edges", self.edge_set.as_deref());
        self.widget_art = None;
        self.sheets.clear();
        self.shared_sheet_names.clear();
        self.shared_manifest_mtime = None;

        // Shared sheets (global/images/icons) so hotbar icons work with no
        // other art at all.
        self.merge_shared_sheets();

        self.sync_textures(ctx, "pool");
        self.widget_art = self.build_widget_art();

        // Reset the creature-card art cache: bases resolve lazily per
        // creature (prepare_creature_art), overlay textures load there too
        // on first demand. The template is the built-in default (pool
        // resolve cascade) rooted at the pool; convention status overlays
        // fold in.
        {
            let mut card = skins::CreatureCardSkin::default();
            card.overlays
                .extend(crate::core::creature_cards::convention_status_overlays(
                    &card.overlays,
                ));
            // Global wound art (creatures/wounds/<part><rank>.png) fills
            // the part tables; tiers with their own wound extras ignore
            // them (tier locking).
            crate::core::creature_cards::convention_wound_parts(&mut card.parts);
            let mut cache = self.creature_art.lock().expect("creature art lock");
            *cache = CreatureArtCache {
                card,
                root: self.root.clone(),
                skin_name: "pool".to_string(),
                ..Default::default()
            };
        }
        // Art (and its sidecars) may have changed on disk: drop the core
        // geometry-calibration store so it re-seeds; placed units
        // recalibrate in place on the revision bump (nobody moves).
        crate::core::creature_cards::geometry::calibrations()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .invalidate();
    }

    /// Shared handle to the creature-card art cache for renderers.
    pub fn creature_art(&self) -> SharedCreatureArt {
        self.creature_art.clone()
    }

    /// Resolve + load base art for the given creatures and the card's
    /// overlay textures. Called once per frame from the update loop with
    /// the current field roster; everything is cached (including misses),
    /// so a settled room costs a few hash lookups. Bases are keyed by the
    /// creature's NAME token (boon-stripped slug of obj.name — matches
    /// the art folder/file naming), tier-locked through
    /// `resolve_tier_art`; pose and wound overlay textures for each
    /// creature's current state load on demand and stay path-cached.
    pub fn prepare_creature_art(&mut self, ctx: &egui::Context, wanted: &[WantedCreature]) {
        use crate::core::creature_cards::naming;
        let cache = self.creature_art.clone();
        let mut cache = cache.lock().expect("creature art lock");
        for want in wanted {
            let token = naming::name_token(&want.name);
            if token.is_empty() {
                continue;
            }
            if !cache.bases.contains_key(token.as_str()) {
                let art = crate::core::creature_cards::resolve_tier_art(
                    &cache.root,
                    &cache.card,
                    Some(&want.name),
                    want.noun.as_deref(),
                    want.family.as_deref(),
                )
                .and_then(|tier| load_creature_art(ctx, &tier.base, &cache.skin_name));
                cache.bases.insert(token.clone(), art);
            }
            // The tier's prone pose loads as full creature art (own
            // anchors/footprint) UNCONDITIONALLY — not on first prone —
            // so its geometry metadata (content ratio, sidecar size,
            // footprint) reaches the core store before the solver ever
            // reserves this creature's standing/prone envelope (finding
            // 8). Wound overlays stay demand-loaded from the CURRENT
            // state. Both keyed by absolute path so re-loads are lookups.
            let Some(Some(art)) = cache.bases.get(token.as_str()) else {
                continue;
            };
            let prone = art.extra("prone").cloned();
            let mut wounds: Vec<PathBuf> = want
                .injuries
                .iter()
                .filter_map(|(part, rank)| {
                    art.extra(&format!("{}{rank}", part.to_ascii_lowercase()))
                        .cloned()
                })
                .collect();
            if let Some(path) = prone {
                let key = path.to_string_lossy().into_owned();
                if !cache.variant_bases.contains_key(&key) {
                    let art = load_creature_art(ctx, &path, &cache.skin_name);
                    cache.variant_bases.insert(key.clone(), art);
                }
                // Prone wound layers ({token}_prone_{loc}{rank}) are the
                // PRONE art's own extras (its stem prefixes them), so a
                // downed creature's wounds draw pose-matched art instead
                // of standing layers on a sprawled body.
                if let Some(Some(prone_art)) = cache.variant_bases.get(&key) {
                    wounds.extend(want.injuries.iter().filter_map(|(part, rank)| {
                        prone_art
                            .extra(&format!("{}{rank}", part.to_ascii_lowercase()))
                            .cloned()
                    }));
                }
            }
            // Refine the core geometry-calibration store with the decoded
            // art's content data: alpha-bounds aspect (transparent padding
            // and art resolution both neutral), sidecar size/footprint,
            // and the prone pose's own calibration once its art loads.
            // `refine` bumps the store revision only on actual change;
            // `sync_field` recalibrates placed units in place on the bump.
            if let Some(Some(art)) = cache.bases.get(token.as_str()) {
                let prone_cal = art
                    .extra("prone")
                    .and_then(|p| cache.variant_bases.get(p.to_string_lossy().as_ref()))
                    .and_then(|a| a.as_ref())
                    .map(|prone| pose_calibration(prone, Some(art)));
                let cal = crate::core::creature_cards::geometry::ArtCalibration {
                    standing: pose_calibration(art, None),
                    prone: prone_cal,
                };
                crate::core::creature_cards::geometry::calibrations()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .refine(&token, cal);
            }
            for path in wounds {
                let key = path.to_string_lossy().into_owned();
                if !cache.overlays.contains_key(&key) {
                    let name = cache.skin_name.clone();
                    // Anchored tier wounds size by the art's calibrated
                    // overlay_scale, exactly like the shared wounds pool.
                    if let Some(scale) = crate::config::pool::read_sidecar::<
                        crate::config::pool::CreatureSidecar,
                    >(&path)
                    .and_then(|sidecar| sidecar.overlay_scale)
                    {
                        cache.overlay_scales.insert(key.clone(), scale);
                    }
                    let tex = super::image_store::load_texture_file(
                        ctx,
                        &path,
                        &format!("wound:{key}"),
                        &name,
                    );
                    cache.overlays.insert(key, tex);
                }
            }
        }
        // Placeholder-free variant bases (pose art) load through the full
        // creature loader so each pose image carries its own derived +
        // sidecar anchors and footprint; a variant with a {family}/{noun}
        // template resolves per creature (not yet sourced), so those keep
        // the ground pose for now.
        let variant_paths: Vec<String> = cache
            .card
            .variants
            .iter()
            .filter_map(|v| v.skin.base.clone())
            .filter(|p| !p.contains('{'))
            .collect();
        for path in variant_paths {
            if cache.variant_bases.contains_key(&path) {
                continue;
            }
            let abs = skins::resolve_image_path(&cache.root, &path);
            let art = abs
                .is_file()
                .then(|| load_creature_art(ctx, &abs, &cache.skin_name))
                .flatten();
            if art.is_none() {
                tracing::warn!(
                    "Skin '{}': variant base '{}' missing or unloadable",
                    cache.skin_name,
                    path
                );
            }
            cache.variant_bases.insert(path, art);
        }
        // Part-table wound art (the global creatures/wounds convention and
        // any authored tables): small fixed set, preloaded with the shared
        // overlays below so `part_overlay` hits are texture lookups.
        let part_paths: Vec<String> = cache
            .card
            .parts
            .values()
            .chain(
                cache
                    .card
                    .variants
                    .iter()
                    .flat_map(|v| v.skin.parts.values()),
            )
            .flat_map(|spec| spec.overlays.values().cloned())
            .filter(|path| !path.contains('{'))
            .collect();
        // Shared overlay textures: small set, loaded once. Placeholder
        // paths ({severity}) expand 1-3.
        let overlay_paths: Vec<String> = part_paths.into_iter().chain(cache.card.overlays.iter().flat_map(|o| {
                if o.image.contains("{severity}") {
                    (1..=3)
                        .map(|s| o.image.replace("{severity}", &s.to_string()))
                        .collect::<Vec<_>>()
                } else if o.image.contains('{') {
                    // Per-family/noun status art is the asset explosion the
                    // plan forbids; refuse to expand it.
                    tracing::warn!(
                        "creature_card overlay '{}' uses a per-creature placeholder; \
                         status art is shared - overlay skipped",
                        o.image
                    );
                    Vec::new()
                } else {
                    vec![o.image.clone()]
                }
            }))
            .collect();
        for path in overlay_paths {
            if cache.overlays.contains_key(&path) {
                continue;
            }
            let abs = skins::resolve_image_path(&cache.root, &path);
            // Calibrated size rides the art's sidecar, like everything
            // else about an image's physical reality.
            if let Some(scale) = crate::config::pool::read_sidecar::<
                crate::config::pool::CreatureSidecar,
            >(&abs)
            .and_then(|sidecar| sidecar.overlay_scale)
            {
                cache.overlay_scales.insert(path.clone(), scale);
            }
            let tex = super::image_store::load_texture_file(
                ctx,
                &abs,
                &format!("skin:{}:{}", cache.skin_name, path),
                &cache.skin_name,
            );
            cache.overlays.insert(path, tex);
        }
    }

    /// Load a scene's art (background texture + prop art) into the
    /// creature-art cache, misses included, so a settled scene costs hash
    /// lookups only. Called per frame by whoever renders with a scene (the
    /// Studio Stage today; the game passes no scene and never calls this).
    pub fn prepare_scene_art(&mut self, ctx: &egui::Context, scene: &crate::config::scenes::StageScene) {
        let cache = self.creature_art.clone();
        let mut cache = cache.lock().expect("creature art lock");
        if let Some(bg) = &scene.background {
            if !cache.scene_backgrounds.contains_key(bg) {
                let abs = skins::resolve_image_path(&cache.root, bg);
                let tex = super::image_store::load_texture_file(
                    ctx,
                    &abs,
                    &format!("scene-bg:{bg}"),
                    "scene",
                );
                cache.scene_backgrounds.insert(bg.clone(), tex);
            }
        }
        for prop in &scene.props {
            if cache.scenery.contains_key(&prop.image) {
                continue;
            }
            let abs = skins::resolve_image_path(&cache.root, &prop.image);
            let art = abs
                .is_file()
                .then(|| load_creature_art(ctx, &abs, "scene"))
                .flatten();
            if art.is_none() {
                tracing::warn!("scene prop '{}' missing or unloadable", prop.image);
            }
            cache.scenery.insert(prop.image.clone(), art);
        }
    }

    /// Load the shared icon store's sheets (global/images/icons); shared
    /// paths are absolutized against the shared directory so they resolve
    /// from the pool root.
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
        self.shared_sheet_names = merge_shared_sheets_into(&mut self.sheets, shared, &shared_root);
    }

    /// Force a full reload on the next frame (`.reloadskin`). Unlike the
    /// mtime poll this also picks up edited *images*, which don't touch
    /// the shared icons.toml.
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

    /// Declare which pool dolls per-window `doll_set` bindings reference
    /// (pool paths). Call before `apply_if_changed`; a change triggers a
    /// reload.
    pub fn set_needed_pool_dolls(&mut self, paths: impl IntoIterator<Item = String>) {
        let mut paths: Vec<String> = paths.into_iter().collect();
        paths.sort();
        paths.dedup();
        if paths != self.needed_pool_dolls {
            self.needed_pool_dolls = paths;
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

    /// Declare the control-face assignments (control key -> pool frame
    /// stem). Call before `apply_if_changed`; a change triggers a reload.
    pub fn set_control_frames(&mut self, assignments: &HashMap<String, String>) {
        let assignments: HashMap<String, String> = assignments
            .iter()
            .map(|(key, stem)| (key.to_ascii_lowercase(), stem.to_ascii_lowercase()))
            .collect();
        if assignments != self.control_frames {
            self.control_frames = assignments;
            self.applied = false;
        }
    }

    /// Declare the edge-overlay pool set. Call before `apply_if_changed`;
    /// a change triggers a reload.
    pub fn set_edge_set(&mut self, set: Option<&str>) {
        let set = set.map(|s| s.to_ascii_lowercase());
        if set != self.edge_set {
            self.edge_set = set;
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

    /// True when the shared icons.toml mtime differs from what was loaded.
    /// Rate-limited to one stat per second.
    fn manifest_changed_on_disk(&mut self) -> bool {
        let now = std::time::Instant::now();
        if self
            .last_mtime_check
            .is_some_and(|last| now.duration_since(last) < std::time::Duration::from_secs(1))
        {
            return false;
        }
        self.last_mtime_check = Some(now);
        // != (not is_some &&) so deleting icons.toml also unloads its sheets.
        shared_icons_mtime() != self.shared_manifest_mtime
    }

    /// Sprite lookups for widget renderers; None when no widget art is
    /// assigned (renderers then use their vector drawings).
    pub fn widget_art(&self) -> Option<std::sync::Arc<SkinWidgetArt>> {
        self.widget_art.clone()
    }

    /// True when `sheet` (any case) came from the shared icon store.
    pub fn sheet_is_shared(&self, sheet: &str) -> bool {
        self.shared_sheet_names
            .contains(&sheet.to_ascii_lowercase())
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
            self.store.texture(path).map(|handle| SkinTexture {
                texture: handle.id(),
                size: handle.size_vec2(),
            })
        };

        let mut art = SkinWidgetArt::default();
        // Pool set art fills each glyph id.
        for (id, path) in &self.pool_status_icons {
            if let Some(texture) = tex(path) {
                art.icons
                    .entry(id.to_ascii_uppercase())
                    .or_insert(IconSlot::Sprite(texture));
            }
        }
        // Per-indicator overrides beat the set.
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
                art.pool_icons.insert(path.to_ascii_lowercase(), texture);
            }
        }
        for (name, spec) in &self.sheets {
            if spec.cell == 0 {
                tracing::warn!("Icon sheet '{}': cell size must be > 0", name);
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
        // User control-face assignments (pool frames), for dialog-panel
        // controls (button/dropdown/... states).
        for (key, stem) in &self.control_frames {
            let border = self
                .pool_frames
                .get(stem)
                .and_then(|spec| self.resolve_border(spec));
            if let Some(border) = border {
                art.controls.insert(key.clone(), border);
            }
        }
        // Decorative edge overlays from the edge pool set. Roles:
        // `<side>.png` strips, `<side>-ornament.png` corner art; paint
        // parameters ride in each strip's edge sidecar. The "none"
        // sentinel strips edge art entirely.
        if !self
            .edge_set
            .as_deref()
            .is_some_and(|set| set.eq_ignore_ascii_case("none"))
            && !self.pool_edges.is_empty()
        {
            for side in ["top", "right", "bottom", "left"] {
                let strip = self.pool_edges.get(side).and_then(tex);
                let ornament = self
                    .pool_edges
                    .get(&format!("{side}-ornament"))
                    .and_then(tex);
                if strip.is_none() && ornament.is_none() {
                    continue;
                }
                let sidecar = self
                    .pool_edges
                    .get(side)
                    .map(|path| skins::resolve_image_path(&self.root, path))
                    .and_then(|abs| {
                        crate::config::pool::read_sidecar::<crate::config::pool::EdgeSidecar>(&abs)
                    })
                    .unwrap_or_default();
                let scale = sidecar.scale.unwrap_or(1.0).max(0.05);
                art.edges.insert(
                    side.to_string(),
                    ResolvedEdge {
                        strip,
                        ornament,
                        tile: sidecar.tile,
                        anchor_end: sidecar
                            .anchor
                            .as_deref()
                            .map(|a| a.eq_ignore_ascii_case("end"))
                            .unwrap_or(false),
                        thickness: sidecar.thickness.map(|t| t * scale),
                        scale,
                    },
                );
            }
        }
        // The compass pool set: rose + direction overlays (same-canvas
        // art). The "none" sentinel (picker "None") leaves compass art
        // empty so the widget draws its vector rose.
        if !self
            .compass_set
            .as_deref()
            .is_some_and(|set| set.eq_ignore_ascii_case("none"))
        {
            if let Some(rose) = self.pool_compass.get("rose").and_then(tex) {
                art.compass_rose = Some(rose);
                for (role, path) in &self.pool_compass {
                    if role == "rose" {
                        continue;
                    }
                    if let Some(texture) = tex(path) {
                        art.compass_dirs.insert(role.clone(), texture);
                    }
                }
            }
        }

        // The doll override: base from the pool image, anchors/dots from
        // its sidecar, severity rendered as generated dots (pool dolls
        // carry no overlay art). The "none" sentinel (picker "None")
        // leaves doll art empty so the widget draws its built-in vector
        // body.
        if let Some(path) = &self.doll_override {
            if !path.eq_ignore_ascii_case("none") {
                if let Some(texture) = tex(path) {
                    art.doll_base = Some(texture);
                    let abs = skins::resolve_image_path(&self.root, path);
                    match crate::config::pool::read_sidecar::<crate::config::pool::DollSidecar>(
                        &abs,
                    ) {
                        Some(sidecar) => {
                            for (part, anchor) in &sidecar.anchors {
                                art.doll_anchors.insert(
                                    part.to_ascii_lowercase(),
                                    egui::vec2(
                                        anchor[0].clamp(0.0, 1.0),
                                        anchor[1].clamp(0.0, 1.0),
                                    ),
                                );
                            }
                            art.doll_dots = ResolvedDotStyle::from_spec(&sidecar.dots);
                        }
                        None => art.doll_dots = ResolvedDotStyle::default(),
                    }
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
            art.doll_base_gray = self
                .doll_override
                .clone()
                .and_then(|p| tex(&format!("{p}#gray")));
        }

        // Pool dolls bound per-window load as named sets keyed by their
        // pool path — two doll windows can show two different pool dolls.
        // Base from the pool image, anchors/dots from its sidecar,
        // severity as generated dots (like the global override).
        for path in &self.needed_pool_dolls {
            let Some(texture) = tex(path) else {
                continue;
            };
            let abs = skins::resolve_image_path(&self.root, path);
            let sidecar = crate::config::pool::read_sidecar::<crate::config::pool::DollSidecar>(
                &abs,
            )
            .unwrap_or_default();
            let mut set = LoadedDollSet {
                base: Some(texture),
                dots: ResolvedDotStyle::from_spec(&sidecar.dots),
                ..Default::default()
            };
            for (part, anchor) in &sidecar.anchors {
                set.anchors.insert(
                    part.to_ascii_lowercase(),
                    egui::vec2(anchor[0].clamp(0.0, 1.0), anchor[1].clamp(0.0, 1.0)),
                );
            }
            if self.gray_doll {
                set.base_gray = tex(&format!("{path}#gray"));
            }
            art.doll_sets.insert(path.clone(), set);
        }

        if art.is_empty() {
            None
        } else {
            Some(std::sync::Arc::new(art))
        }
    }

    /// Sync the texture store to everything the pool sets and declared
    /// overrides reference. Incremental: unchanged files keep their
    /// textures (a checkbox toggle no longer re-decodes the world),
    /// no-longer-referenced entries free theirs, edited files reload.
    fn sync_textures(&mut self, ctx: &egui::Context, skin_name: &str) {
        let mut images: Vec<String> =
            self.pool_frames.values().map(|frame| frame.image.clone()).collect();
        images.extend(self.needed_pool_backgrounds.iter().cloned());
        images.extend(self.needed_pool_icons.iter().cloned());
        images.extend(self.needed_pool_dolls.iter().cloned());
        images.extend(self.doll_override.iter().cloned());
        images.extend(self.pool_status_icons.values().cloned());
        images.extend(self.pool_compass.values().cloned());
        images.extend(self.pool_edges.values().cloned());
        images.extend(
            self.statusicon_overrides
                .values()
                .filter_map(|icon| match icon {
                    crate::data::IconRef::Image { path } => Some(path.clone()),
                    _ => None,
                }),
        );
        images.extend(self.sheets.values().map(|s| s.path.clone()));
        // Grayscale twins for hotbar sheets (barbar's gs variant), cached
        // under a synthetic "<path>#gray" key.
        let mut gray_paths: Vec<String> =
            self.sheets.values().map(|spec| spec.path.clone()).collect();
        // Lazy grayscale twins: built only for what the checkboxes demand
        // (status icons when "gray inactive" is on, doll art when
        // "grayscale doll" is on). Unchecking drops them.
        if self.gray_status_icons {
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
            gray_paths.extend(self.needed_pool_dolls.iter().cloned());
            gray_paths.extend(self.doll_override.iter().cloned());
        }
        // One wanted list: bases first, then gray twins (order lets the
        // store skip a twin whose base failed). Keys stay the manifest /
        // pool-relative strings every lookup site uses; resolution to a
        // file happens here so the store can key change detection on it.
        let mut wanted: Vec<super::image_store::WantedImage> = images
            .into_iter()
            .map(|key| super::image_store::WantedImage {
                path: skins::resolve_image_path(&self.root, &key),
                key,
                gray: false,
            })
            .collect();
        wanted.extend(
            gray_paths
                .into_iter()
                .map(|path| super::image_store::WantedImage {
                    key: format!("{path}{}", super::image_store::GRAY_SUFFIX),
                    path: skins::resolve_image_path(&self.root, &path),
                    gray: true,
                }),
        );
        self.store.sync(ctx, &wanted, skin_name);
    }

    /// Background resolution honoring a per-window override: "none" (and
    /// no override at all) means no background art; a pool-relative path
    /// renders that image with readable defaults (a light theme scrim,
    /// cover fit unless the image's sidecar picks another mode).
    pub fn background_for_with_override(
        &self,
        _window_name: &str,
        background_override: Option<&str>,
    ) -> Option<ResolvedBackground> {
        match background_override {
            Some(path) if path.eq_ignore_ascii_case("none") => None,
            Some(path) => {
                let texture = self.store.texture(path)?;
                let meta = self.pool_background_meta.get(path);
                Some(ResolvedBackground {
                    texture: texture.id(),
                    tex_size: texture.size_vec2(),
                    fit: meta.and_then(|m| m.fit).unwrap_or(BackgroundFit::Cover),
                    tile_scale: meta.map(|m| m.tile_scale).unwrap_or(1.0),
                    tint: egui::Color32::WHITE,
                    // Text stays readable over arbitrary pool art.
                    scrim_alpha: (0.25 * 255.0) as u8,
                })
            }
            None => None,
        }
    }

    /// The fit a pool background's sidecar declares, for the Appearance
    /// menu's checkmarks. None = no declaration (the cover default).
    pub fn pool_background_fit(&self, path: &str) -> Option<BackgroundFit> {
        self.pool_background_meta.get(path).and_then(|meta| meta.fit)
    }

    /// Border resolution honoring a per-window user override: "none" (and
    /// no override at all) means no frame; a pool frame stem nine-slices
    /// through its sidecar. An unknown name (stale layout) resolves to
    /// nothing.
    pub fn border_for_with_override(
        &self,
        _window_name: &str,
        frame_override: Option<&str>,
    ) -> Option<ResolvedBorder> {
        match frame_override {
            Some(name) if name.eq_ignore_ascii_case(skins::NO_FRAME) => None,
            Some(name) => self
                .pool_frames
                .get(&name.to_ascii_lowercase())
                .and_then(|spec| self.resolve_border(spec)),
            None => None,
        }
    }

    fn resolve_border(&self, spec: &skins::BorderSpec) -> Option<ResolvedBorder> {
        let texture = self.store.texture(&spec.image)?;
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

    /// Frames the Appearance picker offers: every pool frame with a
    /// sidecar (names only — textures load lazily for frames actually
    /// assigned).
    pub fn frame_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for image in crate::config::pool::list_category("frames") {
            let stem = image.stem();
            if stem.eq_ignore_ascii_case(skins::NO_FRAME) {
                continue;
            }
            // Without a slice/scale sidecar the frame can't nine-slice;
            // leave it out rather than offering a dead entry. Uses the
            // scan-time flag: this runs every frame a picker is open.
            if !image.has_sidecar {
                continue;
            }
            if !names.iter().any(|name| name.eq_ignore_ascii_case(stem)) {
                names.push(stem.to_owned());
            }
        }
        names.sort_by_cached_key(|name| name.to_ascii_lowercase());
        names
    }
}

/// Resolve one pool set to `role -> pool path` (glyph ids for statusicons,
/// directions for compass). No set selected = empty. Set membership itself
/// is decided by `pool::set_members`, which handles both the foldered and
/// legacy `<set>_<role>` layouts.
fn load_pool_set(category: &str, set: Option<&str>) -> HashMap<String, String> {
    match set {
        Some(set) => crate::config::pool::set_members(category, set),
        None => HashMap::new(),
    }
}

/// Parsed background sidecar for one pool background image: how the art
/// wants to be painted, resolved once per reload pass (see
/// `pool_background_meta`).
#[derive(Debug, Clone, Copy)]
struct PoolBackgroundMeta {
    /// Declared fit; None = no declaration (renderers use cover).
    fit: Option<BackgroundFit>,
    /// Tile-mode scale multiplier, already defaulted and clamped.
    tile_scale: f32,
}

/// Map a sidecar fit string to the renderer's mode. An unrecognized value
/// degrades to None (the cover default) with a warning — bad metadata must
/// never blank a window.
fn parse_background_fit(fit: &str, source: &str) -> Option<BackgroundFit> {
    match fit.to_ascii_lowercase().as_str() {
        "stretch" => Some(BackgroundFit::Stretch),
        "cover" => Some(BackgroundFit::Cover),
        "contain" => Some(BackgroundFit::Contain),
        "tile" => Some(BackgroundFit::Tile),
        "center" => Some(BackgroundFit::Center),
        other => {
            tracing::warn!(
                "background sidecar for '{}' declares unknown fit '{}'; using cover",
                source,
                other
            );
            None
        }
    }
}

/// Read the sidecars for the needed pool backgrounds. Images without one
/// (or with unusable metadata) simply take the defaults — a background
/// never fails to paint over its metadata.
fn load_pool_background_meta(
    root: &std::path::Path,
    needed: &[String],
) -> HashMap<String, PoolBackgroundMeta> {
    let mut meta = HashMap::new();
    for path in needed {
        let abs = skins::resolve_image_path(root, path);
        let Some(sidecar) =
            crate::config::pool::read_sidecar::<crate::config::pool::BackgroundSidecar>(&abs)
        else {
            continue;
        };
        meta.insert(
            path.clone(),
            PoolBackgroundMeta {
                fit: sidecar
                    .fit
                    .as_deref()
                    .and_then(|fit| parse_background_fit(fit, path)),
                tile_scale: sidecar.effective_scale(),
            },
        );
    }
    meta
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

/// Decode one creature-card base image and derive its anchors from the
/// alpha bbox: head = top-centre, feet = bottom-centre. Calibration for the
/// common case comes free from the art itself; a `<image>.toml` sidecar
/// (anchors + footprint) overrides the derivation per image, so pose art
/// grounds itself (manifest anchors still win when authored).
fn load_creature_art(ctx: &egui::Context, path: &Path, skin_name: &str) -> Option<CreatureArt> {
    let rgba = super::image_store::decode_rgba_logged(path, skin_name)?;
    let (w, h) = (rgba.width(), rgba.height());
    if w == 0 || h == 0 {
        return None;
    }
    // Alpha bbox (threshold matches the palette sampler's) + opaque pixel
    // count (the pose-scale normalizer keys on body mass, not box height).
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    let mut alpha_px: u64 = 0;
    for (x, y, px) in rgba.enumerate_pixels() {
        if px.0[3] >= 32 {
            alpha_px += 1;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    let bbox = if x0 <= x1 && y0 <= y1 {
        [
            x0 as f32 / w as f32,
            y0 as f32 / h as f32,
            (x1 + 1) as f32 / w as f32,
            (y1 + 1) as f32 / h as f32,
        ]
    } else {
        [0.0, 0.0, 1.0, 1.0] // fully transparent: degenerate but harmless
    };
    let mid_x = (bbox[0] + bbox[2]) / 2.0;
    let size = [w as usize, h as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let texture = ctx.load_texture(
        format!("creature:{}", path.display()),
        color_image,
        egui::TextureOptions::LINEAR,
    );
    // Sidecar metadata travels with the image: authored anchors replace
    // the derived head/feet, and the footprint drives the contact shadow.
    let sidecar = crate::config::pool::read_sidecar::<crate::config::pool::CreatureSidecar>(path)
        .unwrap_or_default();
    // Tier extras: images beside the base named "{token}_<suffix>" (pose
    // art, per-wound overlays), discovered once per load. Tier locking:
    // these are the ONLY overlays this creature's art may use.
    let mut extras: HashMap<String, PathBuf> = HashMap::new();
    if let (Some(dir), Some(stem)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) {
        let prefix = format!("{stem}_");
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let extra = entry.path();
                if !extra.is_file() {
                    continue;
                }
                let is_image = extra
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        matches!(
                            ext.to_ascii_lowercase().as_str(),
                            "png" | "webp" | "jpg" | "jpeg" | "bmp"
                        )
                    });
                if !is_image {
                    continue;
                }
                let Some(suffix) = extra
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.strip_prefix(&prefix))
                else {
                    continue;
                };
                if !suffix.is_empty() {
                    extras.insert(suffix.to_ascii_lowercase(), extra);
                }
            }
        }
    }
    let sidecar_pt = |name: &str| {
        sidecar
            .anchors
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, a)| *a)
    };
    // Template-canvas art (scale-spec): pixels ARE feet (px ÷
    // px_per_foot), and the baseline sits on the bottom edge — the art
    // self-calibrates, no bbox-to-bestiary guessing. The renderer's
    // authored-size path maps `size` onto the alpha-content height, so
    // the derived size measures the CONTENT at the canvas's px/ft (the
    // net scale is exactly world-units-per-pixel × px_per_foot,
    // independent of where the content sits on the canvas). An authored
    // `size` still wins; the derived value feeds the same field so the
    // renderer needs no second path.
    let template_ppf = sidecar.px_per_foot.filter(|ppf| *ppf > 0.0);
    let template_size = template_ppf.map(|ppf| {
        (bbox[3] - bbox[1]) * h as f32 / ppf * crate::core::creature_cards::UNITS_PER_FOOT
    });
    let template_feet = template_ppf.map(|_| [mid_x, 1.0]);
    Some(CreatureArt {
        texture,
        head: sidecar_pt("head").unwrap_or([mid_x, bbox[1]]),
        feet: sidecar_pt("feet")
            .or(template_feet)
            .unwrap_or([mid_x, bbox[3]]),
        bbox,
        anchors: sidecar.anchors,
        footprint: sidecar.footprint,
        size: sidecar.size.or(template_size),
        lift: sidecar.lift,
        alpha_px,
        extras,
    })
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
    let decoded = image::DynamicImage::ImageRgba8(super::image_store::decode_rgba(&path)?);
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
            // Sidecar scale multiplies the native tile size (floor guards a
            // hand-edited zero from dividing the rect by nothing).
            let tile = bg.tex_size * bg.tile_scale.max(0.05);
            let cols = ((rect.width() / tile.x).ceil() as usize).min(MAX_TILES_PER_AXIS);
            let rows = ((rect.height() / tile.y).ceil() as usize).min(MAX_TILES_PER_AXIS);
            for row in 0..rows {
                for col in 0..cols {
                    let min = rect.min + egui::vec2(col as f32 * tile.x, row as f32 * tile.y);
                    shapes.push(image(egui::Rect::from_min_size(min, tile), full_uv));
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
    let luminance = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if luminance > 140.0 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

/// Insert (or replace) one `[sheets.<name>]` entry in an icon-sheet manifest, preserving
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

/// Register a hotbar icon sprite sheet into the shared store
/// (`global/images/icons/`), where every skin — and a skinless setup —
/// can use it. Creates the store and its icons.toml on first use.
pub fn register_sheet_shared(sheet_name: &str, source: &Path, cell: u32) -> anyhow::Result<()> {
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
    register_sheet_impl(
        &root,
        &manifest_path,
        &contents,
        "",
        sheet_name,
        source,
        cell,
    )
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

/// Paint an edge overlay (strip + optional corner ornament) along one window
/// edge, over the nine-slice border. The strip runs the edge's full length
/// (tiled or stretched, `thickness` deep); the ornament is drawn at native
/// size anchored to one end. No-op when the edge carries neither.
pub fn paint_edge_overlay(
    painter: &egui::Painter,
    window: egui::Rect,
    edge_name: &str,
    edge: &ResolvedEdge,
    top_inset: f32,
) {
    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let vertical = matches!(edge_name, "left" | "right");
    // Strip thickness: explicit override, else the strip's cross-axis native
    // size (× scale) — width for a vertical edge, height for a horizontal one.
    let thickness = edge.thickness.unwrap_or_else(|| {
        edge.strip
            .map(|s| {
                let cross = if vertical { s.size.x } else { s.size.y };
                cross * edge.scale
            })
            .unwrap_or(0.0)
    });
    let ornament_size = edge
        .ornament
        .map(|o| o.size * edge.scale)
        .unwrap_or(egui::Vec2::ZERO);
    let layout = edge_overlay_layout(
        window,
        edge_name,
        thickness,
        ornament_size,
        edge.anchor_end,
        top_inset,
    );

    if let Some(strip) = edge.strip {
        if edge.tile {
            // Tile along the edge's long axis at the sprite's native length.
            let painter = painter.with_clip_rect(layout.strip);
            let step = if vertical {
                (strip.size.y * edge.scale).max(1.0)
            } else {
                (strip.size.x * edge.scale).max(1.0)
            };
            let (start, end) = if vertical {
                (layout.strip.top(), layout.strip.bottom())
            } else {
                (layout.strip.left(), layout.strip.right())
            };
            let mut p = start;
            while p < end {
                let cell = if vertical {
                    egui::Rect::from_min_size(
                        egui::pos2(layout.strip.left(), p),
                        egui::vec2(layout.strip.width(), step),
                    )
                } else {
                    egui::Rect::from_min_size(
                        egui::pos2(p, layout.strip.top()),
                        egui::vec2(step, layout.strip.height()),
                    )
                };
                painter.image(strip.texture, cell, full_uv, egui::Color32::WHITE);
                p += step;
            }
        } else {
            painter.image(strip.texture, layout.strip, full_uv, egui::Color32::WHITE);
        }
    }
    if let Some(ornament) = edge.ornament {
        painter.image(
            ornament.texture,
            layout.ornament,
            full_uv,
            egui::Color32::WHITE,
        );
    }
}

/// Geometry for an edge overlay along one side of `window`. Returns the
/// `strip` rect (runs the length of the edge, `thickness` deep, flush INSIDE
/// the edge) and the `ornament` rect (native `ornament_size`, kept INSIDE the
/// window and flush to the edge). Pure so the layout is unit-tested apart from
/// the paint path.
///
/// `edge` is "top" | "right" | "bottom" | "left". `thickness` is the inward
/// reach (points). `anchor_end` puts the ornament at the far end. `top_inset`
/// (vertical edges only) starts the strip AND ornament below the title bar so a
/// corner flourish lines up with the body top instead of overlapping the bar.
///
/// The ornament is pinned so it never spills OUTSIDE the window: on the right
/// edge its right side aligns to the window's right (extending inward); on the
/// left edge its left side aligns to the window's left; likewise top/bottom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeOverlayLayout {
    pub strip: egui::Rect,
    pub ornament: egui::Rect,
}

pub fn edge_overlay_layout(
    window: egui::Rect,
    edge: &str,
    thickness: f32,
    ornament_size: egui::Vec2,
    anchor_end: bool,
    top_inset: f32,
) -> EdgeOverlayLayout {
    debug_assert!(
        matches!(edge, "top" | "right" | "bottom" | "left"),
        "edge_overlay_layout: unknown edge '{edge}' (falls back to top)"
    );
    let t = thickness.max(0.0);
    let inset = top_inset.max(0.0);
    // A window smaller than the ornament must not have the ornament spill
    // past its far side onto neighbors — shrink it to fit (keeps the
    // flush-to-edge anchoring below valid on tiny windows).
    let ornament_size = ornament_size.min(window.size());
    let (strip, ornament) = match edge {
        "left" | "right" => {
            // Strip flush to the edge, inside the window; starts below the
            // title bar (top_inset).
            let x0 = if edge == "left" {
                window.left()
            } else {
                window.right() - t
            };
            let strip = egui::Rect::from_min_max(
                egui::pos2(x0, window.top() + inset),
                egui::pos2(x0 + t, window.bottom()),
            );
            // Ornament stays INSIDE: on the right edge its RIGHT side hugs the
            // window's right (grows leftward); on the left edge its LEFT side
            // hugs the window's left. Vertically it sits at the body top
            // (top_inset) or the bottom end.
            let ox = if edge == "right" {
                window.right() - ornament_size.x
            } else {
                window.left()
            };
            let oy = if anchor_end {
                window.bottom() - ornament_size.y
            } else {
                window.top() + inset
            };
            let ornament = egui::Rect::from_min_size(egui::pos2(ox, oy), ornament_size);
            (strip, ornament)
        }
        _ => {
            // "top" | "bottom" (default): horizontal strip.
            let y0 = if edge == "bottom" {
                window.bottom() - t
            } else {
                window.top()
            };
            let strip = egui::Rect::from_min_max(
                egui::pos2(window.left(), y0),
                egui::pos2(window.right(), y0 + t),
            );
            // Keep the ornament inside: bottom edge hugs the window bottom for
            // the bottom edge; top edge hugs the window top otherwise.
            let oy = if edge == "bottom" {
                window.bottom() - ornament_size.y
            } else {
                window.top()
            };
            let ox = if anchor_end {
                window.right() - ornament_size.x
            } else {
                window.left()
            };
            let ornament = egui::Rect::from_min_size(egui::pos2(ox, oy), ornament_size);
            (strip, ornament)
        }
    };
    EdgeOverlayLayout { strip, ornament }
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
    for (dest, uv) in nine_slice_patches(border.tex_size, border.slice, border.scale, rect, sides) {
        painter.image(border.texture, dest, uv, full_alpha);
    }
}

/// Nine-slice for a control FACE (button, tab, dropdown): like
/// `paint_nine_slice` but the center patch is painted too, stretched from
/// the sprite's own center region. A window frame wants the hollow center
/// (content shows through); a button face painted hollow shows the window
/// mesh through its middle — the reported dark box behind every combat
/// button label.
pub fn paint_nine_slice_filled(painter: &egui::Painter, rect: egui::Rect, border: &ResolvedBorder) {
    let full_alpha = egui::Color32::WHITE;
    for (dest, uv) in nine_slice_patches_impl(
        border.tex_size,
        border.slice,
        border.scale,
        rect,
        [true; 4],
        true,
    ) {
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
    nine_slice_patches_impl(tex, slice, scale, rect, sides, false)
}

fn nine_slice_patches_impl(
    tex: egui::Vec2,
    slice: [f32; 4],
    scale: f32,
    rect: egui::Rect,
    sides: [bool; 4],
    include_center: bool,
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
    let ux = [
        0.0,
        (left / tex.x).min(1.0),
        1.0 - (right / tex.x).min(1.0),
        1.0,
    ];
    let uy = [
        0.0,
        (top / tex.y).min(1.0),
        1.0 - (bottom / tex.y).min(1.0),
        1.0,
    ];

    let mut patches = Vec::with_capacity(9);
    for row in 0..3 {
        for col in 0..3 {
            if row == 1 && col == 1 && !include_center {
                continue; // window frames: center stays empty
            }
            let dest = egui::Rect::from_min_max(
                egui::pos2(dx[col], dy[row]),
                egui::pos2(dx[col + 1], dy[row + 1]),
            );
            let uv = egui::Rect::from_min_max(
                egui::pos2(ux[col], uy[row]),
                egui::pos2(ux[col + 1], uy[row + 1]),
            );
            if dest.width() > 0.0 && dest.height() > 0.0 && uv.width() > 0.0 && uv.height() > 0.0 {
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

    #[test]
    fn edge_overlay_layout_positions_strip_and_ornament() {
        // 200 wide x 100 tall window at (10, 20).
        let win = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 100.0));
        let orn = egui::vec2(20.0, 34.0);

        // RIGHT edge, thickness 3, ornament START (top), no title inset.
        let r = edge_overlay_layout(win, "right", 3.0, orn, false, 0.0);
        // Strip: 3px wide, flush to the right edge, full height.
        assert_eq!(r.strip.right(), win.right());
        assert_eq!(r.strip.left(), win.right() - 3.0);
        assert_eq!(r.strip.top(), win.top());
        assert_eq!(r.strip.bottom(), win.bottom());
        // Ornament stays INSIDE: right side hugs the window's right edge (grows
        // leftward), never spilling out; top-anchored.
        assert_eq!(r.ornament.right(), win.right());
        assert_eq!(r.ornament.top(), win.top());
        assert_eq!(r.ornament.size(), orn);
        assert!(r.ornament.left() >= win.left());

        // top_inset pushes strip + ornament below the title bar.
        let r_inset = edge_overlay_layout(win, "right", 3.0, orn, false, 28.0);
        assert_eq!(r_inset.strip.top(), win.top() + 28.0);
        assert_eq!(r_inset.ornament.top(), win.top() + 28.0);

        // RIGHT edge, ornament anchored to END (bottom).
        let r_end = edge_overlay_layout(win, "right", 3.0, orn, true, 0.0);
        assert_eq!(r_end.ornament.bottom(), win.bottom());

        // TOP edge, thickness 5: horizontal strip flush to the top.
        let t = edge_overlay_layout(win, "top", 5.0, orn, false, 0.0);
        assert_eq!(t.strip.top(), win.top());
        assert_eq!(t.strip.bottom(), win.top() + 5.0);
        assert_eq!(t.strip.left(), win.left());
        assert_eq!(t.strip.right(), win.right());
        assert_eq!(t.ornament.min, egui::pos2(win.left(), win.top()));

        // LEFT edge strip is flush to the left; ornament left side hugs it.
        let l = edge_overlay_layout(win, "left", 4.0, orn, false, 0.0);
        assert_eq!(l.strip.left(), win.left());
        assert_eq!(l.strip.right(), win.left() + 4.0);
        assert_eq!(l.ornament.left(), win.left());
    }

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
        let patches = nine_slice_patches(
            egui::vec2(32.0, 32.0),
            [8.0, 8.0, 8.0, 8.0],
            1.0,
            rect,
            [true; 4],
        );
        assert_eq!(patches.len(), 8);

        // Top-left corner: fixed 8x8 at the origin, UV = top-left quarter.
        let (dest, uv) = patches[0];
        assert_eq!(
            dest,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(8.0, 8.0))
        );
        assert_eq!(
            uv,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(0.25, 0.25))
        );

        // No patch covers the center point.
        let center = rect.center();
        assert!(patches.iter().all(|(dest, _)| !dest.contains(center)));
    }

    #[test]
    fn nine_slice_patches_filled_covers_the_center() {
        // Control faces (buttons/tabs/dropdowns) paint their center from the
        // sprite; the hollow variant let the window mesh show through as a
        // dark box behind every button label.
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 80.0));
        let patches = nine_slice_patches_impl(
            egui::vec2(32.0, 32.0),
            [8.0, 8.0, 8.0, 8.0],
            1.0,
            rect,
            [true; 4],
            true,
        );
        assert_eq!(patches.len(), 9);
        let center = rect.center();
        let (dest, uv) = patches
            .iter()
            .find(|(dest, _)| dest.contains(center))
            .expect("center patch present");
        // Center dest spans between the border insets; UV is the sprite's
        // own middle region.
        assert_eq!(
            *dest,
            egui::Rect::from_min_max(egui::pos2(8.0, 8.0), egui::pos2(92.0, 72.0))
        );
        assert_eq!(
            *uv,
            egui::Rect::from_min_max(egui::pos2(0.25, 0.25), egui::pos2(0.75, 0.75))
        );
    }

    #[test]
    fn nine_slice_patches_shrink_when_rect_is_small() {
        // 8px insets at scale 1 into a 10px-tall rect: top+bottom shrink to
        // 5px each instead of overlapping.
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 10.0));
        let patches = nine_slice_patches(
            egui::vec2(32.0, 32.0),
            [8.0, 8.0, 8.0, 8.0],
            1.0,
            rect,
            [true; 4],
        );
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
        assert!(patches
            .iter()
            .all(|(dest, _)| dest.min.y == 0.0 || dest.min.y >= 72.0));
        // The left rail now runs from the very top of the window.
        let left_rail = patches
            .iter()
            .find(|(dest, _)| dest.min.x == 0.0 && dest.min.y == 0.0 && dest.height() > 8.0)
            .expect("left rail present");
        assert_eq!(left_rail.0.height(), 72.0);
        // All sides hidden = nothing drawn.
        assert!(
            nine_slice_patches(egui::vec2(32.0, 32.0), [8.0; 4], 1.0, rect, [false; 4]).is_empty()
        );
    }

    #[test]
    fn nine_slice_patches_empty_on_degenerate_input() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 80.0));
        assert!(
            nine_slice_patches(egui::vec2(0.0, 32.0), [8.0; 4], 1.0, rect, [true; 4]).is_empty()
        );
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
    fn sheet_registration_toml_preserves_comments_and_upserts() {
        let original = r##"# My hand-written skin.
[meta]
name = "Test" # keep me

[sheets.old]
path = "icons/old.png"
cell = 32
"##;
        // New sheet appends; existing content survives byte-for-byte.
        let updated = sheet_registration_toml(original, "rogue", "icons/rogue.png", 64).unwrap();
        assert!(updated.contains("# My hand-written skin."));
        assert!(updated.contains(r#"name = "Test" # keep me"#));
        assert!(updated.contains(r#"path = "icons/old.png""#));
        let manifest: skins::SkinManifest = toml::from_str(&updated).unwrap();
        assert_eq!(manifest.sheets["rogue"].path, "icons/rogue.png");
        assert_eq!(manifest.sheets["rogue"].cell, 64);
        assert_eq!(manifest.sheets["old"].cell, 32);

        // Re-registering the same name replaces its entry.
        let updated = sheet_registration_toml(&updated, "rogue", "icons/rogue2.png", 48).unwrap();
        let manifest: skins::SkinManifest = toml::from_str(&updated).unwrap();
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
        let updated = sheet_registration_toml(original, "combat", "icons/combat.png", 64).unwrap();
        let manifest: skins::SkinManifest = toml::from_str(&updated).unwrap();
        assert_eq!(manifest.sheets["combat"].path, "icons/combat.png");
        // No stray bare [sheets] header for the implicit parent.
        assert!(!updated.contains("[sheets]\n[sheets."));
    }

    #[test]
    fn doll_set_named_resolves_sets_then_variants_case_insensitively() {
        let texture = SkinTexture {
            texture: egui::TextureId::default(),
            size: egui::vec2(8.0, 8.0),
        };
        let mut art = SkinWidgetArt::default();
        art.doll_sets.insert(
            "Silhouette".to_string(),
            LoadedDollSet {
                base: Some(texture),
                ..Default::default()
            },
        );
        art.doll_variants.push(LoadedDollVariant {
            name: "downed".to_string(),
            when: crate::config::Condition::Injury {
                area: "leftLeg".to_string(),
                cmp: crate::config::Cmp::Ge,
                level: 3,
            },
            set: LoadedDollSet::default(),
        });
        // Named set resolves case-insensitively.
        assert!(art.doll_set_named("silhouette").is_some());
        assert!(art.doll_set_named("SILHOUETTE").unwrap().base.is_some());
        // A variant name resolves too (pinned, condition ignored).
        assert!(art.doll_set_named("Downed").is_some());
        // Unknown names miss so callers fall back to the default doll.
        assert!(art.doll_set_named("nope").is_none());
        // The picker offers sets then variants.
        assert_eq!(art.doll_set_names(), vec!["Silhouette", "downed"]);
        // Named sets alone keep the widget-art bundle alive.
        assert!(!art.is_empty());
    }

    #[test]
    fn doll_variant_resolution_is_first_match_and_full_replace() {
        let texture = SkinTexture {
            texture: egui::TextureId::default(),
            size: egui::vec2(8.0, 8.0),
        };
        let injury = |area: &str, level: u8| crate::config::Condition::Injury {
            area: area.to_string(),
            cmp: crate::config::Cmp::Ge,
            level,
        };
        let empty_variant = |name: &str, when: crate::config::Condition| LoadedDollVariant {
            name: name.to_string(),
            when,
            set: LoadedDollSet {
                base: Some(texture),
                ..Default::default()
            },
        };

        let mut art = SkinWidgetArt::default();
        art.doll_base = Some(texture);
        // The default set hand-draws leftArm (a healthy layer).
        art.doll_parts
            .entry("leftarm".to_string())
            .or_default()
            .insert(0, texture);
        art.doll_variants.push(empty_variant(
            "downed",
            crate::config::Condition::All {
                conditions: vec![injury("leftLeg", 3), injury("rightLeg", 3)],
            },
        ));
        art.doll_variants
            .push(empty_variant("hurt", injury("leftLeg", 1)));

        let mut gs = crate::core::state::GameState::new();
        // Healthy: no variant matches -> default set.
        assert_eq!(art.resolve_doll_variant(&gs, 0, None), None);
        // One severed leg: only the broader "hurt" variant matches.
        gs.injuries.insert("leftLeg".to_string(), 3);
        assert_eq!(art.resolve_doll_variant(&gs, 0, None), Some(1));
        // Both legs severed: both match; first declared wins.
        gs.injuries.insert("rightLeg".to_string(), 3);
        assert_eq!(art.resolve_doll_variant(&gs, 0, None), Some(0));

        // Full replace: the variant view exposes only its own art — the
        // default set's hand-drawn leftArm does not leak through.
        let default_view = art.doll_set(None);
        assert!(default_view.has_overlays("leftArm"));
        assert!(default_view.overlay("LEFTARM", 0).is_some());
        let variant_view = art.doll_set(Some(0));
        assert!(!variant_view.has_overlays("leftArm"));
        assert!(variant_view.overlay("leftArm", 0).is_none());
        // Out-of-range index falls back to the default set.
        assert!(art.doll_set(Some(9)).has_overlays("leftArm"));
        // Variant art alone keeps the whole widget-art bundle alive.
        assert!(!art.is_empty());
    }

    #[test]
    fn doll_set_view_anchor_falls_back_like_the_default() {
        let mut art = SkinWidgetArt::default();
        art.doll_anchors
            .insert("head".to_string(), egui::vec2(0.4, 0.2));
        let view = art.doll_set(None);
        // Calibrated part, case-insensitive.
        assert_eq!(view.anchor("Head"), egui::vec2(0.4, 0.2));
        // Uncalibrated known part -> built-in default; unknown -> center.
        let [dx, dy] = skins::default_doll_anchor("leftarm").unwrap();
        assert_eq!(view.anchor("leftArm"), egui::vec2(dx, dy));
        assert_eq!(view.anchor("tail"), egui::vec2(0.5, 0.5));
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

    // ------------------------------------------------------------------
    // Characterization tests for the skin-system overhaul: these pin the
    // load/precedence behavior of the CURRENT design (manifest + pool +
    // overrides through one SkinState) so the phased rewrite can prove it
    // preserved what users see. Each builds a real ~/.vellum-fe tree in a
    // tempdir and drives the same `apply_if_changed` path the app uses.
    // ------------------------------------------------------------------

    struct TestEnv {
        _guard: std::sync::MutexGuard<'static, ()>,
        _dir: tempfile::TempDir,
        ctx: egui::Context,
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            std::env::remove_var("VELLUM_FE_DIR");
        }
    }

    fn test_env() -> TestEnv {
        let guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());
        crate::config::pool::invalidate_cache();
        TestEnv {
            _guard: guard,
            _dir: dir,
            ctx: egui::Context::default(),
        }
    }

    /// Write a real decodable PNG, `px` square, creating parent dirs.
    /// Distinct sizes let assertions tell which source won a slot.
    fn write_png(path: &Path, px: u32) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let pixels = vec![0xffu8; (px * px * 4) as usize];
        image::save_buffer(path, &pixels, px, px, image::ExtendedColorType::Rgba8).unwrap();
    }

    fn pool_dir() -> PathBuf {
        crate::config::Config::global_images_dir().unwrap()
    }

    fn skin_dir(name: &str) -> PathBuf {
        let dir = crate::config::Config::skins_dir().unwrap().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn skinless_status_icons_pool_set_with_override_precedence() {
        let env = test_env();
        write_png(&pool_dir().join("statusicons/runic/stunned.png"), 2);
        write_png(&pool_dir().join("statusicons/runic/hidden.png"), 2);
        write_png(&pool_dir().join("statusicons/custom.png"), 4);

        let mut overrides = HashMap::new();
        overrides.insert("hidden".to_string(), crate::data::IconRef::None);
        overrides.insert(
            "bleeding".to_string(),
            crate::data::IconRef::Image {
                path: "statusicons/custom.png".to_string(),
            },
        );
        let mut state = SkinState::default();
        state.set_status_icon_config(Some("runic"), &overrides);
        state.apply_if_changed(&env.ctx, None);

        let art = state.widget_art().expect("pool icons alone make art");
        // Pool set fills the glyph, keyed by role stem, case-insensitively.
        assert!(art.icon("stunned").is_some());
        assert!(art.icon("STUNNED").is_some());
        // IconRef::None removes the pool-resolved entry entirely.
        assert!(art.icon("hidden").is_none());
        // IconRef::Image resolves a pool path the set never mentioned.
        let bleeding = art.icon("bleeding").expect("override image resolves");
        assert_eq!(bleeding.size, egui::vec2(4.0, 4.0));
    }

    #[test]
    fn compass_pool_set_loads_and_none_strips() {
        let env = test_env();
        write_png(&pool_dir().join("compass/brass/rose.png"), 2);
        write_png(&pool_dir().join("compass/brass/ne.png"), 2);

        let mut state = SkinState::default();
        state.set_compass_set(Some("brass"));
        state.apply_if_changed(&env.ctx, None);
        let art = state.widget_art().unwrap();
        assert_eq!(art.compass_rose.unwrap().size, egui::vec2(2.0, 2.0));
        assert!(art.compass_dir("ne").is_some());
        assert!(art.compass_dir("n").is_none());

        // The "none" sentinel strips all compass art — and since the
        // compass was the ONLY art, the whole bundle collapses to None
        // (renderers then use their vector drawings).
        state.set_compass_set(Some("none"));
        state.apply_if_changed(&env.ctx, None);
        assert!(state.widget_art().is_none());
    }

    #[test]
    fn doll_override_loads_pool_base_and_sidecar_anchors() {
        let env = test_env();
        write_png(&pool_dir().join("dolls/human.png"), 2);
        std::fs::write(
            pool_dir().join("dolls/human.toml"),
            "[anchors]\nhead = [0.4, 0.2]\n",
        )
        .unwrap();

        // Override active: pool base + sidecar anchors, no overlays.
        let mut state = SkinState::default();
        state.apply_if_changed(&env.ctx, Some("dolls/human.png"));
        let art = state.widget_art().unwrap();
        assert_eq!(art.doll_base.unwrap().size, egui::vec2(2.0, 2.0));
        assert!(art.doll_parts.is_empty());
        assert_eq!(art.doll_anchor("head"), egui::vec2(0.4, 0.2));
        assert!(art.doll_variants.is_empty());

        // The "none" sentinel strips the doll (vector body renders).
        state.apply_if_changed(&env.ctx, Some("none"));
        assert!(state.widget_art().is_none());
    }

    #[test]
    fn background_override_none_pool_path_and_fallback() {
        let env = test_env();
        write_png(&pool_dir().join("backgrounds/paper.png"), 2);

        let mut state = SkinState::default();
        state.set_needed_pool_backgrounds(vec!["backgrounds/paper.png".to_string()]);
        state.apply_if_changed(&env.ctx, None);

        // "none" kills the background outright.
        assert!(state
            .background_for_with_override("main", Some("none"))
            .is_none());
        // A pool path renders with the readable defaults: cover + scrim.
        let bg = state
            .background_for_with_override("main", Some("backgrounds/paper.png"))
            .unwrap();
        assert_eq!(bg.tex_size, egui::vec2(2.0, 2.0));
        assert_eq!(bg.fit, BackgroundFit::Cover);
        assert_eq!(bg.scrim_alpha, (0.25 * 255.0) as u8);
        // No override: nothing.
        assert!(state.background_for_with_override("main", None).is_none());
    }

    #[test]
    fn background_sidecar_fit_and_tile_scale_are_honored() {
        let env = test_env();
        write_png(&pool_dir().join("backgrounds/mesh.png"), 2);
        std::fs::write(
            pool_dir().join("backgrounds/mesh.toml"),
            "kind = \"background\"\nfit = \"tile\"\nscale = 2.0\n",
        )
        .unwrap();

        let mut state = SkinState::default();
        state.set_needed_pool_backgrounds(vec!["backgrounds/mesh.png".to_string()]);
        state.apply_if_changed(&env.ctx, None);

        let bg = state
            .background_for_with_override("main", Some("backgrounds/mesh.png"))
            .unwrap();
        assert_eq!(bg.fit, BackgroundFit::Tile);
        assert_eq!(bg.tile_scale, 2.0);
        assert_eq!(
            state.pool_background_fit("backgrounds/mesh.png"),
            Some(BackgroundFit::Tile)
        );

        // The tile grid honors the scale: a 2px tile at 2x fills a 8x4
        // rect with 2 cols x 1 row of 4px tiles (plus the scrim shape).
        let shapes = background_shapes(
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(8.0, 4.0)),
            &bg,
            egui::Color32::BLACK,
        );
        assert_eq!(shapes.len(), 2 + 1);
    }

    #[test]
    fn background_sidecar_unknown_fit_degrades_to_cover() {
        let env = test_env();
        write_png(&pool_dir().join("backgrounds/odd.png"), 2);
        std::fs::write(
            pool_dir().join("backgrounds/odd.toml"),
            "kind = \"background\"\nfit = \"mosaic\"\n",
        )
        .unwrap();

        let mut state = SkinState::default();
        state.set_needed_pool_backgrounds(vec!["backgrounds/odd.png".to_string()]);
        state.apply_if_changed(&env.ctx, None);

        let bg = state
            .background_for_with_override("main", Some("backgrounds/odd.png"))
            .unwrap();
        assert_eq!(bg.fit, BackgroundFit::Cover);
        // The menu sees no declaration (Default stays checked).
        assert!(state.pool_background_fit("backgrounds/odd.png").is_none());
    }

    #[test]
    fn border_override_resolves_none_and_pool_frames() {
        let env = test_env();
        write_png(&pool_dir().join("frames/brass.png"), 2);
        std::fs::write(pool_dir().join("frames/brass.toml"), "slice = 300\n").unwrap();

        let mut state = SkinState::default();
        state.set_needed_pool_frames(vec!["brass".to_string()]);
        state.apply_if_changed(&env.ctx, None);

        // "none" means no frame.
        assert!(state
            .border_for_with_override("main", Some("none"))
            .is_none());
        // A pool frame resolves through its sidecar, scale derived so the
        // largest inset lands at DEFAULT_FRAME_BORDER_PT on screen.
        let border = state.border_for_with_override("main", Some("Brass")).unwrap();
        assert_eq!(border.slice, [300.0; 4]);
        assert!((border.scale - 15.0 / 300.0).abs() < 1e-6);
        // An unknown name (stale layout) resolves to nothing.
        assert!(state
            .border_for_with_override("main", Some("ghost"))
            .is_none());
        assert!(state.border_for_with_override("main", None).is_none());
    }

    #[test]
    fn frame_names_skip_sidecarless_pool_frames_and_reserved_none() {
        let env = test_env();
        write_png(&pool_dir().join("frames/withsc.png"), 2);
        std::fs::write(pool_dir().join("frames/withsc.toml"), "slice = 8\n").unwrap();
        write_png(&pool_dir().join("frames/nosc.png"), 2);

        let mut state = SkinState::default();
        state.apply_if_changed(&env.ctx, None);
        // Sidecar-less pool frames are omitted (they can't nine-slice).
        assert_eq!(state.frame_names(), ["withsc"]);
    }

    #[test]
    fn shared_sheets_load_without_a_skin() {
        let env = test_env();
        let icons = crate::config::Config::global_icons_dir().unwrap();
        write_png(&icons.join("combat.png"), 2);
        std::fs::write(
            icons.join("icons.toml"),
            "[sheets.combat]\npath = \"combat.png\"\ncell = 1\n",
        )
        .unwrap();

        let mut state = SkinState::default();
        state.apply_if_changed(&env.ctx, None);
        let art = state.widget_art().expect("shared sheets alone make art");
        assert!(art.sheet_cell("combat", 1, false).is_some());
        assert_eq!(art.sheet_cell_count("combat"), Some(4));
        assert!(state.sheet_is_shared("combat"));
        assert!(state.sheet_is_shared("COMBAT"));
    }

    #[test]
    fn resolve_image_path_prefers_skin_dir_then_pool_then_names_local() {
        let env = test_env();
        let _ = &env; // env redirect + lock only
        let skin = skin_dir("test");
        write_png(&skin.join("a.png"), 2);
        write_png(&pool_dir().join("a.png"), 2);
        write_png(&pool_dir().join("b.png"), 2);

        assert_eq!(skins::resolve_image_path(&skin, "a.png"), skin.join("a.png"));
        assert_eq!(
            skins::resolve_image_path(&skin, "b.png"),
            pool_dir().join("b.png")
        );
        // Missing everywhere: the skin-local path names the natural spot.
        assert_eq!(skins::resolve_image_path(&skin, "c.png"), skin.join("c.png"));
    }

    #[test]
    fn unrelated_appearance_changes_keep_loaded_textures() {
        // Phase 2 regression guard: before the ImageStore, ANY declaration
        // change tore down and re-decoded every texture. Now an unrelated
        // toggle must leave loaded art untouched (same TextureId).
        let env = test_env();
        write_png(&pool_dir().join("statusicons/runic/stunned.png"), 2);
        write_png(&pool_dir().join("backgrounds/paper.png"), 2);

        let mut state = SkinState::default();
        state.set_status_icon_config(Some("runic"), &HashMap::new());
        state.apply_if_changed(&env.ctx, None);
        let icon_id = state.widget_art().unwrap().icon("stunned").unwrap().texture;

        // Declare a new pool background: a reload pass runs, but the icon's
        // texture survives it untouched.
        state.set_needed_pool_backgrounds(vec!["backgrounds/paper.png".to_string()]);
        state.apply_if_changed(&env.ctx, None);
        assert_eq!(
            state.widget_art().unwrap().icon("stunned").unwrap().texture,
            icon_id
        );
        assert!(state
            .background_for_with_override("main", Some("backgrounds/paper.png"))
            .is_some());

        // Grayscale toggle: the color icon still keeps its texture, and the
        // gray twin appears alongside it.
        state.set_grayscale(true, false);
        state.apply_if_changed(&env.ctx, None);
        let art = state.widget_art().unwrap();
        assert_eq!(art.icon("stunned").unwrap().texture, icon_id);
        assert!(art.icon_gray("stunned").is_some());
    }

    #[test]
    fn pool_dolls_bind_per_window_as_named_sets_without_a_skin() {
        // Phase 4: a window's doll_set binding may hold a pool path; each
        // loads as a named set keyed by that path, so two doll windows can
        // show two different pool dolls with no skin at all.
        let env = test_env();
        write_png(&pool_dir().join("dolls/human.png"), 2);
        write_png(&pool_dir().join("dolls/elf.png"), 4);
        std::fs::write(
            pool_dir().join("dolls/elf.toml"),
            "kind = \"doll\"\n[anchors]\nhead = [0.3, 0.1]\n",
        )
        .unwrap();

        let mut state = SkinState::default();
        state.set_needed_pool_dolls(vec![
            "dolls/human.png".to_string(),
            "dolls/elf.png".to_string(),
        ]);
        state.apply_if_changed(&env.ctx, None);
        let art = state.widget_art().expect("pool doll sets alone make art");
        let human = art.doll_set_named("dolls/human.png").unwrap();
        assert_eq!(human.base.unwrap().size, egui::vec2(2.0, 2.0));
        let elf = art.doll_set_named("dolls/elf.png").unwrap();
        assert_eq!(elf.base.unwrap().size, egui::vec2(4.0, 4.0));
        // Sidecar anchors ride along per window.
        assert_eq!(elf.anchor("head"), egui::vec2(0.3, 0.1));
        // Dropping a binding evicts its set on the next apply.
        state.set_needed_pool_dolls(vec!["dolls/human.png".to_string()]);
        state.apply_if_changed(&env.ctx, None);
        let art = state.widget_art().unwrap();
        assert!(art.doll_set_named("dolls/elf.png").is_none());
        assert!(art.doll_set_named("dolls/human.png").is_some());
    }

    #[test]
    fn control_faces_and_edge_sets_work_without_a_skin() {
        let env = test_env();
        // A calibrated pool frame assigned as the button face.
        write_png(&pool_dir().join("frames/brass.png"), 2);
        std::fs::write(pool_dir().join("frames/brass.toml"), "slice = 300\n").unwrap();
        // An edge set: top strip with paint params, right ornament only.
        write_png(&pool_dir().join("edges/vines/top.png"), 2);
        std::fs::write(
            pool_dir().join("edges/vines/top.toml"),
            "kind = \"edge\"\ntile = true\nthickness = 24\nscale = 0.5\n",
        )
        .unwrap();
        write_png(&pool_dir().join("edges/vines/right-ornament.png"), 4);

        let mut state = SkinState::default();
        let mut controls = HashMap::new();
        controls.insert("button".to_string(), "brass".to_string());
        state.set_control_frames(&controls);
        state.set_needed_pool_frames(vec!["brass".to_string()]);
        state.set_edge_set(Some("vines"));
        state.apply_if_changed(&env.ctx, None);

        let art = state.widget_art().expect("assignments alone make art");
        // The button face nine-slices with the frame's sidecar geometry.
        let button = art.control_border("button", "hover").unwrap();
        assert_eq!(button.slice, [300.0; 4]);
        // Edge strips carry their sidecar paint params (thickness × scale).
        let top = art.edge("top").unwrap();
        assert!(top.strip.is_some());
        assert!(top.tile);
        assert_eq!(top.thickness, Some(12.0));
        let right = art.edge("right").unwrap();
        assert!(right.strip.is_none());
        assert_eq!(right.ornament.unwrap().size, egui::vec2(4.0, 4.0));
        assert!(art.edge("bottom").is_none());

        // The "none" sentinel strips edge art.
        state.set_edge_set(Some("none"));
        state.apply_if_changed(&env.ctx, None);
        let art = state.widget_art().unwrap();
        assert!(!art.has_edges());
        assert!(art.control_border("button", "normal").is_some());
    }

    #[test]
    fn creature_art_resolves_from_the_pool_without_a_skin() {
        // Phase 4: the active-skin gate is gone — pool creature art plus
        // the convention status overlays load with no skin at all.
        let env = test_env();
        write_png(&pool_dir().join("creatures/coyote.png"), 2);
        write_png(&pool_dir().join("creatures/status/rooted.png"), 2);
        std::fs::write(
            pool_dir().join("creatures/coyote.toml"),
            "kind = \"creature\"\nsize = 1.5\n[anchors]\nfeet = [0.5, 0.9]\n",
        )
        .unwrap();

        let wanted = |name: &str| WantedCreature {
            name: name.to_string(),
            noun: Some(name.split(' ').next_back().unwrap_or(name).to_string()),
            family: None,
            injuries: Vec::new(),
        };
        let mut state = SkinState::default();
        state.apply_if_changed(&env.ctx, None);
        state.prepare_creature_art(&env.ctx, &[wanted("coyote")]);
        let cache = state.creature_art.lock().unwrap();
        let art = cache.base("coyote").expect("pool art resolves skinless");
        assert_eq!(art.feet, [0.5, 0.9]);
        assert_eq!(art.size, Some(1.5));
        // The convention overlay was synthesized, bound to its flag, and
        // its texture loaded.
        let overlay = cache
            .card
            .overlays
            .iter()
            .find(|o| o.image == "creatures/status/rooted.png")
            .expect("convention overlay present");
        assert!(matches!(
            &overlay.when,
            crate::config::Condition::CrtrStatus { id, active: true } if id == "rooted"
        ));
        assert!(cache
            .overlays
            .get("creatures/status/rooted.png")
            .is_some_and(|tex| tex.is_some()));
        // An unknown noun still negative-caches instead of erroring.
        drop(cache);
        state.prepare_creature_art(&env.ctx, &[wanted("gryphon")]);
        let cache = state.creature_art.lock().unwrap();
        assert!(cache.bases.get("gryphon").is_some_and(|art| art.is_none()));
    }

    #[test]
    fn prone_wound_layers_load_from_the_pose_arts_extras() {
        let env = test_env();
        // {token}_prone_{loc}{rank}: the prone pose's own wound layer,
        // discovered as the PRONE art's extra (its stem prefixes it).
        let dir = pool_dir().join("creatures");
        write_png(&dir.join("kobold.png"), 4);
        write_png(&dir.join("kobold_prone.png"), 8);
        write_png(&dir.join("kobold_prone_chest1.png"), 2);

        let mut state = SkinState::default();
        state.apply_if_changed(&env.ctx, None);
        state.prepare_creature_art(
            &env.ctx,
            &[WantedCreature {
                name: "a kobold".to_string(),
                noun: Some("kobold".to_string()),
                family: None,
                injuries: vec![("chest".to_string(), 1)],
            }],
        );
        let cache = state.creature_art.lock().unwrap();
        let art = cache.base("kobold").expect("flat base resolves");
        let prone_path = art.extra("prone").cloned().expect("prone extra listed");
        let prone = cache
            .variant_base(prone_path.to_string_lossy().as_ref())
            .expect("prone pose loaded");
        let wound_path = prone
            .extra("chest1")
            .expect("prone wound is the pose art's extra")
            .to_string_lossy()
            .into_owned();
        assert!(
            cache.overlays.get(&wound_path).is_some_and(|t| t.is_some()),
            "prone wound texture loads so the renderer's lookup on the active art hits"
        );
    }

    #[test]
    fn tiered_creature_art_locks_a_tier_and_loads_pose_and_wounds() {
        let env = test_env();
        // Variant tier for the mongrel kobold: base + prone pose + a
        // chest wound overlay, token-prefixed per the tier scheme.
        // Full-name tiers live FLAT under creatures/<name>/ since
        // dfe758a2 (full-name art is the primary tier) — this fixture
        // still wrote the older creatures/<noun>/<name>/ nesting, which
        // resolve_tier_art no longer probes, so the noun tier's 2px art
        // won and the assertion below read [2,2]. The core tier-order
        // test (resolve_tier_art_prefers_full_name_over_noun) pins the
        // flat layout; this fixture now matches it.
        let variant = pool_dir().join("creatures/mongrel_kobold");
        write_png(&variant.join("mongrel_kobold.png"), 4);
        write_png(&variant.join("mongrel_kobold_prone.png"), 8);
        write_png(&variant.join("mongrel_kobold_chest2.png"), 2);
        // Noun tier with its own base (big ugly kobold falls here).
        write_png(&pool_dir().join("creatures/kobold/kobold.png"), 2);

        let mut state = SkinState::default();
        state.apply_if_changed(&env.ctx, None);
        // Boon-decorated live name normalizes onto the variant token.
        state.prepare_creature_art(
            &env.ctx,
            &[WantedCreature {
                name: "a shimmering mongrel kobold".to_string(),
                noun: Some("kobold".to_string()),
                family: None,
                injuries: vec![("chest".to_string(), 2)],
            }],
        );
        let cache = state.creature_art.lock().unwrap();
        let art = cache.base("mongrel_kobold").expect("variant tier resolves");
        assert_eq!(art.texture.size_vec2(), egui::vec2(4.0, 4.0));
        // The tier's extras were discovered and the current state's
        // textures loaded: prone as full art, the wound as a texture.
        let prone_path = art.extra("prone").expect("prone extra listed").clone();
        let prone = cache
            .variant_base(prone_path.to_string_lossy().as_ref())
            .expect("prone pose loaded");
        assert_eq!(prone.texture.size_vec2(), egui::vec2(8.0, 8.0));
        let wound_path = art.extra("chest2").unwrap().to_string_lossy().into_owned();
        assert!(cache.overlays.get(&wound_path).is_some_and(|t| t.is_some()));
        assert!(art.has_wound_extras());
        // Finding 8: the prone pose loaded even though the creature has
        // never been prone, so its calibration (fed to the core store by
        // this same pass) is available before the solver first reserves
        // the envelope. 8px prone content / 4px standing content = 2.
        // (Asserted through pose_calibration directly — the global store
        // is shared across parallel tests.)
        let prone_cal = pose_calibration(prone, Some(art));
        assert!((prone_cal.content_ratio.unwrap() - 2.0).abs() < 1e-4);
        // Tier locking: a creature without its own variant folder locks
        // the noun tier — mongrel art never leaks onto it.
        drop(cache);
        state.prepare_creature_art(
            &env.ctx,
            &[WantedCreature {
                name: "big ugly kobold".to_string(),
                noun: Some("kobold".to_string()),
                family: None,
                injuries: Vec::new(),
            }],
        );
        let cache = state.creature_art.lock().unwrap();
        let art = cache.base("big_ugly_kobold").expect("noun tier resolves");
        assert_eq!(art.texture.size_vec2(), egui::vec2(2.0, 2.0));
        assert!(art.extra("prone").is_none(), "no cross-tier borrowing");
    }

    /// Finding 6 acceptance: transparent padding and art resolution are
    /// both neutral for the geometry contract — the content aspect fed to
    /// the core store depends only on the opaque content.
    #[test]
    fn content_aspect_ignores_padding_and_resolution() {
        // 100x200 canvas, content filling it exactly: aspect 0.5.
        let tight = content_aspect([0.0, 0.0, 1.0, 1.0], 100.0, 200.0);
        assert!((tight - 0.5).abs() < 1e-4);
        // Same content centred in a padded 200x400 canvas.
        let padded = content_aspect([0.25, 0.25, 0.75, 0.75], 200.0, 400.0);
        assert!((padded - tight).abs() < 1e-4, "padding must be neutral");
        // Same art at 4x the resolution.
        let hires = content_aspect([0.0, 0.0, 1.0, 1.0], 400.0, 800.0);
        assert!((hires - tight).abs() < 1e-4, "resolution must be neutral");
    }

    /// Finding 7 acceptance (the worked example): standing content 200px,
    /// prone content 100px on the same canvas convention → ratio 0.5, and
    /// transparent padding around either image does not change it. The
    /// ratio is deliberately measured in PIXELS (the common canvas-scale
    /// convention the renderer inherits) — a pose file at a different
    /// resolution is calibrated through its sidecar `size`, never by
    /// inferring thickness from resolution.
    #[test]
    fn pose_content_ratio_matches_worked_example() {
        // Full-canvas content: 200px standing, 100px prone.
        let standing = content_height_px([0.0, 0.0, 1.0, 1.0], 200.0);
        let prone = content_height_px([0.0, 0.0, 1.0, 1.0], 100.0);
        assert!((prone / standing - 0.5).abs() < 1e-4);
        // Same content padded into a 400px-tall canvas (content 100px).
        let padded = content_height_px([0.4, 0.375, 0.6, 0.625], 400.0);
        assert!((padded / standing - 0.5).abs() < 1e-4, "padding neutral");
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
