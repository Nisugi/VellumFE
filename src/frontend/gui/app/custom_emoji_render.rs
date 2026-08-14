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
//! - ANIMATED formats (GIF, animated WebP, APNG) decode into a
//!   `Vec<(TextureHandle, delay)>`; the painter picks the current frame from
//!   egui's monotonic frame clock (`ctx.input(|i| i.time)`), never wall-clock
//!   time, and requests a repaint so the animation keeps ticking. (Discord
//!   serves animated custom emoji as WebP.) A static file with an
//!   animation-capable extension falls back to a single-frame texture.
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

// User-tunable emoji geometry, published once per frame from config (same
// pattern as color_emoji::set_enabled), so build_line_job / the painter — which
// only have &Context — can read them without threading config everywhere.
// Stored as integer bits so they fit in an AtomicU32.
static EMOJI_SIZE_BITS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0x3f80_0000); // 1.0
static EMOJI_SPACING_BITS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0x3e4c_cccd); // 0.2

/// Publish the emoji size/spacing settings for this frame's layout + paint.
/// `size` = square height as a multiple of row height; `spacing` = extra
/// inline width as a fraction of row height.
pub(super) fn set_geometry(size: f32, spacing: f32) {
    use std::sync::atomic::Ordering;
    EMOJI_SIZE_BITS.store(size.max(0.1).to_bits(), Ordering::Relaxed);
    EMOJI_SPACING_BITS.store(spacing.max(0.0).to_bits(), Ordering::Relaxed);
}

