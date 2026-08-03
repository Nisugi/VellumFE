//! Custom (Discord-server-style) emoji painting for the GUI.
//!
//! This is the disk-loading, animation-capable sibling of [`super::color_emoji`].
//! Where `color_emoji` paints bundled Twemoji PNGs over standard Unicode emoji
//! glyphs, this module paints user-supplied images referenced by `:name:`
//! shortcodes (see [`crate::core::custom_emoji`]). A resolved custom-emoji
//! segment keeps its literal `:name:` fallback text in the buffer/galley
//! (so layout, selection, and copy see the shortcode), and we paint the
//! picture as a pure overlay over that run — the same non-destructive pattern
//! `color_emoji` uses.
//!
//! Differences from `color_emoji`:
//! - Images load lazily FROM DISK (`custom_emoji::get(name).path`), not from an
//!   embedded `include_dir`. A file that vanished negative-caches so the caller
//!   falls back to leaving the `:name:` text visible.
//! - ANIMATED formats (GIF, APNG) decode into a `Vec<(TextureHandle, delay)>`;
//!   the painter picks the current frame from egui's monotonic frame clock
//!   (`ctx.input(|i| i.time)`), never wall-clock time, and requests a repaint
//!   so the animation keeps ticking.
//!
//! Toggle policy: custom emoji are images with no monochrome fallback, so they
//! always render as images regardless of the `ui.color_emoji` setting (that
//! toggle governs COLOR-vs-monochrome for Unicode emoji, a distinction that
//! does not apply here). The caller still only reaches this path for segments
//! the resolver tagged as custom emoji.
//!
//! Like the rest of `frontend/gui`, this module compiles only with the `gui`
//! feature.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::core::custom_emoji::{self, EmojiFormat};

/// One decoded custom emoji: its animation frames as textures plus the total
/// cycle duration in seconds. A static image is a single frame with a zero
/// cycle length (never advanced).
#[derive(Clone)]
struct EmojiFrames {
    /// `(texture, cumulative_end_time_secs)`. `cumulative_end_time` is the
    /// running sum of frame delays, so the current frame is the first whose
    /// end time exceeds `time % total`. A single static frame stores its end
    /// at `0.0` and is always chosen by the `total == 0.0` short-circuit.
    frames: Vec<(egui::TextureHandle, f32)>,
    /// Sum of all frame delays in seconds; `0.0` for a static image.
    total: f32,
}

impl EmojiFrames {
    /// Pick the texture to show at monotonic `time` (seconds since app start),
    /// cycling by accumulated frame delays. Static emoji (one frame) always
    /// return that frame.
    fn frame_at(&self, time: f64) -> &egui::TextureHandle {
        if self.total <= 0.0 || self.frames.len() <= 1 {
            return &self.frames[0].0;
        }
        let phase = (time % self.total as f64) as f32;
        for (texture, end) in &self.frames {
            if phase < *end {
                return texture;
            }
        }
        // Float slop at the wrap boundary: fall back to the last frame.
        &self.frames.last().expect("frames is non-empty").0
    }
}

/// Texture cache: one decoded entry per custom-emoji name actually seen,
/// keyed by lowercased shortcode. `None` negative-caches names whose file is
/// missing or failed to decode so repeated misses stay cheap and the caller
/// can fall back to the `:name:` text. Lives in egui ctx data behind an Arc so
/// renderers stay stateless (same idiom as `color_emoji`'s texture cache).
type FrameCache = Arc<Mutex<HashMap<String, Option<EmojiFrames>>>>;

fn frame_cache(ctx: &egui::Context) -> FrameCache {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_insert_with::<FrameCache>(
            egui::Id::new("custom_emoji_frame_cache"),
            Default::default,
        )
        .clone()
    })
}

/// Upload a single RGBA image as a texture.
fn upload_rgba(
    ctx: &egui::Context,
    name: &str,
    idx: usize,
    rgba: &image::RgbaImage,
) -> egui::TextureHandle {
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    ctx.load_texture(
        format!("custom-emoji-{name}-{idx}"),
        color_image,
        egui::TextureOptions::LINEAR,
    )
}

