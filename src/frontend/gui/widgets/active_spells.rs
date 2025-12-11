//! Active Spells/Effects Widget - Display buffs, debuffs, cooldowns
//!
//! Renders ActiveEffectsContent as a list of effects with customizable progress bars.

use crate::config::{ActiveEffectsStyle, ActiveEffectsWidgetData, TimerPosition};
use crate::data::widget::ActiveEffectsContent;
use eframe::egui::{self, Color32, ProgressBar, RichText, ScrollArea, Ui};

use super::text_window::parse_hex_to_color32;

/// Render an active effects widget (buffs, debuffs, cooldowns, active spells)
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `content` - The ActiveEffectsContent data to render
/// * `config` - The ActiveEffectsWidgetData configuration
/// * `window_name` - Window name (for identification)
pub fn render_active_effects(
    ui: &mut Ui,
    content: &ActiveEffectsContent,
    config: &ActiveEffectsWidgetData,
    _window_name: &str,
) {
    if content.effects.is_empty() {
        ui.weak(format!("No active {}", content.category.to_lowercase()));
        return;
    }

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for effect in &content.effects {
                match config.style {
                    ActiveEffectsStyle::Overlay => {
                        render_overlay_style(ui, effect, config);
                    }
                    ActiveEffectsStyle::Separate => {
                        render_separate_style(ui, effect, config);
                    }
                    ActiveEffectsStyle::ThinBar => {
                        render_thin_bar_style(ui, effect, config);
                    }
                    ActiveEffectsStyle::SideIndicator => {
                        render_side_indicator_style(ui, effect, config);
                    }
                    ActiveEffectsStyle::Minimal => {
                        render_minimal_style(ui, effect, config);
                    }
                }
                ui.add_space(config.spacing);
            }
        });
}

/// Render effect with text overlaid on progress bar (most compact)
fn render_overlay_style(
    ui: &mut Ui,
    effect: &crate::data::widget::ActiveEffect,
    config: &ActiveEffectsWidgetData,
) {
    let bar_height = config.bar_height;
    let available_width = ui.available_width();

    // Allocate space
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, bar_height),
        egui::Sense::hover(),
    );

    // Determine colors
    let bar_color = effect
        .bar_color
        .as_ref()
        .and_then(|c| parse_hex_to_color32(c))
        .unwrap_or(Color32::from_rgb(100, 150, 200));

    let bar_color_with_alpha = Color32::from_rgba_premultiplied(
        bar_color.r(),
        bar_color.g(),
        bar_color.b(),
        (255.0 * config.bar_opacity) as u8,
    );

    // Draw background (unfilled portion)
    ui.painter().rect_filled(
        rect,
        config.bar_rounding,
        Color32::from_gray(30), // Dark background
    );

    // Draw progress bar
    if effect.value > 0 {
        let fraction = (effect.value as f32 / 100.0).clamp(0.0, 1.0);
        let filled_rect = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(rect.width() * fraction, rect.height()),
        );
        ui.painter().rect_filled(
            filled_rect,
            config.bar_rounding,
            bar_color_with_alpha,
        );

        // Pulse effect if expiring (TODO: Fix after egui API update)
        // if config.pulse_expiring && is_expiring(effect, config.expiring_threshold) {
        //     // Add pulsing border - disabled until egui StrokeKind API is clarified
        // }
    }

    // Auto-contrast text color
    let text_color = if config.auto_contrast {
        determine_contrast_color(&bar_color, effect.value)
    } else {
        effect
            .text_color
            .as_ref()
            .and_then(|c| parse_hex_to_color32(c))
            .unwrap_or(Color32::WHITE)
    };

    // Text shadow for readability
    if config.text_shadow {
        ui.painter().text(
            egui::pos2(rect.min.x + 5.0, rect.center().y + 1.0),
            egui::Align2::LEFT_CENTER,
            &effect.text,
            egui::FontId::proportional(config.text_size),
            Color32::from_black_alpha(180), // Shadow
        );
    }

    // Main text
    ui.painter().text(
        egui::pos2(rect.min.x + 4.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &effect.text,
        egui::FontId::proportional(config.text_size),
        text_color,
    );

    // Timer
    if config.show_timer && !effect.time.is_empty() {
        let timer_text = if config.show_percentage {
            format!("{} ({}%)", effect.time, effect.value)
        } else {
            effect.time.clone()
        };

        let timer_pos = match config.timer_position {
            TimerPosition::Right => egui::pos2(rect.max.x - 4.0, rect.center().y),
            TimerPosition::Left => egui::pos2(rect.min.x + 80.0, rect.center().y),
            TimerPosition::Inline => egui::pos2(rect.min.x + 120.0, rect.center().y),
        };

        if config.text_shadow {
            ui.painter().text(
                egui::pos2(timer_pos.x + 1.0, timer_pos.y + 1.0),
                egui::Align2::RIGHT_CENTER,
                &timer_text,
                egui::FontId::proportional(config.text_size * 0.9),
                Color32::from_black_alpha(180),
            );
        }

        ui.painter().text(
            timer_pos,
            egui::Align2::RIGHT_CENTER,
            &timer_text,
            egui::FontId::proportional(config.text_size * 0.9),
            Color32::LIGHT_GRAY,
        );
    }

    // Tooltip on hover
    response.on_hover_ui(|ui| {
        ui.label(format!("{}: {}%", effect.text, effect.value));
        if !effect.time.is_empty() {
            ui.label(format!("Time: {}", effect.time));
        }
    });
}

