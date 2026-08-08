//! Discord-style color emoji painting for the GUI.
//!
//! Phase 1 bundled monochrome Noto Emoji as a font fallback and Phase 2
//! rewrites `:grin:`-style shortcodes into emoji chars in core, so emoji lay
//! out and render monochrome everywhere. This module paints them in full
//! color as a pure overlay: buffer text, layout jobs, galleys, selection,
//! copy, and link hit-testing are untouched — after a galley is painted we
//! walk its glyphs and draw the matching Twemoji PNG texture over each emoji
//! glyph's cell.
//!
//! Assets are the Twemoji 72x72 PNG set from the maintained fork
//! jdecked/twemoji v17.0.3 (CC-BY 4.0, see assets/twemoji/ATTRIBUTION.md),
//! embedded via `include_dir` so the GUI build is self-contained. This
//! module only compiles with the `gui` feature (the whole `frontend/gui`
//! tree is feature-gated), so headless/Android builds gain zero bytes.
//!
//! Coverage in v1:
//! - single-codepoint emoji (`1f601.png`), including unqualified forms
//! - VS16 (U+FE0F) variation selectors: consumed and covered by the base
//!   glyph's image (Twemoji filenames omit `fe0f` outside ZWJ sequences)
//! - two-codepoint skin-tone modifier sequences (`1f44d-1f3fb.png`) and
//!   regional-indicator flag pairs (`1f1fa-1f1f8.png`)
//! - ZWJ sequences stay monochrome (skipped entirely, never half-painted)
//! - `(c)`/`(r)`/`(tm)` and other chars below U+203C stay monochrome text
//!   on purpose

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Twemoji 72x72 PNGs, named by lowercase hex codepoints joined with '-'
/// and without leading zeros (e.g. "1f601.png", "23-20e3.png").
static TWEMOJI_72: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets/twemoji/72x72");

/// The `ui.color_emoji` toggle, published once per frame by the app update
/// (reading config inside the paint helpers would need plumbing through
/// every render signature for a value that changes at most once per frame).
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Publish the `ui.color_emoji` setting for this frame's paint calls.
pub(super) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Cheap early-out: can `text` contain anything we would colorize? Every
/// emoji we paint is U+203C or above (this deliberately leaves ©/®/™ and
/// other low-codepoint text symbols monochrome), so lines of plain prose
/// skip the glyph walk entirely.
pub(super) fn may_have_emoji(text: &str) -> bool {
    text.chars().any(|c| c as u32 >= 0x203C)
}

/// Should the caller take the emoji-overlay path for this text at all?
/// Combines the user toggle with the cheap text predicate.
pub(super) fn should_overlay(text: &str) -> bool {
    enabled() && may_have_emoji(text)
}

const VS16: char = '\u{FE0F}';
const ZWJ: char = '\u{200D}';
const KEYCAP: char = '\u{20E3}';

fn is_skin_tone_modifier(c: char) -> bool {
    ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)
}

fn is_regional_indicator(c: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
}

/// Twemoji asset filename for a one- or two-codepoint emoji sequence:
/// lowercase hex, no leading zeros, hyphen-joined, VS16 already stripped by
/// the caller (Twemoji filenames omit U+FE0F outside ZWJ sequences).
pub(super) fn asset_name(base: char, second: Option<char>) -> String {
    match second {
        Some(second) => format!("{:x}-{:x}.png", base as u32, second as u32),
        None => format!("{:x}.png", base as u32),
    }
}

/// Embedded PNG bytes for the sequence, or None when Twemoji does not ship
/// it (which also serves as the "is this an emoji" test for the overlay:
/// anything with a Twemoji file is paintable, everything else stays
/// monochrome text).
pub(super) fn asset_bytes(base: char, second: Option<char>) -> Option<&'static [u8]> {
    // Never map the joiner/selector machinery itself to an image.
    if matches!(base, VS16 | ZWJ | KEYCAP) {
        return None;
    }
    TWEMOJI_72
        .get_file(asset_name(base, second))
        .map(|file| file.contents())
}

/// Texture cache: one texture per emoji sequence actually seen, decoded
/// lazily from the embedded PNG and kept for the app's lifetime (bounded by
/// distinct emoji on screen, ~KBs each). `None` entries negative-cache
/// sequences without an asset so misses stay decode-free too. Lives in egui
/// ctx data behind an Arc so renderers stay stateless (same idiom as the
/// row-height cache in widgets.rs).
type TextureCache = Arc<Mutex<HashMap<(u32, u32), Option<egui::TextureHandle>>>>;

