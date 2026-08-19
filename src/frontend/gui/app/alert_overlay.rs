//! Alert overlay renderer: paints the core's active alerts over the game UI
//! as banners, one-shot art, and edge flashes.
//!
//! This module DRAWS ONLY. It never decides what fires, how long it lives, or
//! whether it was rate-limited — that is `core::alerts`, deliberately, so a
//! detached viewport rendering the same state cannot double-fire anything.
//! Everything here is a pure function of `AlertState` plus the clock.
//!
//! Two invariants worth keeping:
//! - **Strictly click-through.** Alerts appear over the story window during
//!   combat. An overlay that eats a click is worse than no overlay at all, so
//!   every layer here is painted, never `interact`ed.
//! - **Never fully opaque over text.** The user's opacity ceiling is applied
//!   to every layer, and flashes get their own intensity scale on top.

use super::custom_emoji_render;
use crate::config::AlertAnchor;
use crate::core::alerts::ActiveAlert;
use std::time::Instant;

/// Gap between stacked alerts sharing one anchor.
const STACK_GAP: f32 = 6.0;
/// Padding inside a banner's background pill.
const BANNER_PAD: egui::Vec2 = egui::vec2(14.0, 8.0);
/// Fraction of the viewport's smaller side an art overlay may occupy.
const ART_MAX_FRACTION: f32 = 0.35;
/// Thickness of the edge-flash band, as a fraction of the smaller side.
const FLASH_BAND_FRACTION: f32 = 0.12;

/// Knobs the user controls, resolved once per frame from config.
#[derive(Clone, Copy)]
pub(super) struct AlertRenderSettings {
    /// Hard ceiling on any alert layer's alpha.
    pub max_opacity: f32,
    /// Scales flash brightness; 0 disables flashes entirely.
    pub flash_intensity: f32,
    /// Degrade art to banners and suppress flashes.
    pub reduce_motion: bool,
}

/// Resolve where an alert's box sits, given its anchor, the viewport, the box
/// size, and how far down the stack it is.
///
/// The result is clamped into the viewport so an offset (or an oversized art
/// asset) can never push an alert off-screen — an alert you cannot see is a
/// silent failure, and silent failures in a warning system are the worst kind.
fn anchored_rect(
    anchor: AlertAnchor,
    viewport: egui::Rect,
    size: egui::Vec2,
    offset: (f32, f32),
    stack_depth: f32,
) -> egui::Rect {
    let (ax, ay) = match anchor {
        AlertAnchor::TopLeft => (0.0, 0.0),
        AlertAnchor::TopCenter => (0.5, 0.0),
        AlertAnchor::TopRight => (1.0, 0.0),
        AlertAnchor::CenterLeft => (0.0, 0.5),
        AlertAnchor::Center => (0.5, 0.5),
        AlertAnchor::CenterRight => (1.0, 0.5),
        AlertAnchor::BottomLeft => (0.0, 1.0),
        AlertAnchor::BottomCenter => (0.5, 1.0),
        AlertAnchor::BottomRight => (1.0, 1.0),
    };

    // Anchor point on the viewport, then pull the box back by the same
    // fraction of its own size so `ax = 1.0` right-aligns rather than
    // hanging the box off the right edge.
    let x = viewport.left() + viewport.width() * ax - size.x * ax + offset.0;
    let y = viewport.top() + viewport.height() * ay - size.y * ay + offset.1;

    // Stack downward from top/center anchors, upward from bottom ones, so a
    // stack never grows into the edge it is pinned to.
    let dir = if ay > 0.75 { -1.0 } else { 1.0 };
    let y = y + dir * stack_depth;

    let rect = egui::Rect::from_min_size(egui::pos2(x, y), size);
    clamp_into(rect, viewport)
}

/// Nudge `rect` fully inside `bounds` where it fits; oversized boxes keep
/// their top-left pinned so their most important content stays visible.
fn clamp_into(rect: egui::Rect, bounds: egui::Rect) -> egui::Rect {
    let mut min = rect.min;
    if rect.max.x > bounds.max.x {
        min.x -= rect.max.x - bounds.max.x;
    }
    if rect.max.y > bounds.max.y {
        min.y -= rect.max.y - bounds.max.y;
    }
    min.x = min.x.max(bounds.min.x);
    min.y = min.y.max(bounds.min.y);
    egui::Rect::from_min_size(min, rect.size())
}