/// Render effect with text row then separate bar (original style)
fn render_separate_style(
    ui: &mut Ui,
    effect: &crate::data::widget::ActiveEffect,
    config: &ActiveEffectsWidgetData,
) {
    ui.horizontal(|ui| {
        // Effect name with optional color
        let text = if let Some(ref color_hex) = effect.text_color {
            if let Some(color) = parse_hex_to_color32(color_hex) {
                RichText::new(&effect.text)
                    .color(color)
                    .size(config.text_size)
            } else {
                RichText::new(&effect.text).size(config.text_size)
            }
        } else {
            RichText::new(&effect.text).size(config.text_size)
        };
        ui.label(text);

        // Time remaining
        if config.show_timer && !effect.time.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(&effect.time)
                        .size(config.text_size * 0.9)
                        .weak(),
                );
            });
        }
    });

    // Progress bar if value > 0
    if effect.value > 0 {
        let fraction = effect.value as f32 / 100.0;
        let bar_color = effect
            .bar_color
            .as_ref()
            .and_then(|c| parse_hex_to_color32(c))
            .unwrap_or(Color32::from_rgb(100, 150, 200));

        ui.add(
            ProgressBar::new(fraction)
                .fill(bar_color)
                .desired_height(6.0),
        );
    }
}

/// Render effect with text row and thin 2px bar below
fn render_thin_bar_style(
    ui: &mut Ui,
    effect: &crate::data::widget::ActiveEffect,
    config: &ActiveEffectsWidgetData,
) {
    ui.horizontal(|ui| {
        let text = if let Some(ref color_hex) = effect.text_color {
            if let Some(color) = parse_hex_to_color32(color_hex) {
                RichText::new(&effect.text)
                    .color(color)
                    .size(config.text_size)
            } else {
                RichText::new(&effect.text).size(config.text_size)
            }
        } else {
            RichText::new(&effect.text).size(config.text_size)
        };
        ui.label(text);

        if config.show_timer && !effect.time.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(&effect.time)
                        .size(config.text_size * 0.9)
                        .weak(),
                );
            });
        }
    });

    // Ultra-thin bar
    if effect.value > 0 {
        let fraction = effect.value as f32 / 100.0;
        let bar_color = effect
            .bar_color
            .as_ref()
            .and_then(|c| parse_hex_to_color32(c))
            .unwrap_or(Color32::from_rgb(100, 150, 200));

        ui.add(
            ProgressBar::new(fraction)
                .fill(bar_color)
                .desired_height(2.0), // Very thin
        );
    }
}

/// Render effect with colored bar on left edge
fn render_side_indicator_style(
    ui: &mut Ui,
    effect: &crate::data::widget::ActiveEffect,
    config: &ActiveEffectsWidgetData,
) {
    ui.horizontal(|ui| {
        // Colored vertical bar on left (4px wide)
        let bar_color = effect
            .bar_color
            .as_ref()
            .and_then(|c| parse_hex_to_color32(c))
            .unwrap_or(Color32::from_rgb(100, 150, 200));

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(4.0, config.bar_height), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0, bar_color);

        // Text
        let text = if let Some(ref color_hex) = effect.text_color {
            if let Some(color) = parse_hex_to_color32(color_hex) {
                RichText::new(&effect.text)
                    .color(color)
                    .size(config.text_size)
            } else {
                RichText::new(&effect.text).size(config.text_size)
            }
        } else {
            RichText::new(&effect.text).size(config.text_size)
        };
        ui.label(text);

        // Optional inline progress indicator
        if config.show_percentage {
            ui.label(
                RichText::new(format!("[{}%]", effect.value))
                    .size(config.text_size * 0.85)
                    .weak(),
            );
        }

        // Time on right
        if config.show_timer && !effect.time.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(&effect.time)
                        .size(config.text_size * 0.9)
                        .weak(),
                );
            });
        }
    });
}

