//! Injury Doll Widget - Image-based wound/scar visualization for GUI
//!
//! Renders character silhouette with colored markers indicating wound/scar levels.
//! Supports calibration mode for interactive body part positioning.

use crate::config::{InjuryBodyPart, InjuryCalibration, InjuryDollWidgetData, InjuryMarkerStyle};
use crate::data::widget::InjuryDollData;
use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Ui, Vec2};

/// Response from rendering injury doll (for calibration interaction)
#[derive(Default)]
pub struct InjuryDollResponse {
    /// Body part clicked during calibration (if any)
    pub clicked_body_part: Option<String>,
    /// Normalized click position (0.0-1.0) if clicked during calibration
    pub clicked_position: Option<(f32, f32)>,
}

impl InjuryDollResponse {
    /// Create a new default response
    pub fn new() -> Self {
        Self::default()
    }
}

/// Render injury doll widget with image and overlays
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `data` - The InjuryDollData from the game
/// * `config` - The InjuryDollWidgetData configuration
/// * `texture_handle` - The loaded character silhouette texture
/// * `overlay_textures` - Phase 5: Overlay layer textures (nervous_system, nerves_greyscale, etc.)
/// * `rank_textures` - Phase 5: Rank indicator textures (rank1/2/3/nerves)
/// * `calibration_mode` - Whether calibration UI is active
/// * `calibration_target` - Current body part being calibrated (if any)
pub fn render_injury_doll(
    ui: &mut Ui,
    data: &InjuryDollData,
    config: &InjuryDollWidgetData,
    texture_handle: &egui::TextureHandle,
    overlay_textures: &std::collections::HashMap<String, egui::TextureHandle>,
    rank_textures: &Option<(egui::TextureHandle, egui::TextureHandle, egui::TextureHandle, egui::TextureHandle)>,
    calibration_mode: bool,
    calibration_target: Option<&str>,
) -> InjuryDollResponse {
    let mut response = InjuryDollResponse::default();

    // Calculate scaled image size
    // Base images: 800x600. Scale formula: 1.0 scale = 200x150 (25% of original)
    // At scale 1.0: 800→200, 600→150
    // At scale 2.0: 800→400, 600→300
    // At scale 0.5: 800→100, 600→75
    let original_size = texture_handle.size_vec2();
    let scaled_size = original_size * (config.scale * 0.25);

    // Proportional marker sizing: marker_size scales with image scale
    // Base marker size (config.marker_size) at scale 1.0 = 6.0px (default)
    // At scale 2.0: marker = 12.0px
    // At scale 0.5: marker = 3.0px
    let effective_marker_size = config.marker_size * config.scale;

    // Allocate space for the image
    // Only use Sense::click() in calibration mode to avoid consuming right-clicks
    let sense = if calibration_mode {
        Sense::click() // Calibration needs click detection for body part positioning
    } else {
        Sense::hover() // Normal mode doesn't need clicks, prevents hijacking right-click
    };
    let (rect, img_response) = ui.allocate_exact_size(scaled_size, sense);

    // Render background color if specified
    if let Some(ref bg_hex) = config.background_color {
        if let Some(bg_color) = parse_hex_to_color32(bg_hex) {
            ui.painter().rect_filled(rect, 0.0, bg_color);
        }
    }

    // Phase 5: Multi-layer rendering order:
    // 1. Base character silhouette
    render_silhouette(ui, texture_handle, rect, config);

    // 2. Overlay layers (nervous_system, nerves_greyscale, etc.) - sorted by z_index
    render_overlay_layers(ui, overlay_textures, config, rect, data);

    // 3. Wound/scar markers (using rank PNG textures) or calibration targets
    if !calibration_mode {
        render_markers(ui, data, config, rect, rank_textures, effective_marker_size);
    } else {
        // Calibration mode: show rank1 indicator for current body part being calibrated
        let (clicked_part, clicked_pos) = render_calibration_targets(
            ui,
            config,
            rect,
            calibration_target,
            &img_response,
            rank_textures,
            effective_marker_size,
        );
        response.clicked_body_part = clicked_part;
        response.clicked_position = clicked_pos;
    }

    response
}

