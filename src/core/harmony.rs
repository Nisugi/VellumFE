//! Harmony-driven palette generation.
//!
//! Generates the full set of game-text preset colors from one seed color
//! (normally picked from the active theme's swatches) against the theme
//! background, using OKLCH color math: equal lightness reads equally bright
//! across hues, so harmony built on it keeps every role readable. Ported from
//! Niffy's vellum-palette-harmony.html prototype.
//!
//! The engine is pure string/scalar math with no config, theme, or frontend
//! dependencies; callers (the `.harmony` command, the GUI Generate tab) own
//! seeding, persistence, and presentation.

use std::collections::HashMap;

// ─── sRGB <-> OKLab / OKLCH (Ottosson's transform) ──────────────────────────

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Parse `#rgb`, `#rrggbb`, or `#rrggbbaa` (alpha ignored) into 0..1 sRGB.
pub fn parse_hex(hex: &str) -> Option<[f64; 3]> {
    let h = hex.trim().strip_prefix('#')?;
    let expand = |s: &str| -> Option<Vec<u8>> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok())
            .collect()
    };
    let bytes = match h.len() {
        3 => {
            let doubled: String = h.chars().flat_map(|c| [c, c]).collect();
            expand(&doubled)?
        }
        6 | 8 => expand(h)?,
        _ => return None,
    };
    Some([
        bytes[0] as f64 / 255.0,
        bytes[1] as f64 / 255.0,
        bytes[2] as f64 / 255.0,
    ])
}

pub fn to_hex(rgb: [f64; 3]) -> String {
    let ch = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", ch(rgb[0]), ch(rgb[1]), ch(rgb[2]))
}