fn cache_key(base: char, second: Option<char>) -> (u32, u32) {
    (base as u32, second.map_or(0, |c| c as u32))
}

fn texture_for(
    ctx: &egui::Context,
    cache: &TextureCache,
    base: char,
    second: Option<char>,
) -> Option<egui::TextureHandle> {
    let key = cache_key(base, second);
    let mut cache = cache.lock().expect("color emoji texture cache poisoned");
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }
    let texture = asset_bytes(base, second).and_then(|bytes| {
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|err| {
                tracing::warn!(
                    "Embedded Twemoji PNG {} failed to decode: {err}",
                    asset_name(base, second)
                );
            })
            .ok()?
            .to_rgba8();
        let size = [decoded.width() as usize, decoded.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
        Some(ctx.load_texture(
            format!("twemoji-{}", asset_name(base, second)),
            color_image,
            egui::TextureOptions::LINEAR,
        ))
    });
    cache.insert(key, texture.clone());
    texture
}

fn texture_cache(ctx: &egui::Context) -> TextureCache {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_insert_with::<TextureCache>(
            egui::Id::new("color_emoji_texture_cache"),
            Default::default,
        )
        .clone()
    })
}

/// Paint color emoji textures over a galley that has just been painted at
/// `galley_pos`. Call AFTER the galley so the images draw on top of the
/// monochrome glyphs; the painter's clip rect applies to the images too.
///
/// The walk consumes at most two glyph cells per image (base [+ VS16]
/// [+ skin-tone modifier | flag partner]) and covers their union with one
/// square image sized to the glyph cell height, baseline-aligned, so the
/// monochrome glyph underneath is hidden without touching the galley,
/// selection highlights, or copy text.
pub(super) fn paint_color_emoji(
    ctx: &egui::Context,
    painter: &egui::Painter,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
) {
    if !enabled() || !may_have_emoji(galley.text()) {
        return;
    }
    let cache = texture_cache(ctx);

    for placed_row in &galley.rows {
        let row_pos = galley_pos + placed_row.pos.to_vec2();
        let glyphs = &placed_row.row.glyphs;
        // Cheap per-row early-out; most rows of a wrapped line are prose.
        if !glyphs.iter().any(|g| g.chr as u32 >= 0x203C) {
            continue;
        }
        let mut prev_is_zwj = false;
        let mut i = 0;
        while i < glyphs.len() {
            let base = glyphs[i].chr;
            if base == ZWJ {
                prev_is_zwj = true;
                i += 1;
                continue;
            }
            if (base as u32) < 0x203C {
                prev_is_zwj = false;
                i += 1;
                continue;
            }

            // Cluster: base [+ VS16] [+ modifier or flag partner].
            let mut last = i;
            let mut second = None;
            let mut next = i + 1;
            if next < glyphs.len() && glyphs[next].chr == VS16 {
                last = next;
                next += 1;
            }
            if next < glyphs.len() {
                let candidate = glyphs[next].chr;
                if is_skin_tone_modifier(candidate)
                    || (is_regional_indicator(base) && is_regional_indicator(candidate))
                {
                    second = Some(candidate);
                    last = next;
                    next += 1;
                }
            }

            // ZWJ sequences keep their monochrome glyphs in v1: painting
            // only the first component would half-colorize the sequence.
            let next_is_zwj = next < glyphs.len() && glyphs[next].chr == ZWJ;
            if prev_is_zwj || next_is_zwj {
                prev_is_zwj = false;
                i = last + 1;
                continue;
            }
            prev_is_zwj = false;

            // Skin-tone pairs missing an asset fall back to the base emoji
            // (still covering both cells); flag pairs do not (painting one
            // letter-box over two would misread as a different flag).
            let texture = texture_for(ctx, &cache, base, second).or_else(|| {
                second
                    .filter(|s| is_skin_tone_modifier(*s))
                    .and_then(|_| texture_for(ctx, &cache, base, None))
            });

            if let Some(texture) = texture {
                let first_glyph = &glyphs[i];
                let x0 = row_pos.x + first_glyph.pos.x;
                let x1 = row_pos.x + glyphs[last].max_x();
                // Square image sized to the glyph cell height,
                // baseline-aligned (cell top = baseline - ascent), centered
                // over the consumed cells.
                let side = first_glyph.font_height;
                let top = row_pos.y + first_glyph.pos.y - first_glyph.font_ascent;
                let center_x = (x0 + x1) * 0.5;
                let rect = egui::Rect::from_min_size(
                    egui::pos2(center_x - side * 0.5, top),
                    egui::vec2(side, side),
                );
                painter.image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            i = last + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names_are_lowercase_hex_without_leading_zeros() {
        assert_eq!(asset_name('\u{1F601}', None), "1f601.png");
        assert_eq!(asset_name('\u{2764}', None), "2764.png");
        assert_eq!(asset_name('\u{1F44D}', Some('\u{1F3FB}')), "1f44d-1f3fb.png");
    }

    #[test]
    fn sample_single_codepoint_assets_exist() {
        // 😁 grin, 👍 thumbs up, ❤ heart (VS16 stripped by the caller:
        // Twemoji names the red heart "2764.png", not "2764-fe0f.png").
        assert!(asset_bytes('\u{1F601}', None).is_some());
        assert!(asset_bytes('\u{1F44D}', None).is_some());
        assert!(asset_bytes('\u{2764}', None).is_some());
    }

    #[test]
    fn skin_tone_modifier_pair_assets_exist() {
        // 👍🏻 .. 👍🏿
        for modifier in '\u{1F3FB}'..='\u{1F3FF}' {
            assert!(
                asset_bytes('\u{1F44D}', Some(modifier)).is_some(),
                "missing thumbs-up asset for modifier {:x}",
                modifier as u32
            );
        }
    }

    #[test]
    fn regional_indicator_flag_pair_asset_exists() {
        // 🇺🇸 = U+1F1FA U+1F1F8
        assert!(asset_bytes('\u{1F1FA}', Some('\u{1F1F8}')).is_some());
    }

    #[test]
    fn non_emoji_chars_have_no_asset() {
        assert!(asset_bytes('a', None).is_none());
        assert!(asset_bytes(':', None).is_none());
        assert!(asset_bytes('\u{2014}', None).is_none()); // em dash
        assert!(asset_bytes('\u{201C}', None).is_none()); // smart quote
        assert!(asset_bytes(VS16, None).is_none());
        assert!(asset_bytes(ZWJ, None).is_none());
        assert!(asset_bytes(KEYCAP, None).is_none());
    }

    #[test]
    fn embedded_assets_decode_as_png() {
        let bytes = asset_bytes('\u{1F601}', None).expect("grin asset present");
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .expect("embedded twemoji decodes");
        assert_eq!(decoded.width(), 72);
        assert_eq!(decoded.height(), 72);
    }

    #[test]
    fn may_have_emoji_predicate() {
        assert!(!may_have_emoji("plain prose, punctuation: 12:30!"));
        // Smart quotes (U+201C/201D) sit below the U+203C floor.
        assert!(!may_have_emoji("smart quotes \u{201C}hi\u{201D} stay cheap"));
        assert!(may_have_emoji("You \u{1F601} broadly."));
        assert!(may_have_emoji("\u{2764}\u{FE0F}"));
        // False positives are allowed (it is only an early-out), e.g. ‼
        assert!(may_have_emoji("\u{203C}"));
    }

    /// Headless paint smoke test: lay out emoji-bearing text, paint the
    /// galley, and run the overlay. Exercises the embedded-asset decode,
    /// texture upload, and the glyph walk end to end (visual placement
    /// still pends a live GUI check).
    #[test]
    fn paint_color_emoji_headless_smoke() {
        let ctx = egui::Context::default();
        let mut painted = false;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 200.0),
                )),
                ..Default::default()
            };
            ctx.begin_pass(input);
            let galley = ctx.fonts_mut(|fonts| {
                fonts.layout_job(egui::text::LayoutJob::simple_singleline(
                    "hi \u{1F601} and \u{2764}\u{FE0F} and \u{1F44D}\u{1F3FB}".to_string(),
                    egui::FontId::proportional(16.0),
                    egui::Color32::WHITE,
                ))
            });
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("color_emoji_test"),
            ));
            paint_color_emoji(&ctx, &painter, &galley, egui::Pos2::ZERO);
            // The overlay must have created textures for the emoji it saw.
            let cache = texture_cache(&ctx);
            let cache = cache.lock().expect("cache lock");
            painted = cache
                .get(&cache_key('\u{1F601}', None))
                .is_some_and(|t| t.is_some());
            drop(cache);
            // egui 0.36 debug-asserts the TexturesDelta is applied before drop;
            // this headless test discards output, so clear the delta.
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
        }
        assert!(painted, "grin texture should be cached after painting");
    }
}
