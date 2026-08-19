//! Skin-asset rendering for the harmony generator.
//!
//! Renders the four images of a "matching skin" — panel background, a
//! darker panel variant, a nine-slice frame, and an accented frame — as raw
//! RGBA buffers, colored from the same harmony as the generated text so the
//! chrome and the words belong together. Ported from the Skin assets tab of
//! Niffy's vellum-palette-harmony.html prototype.
//!
//! Everything here is deterministic pure math (the grain noise uses a fixed
//! LCG, not a real RNG) so previews, written files, and tests always agree.
//! PNG encoding and file layout live in `config::skins`; this module never
//! touches the filesystem.

use super::harmony::{hex_to_lch, lch_to_hex, parse_hex};

/// Nudge a color's OKLCH lightness down by `by`/100, keeping hue.
pub fn darken(hex: &str, by: f64) -> String {
    let [l, c, h] = hex_to_lch(hex).unwrap_or([0.5, 0.0, 0.0]);
    lch_to_hex([(l - by / 100.0).clamp(0.02, 1.0), c, h])
}

/// Nudge a color's OKLCH lightness up by `by`/100, softening chroma — the
/// prototype's "panel, lifted" frame-line default.
pub fn lift(hex: &str, by: f64) -> String {
    let [l, c, h] = hex_to_lch(hex).unwrap_or([0.5, 0.0, 0.0]);
    lch_to_hex([(l + by / 100.0).clamp(0.0, 1.0), c * 0.7, h])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientKind {
    Vertical,
    Horizontal,
    Diagonal,
    Radial,
    Flat,
}

impl GradientKind {
    pub const ALL: [GradientKind; 5] = [
        GradientKind::Vertical,
        GradientKind::Horizontal,
        GradientKind::Diagonal,
        GradientKind::Radial,
        GradientKind::Flat,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GradientKind::Vertical => "vertical",
            GradientKind::Horizontal => "horizontal",
            GradientKind::Diagonal => "diagonal",
            GradientKind::Radial => "radial",
            GradientKind::Flat => "flat",
        }
    }
}

/// Panel texture controls; ranges follow the prototype's sliders.
#[derive(Debug, Clone)]
pub struct PanelSpec {
    pub gradient: GradientKind,
    /// How much darker the bottom sits, in OKLCH lightness (0-60).
    pub fade_depth: f64,
    /// Radial light-center/dark-edge overlay strength (0-90).
    pub vignette: f64,
    /// Checkerboard +-n/255 tone breakup (0-4) that hides gradient banding.
    pub dither: f64,
    /// Random-noise strength (0-30); deterministic LCG, not a real RNG.
    pub grain: f64,
    /// Darkened-row strength (0-40) for a CRT look.
    pub scanlines: f64,
}

impl Default for PanelSpec {
    fn default() -> Self {
        Self {
            gradient: GradientKind::Vertical,
            fade_depth: 14.0,
            vignette: 18.0,
            dither: 1.0,
            grain: 0.0,
            scanlines: 0.0,
        }
    }
}

/// Nine-slice frame controls; units are 1/32 of the frame image edge so the
/// same numbers work for the 32px written file and any preview size.
#[derive(Debug, Clone)]
pub struct FrameSpec {
    /// Stroke thickness (0-6). 0 draws no frame line.
    pub width: f64,
    /// Gap between the image edge and the stroke (0-10).
    pub inset: f64,
    /// Corner radius (0-14).
    pub radius: f64,
    /// Accent stub length along the top edge (0-14). 0 = no accent, and the
    /// accented frame renders identical to the plain one.
    pub stub: f64,
    /// Nine-slice inset each edge reserves, in source pixels of the 32px
    /// file (1-15). Warn when under radius + width: corners would clip.
    pub slice: f64,
}

impl Default for FrameSpec {
    fn default() -> Self {
        Self {
            width: 1.0,
            inset: 0.5,
            radius: 3.0,
            stub: 6.0,
            slice: 4.0,
        }
    }
}

impl FrameSpec {
    /// Corners clip when the reserved slice is thinner than the drawn
    /// corner; surface this in UIs before writing files.
    pub fn slice_clips_corners(&self) -> bool {
        self.slice < self.radius + self.width
    }
}

fn oklab(hex: &str) -> [f64; 3] {
    // Round-trip through the harmony module's parsers to stay consistent.
    let [l, c, h] = hex_to_lch(hex).unwrap_or([0.5, 0.0, 0.0]);
    [l, c * h.to_radians().cos(), c * h.to_radians().sin()]
}