fn rgb_to_oklab(rgb: [f64; 3]) -> [f64; 3] {
    let r = srgb_to_linear(rgb[0]);
    let g = srgb_to_linear(rgb[1]);
    let b = srgb_to_linear(rgb[2]);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn oklab_to_rgb(lab: [f64; 3]) -> [f64; 3] {
    let [ll, a, b] = lab;
    let l = (ll + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (ll - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (ll - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        linear_to_srgb(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
        linear_to_srgb(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
        linear_to_srgb(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s),
    ]
}

fn lab_to_lch(lab: [f64; 3]) -> [f64; 3] {
    let [l, a, b] = lab;
    [l, a.hypot(b), (b.atan2(a).to_degrees() + 360.0) % 360.0]
}

fn lch_to_lab(lch: [f64; 3]) -> [f64; 3] {
    let [l, c, h] = lch;
    [l, c * h.to_radians().cos(), c * h.to_radians().sin()]
}

fn in_gamut(rgb: [f64; 3]) -> bool {
    rgb.iter().all(|v| (-0.001..=1.001).contains(v))
}

/// OKLCH to hex. Rotating hue often lands outside sRGB; reduce chroma by
/// bisection until it fits — preserving hue and lightness, which is what the
/// eye keys on.
pub fn lch_to_hex(lch: [f64; 3]) -> String {
    let [l, c, h] = lch;
    let rgb = oklab_to_rgb(lch_to_lab([l, c, h]));
    if in_gamut(rgb) {
        return to_hex(rgb);
    }
    let (mut lo, mut hi) = (0.0, c);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if in_gamut(oklab_to_rgb(lch_to_lab([l, mid, h]))) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    to_hex(oklab_to_rgb(lch_to_lab([l, lo, h])))
}

/// Hex to OKLCH `[lightness, chroma, hue-degrees]`.
pub fn hex_to_lch(hex: &str) -> Option<[f64; 3]> {
    Some(lab_to_lch(rgb_to_oklab(parse_hex(hex)?)))
}

/// WCAG 2.x relative-luminance contrast ratio (1..21). Returns 1.0 when
/// either color fails to parse, i.e. "no measurable contrast".
pub fn wcag_contrast(a: &str, b: &str) -> f64 {
    let lum = |hex: &str| -> Option<f64> {
        let [r, g, b] = parse_hex(hex)?;
        Some(0.2126 * srgb_to_linear(r) + 0.7152 * srgb_to_linear(g) + 0.0722 * srgb_to_linear(b))
    };
    match (lum(a), lum(b)) {
        (Some(l1), Some(l2)) => (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05),
        _ => 1.0,
    }
}

/// Perceptual distance: euclidean in OKLab. ~0.02 is "barely different";
/// >0.1 is clearly distinct. Returns 0.0 when either color fails to parse.
pub fn delta_e(a: &str, b: &str) -> f64 {
    match (parse_hex(a), parse_hex(b)) {
        (Some(ra), Some(rb)) => {
            let la = rgb_to_oklab(ra);
            let lb = rgb_to_oklab(rb);
            ((la[0] - lb[0]).powi(2) + (la[1] - lb[1]).powi(2) + (la[2] - lb[2]).powi(2)).sqrt()
        }
        _ => 0.0,
    }
}

// ─── Harmony schemes ────────────────────────────────────────────────────────

/// Hue offsets in degrees around the OKLCH hue circle. The golden-angle
/// entry is the honest answer to "give me N maximally distinct hues" —
/// 137.5° never repeats and spreads evenly however many you take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Monochrome,
    Analogous,
    Complementary,
    Split,
    Triadic,
    Tetradic,
    Golden,
    Compound,
}

impl Scheme {
    pub const ALL: [Scheme; 8] = [
        Scheme::Monochrome,
        Scheme::Analogous,
        Scheme::Complementary,
        Scheme::Split,
        Scheme::Triadic,
        Scheme::Tetradic,
        Scheme::Golden,
        Scheme::Compound,
    ];

    pub fn offsets(self) -> &'static [f64] {
        match self {
            Scheme::Monochrome => &[0.0],
            Scheme::Analogous => &[0.0, 30.0, -30.0, 60.0, -60.0],
            Scheme::Complementary => &[0.0, 180.0],
            Scheme::Split => &[0.0, 150.0, 210.0],
            Scheme::Triadic => &[0.0, 120.0, 240.0],
            Scheme::Tetradic => &[0.0, 90.0, 180.0, 270.0],
            Scheme::Golden => &[0.0, 137.5, 275.0, 52.5, 190.0, 327.5],
            Scheme::Compound => &[0.0, 30.0, 180.0, 210.0],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Scheme::Monochrome => "monochrome",
            Scheme::Analogous => "analogous",
            Scheme::Complementary => "complementary",
            Scheme::Split => "split",
            Scheme::Triadic => "triadic",
            Scheme::Tetradic => "tetradic",
            Scheme::Golden => "golden",
            Scheme::Compound => "compound",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Scheme::Monochrome => "one hue, varied lightness and chroma",
            Scheme::Analogous => "neighbours - calm, low contrast",
            Scheme::Complementary => "opposites - strong pairing",
            Scheme::Split => "softer than complementary",
            Scheme::Triadic => "even thirds - vivid, balanced",
            Scheme::Tetradic => "two complementary pairs",
            Scheme::Golden => "golden angle - max separation",
            Scheme::Compound => "analogous pair plus its opposite",
        }
    }

    pub fn parse(name: &str) -> Option<Scheme> {
        Scheme::ALL
            .into_iter()
            .find(|s| s.name().eq_ignore_ascii_case(name.trim()))
    }
}

// ─── Roles ──────────────────────────────────────────────────────────────────

/// OKLCH hue of a convincing alarm red. Hue-anchored roles carry meaning by
/// staying warm (the target indicator is held red on purpose); harmony only
/// varies their lightness/chroma for readability.
const ALARM_HUE: f64 = 27.0;

/// One generated color role. `scheme_slot` indexes into the scheme's hue
/// offsets; `None` means hue-anchored to `anchor_hue`. `dl`/`dc` nudge
/// lightness and chroma so roles sharing a hue stay apart.
pub struct Role {
    pub name: &'static str,
    scheme_slot: Option<usize>,
    anchor_hue: f64,
    dl: f64,
    dc: f64,
    pub description: &'static str,
}

const fn free(
    name: &'static str,
    slot: usize,
    dl: f64,
    dc: f64,
    description: &'static str,
) -> Role {
    Role {
        name,
        scheme_slot: Some(slot),
        anchor_hue: 0.0,
        dl,
        dc,
        description,
    }
}

/// The game-text preset roles, matching `[presets.*]` keys in colors.toml.
pub const ROLES: [Role; 11] = [
    free("links", 0, 0.00, 0.00, "clickable nouns and exits"),
    free("commands", 0, 0.06, -0.02, "command links"),
    free("speech", 1, 0.04, 0.00, "says / asks / exclaims"),
    free("whisper", 1, -0.08, -0.03, "whispers"),
    free("thought", 2, 0.06, 0.00, "thought and ESP channels"),
    free("familiar", 2, -0.10, -0.04, "familiar window"),
    free("monsterbold", 3, 0.02, 0.02, "creature names"),
    free("roomName", 4, -0.04, -0.06, "room title"),
    free("percWindow", 4, 0.08, 0.00, "perception window"),
    free("voln", 5, -0.02, 0.00, "Voln society"),
    Role {
        name: "target_indicator",
        scheme_slot: None,
        anchor_hue: ALARM_HUE,
        dl: 0.0,
        dc: 0.0,
        description: "current target - held warm on purpose",
    },
];

/// Prompt indicators are status signaling, not decoration: Roundtime and
/// Bleeding carry meaning by being warm, Stunned by being amber, Hiding by
/// being cool. Harmony varies their lightness/chroma for readability but
/// holds each inside its semantic hue band regardless of the scheme.
/// `anchor_hue: None` means "follow the seed hue" (the neutral prompt char
/// is desaturated to near-gray, so its hue barely shows).
pub struct PromptRole {
    /// The prompt glyph matched by `[[prompt_colors]]` (R, S, H, >, !).
    pub character: &'static str,
    pub label: &'static str,
    anchor_hue: Option<f64>,
    dl: f64,
    dc: f64,
}

/// OKLCH hues for the semantic bands: amber warning and cool violet.
const WARN_HUE: f64 = 85.0;
const HIDE_HUE: f64 = 300.0;

pub const PROMPT_ROLES: [PromptRole; 5] = [
    PromptRole {
        character: "R",
        label: "roundtime",
        anchor_hue: Some(ALARM_HUE),
        dl: 0.0,
        dc: 0.05,
    },
    PromptRole {
        character: "!",
        label: "bleeding",
        anchor_hue: Some(ALARM_HUE),
        dl: -0.15,
        dc: 0.0,
    },
    PromptRole {
        character: "S",
        label: "stunned",
        anchor_hue: Some(WARN_HUE),
        dl: 0.05,
        dc: 0.02,
    },
    PromptRole {
        character: "H",
        label: "hiding",
        anchor_hue: Some(HIDE_HUE),
        dl: 0.0,
        dc: -0.06,
    },
    PromptRole {
        character: ">",
        label: "prompt",
        anchor_hue: None,
        dl: -0.10,
        dc: -0.25,
    },
];

// ─── Parameters and generation ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HarmonyParams {
    /// Seed color hex; its OKLCH hue anchors the scheme, its lightness and
    /// chroma anchor the candidate ladder.
    pub seed: String,
    /// Background every generated color must stay readable against.
    pub background: String,
    pub scheme: Scheme,
    /// Hue-spread multiplier on the scheme offsets (low 0.7 / med 1.0 / high 1.4).
    pub variance: f64,
    /// WCAG contrast floor against `background` (low 3.0 / med 4.5 / high 7.0).
    pub min_contrast: f64,
    /// Preferred minimum OKLab distance between two roles
    /// (low 0.04 / med 0.09 / high 0.15).
    pub separation: f64,
    /// Contrast ratio of the room title against its own background plate
    /// (low 2.5 reads as a subtle plate, high 7.0 as a hard label).
    pub room_title_spread: f64,
    /// Pinned roles: name -> hex kept verbatim; the rest harmonize around them.
    pub pins: HashMap<String, String>,
}

impl Default for HarmonyParams {
    fn default() -> Self {
        Self {
            seed: "#477ab3".to_string(),
            background: "#000000".to_string(),
            scheme: Scheme::Triadic,
            variance: 1.0,
            min_contrast: 4.5,
            separation: 0.09,
            room_title_spread: 2.5,
            pins: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HarmonyResult {
    /// `(role name, hex)` in ROLES order.
    pub colors: Vec<(String, String)>,
    /// Room title background plate, lightness-solved against the generated
    /// roomName color to hit `room_title_spread`.
    pub room_bg: String,
    /// `(prompt character, hex)` in PROMPT_ROLES order, hue-anchored to
    /// their semantic bands.
    pub prompts: Vec<(String, String)>,
}

impl HarmonyResult {
    pub fn color_for(&self, role: &str) -> Option<&str> {
        self.colors
            .iter()
            .find(|(name, _)| name == role)
            .map(|(_, hex)| hex.as_str())
    }
}

fn hue_for(role: &Role, params: &HarmonyParams, seed_hue: f64) -> f64 {
    match role.scheme_slot {
        None => role.anchor_hue,
        Some(slot) => {
            let off = params.scheme.offsets();
            (seed_hue + off[slot % off.len()] * params.variance).rem_euclid(360.0)
        }
    }
}

/// Raise (or lower, on light backgrounds) lightness until the color clears
/// the contrast floor against the background. Cheaper and less destructive
/// than desaturating.
fn lift_to_contrast(mut l: f64, c: f64, h: f64, params: &HarmonyParams) -> String {
    let mut hex = lch_to_hex([l, c, h]);
    if wcag_contrast(&hex, &params.background) >= params.min_contrast {
        return hex;
    }
    let bg_light = hex_to_lch(&params.background)
        .map(|lch| lch[0])
        .unwrap_or(0.0);
    let step = if bg_light < 0.5 { 0.015 } else { -0.015 };
    for _ in 0..40 {
        l = (l + step).clamp(0.05, 0.99);
        hex = lch_to_hex([l, c, h]);
        if wcag_contrast(&hex, &params.background) >= params.min_contrast {
            break;
        }
    }
    hex
}

/// Ladder of lightness and chroma around a nominal point, contrast-lifted.
/// Roles that share a hue separate along L, which is what the eye reads
/// first. Ordered nearest-to-nominal first.
fn candidates_at(
    hue: f64,
    dl: f64,
    dc: f64,
    params: &HarmonyParams,
    seed: [f64; 3],
) -> Vec<String> {
    let mut out = Vec::new();
    for step_l in [0.0, 0.09, -0.09, 0.18, -0.18, 0.27, -0.27] {
        for step_c in [0.0, 0.05, -0.05, 0.10] {
            let l = (seed[0] + dl + step_l).clamp(0.28, 0.94);
            let c = (seed[1] + dc + step_c).clamp(0.015, 0.33);
            let hex = lift_to_contrast(l, c, hue, params);
            if !out.contains(&hex) {
                out.push(hex);
            }
        }
    }
    out
}

/// First candidate (nearest the nominal point) that clears the separation
/// floor and isn't lost in the background; else greedy max-min so roles
/// sharing a hue never come out identical. (The prototype exposed the
/// separation knob but never read it.)
fn pick_color(cands: &[String], used: &[String], params: &HarmonyParams) -> String {
    let min_dist = |hex: &str| -> f64 {
        used.iter()
            .map(|u| delta_e(u, hex))
            .fold(f64::INFINITY, f64::min)
    };
    cands
        .iter()
        .find(|c| delta_e(c, &params.background) >= 0.06 && min_dist(c) >= params.separation)
        .or_else(|| {
            cands
                .iter()
                .filter(|c| delta_e(c, &params.background) >= 0.06)
                .max_by(|a, b| min_dist(a).total_cmp(&min_dist(b)))
        })
        .or_else(|| cands.first())
        .cloned()
        .unwrap_or_else(|| params.seed.clone())
}

/// Generate the full role set. Deterministic for a given `HarmonyParams`.
pub fn generate(params: &HarmonyParams) -> HarmonyResult {
    let seed = hex_to_lch(&params.seed).unwrap_or([0.7, 0.12, 250.0]);
    let mut used: Vec<String> = Vec::new();
    let mut colors: Vec<(String, String)> = Vec::new();

    for role in &ROLES {
        if let Some(pinned) = params.pins.get(role.name) {
            used.push(pinned.clone());
            colors.push((role.name.to_string(), pinned.clone()));
            continue;
        }
        let cands = candidates_at(
            hue_for(role, params, seed[2]),
            role.dl,
            role.dc,
            params,
            seed,
        );
        let pick = pick_color(&cands, &used, params);
        used.push(pick.clone());
        colors.push((role.name.to_string(), pick));
    }

    // Prompt indicators: separate used-list (they appear in a different
    // context than story text, so they only need to stay distinct from each
    // other), hue-anchored to their semantic bands.
    let mut prompt_used: Vec<String> = Vec::new();
    let mut prompts: Vec<(String, String)> = Vec::new();
    for role in &PROMPT_ROLES {
        let hue = role.anchor_hue.unwrap_or(seed[2]);
        let cands = candidates_at(hue, role.dl, role.dc, params, seed);
        let pick = pick_color(&cands, &prompt_used, params);
        prompt_used.push(pick.clone());
        prompts.push((role.character.to_string(), pick));
    }

    // Room title background: same hue family as the roomName color, lightness
    // solved by bisection so the pair hits the requested contrast ratio.
    let room_name = colors
        .iter()
        .find(|(n, _)| n == "roomName")
        .map(|(_, hex)| hex.clone())
        .unwrap_or_else(|| params.seed.clone());
    let rl = hex_to_lch(&room_name).unwrap_or([0.7, 0.1, 250.0]);
    let c = (rl[1] * 0.7).clamp(0.02, 0.2);
    let (mut lo, mut hi) = (0.05, (rl[0] - 0.02).max(0.05));
    let mut room_bg = lch_to_hex([lo, c, rl[2]]);
    for _ in 0..26 {
        let mid = (lo + hi) / 2.0;
        let hex = lch_to_hex([mid, c, rl[2]]);
        if wcag_contrast(&room_name, &hex) > params.room_title_spread {
            lo = mid;
        } else {
            hi = mid;
        }
        room_bg = hex;
    }

    HarmonyResult {
        colors,
        room_bg,
        prompts,
    }
}

/// For the click-a-swatch explorer: hold the role's lightness/chroma target
/// but rotate the hue by noticeable steps around the wheel, so the user
/// chooses between genuinely different hues rather than shades of one.
pub fn hue_variants(role_name: &str, params: &HarmonyParams) -> Vec<String> {
    let Some(role) = ROLES.iter().find(|r| r.name == role_name) else {
        return Vec::new();
    };
    let seed = hex_to_lch(&params.seed).unwrap_or([0.7, 0.12, 250.0]);
    let base_h = hue_for(role, params, seed[2]);
    let l = (seed[0] + role.dl).clamp(0.30, 0.92);
    let c = (seed[1] + role.dc).clamp(0.03, 0.30);
    let mut out = Vec::new();
    for d in [
        0.0, 30.0, -30.0, 60.0, -60.0, 90.0, -90.0, 140.0, 180.0, 220.0,
    ] {
        let hex = lift_to_contrast(l, c, (base_h + d).rem_euclid(360.0), params);
        if !out.contains(&hex) {
            out.push(hex);
        }
    }
    out
}

// ─── Seed swatch filtering ──────────────────────────────────────────────────

/// Curate raw theme colors into seed candidates: drop near-grays and colors
/// that vanish into the background, dedupe perceptually, sort most vivid
/// first, cap the strip. This filter is part of the feature's quality — every
/// chip shown must be a safe seed, or the "click and see" workflow regresses
/// to guesswork.
pub fn filter_seed_swatches(raw: &[String], background: &str, cap: usize) -> Vec<String> {
    let pass = |min_chroma: f64| -> Vec<(String, f64)> {
        let mut swatches: Vec<(String, f64)> = Vec::new();
        for hex in raw {
            let Some([_, chroma, _]) = hex_to_lch(hex) else {
                continue;
            };
            if chroma < min_chroma {
                continue; // near-gray: seeds a mud palette
            }
            if delta_e(hex, background) < 0.15 {
                continue; // vanishes into the background
            }
            if swatches.iter().any(|(seen, _)| delta_e(seen, hex) < 0.02) {
                continue; // perceptual duplicate
            }
            swatches.push((hex.clone(), chroma));
        }
        swatches
    };
    // All-gray themes (e.g. the built-in monochrome) have no vivid colors at
    // all; a gray seed is then coherent, so relax the chroma floor rather
    // than offer an empty strip.
    let mut swatches = pass(0.04);
    if swatches.is_empty() {
        swatches = pass(0.0);
    }
    swatches.sort_by(|a, b| b.1.total_cmp(&a.1));
    swatches.truncate(cap);
    swatches.into_iter().map(|(hex, _)| hex).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> HarmonyParams {
        HarmonyParams {
            seed: "#bf616a".to_string(), // Nord's red accent
            background: "#2e3440".to_string(),
            ..HarmonyParams::default()
        }
    }

    #[test]
    fn hex_parsing_roundtrip_and_forms() {
        assert_eq!(to_hex(parse_hex("#4a7ab3").unwrap()), "#4a7ab3");
        // Short form expands per-digit.
        assert_eq!(to_hex(parse_hex("#abc").unwrap()), "#aabbcc");
        // 8-digit hex ignores alpha (target_indicator default is #ad0d0dff).
        assert_eq!(to_hex(parse_hex("#ad0d0dff").unwrap()), "#ad0d0d");
        assert!(parse_hex("#12345").is_none());
        assert!(parse_hex("not a color").is_none());
    }

    #[test]
    fn oklch_roundtrip_stays_close() {
        for hex in [
            "#ff0000", "#00ff00", "#0000ff", "#808080", "#bf616a", "#123456",
        ] {
            let lch = hex_to_lch(hex).unwrap();
            let back = lch_to_hex(lch);
            assert!(
                delta_e(hex, &back) < 0.01,
                "{hex} -> {lch:?} -> {back} drifted"
            );
        }
    }

    #[test]
    fn known_oklch_values() {
        // White: L=1, C~0. Black: L=0.
        let white = hex_to_lch("#ffffff").unwrap();
        assert!(
            (white[0] - 1.0).abs() < 0.01 && white[1] < 0.01,
            "{white:?}"
        );
        let black = hex_to_lch("#000000").unwrap();
        assert!(black[0].abs() < 0.01, "{black:?}");
        // sRGB red: Ottosson's reference gives L~0.628, C~0.258, h~29.2°.
        let red = hex_to_lch("#ff0000").unwrap();
        assert!((red[0] - 0.628).abs() < 0.01, "{red:?}");
        assert!((red[1] - 0.258).abs() < 0.01, "{red:?}");
        assert!((red[2] - 29.23).abs() < 1.0, "{red:?}");
    }

    #[test]
    fn gamut_fit_preserves_hue_and_lightness() {
        // Chroma 0.4 at this hue is far outside sRGB; the fit must land in
        // gamut with hue and lightness intact.
        let hex = lch_to_hex([0.6, 0.4, 145.0]);
        let lch = hex_to_lch(&hex).unwrap();
        assert!((lch[0] - 0.6).abs() < 0.02, "lightness held: {lch:?}");
        assert!((lch[2] - 145.0).abs() < 2.0, "hue held: {lch:?}");
    }

    #[test]
    fn wcag_contrast_known_values() {
        assert!((wcag_contrast("#ffffff", "#000000") - 21.0).abs() < 0.01);
        assert!((wcag_contrast("#000000", "#ffffff") - 21.0).abs() < 0.01);
        assert!((wcag_contrast("#777777", "#777777") - 1.0).abs() < 0.01);
        // Unparseable input reads as "no contrast", never a panic.
        assert_eq!(wcag_contrast("bogus", "#000000"), 1.0);
    }

    #[test]
    fn generate_is_deterministic_and_complete() {
        let p = params();
        let a = generate(&p);
        let b = generate(&p);
        assert_eq!(a.colors, b.colors);
        assert_eq!(a.room_bg, b.room_bg);
        assert_eq!(a.colors.len(), ROLES.len());
        for role in &ROLES {
            assert!(a.color_for(role.name).is_some(), "missing {}", role.name);
        }
    }

    #[test]
    fn generated_colors_clear_the_contrast_floor() {
        for scheme in Scheme::ALL {
            let p = HarmonyParams { scheme, ..params() };
            let result = generate(&p);
            for (name, hex) in &result.colors {
                let cr = wcag_contrast(hex, &p.background);
                assert!(
                    cr >= p.min_contrast - 0.35,
                    "{name} = {hex} only {cr:.2}:1 against {} ({})",
                    p.background,
                    scheme.name()
                );
            }
        }
    }

    #[test]
    fn generated_colors_are_mutually_distinct() {
        let result = generate(&params());
        for (i, (an, ah)) in result.colors.iter().enumerate() {
            for (bn, bh) in result.colors.iter().skip(i + 1) {
                assert!(
                    delta_e(ah, bh) > 0.015,
                    "{an} ({ah}) and {bn} ({bh}) are nearly identical"
                );
            }
        }
    }

    #[test]
    fn pinned_roles_survive_generation() {
        let mut p = params();
        p.pins.insert("speech".to_string(), "#12ab34".to_string());
        let result = generate(&p);
        assert_eq!(result.color_for("speech"), Some("#12ab34"));
    }

    #[test]
    fn target_indicator_stays_hue_anchored() {
        // Whatever the scheme or seed, the target indicator holds the alarm
        // hue band (warm red/orange).
        for scheme in Scheme::ALL {
            for seed in ["#50fa7b", "#6a9fb5", "#c09eff"] {
                let p = HarmonyParams {
                    seed: seed.to_string(),
                    scheme,
                    ..params()
                };
                let result = generate(&p);
                let hex = result.color_for("target_indicator").unwrap();
                let hue = hex_to_lch(hex).unwrap()[2];
                let dist = (hue - ALARM_HUE).abs().min(360.0 - (hue - ALARM_HUE).abs());
                assert!(
                    dist < 25.0,
                    "target_indicator {hex} hue {hue:.0} left the alarm band ({} seed {seed})",
                    scheme.name()
                );
            }
        }
    }

    #[test]
    fn prompt_roles_stay_in_their_semantic_hue_bands() {
        let hue_dist = |a: f64, b: f64| (a - b).abs().min(360.0 - (a - b).abs());
        // Whatever the scheme or seed, anchored prompts hold their bands.
        for scheme in Scheme::ALL {
            for seed in ["#50fa7b", "#6a9fb5", "#c09eff"] {
                let p = HarmonyParams {
                    seed: seed.to_string(),
                    scheme,
                    ..params()
                };
                let result = generate(&p);
                for (character, hex) in &result.prompts {
                    let role = PROMPT_ROLES
                        .iter()
                        .find(|r| r.character == character)
                        .unwrap();
                    let Some(anchor) = role.anchor_hue else {
                        continue; // ">" follows the seed and is near-gray
                    };
                    let [_, chroma, hue] = hex_to_lch(hex).unwrap();
                    // Heavy desaturation makes hue meaningless; only assert
                    // the band when the color visibly carries one.
                    if chroma >= 0.04 {
                        assert!(
                            hue_dist(hue, anchor) < 30.0,
                            "{} ({hex}) hue {hue:.0} left its band around {anchor} \
                             ({} seed {seed})",
                            role.label,
                            scheme.name()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn prompt_colors_are_readable_and_distinct() {
        let p = params();
        let result = generate(&p);
        assert_eq!(result.prompts.len(), PROMPT_ROLES.len());
        for (character, hex) in &result.prompts {
            let cr = wcag_contrast(hex, &p.background);
            assert!(
                cr >= p.min_contrast - 0.35,
                "prompt '{character}' = {hex} only {cr:.2}:1"
            );
        }
        // Roundtime and Bleeding share the alarm hue: they must separate
        // along lightness, not collapse into one color.
        let r = result.prompts.iter().find(|(c, _)| c == "R").unwrap();
        let bang = result.prompts.iter().find(|(c, _)| c == "!").unwrap();
        assert!(
            delta_e(&r.1, &bang.1) > 0.015,
            "roundtime {} and bleeding {} are nearly identical",
            r.1,
            bang.1
        );
    }

    #[test]
    fn room_bg_hits_requested_spread() {
        for spread in [2.5, 7.0] {
            let p = HarmonyParams {
                room_title_spread: spread,
                ..params()
            };
            let result = generate(&p);
            let room = result.color_for("roomName").unwrap();
            let cr = wcag_contrast(room, &result.room_bg);
            assert!(
                (cr - spread).abs() < 0.6,
                "roomName {room} on {} is {cr:.2}:1, wanted ~{spread}",
                result.room_bg
            );
        }
    }

    #[test]
    fn hue_variants_are_distinct_and_readable() {
        let p = params();
        let variants = hue_variants("speech", &p);
        assert!(variants.len() >= 5, "want a real choice: {variants:?}");
        for v in &variants {
            assert!(
                wcag_contrast(v, &p.background) >= p.min_contrast - 0.35,
                "variant {v} unreadable"
            );
        }
        assert!(hue_variants("no_such_role", &p).is_empty());
    }

    #[test]
    fn swatch_filter_drops_grays_bg_and_duplicates() {
        let bg = "#2e3440";
        let raw: Vec<String> = [
            "#808080", // gray: dropped (chroma)
            "#2f3541", // ~= background: dropped
            "#bf616a", // vivid: kept
            "#bf616b", // perceptual duplicate of the above: dropped
            "#88c0d0", // kept
            "#a3be8c", // kept
            "not-a-color",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let out = filter_seed_swatches(&raw, bg, 12);
        assert!(out.contains(&"#bf616a".to_string()));
        assert!(out.contains(&"#88c0d0".to_string()));
        assert!(out.contains(&"#a3be8c".to_string()));
        assert_eq!(out.len(), 3, "{out:?}");
        // Cap respected and most-vivid-first ordering.
        let capped = filter_seed_swatches(&raw, bg, 2);
        assert_eq!(capped.len(), 2);
        let c0 = hex_to_lch(&capped[0]).unwrap()[1];
        let c1 = hex_to_lch(&capped[1]).unwrap()[1];
        assert!(c0 >= c1, "sorted by chroma desc");
    }

    #[test]
    fn scheme_parse_and_names_roundtrip() {
        for scheme in Scheme::ALL {
            assert_eq!(Scheme::parse(scheme.name()), Some(scheme));
            assert_eq!(Scheme::parse(&scheme.name().to_uppercase()), Some(scheme));
        }
        assert_eq!(Scheme::parse("cubist"), None);
    }
}