/// Render effect as text only (no bars)
fn render_minimal_style(
    ui: &mut Ui,
    effect: &crate::data::widget::ActiveEffect,
    config: &ActiveEffectsWidgetData,
) {
    ui.horizontal(|ui| {
        let text = if let Some(ref color_hex) = effect.text_color {
            if let Some(color) = parse_hex_to_color32(color_hex) {
                RichText::new(&effect.text)
                    .color(color)
                    .size(config.text_size)
            } else {
                RichText::new(&effect.text).size(config.text_size)
            }
        } else {
            RichText::new(&effect.text).size(config.text_size)
        };
        ui.label(text);

        if config.show_percentage {
            ui.label(
                RichText::new(format!("{}%", effect.value))
                    .size(config.text_size * 0.85)
                    .weak(),
            );
        }

        if config.show_timer && !effect.time.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(&effect.time)
                        .size(config.text_size * 0.9)
                        .weak(),
                );
            });
        }
    });
}

// ==================== Helper Functions ====================

/// Helper: Determine if effect is expiring
fn is_expiring(effect: &crate::data::widget::ActiveEffect, threshold: u32) -> bool {
    if let Some(seconds) = parse_time_to_seconds(&effect.time) {
        seconds <= threshold
    } else {
        false
    }
}

/// Helper: Parse time string to seconds
fn parse_time_to_seconds(time: &str) -> Option<u32> {
    // Parse formats like "03:45", "1:23:45", "45s"
    let parts: Vec<&str> = time.split(':').collect();
    match parts.len() {
        2 => {
            // MM:SS
            let min: u32 = parts[0].parse().ok()?;
            let sec: u32 = parts[1].parse().ok()?;
            Some(min * 60 + sec)
        }
        3 => {
            // HH:MM:SS
            let hr: u32 = parts[0].parse().ok()?;
            let min: u32 = parts[1].parse().ok()?;
            let sec: u32 = parts[2].parse().ok()?;
            Some(hr * 3600 + min * 60 + sec)
        }
        _ => None,
    }
}

/// Helper: Auto-contrast text color
fn determine_contrast_color(bar_color: &Color32, progress: u32) -> Color32 {
    // If bar is mostly filled, use color contrast against bar
    // If bar is mostly empty, use contrast against dark background
    let use_bar_contrast = progress > 30;

    if use_bar_contrast {
        let luminance =
            0.299 * bar_color.r() as f32 + 0.587 * bar_color.g() as f32 + 0.114 * bar_color.b() as f32;

        if luminance > 128.0 {
            Color32::BLACK // Dark text on light bar
        } else {
            Color32::WHITE // Light text on dark bar
        }
    } else {
        Color32::WHITE // Light text on dark background
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::ActiveEffect;

    #[test]
    fn test_empty_effects() {
        let content = ActiveEffectsContent {
            category: "Buffs".to_string(),
            effects: vec![],
        };
        assert!(content.effects.is_empty());
    }

    #[test]
    fn test_effects_with_data() {
        let content = ActiveEffectsContent {
            category: "Active Spells".to_string(),
            effects: vec![
                ActiveEffect {
                    id: "spell_1".to_string(),
                    text: "Spirit Shield".to_string(),
                    value: 75,
                    time: "03:45".to_string(),
                    bar_color: Some("#4488CC".to_string()),
                    text_color: Some("#AACCFF".to_string()),
                },
                ActiveEffect {
                    id: "spell_2".to_string(),
                    text: "Haste".to_string(),
                    value: 50,
                    time: "01:30".to_string(),
                    bar_color: None,
                    text_color: None,
                },
            ],
        };
        assert_eq!(content.effects.len(), 2);
        assert_eq!(content.effects[0].text, "Spirit Shield");
        assert_eq!(content.effects[1].value, 50);
    }

    #[test]
    fn test_parse_time_to_seconds() {
        assert_eq!(parse_time_to_seconds("03:45"), Some(225));
        assert_eq!(parse_time_to_seconds("1:23:45"), Some(5025));
        assert_eq!(parse_time_to_seconds("00:30"), Some(30));
        assert_eq!(parse_time_to_seconds("invalid"), None);
    }

    #[test]
    fn test_is_expiring() {
        let effect = ActiveEffect {
            id: "test".to_string(),
            text: "Test".to_string(),
            value: 50,
            time: "00:20".to_string(),
            bar_color: None,
            text_color: None,
        };
        assert!(is_expiring(&effect, 30)); // 20 seconds < 30 threshold

        let effect2 = ActiveEffect {
            id: "test2".to_string(),
            text: "Test2".to_string(),
            value: 50,
            time: "02:00".to_string(),
            bar_color: None,
            text_color: None,
        };
        assert!(!is_expiring(&effect2, 30)); // 120 seconds > 30 threshold
    }
}