/// Render character silhouette with optional tinting/greyscale
fn render_silhouette(
    ui: &mut Ui,
    texture: &egui::TextureHandle,
    rect: Rect,
    config: &InjuryDollWidgetData,
) {
    // Apply tinting ONLY when greyscale is enabled
    let tint = if config.greyscale {
        if let Some(ref tint_hex) = config.tint_color {
            if let Some(tint_color) = parse_hex_to_color32(tint_hex) {
                // Blend tint color with white based on tint_strength
                let strength = config.tint_strength.clamp(0.0, 1.0);
                Color32::from_rgba_premultiplied(
                    (255.0 * (1.0 - strength) + tint_color.r() as f32 * strength) as u8,
                    (255.0 * (1.0 - strength) + tint_color.g() as f32 * strength) as u8,
                    (255.0 * (1.0 - strength) + tint_color.b() as f32 * strength) as u8,
                    255,
                )
            } else {
                Color32::WHITE
            }
        } else {
            Color32::WHITE
        }
    } else {
        Color32::WHITE  // No tint when greyscale is disabled
    };

    // Render image
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), // UV coords
        tint,
    );

    // TODO: Greyscale support requires shader or pixel manipulation
    // Option 1: Pre-process image and cache greyscale version
    // Option 2: Use color tint to approximate greyscale (#808080 at high strength)
}

/// Render overlay layers (Phase 5: nervous_system, nerves_greyscale, etc.)
fn render_overlay_layers(
    ui: &mut Ui,
    overlay_textures: &std::collections::HashMap<String, egui::TextureHandle>,
    config: &InjuryDollWidgetData,
    rect: Rect,
    data: &InjuryDollData,
) {
    // Sort overlays by z_index
    let mut sorted_overlays: Vec<_> = config.overlay_layers.iter().collect();
    sorted_overlays.sort_by_key(|o| o.z_index);

    for overlay in sorted_overlays {
        if !overlay.enabled {
            continue;
        }

        // Get the texture for this overlay
        if let Some(texture) = overlay_textures.get(&overlay.name) {
            // Overlay ALWAYS applies tint (not conditional)
            let tint = calculate_severity_tint(config, data);

            // Apply opacity
            let tint_with_opacity = Color32::from_rgba_unmultiplied(
                tint.r(),
                tint.g(),
                tint.b(),
                (overlay.opacity * 255.0) as u8,
            );

            // Render overlay
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                tint_with_opacity,
            );
        }
    }
}

/// Calculate severity tint color based on injury data (Phase 5)
fn calculate_severity_tint(config: &InjuryDollWidgetData, data: &InjuryDollData) -> Color32 {
    // Find max injury level across all body parts
    let max_level = data.injuries.values().max().copied().unwrap_or(0);

    if max_level == 0 {
        return Color32::WHITE; // No injury, no tint
    }

    // Get tint color from config
    let tint_hex = match config.tint_mode {
        crate::config::TintMode::Unified => config.tint_color.as_ref(),
        crate::config::TintMode::Separate => config.overlay_tint_color.as_ref(),
    };

    let tint_strength = match config.tint_mode {
        crate::config::TintMode::Unified => config.tint_strength,
        crate::config::TintMode::Separate => config.overlay_tint_strength,
    };

    if let Some(hex) = tint_hex {
        if let Some(tint_color) = parse_hex_to_color32(hex) {
            // Blend tint with white based on strength and severity
            let strength = tint_strength.clamp(0.0, 1.0);
            let severity_multiplier = (max_level as f32 / 6.0).clamp(0.0, 1.0);
            let final_strength = strength * severity_multiplier;

            return Color32::from_rgba_premultiplied(
                (255.0 * (1.0 - final_strength) + tint_color.r() as f32 * final_strength) as u8,
                (255.0 * (1.0 - final_strength) + tint_color.g() as f32 * final_strength) as u8,
                (255.0 * (1.0 - final_strength) + tint_color.b() as f32 * final_strength) as u8,
                255,
            );
        }
    }

    Color32::WHITE
}