/// Scale a color's alpha by `factor`, clamped to `[0, 1]`.
fn with_alpha(color: egui::Color32, factor: f32) -> egui::Color32 {
    let a = (color.a() as f32 * factor.clamp(0.0, 1.0)).round() as u8;
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

impl super::VellumGuiApp {
    /// Paint every active alert. Called late in the frame so alerts sit above
    /// windows, but before menus and editors — a menu the user opened must
    /// stay on top of ambiance art.
    pub(super) fn render_alert_overlay(&mut self, ctx: &egui::Context) {
        if self.app_core.alerts.is_empty() {
            return;
        }

        let settings = AlertRenderSettings {
            max_opacity: self
                .app_core
                .config
                .highlight_settings
                .alerts_max_opacity
                .clamp(0.05, 1.0),
            flash_intensity: self
                .app_core
                .config
                .highlight_settings
                .alerts_flash_intensity
                .clamp(0.0, 1.0),
            reduce_motion: self.app_core.config.highlight_settings.alerts_reduce_motion,
        };

        // `content_rect`, not the raw window rect: alerts anchor to the area
        // the game UI actually occupies (this fork's accessor, matching how
        // zone overlays resolve their bounds).
        let viewport = ctx.content_rect();
        let now = Instant::now();

        // One foreground Area for all alerts. `interactable(false)` is the
        // click-through guarantee: the layer is painted but never hit-tested,
        // so clicks land on the game UI underneath as if it weren't there.
        egui::Area::new(egui::Id::new("vellum_alert_overlay"))
            .order(egui::Order::Foreground)
            .interactable(false)
            .fixed_pos(viewport.min)
            .show(ctx, |ui| {
                let painter = ui.painter();

                // Flashes paint first: they are a backdrop for the banners and
                // art that carry the actual information.
                if !settings.reduce_motion && settings.flash_intensity > 0.0 {
                    for alert in self.app_core.alerts.active() {
                        Self::paint_flash(painter, alert, viewport, now, &settings);
                    }
                }

                // Track how far each anchor's stack has grown so co-anchored
                // alerts tile instead of overprinting.
                let mut stack: std::collections::HashMap<u8, f32> =
                    std::collections::HashMap::new();

                for alert in self.app_core.alerts.active() {
                    let depth = stack.entry(alert.anchor as u8).or_insert(0.0);
                    let used = Self::paint_alert_body(
                        ctx, painter, alert, viewport, now, &settings, *depth,
                    );
                    if used > 0.0 {
                        *depth += used + STACK_GAP;
                    }
                }
            });

        // Alerts are time-based; keep frames coming while any is on screen so
        // fades animate and expiry lands promptly even when the game is idle.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    /// Paint one alert's art and/or banner. Returns the vertical space it
    /// consumed, so the caller can stack the next one below it.
    fn paint_alert_body(
        ctx: &egui::Context,
        painter: &egui::Painter,
        alert: &ActiveAlert,
        viewport: egui::Rect,
        now: Instant,
        settings: &AlertRenderSettings,
        stack_depth: f32,
    ) -> f32 {
        let alpha = alert.alpha(now) * settings.max_opacity;
        if alpha <= 0.0 {
            return 0.0;
        }

        // Reduce-motion turns art into its banner text; if the alert had no
        // banner, its art name stands in so the user still learns something
        // fired rather than getting silence.
        let art = (!settings.reduce_motion)
            .then_some(alert.art.as_deref())
            .flatten();

        if let Some(art_name) = art {
            if let Some(natural) = custom_emoji_render::alert_art_size(ctx, art_name) {
                let size = Self::fit_art(natural, viewport);
                let rect = anchored_rect(alert.anchor, viewport, size, alert.offset, stack_depth);
                let tint = with_alpha(egui::Color32::WHITE, alpha);
                if custom_emoji_render::paint_alert_art(
                    ctx,
                    painter,
                    art_name,
                    rect,
                    alert.elapsed_secs(now),
                    tint,
                ) {
                    return rect.height();
                }
            }
            // Art named but unresolvable: fall through to the banner so a
            // missing asset degrades to text instead of vanishing.
        }

        // Banner text, or — when reduce-motion suppressed the art — the art's
        // name, so a motion-sensitive user still sees that something fired
        // instead of getting nothing at all.
        let text = match alert.banner.as_deref() {
            Some(banner) => banner,
            None if settings.reduce_motion => alert.art.as_deref().unwrap_or_default(),
            None => return 0.0,
        };
        Self::paint_banner(painter, alert, text, viewport, alpha, stack_depth)
    }

    /// Largest art size that respects the viewport fraction cap and the art's
    /// own aspect ratio. Art never grows past its natural size — upscaling a
    /// small icon to a third of the screen looks like a bug, not a flourish.
    fn fit_art(natural: egui::Vec2, viewport: egui::Rect) -> egui::Vec2 {
        let limit = viewport.size().min_elem() * ART_MAX_FRACTION;
        let scale = (limit / natural.x.max(1.0))
            .min(limit / natural.y.max(1.0))
            .min(1.0);
        natural * scale
    }

    /// Paint a banner pill with its text. Returns the height consumed.
    fn paint_banner(
        painter: &egui::Painter,
        alert: &ActiveAlert,
        text: &str,
        viewport: egui::Rect,
        alpha: f32,
        stack_depth: f32,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let font = egui::FontId::proportional(20.0);
        let fg = alert
            .banner_fg
            .as_deref()
            .and_then(crate::frontend::gui::skin::parse_hex_rgb)
            .unwrap_or(egui::Color32::from_rgb(255, 240, 200));
        let bg = alert
            .banner_bg
            .as_deref()
            .and_then(crate::frontend::gui::skin::parse_hex_rgb)
            .unwrap_or(egui::Color32::from_rgb(20, 20, 28));

        let galley = painter.layout_no_wrap(text.to_string(), font, with_alpha(fg, alpha));
        let size = galley.size() + BANNER_PAD * 2.0;
        let rect = anchored_rect(alert.anchor, viewport, size, alert.offset, stack_depth);

        painter.rect_filled(rect, 6.0, with_alpha(bg, alpha * 0.85));
        painter.galley(rect.min + BANNER_PAD, galley, with_alpha(fg, alpha));
        rect.height()
    }

    /// Paint the edge-flash band. Kept as a soft vignette rather than a
    /// full-screen fill: it must read peripherally without washing out the
    /// text it surrounds.
    fn paint_flash(
        painter: &egui::Painter,
        alert: &ActiveAlert,
        viewport: egui::Rect,
        now: Instant,
        settings: &AlertRenderSettings,
    ) {
        let Some(color) = alert
            .flash
            .as_deref()
            .and_then(crate::frontend::gui::skin::parse_hex_rgb)
        else {
            return;
        };
        let alpha = alert.alpha(now) * settings.max_opacity * settings.flash_intensity;
        if alpha <= 0.0 {
            return;
        }

        let band = viewport.size().min_elem() * FLASH_BAND_FRACTION;
        let tint = with_alpha(color, alpha);
        // Four edge bands. Corners double up slightly, which reads as a
        // natural vignette rather than a seam.
        let edges = [
            egui::Rect::from_min_max(
                viewport.min,
                egui::pos2(viewport.max.x, viewport.min.y + band),
            ),
            egui::Rect::from_min_max(
                egui::pos2(viewport.min.x, viewport.max.y - band),
                viewport.max,
            ),
            egui::Rect::from_min_max(
                viewport.min,
                egui::pos2(viewport.min.x + band, viewport.max.y),
            ),
            egui::Rect::from_min_max(
                egui::pos2(viewport.max.x - band, viewport.min.y),
                viewport.max,
            ),
        ];
        for edge in edges {
            painter.rect_filled(edge, 0.0, tint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 800.0))
    }

    #[test]
    fn center_anchor_centers_the_box() {
        let size = egui::vec2(100.0, 50.0);
        let rect = anchored_rect(AlertAnchor::Center, viewport(), size, (0.0, 0.0), 0.0);
        assert_eq!(rect.center(), egui::pos2(500.0, 400.0));
    }

    #[test]
    fn corner_anchors_hug_their_corners() {
        let size = egui::vec2(100.0, 50.0);
        let vp = viewport();

        let tl = anchored_rect(AlertAnchor::TopLeft, vp, size, (0.0, 0.0), 0.0);
        assert_eq!(tl.min, egui::pos2(0.0, 0.0));

        // Right/bottom anchors pull the box back by its own size rather than
        // hanging it off the edge.
        let br = anchored_rect(AlertAnchor::BottomRight, vp, size, (0.0, 0.0), 0.0);
        assert_eq!(br.max, egui::pos2(1000.0, 800.0));
    }

    #[test]
    fn offset_shifts_but_clamp_keeps_it_on_screen() {
        let size = egui::vec2(100.0, 50.0);
        let vp = viewport();

        let nudged = anchored_rect(AlertAnchor::TopLeft, vp, size, (20.0, 10.0), 0.0);
        assert_eq!(nudged.min, egui::pos2(20.0, 10.0));

        // A wild offset must not push the alert out of sight.
        let shoved = anchored_rect(AlertAnchor::TopLeft, vp, size, (99999.0, 99999.0), 0.0);
        assert!(vp.contains_rect(shoved), "clamped back into the viewport");
    }

    #[test]
    fn stacking_grows_down_from_top_and_up_from_bottom() {
        let size = egui::vec2(100.0, 50.0);
        let vp = viewport();

        let top_first = anchored_rect(AlertAnchor::TopCenter, vp, size, (0.0, 0.0), 0.0);
        let top_second = anchored_rect(AlertAnchor::TopCenter, vp, size, (0.0, 0.0), 60.0);
        assert!(
            top_second.min.y > top_first.min.y,
            "top stack grows downward"
        );

        let bot_first = anchored_rect(AlertAnchor::BottomCenter, vp, size, (0.0, 0.0), 0.0);
        let bot_second = anchored_rect(AlertAnchor::BottomCenter, vp, size, (0.0, 0.0), 60.0);
        assert!(
            bot_second.min.y < bot_first.min.y,
            "bottom stack grows upward"
        );
    }

    #[test]
    fn oversized_box_pins_top_left_rather_than_centering_offscreen() {
        let vp = viewport();
        let huge = egui::vec2(2000.0, 1600.0);
        let rect = anchored_rect(AlertAnchor::Center, vp, huge, (0.0, 0.0), 0.0);
        // Can't fit; the top-left stays visible so the content's start reads.
        assert_eq!(rect.min, vp.min);
    }

    #[test]
    fn art_scales_down_to_the_cap_but_never_up() {
        let vp = viewport();
        let cap = vp.size().min_elem() * ART_MAX_FRACTION;

        let big = super::super::VellumGuiApp::fit_art(egui::vec2(2000.0, 2000.0), vp);
        assert!((big.x - cap).abs() < 0.5, "large art clamps to the cap");
        assert!((big.x - big.y).abs() < 0.5, "aspect ratio preserved");

        let small = egui::vec2(32.0, 32.0);
        let fitted = super::super::VellumGuiApp::fit_art(small, vp);
        assert_eq!(fitted, small, "small art is never upscaled");
    }

    #[test]
    fn art_aspect_ratio_survives_scaling() {
        let vp = viewport();
        let wide = super::super::VellumGuiApp::fit_art(egui::vec2(1600.0, 400.0), vp);
        assert!(
            (wide.x / wide.y - 4.0).abs() < 0.01,
            "4:1 stays 4:1, got {:?}",
            wide
        );
    }

    #[test]
    fn alpha_scaling_clamps_at_both_ends() {
        let c = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 200);
        assert_eq!(with_alpha(c, 0.0).a(), 0);
        assert_eq!(with_alpha(c, 1.0).a(), 200);
        assert_eq!(
            with_alpha(c, 5.0).a(),
            200,
            "never exceeds the source alpha"
        );
    }
}
