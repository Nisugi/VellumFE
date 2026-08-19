//! TUI alert overlay: the constrained form of the GUI's overlay layer.
//!
//! A terminal cannot fade art in over the story window, so alerts degrade to
//! what a text grid does well: a banner line at the alert's anchor, and a
//! one-cell tint around the screen edge for flashes. The information is the
//! same; only the presentation is smaller.
//!
//! Like the GUI renderer, this module DRAWS ONLY. `core::alerts` decides what
//! fires and when it expires — and already ticks in the TUI today via
//! `poll_map`, so this is purely a matter of reading state nobody was reading.
//!
//! Art has no terminal equivalent, so an art-only alert falls back to its art
//! NAME rather than rendering nothing: the user still learns that something
//! fired, which is the whole job of an alert.

use crate::config::AlertAnchor;
use crate::core::alerts::ActiveAlert;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Clear, Widget};
use std::time::Instant;

/// Rows of vertical padding kept clear of the screen edge so a banner never
/// collides with a window border sitting flush against it.
const EDGE_PAD: u16 = 1;

/// Resolve the banner row for an alert, given the anchor, the screen, the
/// banner width, and how many banners already occupy this anchor.
///
/// Mirrors the GUI's 9-grid semantics — same anchor means the same corner in
/// both frontends — and clamps into the screen so a stack or a wide banner can
/// never place text off-grid where it would simply vanish.
fn anchored_rect(anchor: AlertAnchor, screen: Rect, width: u16, stack_depth: u16) -> Rect {
    let width = width.min(screen.width);
    let x = match anchor {
        AlertAnchor::TopLeft | AlertAnchor::CenterLeft | AlertAnchor::BottomLeft => {
            screen.x + EDGE_PAD.min(screen.width.saturating_sub(width))
        }
        AlertAnchor::TopCenter | AlertAnchor::Center | AlertAnchor::BottomCenter => {
            screen.x + (screen.width.saturating_sub(width)) / 2
        }
        AlertAnchor::TopRight | AlertAnchor::CenterRight | AlertAnchor::BottomRight => {
            screen.x + screen.width.saturating_sub(width).saturating_sub(EDGE_PAD)
        }
    };

    // Top anchors stack downward, bottom anchors upward, so a stack never
    // grows into the edge it is pinned to (same rule as the GUI).
    let y = match anchor {
        AlertAnchor::TopLeft | AlertAnchor::TopCenter | AlertAnchor::TopRight => screen
            .y
            .saturating_add(EDGE_PAD)
            .saturating_add(stack_depth),
        AlertAnchor::CenterLeft | AlertAnchor::Center | AlertAnchor::CenterRight => {
            screen.y + screen.height / 2 + stack_depth
        }
        AlertAnchor::BottomLeft | AlertAnchor::BottomCenter | AlertAnchor::BottomRight => screen
            .y
            .saturating_add(screen.height)
            .saturating_sub(EDGE_PAD + 1)
            .saturating_sub(stack_depth),
    };

    // Clamp so a deep stack cannot walk off the top or bottom.
    let max_y = screen.y + screen.height.saturating_sub(1);
    let y = y.clamp(screen.y, max_y);

    Rect {
        x,
        y,
        width,
        height: 1,
    }
}

/// The text a banner shows. Art has no terminal form, so an art-only alert
/// announces its art name instead of drawing nothing at all.
fn banner_text(alert: &ActiveAlert) -> Option<String> {
    if let Some(banner) = alert.banner.as_deref() {
        if !banner.trim().is_empty() {
            return Some(banner.to_string());
        }
    }
    alert
        .art
        .as_deref()
        .map(|art| format!("[{art}]"))
        .filter(|s| s.len() > 2)
}

/// Paint every active alert over `screen`.
///
/// `reduce_motion` suppresses the edge flash (the banner already carries the
/// information); `flash_intensity` of 0 does the same, so the accessibility
/// controls behave identically in both frontends.
pub fn render(
    alerts: &[ActiveAlert],
    screen: Rect,
    buf: &mut Buffer,
    theme: &crate::theme::AppTheme,
    reduce_motion: bool,
    flash_intensity: f32,
) {
    if alerts.is_empty() || screen.width == 0 || screen.height == 0 {
        return;
    }
    // No fade envelope here: a terminal cell is on or off, so alerts simply
    // appear and disappear. Core still owns expiry, so they last exactly as
    // long as they do in the GUI.

    // Flash first: it is a backdrop for the banners that carry the detail.
    if !reduce_motion && flash_intensity > 0.0 {
        for alert in alerts {
            if let Some(color) = alert.flash.as_deref() {
                render_flash(screen, buf, color);
                // One flash is a full-screen effect; painting several would
                // just overdraw the same cells in the last one's color.
                break;
            }
        }
    }

    // Track per-anchor stacking so co-anchored alerts tile rather than
    // overprinting each other into an unreadable mess.
    let mut depth: std::collections::HashMap<u8, u16> = std::collections::HashMap::new();

    for alert in alerts {
        let Some(text) = banner_text(alert) else {
            continue;
        };
        let stack = depth.entry(alert.anchor as u8).or_insert(0);
        let width = (text.chars().count() as u16).saturating_add(2);
        let rect = anchored_rect(alert.anchor, screen, width, *stack);

        let fg = alert
            .banner_fg
            .as_deref()
            .and_then(super::colors::parse_color_to_ratatui)
            .unwrap_or_else(|| super::crossterm_bridge::to_ratatui_color(theme.form_label_focused));
        let bg = alert
            .banner_bg
            .as_deref()
            .and_then(super::colors::parse_color_to_ratatui)
            .unwrap_or_else(|| super::crossterm_bridge::to_ratatui_color(theme.browser_background));

        Clear.render(rect, buf);
        let style = Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD);
        // Pad by one cell each side so the banner reads as a label rather
        // than text jammed against whatever it is covering.
        buf.set_stringn(
            rect.x,
            rect.y,
            format!(" {text} "),
            rect.width as usize,
            style,
        );

        *stack = stack.saturating_add(1);
    }
}