/// Render rank indicator overlays (Phase 5)
fn render_rank_indicators(
    ui: &mut Ui,
    rank_textures: &Option<(egui::TextureHandle, egui::TextureHandle, egui::TextureHandle)>,
    config: &InjuryDollWidgetData,
    rect: Rect,
    data: &InjuryDollData,
) {
    if !config.rank_indicators.enabled {
        return;
    }

    let Some((rank1_tex, rank2_tex, rank3_tex)) = rank_textures else {
        return;
    };

    // For each body part with an injury, render the appropriate rank indicator
    for (body_part, level) in &data.injuries {
        if *level == 0 {
            continue;
        }

        // Get body part position
        if let Some(bp) = config.calibration.body_parts.get(body_part) {
            if !bp.enabled {
                continue;
            }

            let marker_pos = Pos2::new(
                rect.min.x + bp.x * rect.width(),
                rect.min.y + bp.y * rect.height(),
            );

            // Determine which rank indicator to use (1-3 for injuries, 1-3 for scars)
            let rank_texture = match level {
                1 | 4 => rank1_tex, // Injury 1 or Scar 1
                2 | 5 => rank2_tex, // Injury 2 or Scar 2
                3 | 6 => rank3_tex, // Injury 3 or Scar 3
                _ => continue,
            };

            // Render rank indicator centered on body part
            let indicator_size = Vec2::splat(config.marker_size * 2.0);
            let indicator_rect = Rect::from_center_size(marker_pos, indicator_size);

            let opacity = (config.rank_indicators.opacity * 255.0) as u8;
            let tint = Color32::from_rgba_unmultiplied(255, 255, 255, opacity);

            ui.painter().image(
                rank_texture.id(),
                indicator_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                tint,
            );
        }
    }
}

/// Render wound/scar markers on body parts using rank PNG textures with tints
/// This replaces the old circle-based system with PNG textures (rank1/2/3.png + nerves.png)
fn render_markers(
    ui: &mut Ui,
    data: &InjuryDollData,
    config: &InjuryDollWidgetData,
    image_rect: Rect,
    rank_textures: &Option<(egui::TextureHandle, egui::TextureHandle, egui::TextureHandle, egui::TextureHandle)>,
    effective_marker_size: f32,
) {
    // Early return if no rank textures available
    let Some((rank1_tex, rank2_tex, rank3_tex, nerves_tex)) = rank_textures else {
        return; // Fall back to no rendering if textures not loaded
    };

    // Import parse_hex_to_color32 for nerve tint parsing
    use super::parse_hex_to_color32;

    for (body_part, level) in &data.injuries {
        // Skip if no injury/scar
        if *level == 0 {
            continue;
        }

        // Get body part position from calibration
        if let Some(bp) = config.calibration.body_parts.get(body_part) {
            if !bp.enabled {
                continue;
            }

            // Convert normalized coords to pixel position
            let marker_pos = Pos2::new(
                image_rect.min.x + bp.x * image_rect.width(),
                image_rect.min.y + bp.y * image_rect.height(),
            );

            // Detect if this is nerve damage (nervous system)
            let is_nerve = body_part.to_lowercase() == "nsys";

            let (texture, tint_color) = if is_nerve {
                // Nerve damage: Use nerves.png with severity-based tint
                let nerve_tint = match level {
                    1 | 4 => parse_hex_to_color32(&config.rank_indicators.nerve_tint1_color)
                        .unwrap_or(Color32::YELLOW),
                    2 | 5 => parse_hex_to_color32(&config.rank_indicators.nerve_tint2_color)
                        .unwrap_or(Color32::from_rgb(255, 165, 0)), // Orange
                    3 | 6 => parse_hex_to_color32(&config.rank_indicators.nerve_tint3_color)
                        .unwrap_or(Color32::RED),
                    _ => Color32::WHITE,
                };
                (nerves_tex, nerve_tint)
            } else {
                // Regular wound/scar: Use rank1/2/3.png with existing tint logic
                let rank_texture = match level {
                    1 | 4 => rank1_tex,
                    2 | 5 => rank2_tex,
                    3 | 6 => rank3_tex,
                    _ => continue,
                };
                let tint_color = get_injury_color(config, *level);
                (rank_texture, tint_color)
            };

            // Apply tint strength - blend with white based on config.marker_tint_strength
            let tint_with_strength = apply_tint_strength(tint_color, config.marker_tint_strength);

            // Calculate indicator size based on marker type
            let indicator_size = if is_nerve {
                // nerves.png (125×92) scales with image, not marker_size
                // Use actual texture dimensions scaled by config.scale * 0.25
                let tex_size = texture.size_vec2();
                tex_size * (config.scale * 0.25)
            } else {
                // rank1/2/3.png (40×40) use marker_size
                Vec2::splat(effective_marker_size * 2.0)
            };

            let indicator_rect = Rect::from_center_size(marker_pos, indicator_size);

            ui.painter().image(
                texture.id(),
                indicator_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                tint_with_strength,
            );
        }
    }
}