/// Decode a static image (PNG/WEBP, or the first frame of an APNG) into a
/// single-frame `EmojiFrames`.
fn decode_static(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
    format: image::ImageFormat,
) -> Option<EmojiFrames> {
    let decoded = image::load_from_memory_with_format(bytes, format)
        .map_err(|err| {
            tracing::warn!("Custom emoji :{name}: failed to decode: {err}");
        })
        .ok()?
        .to_rgba8();
    let texture = upload_rgba(ctx, name, 0, &decoded);
    Some(EmojiFrames {
        frames: vec![(texture, 0.0)],
        total: 0.0,
    })
}

/// Decode an animated GIF into all its frames with per-frame delays.
fn decode_gif(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<EmojiFrames> {
    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes))
        .map_err(|err| tracing::warn!("Custom emoji :{name}: GIF open failed: {err}"))
        .ok()?;
    let mut frames = Vec::new();
    let mut acc = 0.0f32;
    for (idx, frame) in decoder.into_frames().enumerate() {
        let frame = match frame {
            Ok(f) => f,
            Err(err) => {
                tracing::warn!("Custom emoji :{name}: GIF frame {idx} decode failed: {err}");
                break;
            }
        };
        // GIF frames can carry a zero/absent delay; browsers clamp very small
        // delays up. Use a 100ms floor so a broken delay does not spin at the
        // frame rate, matching common viewer behavior.
        let delay: Duration = frame.delay().into();
        let mut secs = delay.as_secs_f32();
        if secs < 0.02 {
            secs = 0.1;
        }
        acc += secs;
        let texture = upload_rgba(ctx, name, idx, frame.buffer());
        frames.push((texture, acc));
    }
    if frames.is_empty() {
        return None;
    }
    Some(EmojiFrames {
        total: acc,
        frames,
    })
}

/// Load and decode a custom emoji by name, honoring its format. Animated GIFs
/// yield all frames; APNG currently paints only its first frame (the `image`
/// 0.25 PNG decoder does not expose APNG frame extraction), which is logged
/// once per name. PNG/WEBP are single static textures.
fn decode_emoji(ctx: &egui::Context, name: &str) -> Option<EmojiFrames> {
    let meta = custom_emoji::get(name)?;
    let bytes = std::fs::read(&meta.path)
        .map_err(|err| {
            tracing::warn!(
                "Custom emoji :{name}: file {} unreadable: {err}",
                meta.path.display()
            );
        })
        .ok()?;
    match meta.format {
        EmojiFormat::Gif => decode_gif(ctx, name, &bytes),
        EmojiFormat::Apng => {
            // The bundled `image` crate decodes an APNG's default/first frame
            // as a plain PNG but does not iterate its animation frames, so we
            // show the first frame statically rather than block the feature.
            tracing::warn!(
                "Custom emoji :{name}: APNG animation is not supported; painting first frame only"
            );
            decode_static(ctx, name, &bytes, image::ImageFormat::Png)
        }
        EmojiFormat::Png => decode_static(ctx, name, &bytes, image::ImageFormat::Png),
        EmojiFormat::Webp => decode_static(ctx, name, &bytes, image::ImageFormat::WebP),
    }
}

/// Fetch (decoding + caching on first sight) the frames for `name`, or `None`
/// if the emoji is unknown, its file is gone, or it failed to decode. The
/// `None` result is cached so the caller's text fallback path stays cheap.
fn frames_for(ctx: &egui::Context, cache: &FrameCache, name: &str) -> Option<EmojiFrames> {
    let key = name.to_ascii_lowercase();
    let mut cache = cache.lock().expect("custom emoji frame cache poisoned");
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }
    let decoded = decode_emoji(ctx, &key);
    cache.insert(key, decoded.clone());
    decoded
}

/// Does this custom emoji currently resolve to a paintable image? True lets the
/// caller reserve an image slot; false tells it to render the `:name:` text
/// normally. Decodes-and-caches on first sight.
pub(super) fn is_paintable(ctx: &egui::Context, name: &str) -> bool {
    let cache = frame_cache(ctx);
    frames_for(ctx, &cache, name).is_some()
}