/// The emoji square height as a multiple of the text row height.
pub(super) fn size_factor() -> f32 {
    f32::from_bits(EMOJI_SIZE_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

/// The total inline width a custom emoji reserves, as a multiple of row height
/// (square width + horizontal padding). The placeholder run is sized to
/// `row_height * this`; the square is centered in it.
pub(super) fn width_factor() -> f32 {
    let size = size_factor();
    let spacing = f32::from_bits(EMOJI_SPACING_BITS.load(std::sync::atomic::Ordering::Relaxed));
    size + spacing
}

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
    /// Pick the texture for a ONE-SHOT playback that started `elapsed` seconds
    /// ago: the animation runs forward once and then holds its final frame
    /// instead of looping. `frame_at` wraps with `%` by construction, which is
    /// right for emoji but wrong for an alert flourish that should play and be
    /// done — hence a separate method rather than a flag.
    ///
    /// Returns the last frame once `elapsed` passes the cycle length, so a
    /// caller whose overlay outlives the animation shows a settled image
    /// rather than a restart.
    fn frame_once_at(&self, elapsed: f32) -> &egui::TextureHandle {
        if self.total <= 0.0 || self.frames.len() <= 1 {
            return &self.frames[0].0;
        }
        for (texture, end) in &self.frames {
            if elapsed < *end {
                return texture;
            }
        }
        &self.frames.last().expect("frames is non-empty").0
    }

    /// Whether a one-shot playback that began `elapsed` seconds ago is still
    /// advancing. Lets the caller stop scheduling repaints once the animation
    /// has settled, so a lingering overlay costs nothing.
    fn one_shot_running(&self, elapsed: f32) -> bool {
        self.total > 0.0 && self.frames.len() > 1 && elapsed < self.total
    }

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

/// Which registry a name is resolved through. Inline images (`<vellumImg>`)
/// reuse this module's decode/animation/caching machinery wholesale — only
/// the name→file lookup and the cache bucket differ.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArtSource {
    Emoji,
    InlineImage,
}

impl ArtSource {
    fn lookup(self, name: &str) -> Option<custom_emoji::CustomEmoji> {
        match self {
            ArtSource::Emoji => custom_emoji::get(name),
            ArtSource::InlineImage => crate::core::inline_image::get(name),
        }
    }

    fn cache_id(self) -> egui::Id {
        match self {
            ArtSource::Emoji => egui::Id::new("custom_emoji_frame_cache"),
            ArtSource::InlineImage => egui::Id::new("inline_image_frame_cache"),
        }
    }

    fn cache(self, ctx: &egui::Context) -> FrameCache {
        ctx.data_mut(|data| {
            data.get_temp_mut_or_insert_with::<FrameCache>(self.cache_id(), Default::default)
                .clone()
        })
    }
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

/// Turn an `image` `Frames` iterator (GIF / animated WebP / APNG) into an
/// `EmojiFrames`, uploading each frame and accumulating per-frame delays.
/// Returns None when no frame decodes (caller falls back to a static decode).
fn frames_to_emoji(
    ctx: &egui::Context,
    name: &str,
    kind: &str,
    frames_iter: image::Frames<'_>,
) -> Option<EmojiFrames> {
    let mut frames = Vec::new();
    let mut acc = 0.0f32;
    for (idx, frame) in frames_iter.enumerate() {
        let frame = match frame {
            Ok(f) => f,
            Err(err) => {
                tracing::warn!("Custom emoji :{name}: {kind} frame {idx} decode failed: {err}");
                break;
            }
        };
        // Frames can carry a zero/absent delay; browsers clamp very small
        // delays up. Use a 100ms floor so a broken delay doesn't spin at the
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

/// Decode an animated GIF into all its frames.
fn decode_gif(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<EmojiFrames> {
    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes))
        .map_err(|err| tracing::warn!("Custom emoji :{name}: GIF open failed: {err}"))
        .ok()?;
    frames_to_emoji(ctx, name, "GIF", decoder.into_frames())
}

/// Decode an animated WebP into all its frames (Discord serves animated custom
/// emoji as WebP). Falls back to None for a static WebP so the caller decodes
/// it as a single texture.
fn decode_webp(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<EmojiFrames> {
    use image::AnimationDecoder;
    let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(bytes))
        .map_err(|err| tracing::warn!("Custom emoji :{name}: WebP open failed: {err}"))
        .ok()?;
    if !decoder.has_animation() {
        return None; // static webp: caller uses decode_static
    }
    frames_to_emoji(ctx, name, "WebP", decoder.into_frames())
}

/// Decode an animated PNG (APNG) into all its frames. Falls back to None for a
/// static PNG so the caller decodes it as a single texture.
fn decode_apng(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<EmojiFrames> {
    use image::AnimationDecoder;
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(bytes))
        .map_err(|err| tracing::warn!("Custom emoji :{name}: PNG open failed: {err}"))
        .ok()?;
    if !decoder.is_apng().unwrap_or(false) {
        return None; // static png: caller uses decode_static
    }
    let apng = decoder
        .apng()
        .map_err(|err| tracing::warn!("Custom emoji :{name}: APNG open failed: {err}"))
        .ok()?;
    frames_to_emoji(ctx, name, "APNG", apng.into_frames())
}

/// Load and decode a custom emoji by name, honoring its format. GIF, animated
/// WebP, and APNG all yield their full frame sequences and animate; static
/// PNG/WebP are single textures. An animated decode that finds no animation
/// (static file with an animated-capable extension) falls back to a static
/// texture.
fn decode_emoji(ctx: &egui::Context, name: &str, source: ArtSource) -> Option<EmojiFrames> {
    let meta = source.lookup(name)?;
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
        EmojiFormat::Apng => decode_apng(ctx, name, &bytes)
            .or_else(|| decode_static(ctx, name, &bytes, image::ImageFormat::Png)),
        EmojiFormat::Png => decode_static(ctx, name, &bytes, image::ImageFormat::Png),
        EmojiFormat::Jpeg => decode_static(ctx, name, &bytes, image::ImageFormat::Jpeg),
        // WebP may be animated (Discord's animated emoji) or static.
        EmojiFormat::Webp => decode_webp(ctx, name, &bytes)
            .or_else(|| decode_static(ctx, name, &bytes, image::ImageFormat::WebP)),
    }
}

/// Fetch (decoding + caching on first sight) the frames for `name`, or `None`
/// if the emoji is unknown, its file is gone, or it failed to decode. The
/// `None` result is cached so the caller's text fallback path stays cheap.
fn frames_for(ctx: &egui::Context, cache: &FrameCache, name: &str) -> Option<EmojiFrames> {
    frames_for_source(ctx, cache, name, ArtSource::Emoji)
}

fn frames_for_source(
    ctx: &egui::Context,
    cache: &FrameCache,
    name: &str,
    source: ArtSource,
) -> Option<EmojiFrames> {
    let key = name.to_ascii_lowercase();
    let mut cache = cache.lock().expect("custom emoji frame cache poisoned");
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }
    let decoded = decode_emoji(ctx, &key, source);
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

    // The slot is the real cursor span of the space-run placeholder: row_height
    // tall and >= (row_height * width_factor) wide. The square is row_height *
    // size_factor; the leftover width (the spacing) splits EQUALLY because the
    // square is centered on the slot's own center, so text before AND after the
    // emoji get the same gap.
    let side = slot.height() * size_factor();
    let cx = slot.center().x;
    // Vertically anchor to the text row (bottom of the row) so a taller emoji
    // grows upward, sitting on the baseline like an oversized glyph rather than
    // drifting off-center in a grown row.
    let bottom = slot.bottom();
    let rect = egui::Rect::from_min_max(
        egui::pos2(cx - side * 0.5, bottom - side),
        egui::pos2(cx + side * 0.5, bottom),
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

/// Paint alert art into `rect`, played ONCE from `elapsed` seconds after its
/// spawn and tinted by `tint` (the overlay's fade envelope). Returns `false`
/// when the name doesn't resolve, letting the caller fall back to the alert's
/// banner text rather than showing nothing.
///
/// Art resolves through the inline-image pool, so alert art is authored and
/// installed exactly like every other image asset — no third registry.
pub(super) fn paint_alert_art(
    ctx: &egui::Context,
    painter: &egui::Painter,
    name: &str,
    rect: egui::Rect,
    elapsed: f32,
    tint: egui::Color32,
) -> bool {
    let cache = ArtSource::InlineImage.cache(ctx);
    let Some(frames) = frames_for_source(ctx, &cache, name, ArtSource::InlineImage) else {
        return false;
    };
    let texture = frames.frame_once_at(elapsed);
    painter.image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        tint,
    );

    // Keep a still-playing one-shot advancing; once it settles, stop asking
    // for repaints on its account (the overlay's own fade still drives them).
    if frames.one_shot_running(elapsed) {
        let next_end = frames
            .frames
            .iter()
            .map(|(_, end)| *end)
            .find(|end| *end > elapsed)
            .unwrap_or(frames.total);
        ctx.request_repaint_after(Duration::from_secs_f32((next_end - elapsed).max(0.0)));
    }
    true
}

/// Natural pixel size of an alert art asset's first frame, for aspect-ratio
/// preserving layout. `None` when the name doesn't resolve.
pub(super) fn alert_art_size(ctx: &egui::Context, name: &str) -> Option<egui::Vec2> {
    let cache = ArtSource::InlineImage.cache(ctx);
    let frames = frames_for_source(ctx, &cache, name, ArtSource::InlineImage)?;
    let size = frames.frames.first()?.0.size();
    Some(egui::vec2(size[0] as f32, size[1] as f32))
}

/// Natural pixel size of an inline image's first frame, or `None` when the
/// name doesn't resolve. Callers need this to preserve aspect ratio: the
/// float's height comes from `rows`, and the width follows from this.
pub(super) fn inline_image_size(ctx: &egui::Context, name: &str) -> Option<egui::Vec2> {
    let cache = ArtSource::InlineImage.cache(ctx);
    let frames = frames_for_source(ctx, &cache, name, ArtSource::InlineImage)?;
    let (texture, _) = frames.frames.first()?;
    Some(texture.size_vec2())
}

/// Paint the inline image `name` to exactly fill `rect`, returning `true` if
/// it painted. Unlike [`paint_custom_emoji`] the rect is used verbatim — the
/// caller has already computed an aspect-correct, clamped size — so nothing
/// is squared or re-centered here.
///
/// Animated formats tick the same way custom emoji do.
pub(super) fn paint_inline_image(
    ctx: &egui::Context,
    painter: &egui::Painter,
    name: &str,
    rect: egui::Rect,
) -> bool {
    let cache = ArtSource::InlineImage.cache(ctx);
    let Some(frames) = frames_for_source(ctx, &cache, name, ArtSource::InlineImage) else {
        return false;
    };
    let time = ctx.input(|i| i.time);
    let texture = frames.frame_at(time);
    painter.image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );

    if frames.total > 0.0 && frames.frames.len() > 1 {
        let phase = (time % frames.total as f64) as f32;
        let next_end = frames
            .frames
            .iter()
            .map(|(_, end)| *end)
            .find(|end| *end > phase)
            .unwrap_or(frames.total);
        ctx.request_repaint_after(Duration::from_secs_f32((next_end - phase).max(0.0)));
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
    fn frame_once_at_plays_forward_then_holds_the_last_frame() {
        let ctx = egui::Context::default();
        let frames = decode_gif(&ctx, "flash", &tiny_gif()).expect("gif decodes");
        let f0 = frames.frame_once_at(0.05).id();
        let f1 = frames.frame_once_at(0.20).id();
        assert_ne!(f0, f1, "advances through the animation");

        // The whole point of one-shot: past the cycle it HOLDS the final frame
        // instead of wrapping back to the start like `frame_at` does.
        assert_eq!(frames.frame_once_at(0.30).id(), f1);
        assert_eq!(frames.frame_once_at(60.0).id(), f1);
        assert_eq!(
            frames.frame_at(0.30).id(),
            f0,
            "the looping accessor still wraps — the two must not be confused"
        );
    }

    #[test]
    fn one_shot_running_reports_completion() {
        let ctx = egui::Context::default();
        let frames = decode_gif(&ctx, "flash", &tiny_gif()).expect("gif decodes");
        assert!(frames.one_shot_running(0.0));
        assert!(frames.one_shot_running(0.24));
        // Past total: settled, so callers stop scheduling repaints for it.
        assert!(!frames.one_shot_running(0.26));
    }

    #[test]
    fn static_art_is_a_one_frame_one_shot_that_never_animates() {
        let ctx = egui::Context::default();
        let frames = decode_static(&ctx, "icon", &static_png(), image::ImageFormat::Png)
            .expect("png decodes");
        // A static PNG is simply a one-frame animation: same texture at any
        // elapsed time, and never "running", so it costs no repaints.
        assert_eq!(frames.frame_once_at(0.0).id(), frames.frame_once_at(99.0).id());
        assert!(!frames.one_shot_running(0.0));
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

    fn static_webp() -> Vec<u8> {
        use image::codecs::webp::WebPEncoder;
        use image::ImageEncoder;
        let mut buf = Vec::new();
        WebPEncoder::new_lossless(&mut buf)
            .write_image(&[0, 255, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        buf
    }

    fn static_png() -> Vec<u8> {
        use image::ImageEncoder;
        let mut buf = Vec::new();
        image::codecs::png::PngEncoder::new(&mut buf)
            .write_image(&[0, 0, 255, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        buf
    }

    #[test]
    fn static_webp_is_not_treated_as_animated() {
        let ctx = egui::Context::default();
        let bytes = static_webp();
        // The animated path detects no animation and yields None...
        assert!(
            decode_webp(&ctx, "logo", &bytes).is_none(),
            "a static webp must not be decoded as animated"
        );
        // ...but the static fallback decodes it to a single texture.
        let frames = decode_static(&ctx, "logo", &bytes, image::ImageFormat::WebP)
            .expect("static webp decodes");
        assert_eq!(frames.frames.len(), 1);
    }

    #[test]
    fn static_png_is_not_treated_as_apng() {
        let ctx = egui::Context::default();
        let bytes = static_png();
        assert!(
            decode_apng(&ctx, "logo", &bytes).is_none(),
            "a static png must not be decoded as APNG"
        );
    }

    #[test]
    fn animated_gif_frames_animate_through_the_shared_helper() {
        // GIF exercises the same frames_to_emoji helper that WebP/APNG use, so
        // this confirms multi-frame content produces a cycling animation.
        let ctx = egui::Context::default();
        let frames = decode_gif(&ctx, "dance", &tiny_gif()).expect("gif decodes");
        assert!(frames.frames.len() > 1, "multi-frame => animates");
        assert!(frames.total > 0.0);
        // Different frames at different phases of the cycle.
        assert_ne!(
            frames.frame_at(0.0).id(),
            frames.frame_at(frames.total as f64 - 0.01).id()
        );
    }
}