/// Apply tint strength to a color (blend with white)
/// strength 0.0 = pure white, strength 1.0 = pure color
fn apply_tint_strength(color: Color32, strength: f32) -> Color32 {
    let strength = strength.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (255.0 * (1.0 - strength) + color.r() as f32 * strength) as u8,
        (255.0 * (1.0 - strength) + color.g() as f32 * strength) as u8,
        (255.0 * (1.0 - strength) + color.b() as f32 * strength) as u8,
        color.a(),
    )
}

/// Render solid circle marker
fn render_circle_marker(ui: &mut Ui, pos: Pos2, radius: f32, color: Color32) {
    ui.painter().circle_filled(pos, radius, color);
    // Add subtle border for contrast
    ui.painter()
        .circle_stroke(pos, radius, (1.5, Color32::from_black_alpha(100)));
}

/// Render circle outline marker
fn render_circle_outline_marker(ui: &mut Ui, pos: Pos2, radius: f32, color: Color32) {
    ui.painter().circle_stroke(pos, radius, (2.5, color));
}

/// Render numbered marker using injuryNumbers.png texture
fn render_numbered_marker(ui: &mut Ui, pos: Pos2, radius: f32, color: Color32, level: u8) {
    // TODO: Load injuryNumbers.png as texture atlas
    // For now, fallback to circle + text
    render_circle_marker(ui, pos, radius, color);
    render_level_text(ui, pos, level);
}

/// Render injury level number
fn render_level_text(ui: &mut Ui, pos: Pos2, level: u8) {
    let text = level.to_string();
    ui.painter().text(
        pos,
        egui::Align2::CENTER_CENTER,
        &text,
        egui::FontId::proportional(14.0),
        Color32::WHITE,
    );
}

/// Render calibration targets (clickable body part positions)
/// Shows rank1 indicator at current calibration target position
fn render_calibration_targets(
    ui: &mut Ui,
    config: &InjuryDollWidgetData,
    image_rect: Rect,
    current_target: Option<&str>,
    img_response: &Response,
    rank_textures: &Option<(egui::TextureHandle, egui::TextureHandle, egui::TextureHandle, egui::TextureHandle)>,
    effective_marker_size: f32,
) -> (Option<String>, Option<(f32, f32)>) {
    let mut clicked_part = None;
    let mut clicked_position = None;

    // Show rank1 indicator at current calibration target position (or nerves for nerves body part)
    if let Some(target_name) = current_target {
        if let Some(bp) = config.calibration.body_parts.get(target_name) {
            let marker_pos = Pos2::new(
                image_rect.min.x + bp.x * image_rect.width(),
                image_rect.min.y + bp.y * image_rect.height(),
            );

            // Use nerves.png for nsys (nervous system) calibration, rank1.png for others
            let is_nerve = target_name.to_lowercase() == "nsys";

            if let Some((rank1_tex, _, _, nerves_tex)) = rank_textures {
                let calibration_tex = if is_nerve { nerves_tex } else { rank1_tex };

                // Calculate indicator size based on marker type
                let indicator_size = if is_nerve {
                    // nerves.png (125×92) scales with image, not marker_size
                    let tex_size = calibration_tex.size_vec2();
                    tex_size * (config.scale * 0.25)
                } else {
                    // rank1.png (40×40) uses marker_size
                    Vec2::splat(effective_marker_size * 2.0)
                };

                let indicator_rect = Rect::from_center_size(marker_pos, indicator_size);

                ui.painter().image(
                    calibration_tex.id(),
                    indicator_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::from_rgba_unmultiplied(255, 255, 0, 200), // Yellow tint to indicate calibration mode
                );
            } else {
                // Fallback: yellow circle if textures not available
                ui.painter()
                    .circle_filled(marker_pos, effective_marker_size, Color32::from_rgba_unmultiplied(255, 255, 0, 200));
            }

            // Add label showing which body part is being calibrated
            ui.painter().text(
                Pos2::new(marker_pos.x, marker_pos.y - effective_marker_size - 10.0),
                egui::Align2::CENTER_CENTER,
                target_name,
                egui::FontId::proportional(12.0),
                Color32::YELLOW,
            );
        }
    }

    // Detect click on image area - updates position immediately
    if img_response.clicked() {
        if let Some(click_pos) = img_response.interact_pointer_pos() {
            // Convert to normalized coords (0.0-1.0)
            let normalized_x = (click_pos.x - image_rect.min.x) / image_rect.width();
            let normalized_y = (click_pos.y - image_rect.min.y) / image_rect.height();

            // Clamp to 0.0-1.0 range
            let normalized_x = normalized_x.clamp(0.0, 1.0);
            let normalized_y = normalized_y.clamp(0.0, 1.0);

            // Store the normalized position
            clicked_position = Some((normalized_x, normalized_y));

            // Return the body part name for the response
            if let Some(target_name) = current_target {
                clicked_part = Some(target_name.to_string());
            }
        }
    }

    (clicked_part, clicked_position)
}