/// Paint the custom emoji `name` centered over `slot`, sized to a square of the
/// slot's height (baseline cell), returning `true` if it painted. When it
/// returns `false` the caller must leave the `:name:` fallback text visible.
///
/// For animated emoji this selects the current frame from `ctx.input(time)`
/// and requests a repaint slightly before the frame's end so the animation
/// keeps advancing without the caller polling a clock.
pub(super) fn paint_custom_emoji(
    ctx: &egui::Context,
    painter: &egui::Painter,
    name: &str,
    slot: egui::Rect,
) -> bool {
    let cache = frame_cache(ctx);
    let Some(frames) = frames_for(ctx, &cache, name) else {
        return false;
    };
    let time = ctx.input(|i| i.time);
    let texture = frames.frame_at(time);

    // Square sized to the slot height, centered horizontally over the slot.
    let side = slot.height();
    let center_x = slot.center().x;
    let rect = egui::Rect::from_min_size(
        egui::pos2(center_x - side * 0.5, slot.top()),
        egui::vec2(side, side),
    );
    painter.image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );

    // Keep animations ticking: schedule the next repaint at this frame's end.
    if frames.total > 0.0 && frames.frames.len() > 1 {
        let phase = (time % frames.total as f64) as f32;
        let next_end = frames
            .frames
            .iter()
            .map(|(_, end)| *end)
            .find(|end| *end > phase)
            .unwrap_or(frames.total);
        let wait = (next_end - phase).max(0.0);
        ctx.request_repaint_after(Duration::from_secs_f32(wait));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assemble a tiny animated GIF (2 frames) in memory so the decode path is
    /// exercised without touching disk.
    fn tiny_gif() -> Vec<u8> {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame, RgbaImage};
        let mut buf = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut buf);
            let red = RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
            let blue = RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]));
            encoder
                .encode_frame(Frame::from_parts(
                    red,
                    0,
                    0,
                    Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
            encoder
                .encode_frame(Frame::from_parts(
                    blue,
                    0,
                    0,
                    Delay::from_numer_denom_ms(150, 1),
                ))
                .unwrap();
        }
        buf
    }

    #[test]
    fn decode_gif_yields_all_frames_with_cumulative_delays() {
        let ctx = egui::Context::default();
        let bytes = tiny_gif();
        let frames = decode_gif(&ctx, "dance", &bytes).expect("gif decodes");
        assert_eq!(frames.frames.len(), 2, "two frames decoded");
        // Cumulative ends are ~0.10 then ~0.25 (100ms + 150ms).
        assert!((frames.frames[0].1 - 0.10).abs() < 0.01);
        assert!((frames.frames[1].1 - 0.25).abs() < 0.01);
        assert!((frames.total - 0.25).abs() < 0.01);
    }

    #[test]
    fn frame_at_cycles_by_accumulated_delay() {
        let ctx = egui::Context::default();
        let frames = decode_gif(&ctx, "dance", &tiny_gif()).expect("gif decodes");
        // 0.05s into the cycle -> first frame; 0.20s -> second frame.
        let f0 = frames.frame_at(0.05).id();
        let f1 = frames.frame_at(0.20).id();
        assert_ne!(f0, f1, "different frames selected across the cycle");
        // Wrap: 0.25 + 0.05 lands back on the first frame.
        assert_eq!(frames.frame_at(0.30).id(), f0);
    }

    #[test]
    fn static_frames_ignore_time() {
        let ctx = egui::Context::default();
        // A 1x1 red PNG.
        let mut png = Vec::new();
        {
            use image::ImageEncoder;
            let encoder = image::codecs::png::PngEncoder::new(&mut png);
            encoder
                .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
                .unwrap();
        }
        let frames = decode_static(&ctx, "static", &png, image::ImageFormat::Png)
            .expect("png decodes");
        assert_eq!(frames.frames.len(), 1);
        assert_eq!(frames.total, 0.0);
        // Same texture at any time.
        assert_eq!(frames.frame_at(0.0).id(), frames.frame_at(99.0).id());
    }

    #[test]
    fn missing_emoji_negative_caches() {
        let ctx = egui::Context::default();
        // Empty registry (test-only reset), so any name is unknown.
        custom_emoji::set_for_test(custom_emoji::registry_from_names(&[]));
        let cache = frame_cache(&ctx);
        assert!(frames_for(&ctx, &cache, "nope").is_none());
        // Cached as a negative entry (present key, None value).
        let guard = cache.lock().unwrap();
        assert!(guard.contains_key("nope"));
        assert!(guard.get("nope").unwrap().is_none());
    }
}