fn oklab_to_srgb_bytes(lab: [f64; 3]) -> [f64; 3] {
    // lch_to_hex handles gamut fitting; parse back to linear-free sRGB.
    let c = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    let h = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    parse_hex(&lch_to_hex([lab[0], c, h])).unwrap_or([0.5, 0.5, 0.5])
}

/// Render the panel background: a gradient from `top` to `bottom`
/// interpolated in OKLab (perceptually even, unlike sRGB lerp), plus
/// vignette, scanlines, dither, and grain. Returns tightly packed RGBA8,
/// `size * size * 4` bytes.
pub fn render_panel(size: u32, top: &str, bottom: &str, spec: &PanelSpec) -> Vec<u8> {
    let w = size as usize;
    let top_lab = oklab(top);
    let bottom_lab = oklab(bottom);
    let mut out = vec![0u8; w * w * 4];
    // Deterministic LCG for grain; fixed seed so renders are reproducible.
    let mut rng_state: u32 = 0x9e37_79b9;
    let max_dim = (w - 1).max(1) as f64;

    for y in 0..w {
        for x in 0..w {
            let (fx, fy) = (x as f64, y as f64);
            let t = match spec.gradient {
                GradientKind::Vertical => fy / max_dim,
                GradientKind::Horizontal => fx / max_dim,
                GradientKind::Diagonal => (fx + fy) / (2.0 * max_dim),
                GradientKind::Radial => {
                    let dx = fx - max_dim / 2.0;
                    let dy = fy - max_dim / 2.0;
                    ((dx * dx + dy * dy).sqrt() / (max_dim * 0.75)).min(1.0)
                }
                GradientKind::Flat => 0.0,
            };
            let lab = [
                top_lab[0] + (bottom_lab[0] - top_lab[0]) * t,
                top_lab[1] + (bottom_lab[1] - top_lab[1]) * t,
                top_lab[2] + (bottom_lab[2] - top_lab[2]) * t,
            ];
            let mut rgb = oklab_to_srgb_bytes(lab);

            if spec.vignette > 0.0 {
                // Light center fading to dark edge, like the prototype's
                // two-stop radial overlay.
                let dx = fx - max_dim / 2.0;
                let dy = fy - max_dim / 2.0;
                let vt = ((dx * dx + dy * dy).sqrt() / (max_dim * 0.72)).min(1.0);
                let alpha = (spec.vignette / 1100.0) * (1.0 - vt) + (spec.vignette / 190.0) * vt;
                let overlay = 1.0 - vt; // white at center, black at edge
                for channel in &mut rgb {
                    *channel += (overlay - *channel) * alpha;
                }
            }
            if spec.scanlines > 0.0 {
                let step = ((4 * w) as f64 / 768.0).round().max(2.0) as usize;
                if y % step < (step / 2).max(1) {
                    let alpha = spec.scanlines / 100.0;
                    for channel in &mut rgb {
                        *channel *= 1.0 - alpha;
                    }
                }
            }
            let mut noise = 0.0;
            if spec.dither > 0.0 {
                noise += if (x % 2) ^ (y % 2) == 1 {
                    spec.dither
                } else {
                    -spec.dither
                };
            }
            if spec.grain > 0.0 {
                rng_state = rng_state
                    .wrapping_mul(1_664_525)
                    .wrapping_add(1_013_904_223);
                let unit = ((rng_state >> 16) & 0xffff) as f64 / 65_535.0;
                noise += (unit - 0.5) * spec.grain;
            }

            let idx = (y * w + x) * 4;
            for (offset, channel) in rgb.iter().enumerate() {
                out[idx + offset] = ((channel * 255.0 + noise).round().clamp(0.0, 255.0)) as u8;
            }
            out[idx + 3] = 255;
        }
    }
    out
}