/// Get injury color from config based on level
fn get_injury_color(config: &InjuryDollWidgetData, level: u8) -> Color32 {
    let hex = match level {
        1 => config.injury1_color.as_ref(),
        2 => config.injury2_color.as_ref(),
        3 => config.injury3_color.as_ref(),
        4 => config.scar1_color.as_ref(),
        5 => config.scar2_color.as_ref(),
        6 => config.scar3_color.as_ref(),
        _ => config.injury_default_color.as_ref(),
    };

    hex.and_then(|h| parse_hex_to_color32(h))
        .unwrap_or_else(|| default_injury_color(level))
}

/// Default injury colors if config missing
fn default_injury_color(level: u8) -> Color32 {
    match level {
        1 => Color32::from_rgb(0xaa, 0x55, 0x00),
        2 => Color32::from_rgb(0xff, 0x88, 0x00),
        3 => Color32::from_rgb(0xff, 0x00, 0x00),
        4 => Color32::from_rgb(0x99, 0x99, 0x99),
        5 => Color32::from_rgb(0x77, 0x77, 0x77),
        6 => Color32::from_rgb(0x55, 0x55, 0x55),
        _ => Color32::from_rgb(0x33, 0x33, 0x33),
    }
}

/// Parse hex color string to egui Color32
/// Supports formats: #RGB, #RRGGBB, #RRGGBBAA
fn parse_hex_to_color32(hex: &str) -> Option<Color32> {
    let hex = hex.trim().trim_start_matches('#');

    match hex.len() {
        3 => {
            // #RGB format
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        6 => {
            // #RRGGBB format
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        8 => {
            // #RRGGBBAA format
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Color32::from_rgba_unmultiplied(r, g, b, a))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_to_color32() {
        // #RRGGBB format
        assert_eq!(
            parse_hex_to_color32("#ff0000"),
            Some(Color32::from_rgb(255, 0, 0))
        );
        assert_eq!(
            parse_hex_to_color32("#00ff00"),
            Some(Color32::from_rgb(0, 255, 0))
        );
        assert_eq!(
            parse_hex_to_color32("#0000ff"),
            Some(Color32::from_rgb(0, 0, 255))
        );

        // #RGB format
        assert_eq!(
            parse_hex_to_color32("#f00"),
            Some(Color32::from_rgb(255, 0, 0))
        );

        // Invalid formats
        assert_eq!(parse_hex_to_color32("#invalid"), None);
        assert_eq!(parse_hex_to_color32("#12"), None);
    }

    #[test]
    fn test_get_injury_color_defaults() {
        let config = InjuryDollWidgetData {
            injury_default_color: None,
            injury1_color: None,
            injury2_color: None,
            injury3_color: None,
            scar1_color: None,
            scar2_color: None,
            scar3_color: None,
            image_path: None,
            scale: 1.0,
            greyscale: false,
            tint_color: None,
            tint_strength: 0.3,
            marker_tint_strength: 0.3,
            marker_style: InjuryMarkerStyle::Circles,
            marker_size: 6.0,
            show_numbers: false,
            calibration: InjuryCalibration::default(),
            image_profiles: Vec::new(),
            // Phase 5
            tint_mode: crate::config::TintMode::Unified,
            overlay_tint_color: None,
            overlay_tint_strength: 0.3,
            nerve_indicator_type: crate::config::NerveIndicatorType::Default,
            overlay_layers: Vec::new(),
            rank_indicators: crate::config::RankIndicatorConfig::default(),
            background_color: None,
        };

        // Test default colors
        assert_eq!(
            get_injury_color(&config, 1),
            Color32::from_rgb(0xaa, 0x55, 0x00)
        );
        assert_eq!(
            get_injury_color(&config, 3),
            Color32::from_rgb(0xff, 0x00, 0x00)
        );
        assert_eq!(
            get_injury_color(&config, 6),
            Color32::from_rgb(0x55, 0x55, 0x55)
        );
    }
}