/// Tint the screen's outermost cells. The terminal analogue of the GUI's
/// edge band: peripheral enough to notice, cheap enough to not obscure text.
fn render_flash(screen: Rect, buf: &mut Buffer, color: &str) {
    let Some(color) = super::colors::parse_color_to_ratatui(color) else {
        return;
    };
    let style = Style::default().bg(color);
    let right = screen.x + screen.width.saturating_sub(1);
    let bottom = screen.y + screen.height.saturating_sub(1);

    for x in screen.x..screen.x + screen.width {
        buf.set_style(
            Rect {
                x,
                y: screen.y,
                width: 1,
                height: 1,
            },
            style,
        );
        buf.set_style(
            Rect {
                x,
                y: bottom,
                width: 1,
                height: 1,
            },
            style,
        );
    }
    for y in screen.y..screen.y + screen.height {
        buf.set_style(
            Rect {
                x: screen.x,
                y,
                width: 1,
                height: 1,
            },
            style,
        );
        buf.set_style(
            Rect {
                x: right,
                y,
                width: 1,
                height: 1,
            },
            style,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        }
    }

    #[test]
    fn top_and_bottom_anchors_hug_their_edges() {
        let s = screen();
        let top = anchored_rect(AlertAnchor::TopCenter, s, 10, 0);
        assert_eq!(top.y, EDGE_PAD);
        let bottom = anchored_rect(AlertAnchor::BottomCenter, s, 10, 0);
        assert_eq!(bottom.y, s.height - EDGE_PAD - 1);
    }

    #[test]
    fn left_and_right_anchors_hug_their_sides() {
        let s = screen();
        let left = anchored_rect(AlertAnchor::TopLeft, s, 10, 0);
        assert_eq!(left.x, EDGE_PAD);
        let right = anchored_rect(AlertAnchor::TopRight, s, 10, 0);
        assert_eq!(right.x + right.width, s.width - EDGE_PAD);
    }

    #[test]
    fn center_anchor_centers_the_banner() {
        let rect = anchored_rect(AlertAnchor::Center, screen(), 10, 0);
        assert_eq!(rect.x, 35, "80 wide, 10 banner -> 35");
    }

    #[test]
    fn stacks_grow_down_from_top_and_up_from_bottom() {
        let s = screen();
        let t0 = anchored_rect(AlertAnchor::TopCenter, s, 10, 0);
        let t1 = anchored_rect(AlertAnchor::TopCenter, s, 10, 1);
        assert!(t1.y > t0.y, "top stack grows downward");

        let b0 = anchored_rect(AlertAnchor::BottomCenter, s, 10, 0);
        let b1 = anchored_rect(AlertAnchor::BottomCenter, s, 10, 1);
        assert!(b1.y < b0.y, "bottom stack grows upward");
    }

    #[test]
    fn a_banner_wider_than_the_screen_is_clamped_not_dropped() {
        let s = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 10,
        };
        let rect = anchored_rect(AlertAnchor::TopCenter, s, 500, 0);
        assert_eq!(rect.width, 20, "clamped to the screen");
        assert!(rect.x + rect.width <= s.x + s.width, "stays on screen");
    }

    #[test]
    fn a_deep_stack_cannot_walk_off_screen() {
        let s = screen();
        // Far more alerts than could ever be concurrent, to prove the clamp.
        let rect = anchored_rect(AlertAnchor::TopCenter, s, 10, 999);
        assert!(rect.y < s.y + s.height, "clamped inside the screen");
        let rect = anchored_rect(AlertAnchor::BottomCenter, s, 10, 999);
        assert!(rect.y >= s.y, "saturating_sub keeps it on screen");
    }

    #[test]
    fn a_tiny_screen_does_not_panic_or_overflow() {
        let s = Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        for anchor in [
            AlertAnchor::TopLeft,
            AlertAnchor::Center,
            AlertAnchor::BottomRight,
        ] {
            let rect = anchored_rect(anchor, s, 10, 3);
            assert!(rect.x <= s.width, "no overflow on a 1x1 terminal");
            assert!(rect.y <= s.height);
        }
    }

    #[test]
    fn art_only_alerts_announce_their_art_name() {
        // A terminal cannot draw the art, but silence would be worse: the
        // user must still learn that something fired.
        let mut alert = sample_alert();
        alert.banner = None;
        alert.art = Some("lightning".to_string());
        assert_eq!(banner_text(&alert).as_deref(), Some("[lightning]"));
    }

    #[test]
    fn banner_text_wins_over_art_when_both_are_present() {
        let mut alert = sample_alert();
        alert.banner = Some("STUNNED".to_string());
        alert.art = Some("lightning".to_string());
        assert_eq!(banner_text(&alert).as_deref(), Some("STUNNED"));
    }

    #[test]
    fn a_flash_only_alert_has_no_banner_to_draw() {
        let mut alert = sample_alert();
        alert.banner = None;
        alert.art = None;
        alert.flash = Some("#ff0000".to_string());
        assert!(banner_text(&alert).is_none(), "the flash is its whole body");
    }

    #[test]
    fn a_blank_banner_falls_through_to_art() {
        let mut alert = sample_alert();
        alert.banner = Some("   ".to_string());
        alert.art = Some("boom".to_string());
        assert_eq!(banner_text(&alert).as_deref(), Some("[boom]"));
    }

    // ---- Buffer-level rendering (the geometry tests above prove
    // placement; these prove we can actually paint without panicking) ----

    fn theme() -> crate::theme::AppTheme {
        crate::theme::AppTheme::default()
    }

    #[test]
    fn rendering_writes_the_banner_into_the_buffer() {
        let s = screen();
        let mut buf = Buffer::empty(s);
        let alert = sample_alert();
        render(&[alert], s, &mut buf, &theme(), false, 0.0);

        // The banner text must actually be in the cells.
        let row: String = (0..s.width)
            .map(|x| buf[(x, EDGE_PAD)].symbol().to_string())
            .collect();
        assert!(row.contains("BANNER"), "banner painted, got {row:?}");
    }

    #[test]
    fn rendering_into_a_one_by_one_terminal_does_not_panic() {
        // A terminal can legitimately be this small mid-resize; painting must
        // clip rather than index out of bounds.
        let s = Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let mut buf = Buffer::empty(s);
        let mut alert = sample_alert();
        alert.flash = Some("#ff0000".to_string());
        render(&[alert], s, &mut buf, &theme(), false, 1.0);
    }

    #[test]
    fn a_banner_far_wider_than_the_screen_is_clipped_when_painted() {
        let s = Rect {
            x: 0,
            y: 0,
            width: 12,
            height: 5,
        };
        let mut buf = Buffer::empty(s);
        let mut alert = sample_alert();
        alert.banner = Some("A".repeat(500));
        render(&[alert], s, &mut buf, &theme(), false, 0.0);
    }

    #[test]
    fn reduce_motion_and_zero_intensity_both_suppress_the_flash() {
        let s = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 4,
        };
        let mut alert = sample_alert();
        alert.banner = None;
        alert.art = None;
        alert.flash = Some("#ff0000".to_string());

        // A corner cell carries the flash COLOR when it paints. Compare
        // against the parsed color rather than `bg.is_some()`: an empty
        // buffer's cells already carry a Reset background, so `is_some()`
        // would be true whether or not we painted anything.
        let red =
            super::super::colors::parse_color_to_ratatui("#ff0000").expect("test color parses");
        let flashed = |reduce: bool, intensity: f32| {
            let mut buf = Buffer::empty(s);
            render(
                std::slice::from_ref(&alert),
                s,
                &mut buf,
                &theme(),
                reduce,
                intensity,
            );
            buf[(0, 0)].style().bg == Some(red)
        };

        assert!(flashed(false, 1.0), "flash paints normally");
        assert!(!flashed(true, 1.0), "reduce-motion suppresses it");
        assert!(!flashed(false, 0.0), "zero intensity suppresses it");
    }

    #[test]
    fn co_anchored_alerts_land_on_different_rows() {
        let s = screen();
        let mut buf = Buffer::empty(s);
        let mut first = sample_alert();
        first.banner = Some("FIRST".to_string());
        let mut second = sample_alert();
        second.banner = Some("SECOND".to_string());
        render(&[first, second], s, &mut buf, &theme(), false, 0.0);

        let row_at = |y: u16| -> String {
            (0..s.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect()
        };
        assert!(row_at(EDGE_PAD).contains("FIRST"));
        assert!(
            row_at(EDGE_PAD + 1).contains("SECOND"),
            "the second alert stacks below rather than overprinting"
        );
    }

    fn sample_alert() -> ActiveAlert {
        ActiveAlert {
            key: "k".to_string(),
            banner: Some("BANNER".to_string()),
            banner_fg: None,
            banner_bg: None,
            art: None,
            flash: None,
            anchor: AlertAnchor::TopCenter,
            offset: (0.0, 0.0),
            spawned: Instant::now(),
            duration: std::time::Duration::from_secs(4),
            priority: 0,
        }
    }
}