/// Signed distance from `p` to a rounded rectangle centered at the origin
/// with half-extents `half` and corner radius `r`.
fn rounded_rect_sdf(px: f64, py: f64, half: f64, r: f64) -> f64 {
    let qx = px.abs() - (half - r);
    let qy = py.abs() - (half - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// Distance from `p` to the segment `a`-`b`.
fn segment_distance(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq == 0.0 {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0)
    };
    ((px - ax - dx * t).powi(2) + (py - ay - dy * t).powi(2)).sqrt()
}

/// Render a nine-slice frame: a rounded-rect stroke in `line`, transparent
/// center and margins, plus optional accent stubs along the top edge when
/// `accent` is set and `spec.stub > 0`. Anti-aliased by 2x2 supersampling.
/// Returns tightly packed RGBA8.
pub fn render_frame(size: u32, line: &str, accent: Option<&str>, spec: &FrameSpec) -> Vec<u8> {
    let w = size as usize;
    let unit = size as f64 / 32.0;
    let thickness = (spec.width * unit).max(if spec.width > 0.0 { 0.5 } else { 0.0 });
    let outer = spec.inset * unit + thickness / 2.0;
    let half = size as f64 / 2.0 - outer;
    let radius = (spec.radius * unit - spec.inset * unit).max(0.0).min(half);
    let line_rgb = parse_hex(line).unwrap_or([0.5, 0.5, 0.5]);
    let accent_rgb = accent.and_then(parse_hex);
    let accent_w = (2.0 * unit).max(1.0);
    let stub_len = spec.stub * unit;
    // Stub segments run along the top edge from just past each corner.
    let top_y = outer;
    let corner = outer + radius;
    let center = size as f64 / 2.0;

    let mut out = vec![0u8; w * w * 4];
    for y in 0..w {
        for x in 0..w {
            let mut line_cov: f64 = 0.0;
            let mut accent_cov: f64 = 0.0;
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let px = x as f64 + sx;
                let py = y as f64 + sy;
                if spec.width > 0.0 {
                    let d = rounded_rect_sdf(px - center, py - center, half, radius);
                    if d.abs() <= thickness / 2.0 {
                        line_cov += 0.25;
                    }
                }
                if let Some(_) = accent_rgb {
                    if stub_len > 0.0 {
                        let right_start = size as f64 - corner;
                        let left = segment_distance(
                            px,
                            py,
                            corner,
                            top_y,
                            (corner + stub_len).min(right_start),
                            top_y,
                        );
                        let right = segment_distance(
                            px,
                            py,
                            right_start,
                            top_y,
                            (right_start - stub_len).max(corner),
                            top_y,
                        );
                        if left.min(right) <= accent_w / 2.0 {
                            accent_cov += 0.25;
                        }
                    }
                }
            }
            // Accent draws over the line where they overlap.
            let (rgb, alpha) = if accent_cov > 0.0 {
                (accent_rgb.unwrap_or(line_rgb), accent_cov)
            } else {
                (line_rgb, line_cov)
            };
            if alpha > 0.0 {
                let idx = (y * w + x) * 4;
                for (offset, channel) in rgb.iter().enumerate() {
                    out[idx + offset] = (channel * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out[idx + 3] = (alpha * 255.0).round() as u8;
            }
        }
    }
    out
}

/// The four color anchors of a skin, derived from harmony params: panel
/// gradient from a lifted background, frame line lifted further, accent =
/// the seed. (The prototype offered alternate sources; these are its
/// defaults, which read well on every theme tested.)
#[derive(Debug, Clone)]
pub struct SkinColors {
    pub panel_top: String,
    pub panel_bottom: String,
    pub line: String,
    pub accent: String,
}

impl SkinColors {
    pub fn derive(background: &str, seed: &str, fade_depth: f64) -> Self {
        let panel_top = lift(background, 5.0);
        let panel_bottom = darken(&panel_top, fade_depth);
        Self {
            line: lift(background, 16.0),
            accent: seed.to_string(),
            panel_top,
            panel_bottom,
        }
    }
}

/// File edge sizes: panels are authored large enough to stretch cleanly,
/// frames small so nine-slice corners stay crisp.
pub const PANEL_SIZE: u32 = 768;
pub const FRAME_SIZE: u32 = 32;
/// How much darker the panel-deep variant sits (prototype default).
pub const DEEP_OFFSET: f64 = 14.0;

/// Render the four skin images. Returns `(file name, edge size, RGBA8)` in
/// manifest order: panel, panel-deep, frame, frame-accent.
pub fn render_skin_assets(
    colors: &SkinColors,
    panel: &PanelSpec,
    frame: &FrameSpec,
) -> Vec<(&'static str, u32, Vec<u8>)> {
    vec![
        (
            "panel.png",
            PANEL_SIZE,
            render_panel(PANEL_SIZE, &colors.panel_top, &colors.panel_bottom, panel),
        ),
        (
            "panel-deep.png",
            PANEL_SIZE,
            render_panel(
                PANEL_SIZE,
                &darken(&colors.panel_top, DEEP_OFFSET),
                &darken(&colors.panel_bottom, DEEP_OFFSET),
                panel,
            ),
        ),
        (
            "frame.png",
            FRAME_SIZE,
            render_frame(FRAME_SIZE, &colors.line, None, frame),
        ),
        (
            "frame-accent.png",
            FRAME_SIZE,
            render_frame(FRAME_SIZE, &colors.line, Some(&colors.accent), frame),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::harmony::delta_e;

    fn px(buf: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * size + x) * 4) as usize;
        [buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3]]
    }

    fn hex_of(p: [u8; 4]) -> String {
        format!("#{:02x}{:02x}{:02x}", p[0], p[1], p[2])
    }

    #[test]
    fn panel_render_is_deterministic() {
        let spec = PanelSpec {
            grain: 12.0,
            ..PanelSpec::default()
        };
        let a = render_panel(64, "#223344", "#101820", &spec);
        let b = render_panel(64, "#223344", "#101820", &spec);
        assert_eq!(a, b, "grain must come from a fixed-seed LCG");
        assert_eq!(a.len(), 64 * 64 * 4);
    }

    #[test]
    fn vertical_gradient_runs_top_to_bottom() {
        let spec = PanelSpec {
            vignette: 0.0,
            dither: 0.0,
            ..PanelSpec::default()
        };
        let buf = render_panel(64, "#405060", "#101418", &spec);
        let top = hex_of(px(&buf, 64, 32, 0));
        let bottom = hex_of(px(&buf, 64, 32, 63));
        assert!(
            delta_e(&top, "#405060") < 0.03,
            "top row ~ top color, got {top}"
        );
        assert!(
            delta_e(&bottom, "#101418") < 0.03,
            "bottom row ~ bottom color, got {bottom}"
        );
    }

    #[test]
    fn flat_panel_is_uniform_without_texture() {
        let spec = PanelSpec {
            gradient: GradientKind::Flat,
            vignette: 0.0,
            dither: 0.0,
            grain: 0.0,
            scanlines: 0.0,
            ..PanelSpec::default()
        };
        let buf = render_panel(32, "#223344", "#101820", &spec);
        let first = px(&buf, 32, 0, 0);
        for y in 0..32 {
            for x in 0..32 {
                assert_eq!(px(&buf, 32, x, y), first, "flat panel varies at {x},{y}");
            }
        }
    }

    #[test]
    fn frame_has_transparent_center_and_stroked_border() {
        // Pixel-aligned stroke: inset 1 + width 2 puts the band exactly over
        // pixel columns 1-2, so coverage there is full, not anti-aliased.
        let spec = FrameSpec {
            width: 2.0,
            inset: 1.0,
            ..FrameSpec::default()
        };
        let buf = render_frame(32, "#88aacc", None, &spec);
        assert_eq!(px(&buf, 32, 16, 16)[3], 0, "center must stay transparent");
        let edge = px(&buf, 32, 2, 16);
        assert_eq!(edge[3], 255, "left edge midpoint carries the stroke");
        assert_eq!(hex_of(edge), "#88aacc");
        // Outside the inset there is nothing.
        assert_eq!(px(&buf, 32, 16, 0)[3], 0, "inset margin stays clear");
    }

    #[test]
    fn accent_stubs_appear_only_on_the_accented_variant() {
        let spec = FrameSpec {
            stub: 8.0,
            ..FrameSpec::default()
        };
        let plain = render_frame(32, "#404850", None, &spec);
        let accented = render_frame(32, "#404850", Some("#cc4433"), &spec);
        assert_ne!(plain, accented, "accent must change the image");
        // A stub pixel just past the top-left corner is accent-colored.
        let stub_x = (spec.inset + spec.radius + 2.0).round() as u32;
        let stub_y = spec.inset.round() as u32;
        let p = px(&accented, 32, stub_x, stub_y);
        assert!(p[3] > 0, "stub pixel drawn");
        assert_eq!(hex_of(p), "#cc4433");
        // With stub 0 the variants are identical.
        let no_stub = FrameSpec {
            stub: 0.0,
            ..FrameSpec::default()
        };
        assert_eq!(
            render_frame(32, "#404850", None, &no_stub),
            render_frame(32, "#404850", Some("#cc4433"), &no_stub)
        );
    }

    #[test]
    fn slice_clip_warning_matches_geometry() {
        assert!(!FrameSpec::default().slice_clips_corners());
        let clipping = FrameSpec {
            radius: 6.0,
            width: 2.0,
            slice: 4.0,
            ..FrameSpec::default()
        };
        assert!(clipping.slice_clips_corners());
    }

    #[test]
    fn darken_and_lift_move_lightness_keep_hue() {
        use crate::core::harmony::hex_to_lch;
        let base = "#5577aa";
        let [l0, _, h0] = hex_to_lch(base).unwrap();
        let darker = darken(base, 14.0);
        let [l1, _, h1] = hex_to_lch(&darker).unwrap();
        assert!(l1 < l0, "darken lowers L");
        assert!((h1 - h0).abs() < 2.0, "hue held");
        let lifted = lift(base, 16.0);
        let [l2, _, _] = hex_to_lch(&lifted).unwrap();
        assert!(l2 > l0, "lift raises L");
    }
}
