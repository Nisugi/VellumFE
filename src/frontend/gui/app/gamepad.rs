//! Native gamepad input (gilrs) for the GUI — controller Tier 1.
//!
//! Fixed navigation: while interact mode or a popup menu is open the
//! d-pad is arrows, South (A/cross) confirms, East (B/circle) cancels.
//! Everything else — and those buttons outside the modal modes — goes
//! through the user's `[controller]` bindings in keybinds.toml (edited
//! with `.controller`), which map buttons to keybind actions or macros.
//!
//! Backend notes: Windows uses Windows Gaming Input (needs a focused
//! window — true for the GUI; Xbox + DualShock verified on hardware),
//! Linux evdev, macOS IOKit (verify on hardware; may need a
//! GameController-framework bridge).

use super::VellumGuiApp;
use crate::data::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::data::ui_state::InputMode;
use eframe::egui;

/// Canonical button names used by the `[controller]` config table.
pub(super) fn gamepad_button_name(button: gilrs::Button) -> Option<&'static str> {
    use gilrs::Button;
    Some(match button {
        Button::South => "south",
        Button::East => "east",
        Button::North => "north",
        Button::West => "west",
        Button::DPadUp => "dpad_up",
        Button::DPadDown => "dpad_down",
        Button::DPadLeft => "dpad_left",
        Button::DPadRight => "dpad_right",
        Button::LeftTrigger => "l1",
        Button::LeftTrigger2 => "l2",
        Button::RightTrigger => "r1",
        Button::RightTrigger2 => "r2",
        Button::LeftThumb => "l3",
        Button::RightThumb => "r3",
        Button::Select => "select",
        Button::Start => "start",
        Button::Mode => "guide",
        _ => return None,
    })
}

/// All bindable button names, for the editor's dropdown.
pub(super) const GAMEPAD_BUTTON_NAMES: [&str; 17] = [
    "south", "east", "north", "west", "dpad_up", "dpad_down", "dpad_left", "dpad_right", "l1",
    "r1", "l2", "r2", "l3", "r3", "select", "start", "guide",
];

/// Compass commands by left-stick sector, clockwise from north.
const STICK_DIRS: [&str; 8] = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];
/// Deflection needed to register a direction, and the smaller release
/// threshold that re-arms it (hysteresis so a wobbling stick at the
/// boundary doesn't spam moves).
const STICK_DEFLECT: f32 = 0.6;
const STICK_RELEASE: f32 = 0.35;

/// Map a stick position (y positive = up, gilrs convention) to a compass
/// sector, honoring hysteresis against the previously held sector.
#[allow(clippy::manual_range_contains)]
fn stick_sector(x: f32, y_up: f32, previous: Option<usize>) -> Option<usize> {
    let magnitude = (x * x + y_up * y_up).sqrt();
    let threshold = if previous.is_some() {
        STICK_RELEASE
    } else {
        STICK_DEFLECT
    };
    if magnitude < threshold {
        return None;
    }
    // 0 degrees = north, clockwise positive; 45-degree sectors centered
    // on the compass points.
    let angle = x.atan2(y_up).to_degrees();
    Some((((angle + 360.0 + 22.5) / 45.0) as usize) % 8)
}

impl VellumGuiApp {
    /// Drain pending gamepad events and keep the poll loop alive while a
    /// controller is connected. Called once per frame before keyboard
    /// dispatch.
    pub(super) fn poll_gamepad(&mut self, ctx: &egui::Context) {
        let Some(gilrs) = self.gamepad.as_mut() else {
            return;
        };

        let mut pressed: Vec<gilrs::Button> = Vec::new();
        let mut connections: Vec<(gilrs::GamepadId, bool)> = Vec::new();
        while let Some(event) = gilrs.next_event() {
            match event.event {
                gilrs::EventType::ButtonPressed(button, _) => pressed.push(button),
                gilrs::EventType::Connected => connections.push((event.id, true)),
                gilrs::EventType::Disconnected => connections.push((event.id, false)),
                _ => {}
            }
        }

        let any_connected = gilrs.gamepads().next().is_some();
        for (id, connected) in connections {
            let name = gilrs.gamepad(id).name().to_string();
            self.app_core.add_system_message(&if connected {
                format!("Controller connected: {}", name)
            } else {
                format!("Controller disconnected: {}", name)
            });
        }

        // Left stick: 8-way compass movement, one command per deflection.
        // Right stick: analog story-window scroll (speed follows deflection).
        let (stick, right_y) = self
            .gamepad
            .as_ref()
            .and_then(|g| g.gamepads().next())
            .map(|(_, pad)| {
                (
                    Some((
                        pad.value(gilrs::Axis::LeftStickX),
                        pad.value(gilrs::Axis::LeftStickY),
                    )),
                    pad.value(gilrs::Axis::RightStickY),
                )
            })
            .unwrap_or((None, 0.0));

        for button in pressed {
            // The controller editor's "press a button" capture wins over
            // dispatch while armed.
            if let Some(name) = gamepad_button_name(button) {
                if self.controller_editor_capture(name) {
                    continue;
                }
            }
            self.handle_gamepad_button(button, ctx);
        }

        // Radial wheel: while a wheel button is held, the left stick aims
        // a slice; South opens a folder slice, East backs up, releasing
        // fires the aimed leaf. Owns the stick while active (no movement)
        // and swallows unrelated buttons.
        let held_key = self.held_wheel_key();
        match (self.gp_wheel.is_some(), held_key) {
            (false, Some(key)) => {
                self.gp_wheel = Some(WheelUi {
                    key,
                    path: Vec::new(),
                    aimed: None,
                });
                self.app_core.needs_render = true;
            }
            (true, None) => {
                let ui = self.gp_wheel.take().expect("just matched Some");
                if let Some(index) = ui.aimed {
                    if let Some(slice) = self
                        .wheel_level_slices(&ui.key, &ui.path)
                        .and_then(|level| level.get(index).cloned())
                    {
                        if !slice.is_folder() && !slice.command.is_empty() {
                            self.dispatch_command(slice.command);
                        }
                    }
                }
                self.app_core.needs_render = true;
            }
            (true, Some(_)) => {
                if let Some((x, y_up)) = stick {
                    let (key, path) = {
                        let ui = self.gp_wheel.as_ref().expect("just matched Some");
                        (ui.key.clone(), ui.path.clone())
                    };
                    let count = self
                        .wheel_level_slices(&key, &path)
                        .map(|level| level.len())
                        .unwrap_or(0);
                    let aimed = wheel_slice_at(x, y_up, count);
                    if let Some(ui) = self.gp_wheel.as_mut() {
                        if aimed.is_some() && ui.aimed != aimed {
                            ui.aimed = aimed;
                            self.app_core.needs_render = true;
                        }
                    }
                }
            }
            (false, None) => {}
        }

        if let Some((x, y_up)) = stick.filter(|_| self.gp_wheel.is_none()) {
            let sector = stick_sector(x, y_up, self.gp_stick_sector);
            if sector != self.gp_stick_sector {
                // Movement while a menu is open would be disorienting; the
                // stick stays quiet until it closes. Interact mode is fine —
                // walking away just re-syncs focus in the next room.
                let menu_open = self.app_core.ui_state.input_mode == InputMode::Menu
                    || self.controller_editor.is_some();
                if let Some(new_sector) = sector {
                    if !menu_open {
                        self.dispatch_raw_command(STICK_DIRS[new_sector].to_string());
                    }
                }
                self.gp_stick_sector = sector;
            }
        }

        // Right stick up/down scrolls the main story window; quadratic
        // curve so small deflections creep and full tilt flies. Stick up
        // scrolls up (negative offset delta).
        if right_y.abs() > 0.25 && self.app_core.ui_state.input_mode != InputMode::Menu {
            let delta = -right_y.signum() * right_y * right_y * 40.0;
            ctx.data_mut(|d| {
                d.insert_temp(
                    egui::Id::new(("text_scroll_pending", "main")),
                    (0u8, delta),
                )
            });
        }

        // gilrs events arrive outside egui's own event loop; without a
        // wake-up an idle GUI would stop polling. Cheap while connected.
        if any_connected {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn handle_gamepad_button(&mut self, button: gilrs::Button, ctx: &egui::Context) {
        use gilrs::Button;

        // The wheel owns the pad while it is up: South opens the aimed
        // folder (or fires a leaf immediately), East backs up a level;
        // everything else is swallowed.
        if self.gp_wheel.is_some() {
            match button {
                Button::South => {
                    let (key, path, aimed) = {
                        let ui = self.gp_wheel.as_ref().expect("wheel active");
                        (ui.key.clone(), ui.path.clone(), ui.aimed)
                    };
                    let Some(index) = aimed else { return };
                    let Some(slice) = self
                        .wheel_level_slices(&key, &path)
                        .and_then(|level| level.get(index).cloned())
                    else {
                        return;
                    };
                    if slice.is_folder() {
                        if let Some(ui) = self.gp_wheel.as_mut() {
                            ui.path.push(index);
                            ui.aimed = None;
                        }
                    } else if !slice.command.is_empty() {
                        self.gp_wheel = None;
                        self.dispatch_command(slice.command);
                    }
                    self.app_core.needs_render = true;
                }
                Button::East => {
                    if let Some(ui) = self.gp_wheel.as_mut() {
                        ui.path.pop();
                        ui.aimed = None;
                        self.app_core.needs_render = true;
                    }
                }
                _ => {}
            }
            return;
        }

        // While interact mode or a popup menu is up, the d-pad and the
        // confirm/cancel face buttons are fixed navigation keys — unless
        // the shift button is held, which always means "the other bank".
        let shift_held = self.gamepad_shift_held();
        let modal = matches!(
            self.app_core.ui_state.input_mode,
            InputMode::Interact | InputMode::Menu
        );
        if modal && !shift_held {
            let code = match button {
                Button::DPadUp => Some(KeyCode::Up),
                Button::DPadDown => Some(KeyCode::Down),
                Button::DPadLeft => Some(KeyCode::Left),
                Button::DPadRight => Some(KeyCode::Right),
                Button::South => Some(KeyCode::Enter),
                Button::East => Some(KeyCode::Esc),
                _ => None,
            };
            if let Some(code) = code {
                let key = KeyEvent::new(code, KeyModifiers::NONE);
                self.handle_modal_nav_key(&key, ctx);
                return;
            }
            // Other buttons (e.g. start = interact_mode) still dispatch so
            // the mode can be toggled off from the pad.
        }

        let Some(name) = gamepad_button_name(button) else {
            return;
        };
        if shift_held {
            // Shift layer: strictly the [controller_shift] table (no
            // fall-through — holding shift means "the other bank").
            if let Some(binding) = self
                .app_core
                .config
                .controller_shift_binds
                .get(name)
                .cloned()
            {
                self.execute_macro_keybind(&binding, ctx);
            }
            return;
        }
        if let Some(binding) = self.app_core.config.controller_binds.get(name).cloned() {
            self.execute_macro_keybind(&binding, ctx);
        }
    }

    /// True while any button bound to `controller_shift` is held.
    fn gamepad_shift_held(&self) -> bool {
        self.gamepad_action_button_held("controller_shift")
    }

    /// The wheel key of the wheel button currently held, if any:
    /// "" for `controller_wheel` (the default wheel), "<name>" for
    /// `controller_wheel:<name>`.
    fn held_wheel_key(&self) -> Option<String> {
        let gilrs = self.gamepad.as_ref()?;
        for (button_name, action) in &self.app_core.config.controller_binds {
            let crate::config::KeyBindAction::Action(name) = action else {
                continue;
            };
            let key = if name == "controller_wheel" {
                ""
            } else if let Some(rest) = name.strip_prefix("controller_wheel:") {
                rest
            } else {
                continue;
            };
            let Some(button) = gamepad_button_from_name(button_name) else {
                continue;
            };
            if gilrs.gamepads().any(|(_, pad)| pad.is_pressed(button)) {
                return Some(key.to_string());
            }
        }
        None
    }

    /// The slice list at a folder path within a named wheel. Key "" is
    /// the default wheel ([[controller_wheel]], falling back to
    /// [controller_wheels.default]).
    fn wheel_level_slices(
        &self,
        key: &str,
        path: &[usize],
    ) -> Option<&Vec<crate::config::WheelSlice>> {
        let mut level = if key.is_empty() {
            if self.app_core.config.controller_wheel.is_empty() {
                self.app_core.config.controller_wheels.get("default")?
            } else {
                &self.app_core.config.controller_wheel
            }
        } else {
            self.app_core.config.controller_wheels.get(key)?
        };
        for &index in path {
            level = &level.get(index)?.slices;
        }
        Some(level)
    }

    /// True while any button base-bound to the named action is held. Read
    /// from live pad state — no held/released bookkeeping to desync.
    fn gamepad_action_button_held(&self, action_name: &str) -> bool {
        let Some(gilrs) = self.gamepad.as_ref() else {
            return false;
        };
        let buttons: Vec<gilrs::Button> = self
            .app_core
            .config
            .controller_binds
            .iter()
            .filter(|(_, action)| {
                matches!(action, crate::config::KeyBindAction::Action(name) if name == action_name)
            })
            .filter_map(|(name, _)| gamepad_button_from_name(name))
            .collect();
        if buttons.is_empty() {
            return false;
        }
        gilrs
            .gamepads()
            .any(|(_, pad)| buttons.iter().any(|b| pad.is_pressed(*b)))
    }
}

/// Live radial-wheel state: which named wheel is up, the folder path
/// descended so far, and the aimed slice at the current level.
pub(super) struct WheelUi {
    pub(super) key: String,
    pub(super) path: Vec<usize>,
    pub(super) aimed: Option<usize>,
}

impl VellumGuiApp {
    /// Draw the radial command wheel while its button is held: a ring of
    /// slice labels around the screen center, the aimed slice highlighted.
    pub(super) fn render_controller_wheel(&mut self, ctx: &egui::Context) {
        let Some((key, path, selected)) = self
            .gp_wheel
            .as_ref()
            .map(|ui| (ui.key.clone(), ui.path.clone(), ui.aimed))
        else {
            return;
        };
        let Some(slices) = self.wheel_level_slices(&key, &path).cloned() else {
            return;
        };
        let slices = &slices;
        if slices.is_empty() {
            return;
        }
        let in_folder = !path.is_empty();
        let center = ctx.content_rect().center();
        let radius = (ctx.content_rect().height() * 0.28).clamp(90.0, 220.0);

        egui::Area::new(egui::Id::new("controller_wheel"))
            .order(egui::Order::Foreground)
            .fixed_pos(center - egui::vec2(radius + 70.0, radius + 40.0))
            .interactable(false)
            .show(ctx, |ui| {
                let painter = ui.painter();
                let bg = ui.visuals().window_fill.gamma_multiply(0.92);
                let outer = radius + 34.0;
                let hub = 46.0;
                painter.circle_filled(center, outer, bg);
                painter.circle_stroke(
                    center,
                    outer,
                    egui::Stroke::new(1.0, ui.visuals().window_stroke.color),
                );
                let step = std::f32::consts::TAU / slices.len() as f32;

                // Wedge fills: the whole pie piece carries the slice's
                // color (dim at rest, bright while aimed); colorless
                // slices highlight with the theme selection fill.
                for (i, slice) in slices.iter().enumerate() {
                    let center_angle = i as f32 * step - std::f32::consts::FRAC_PI_2;
                    let is_selected = selected == Some(i);
                    let tint = slice
                        .color
                        .as_deref()
                        .and_then(super::theme::resolve_color);
                    let fill = match (tint, is_selected) {
                        (Some(c), true) => c.gamma_multiply(0.85),
                        (Some(c), false) => c.gamma_multiply(0.22),
                        (None, true) => ui.visuals().selection.bg_fill,
                        (None, false) => egui::Color32::TRANSPARENT,
                    };
                    if fill != egui::Color32::TRANSPARENT {
                        if slices.len() == 1 {
                            painter.circle_filled(center, outer, fill);
                        } else {
                            let a0 = center_angle - step / 2.0;
                            let a1 = center_angle + step / 2.0;
                            let mut points = vec![center];
                            const ARC_STEPS: usize = 16;
                            for k in 0..=ARC_STEPS {
                                let a = a0 + (a1 - a0) * k as f32 / ARC_STEPS as f32;
                                points.push(center + egui::vec2(a.cos(), a.sin()) * outer);
                            }
                            painter.add(egui::Shape::convex_polygon(
                                points,
                                fill,
                                egui::Stroke::NONE,
                            ));
                        }
                    }
                }

                // Wedge separators + labels over the fills.
                for (i, slice) in slices.iter().enumerate() {
                    let center_angle = i as f32 * step - std::f32::consts::FRAC_PI_2;
                    if slices.len() > 1 {
                        let a0 = center_angle - step / 2.0;
                        let dir = egui::vec2(a0.cos(), a0.sin());
                        painter.line_segment(
                            [center + dir * hub, center + dir * outer],
                            egui::Stroke::new(1.0, ui.visuals().window_stroke.color),
                        );
                    }
                    let pos = center + egui::vec2(center_angle.cos(), center_angle.sin()) * radius;
                    let is_selected = selected == Some(i);
                    let (color, size) = if is_selected {
                        (ui.visuals().strong_text_color(), 18.0)
                    } else {
                        (ui.visuals().text_color(), 14.0)
                    };
                    let label = if slice.is_folder() {
                        format!("{} ▸", slice.label)
                    } else {
                        slice.label.clone()
                    };
                    painter.text(
                        pos,
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(size),
                        color,
                    );
                }

                // Center hub hosts the hint text.
                painter.circle_filled(center, hub, bg);
                let hint = match selected.and_then(|i| slices.get(i)) {
                    Some(slice) if slice.is_folder() => format!("{}: A opens", slice.label),
                    Some(slice) => slice.command.clone(),
                    None if in_folder => "aim · A picks · B backs up".to_string(),
                    None => "aim with the left stick".to_string(),
                };
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    hint,
                    egui::FontId::proportional(13.0),
                    ui.visuals().weak_text_color(),
                );
            });
    }
}

/// Which wheel slice the stick is aiming at: slice 0 centered at the top,
/// clockwise. None inside the dead zone or with no slices.
fn wheel_slice_at(x: f32, y_up: f32, count: usize) -> Option<usize> {
    if count == 0 || (x * x + y_up * y_up).sqrt() < 0.5 {
        return None;
    }
    let step = 360.0 / count as f32;
    let angle = x.atan2(y_up).to_degrees();
    Some((((angle + 360.0 + step / 2.0) / step) as usize) % count)
}

/// Reverse of `gamepad_button_name`, for config-driven button lookups.
pub(super) fn gamepad_button_from_name(name: &str) -> Option<gilrs::Button> {
    use gilrs::Button;
    Some(match name {
        "south" => Button::South,
        "east" => Button::East,
        "north" => Button::North,
        "west" => Button::West,
        "dpad_up" => Button::DPadUp,
        "dpad_down" => Button::DPadDown,
        "dpad_left" => Button::DPadLeft,
        "dpad_right" => Button::DPadRight,
        "l1" => Button::LeftTrigger,
        "l2" => Button::LeftTrigger2,
        "r1" => Button::RightTrigger,
        "r2" => Button::RightTrigger2,
        "l3" => Button::LeftThumb,
        "r3" => Button::RightThumb,
        "select" => Button::Select,
        "start" => Button::Start,
        "guide" => Button::Mode,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sectors_hit_the_eight_compass_points() {
        // (x, y_up) -> expected index into STICK_DIRS
        let cases = [
            ((0.0, 1.0), 0),   // n
            ((0.7, 0.7), 1),   // ne
            ((1.0, 0.0), 2),   // e
            ((0.7, -0.7), 3),  // se
            ((0.0, -1.0), 4),  // s
            ((-0.7, -0.7), 5), // sw
            ((-1.0, 0.0), 6),  // w
            ((-0.7, 0.7), 7),  // nw
        ];
        for ((x, y), expected) in cases {
            assert_eq!(stick_sector(x, y, None), Some(expected), "({x},{y})");
        }
    }

    #[test]
    fn dead_zone_and_hysteresis() {
        // Below deflect threshold: nothing registers.
        assert_eq!(stick_sector(0.3, 0.3, None), None);
        // Held sector survives dropping below deflect but above release.
        assert_eq!(stick_sector(0.0, 0.5, Some(0)), Some(0));
        // Below release: re-armed.
        assert_eq!(stick_sector(0.0, 0.2, Some(0)), None);
    }

    #[test]
    fn button_names_round_trip_the_editor_list() {
        use gilrs::Button;
        for button in [
            Button::South, Button::East, Button::North, Button::West,
            Button::DPadUp, Button::DPadDown, Button::DPadLeft, Button::DPadRight,
            Button::LeftTrigger, Button::LeftTrigger2, Button::RightTrigger,
            Button::RightTrigger2, Button::LeftThumb, Button::RightThumb,
            Button::Select, Button::Start, Button::Mode,
        ] {
            let name = gamepad_button_name(button).expect("named button");
            assert!(GAMEPAD_BUTTON_NAMES.contains(&name), "{name} missing from editor list");
        }
    }
}

#[cfg(test)]
mod wheel_tests {
    use super::*;

    #[test]
    fn wheel_slices_map_clockwise_from_top() {
        // 8 slices: up = 0, right = 2, down = 4, left = 6.
        assert_eq!(wheel_slice_at(0.0, 1.0, 8), Some(0));
        assert_eq!(wheel_slice_at(1.0, 0.0, 8), Some(2));
        assert_eq!(wheel_slice_at(0.0, -1.0, 8), Some(4));
        assert_eq!(wheel_slice_at(-1.0, 0.0, 8), Some(6));
        // Dead zone and empty wheel.
        assert_eq!(wheel_slice_at(0.2, 0.2, 8), None);
        assert_eq!(wheel_slice_at(0.0, 1.0, 0), None);
        // Odd slice counts still cover the full circle.
        assert_eq!(wheel_slice_at(0.0, 1.0, 3), Some(0));
    }
}

impl VellumGuiApp {
    /// Binding-legend overlay: a compact right-edge panel listing the
    /// curated entries from [controller_overlay] (base and shift/
    /// bindings), with the shift bank marked. Toggled by the
    /// controller_overlay action (Select by default).
    pub(super) fn render_controller_overlay(&mut self, ctx: &egui::Context) {
        if !self.gp_overlay {
            return;
        }
        let entries = self.app_core.config.controller_overlay.clone();
        if entries.is_empty() {
            return;
        }
        let shift_held = self.gamepad_shift_held();
        egui::Area::new(egui::Id::new("controller_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-12.0, 0.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.strong("Controller");
                    for entry in &entries {
                        let (is_shift, button) = match entry.strip_prefix("shift/") {
                            Some(rest) => (true, rest),
                            None => (false, entry.as_str()),
                        };
                        let binds = if is_shift {
                            &self.app_core.config.controller_shift_binds
                        } else {
                            &self.app_core.config.controller_binds
                        };
                        let Some(action) = binds.get(button) else {
                            continue;
                        };
                        let what = match action {
                            crate::config::KeyBindAction::Action(name) => name.clone(),
                            crate::config::KeyBindAction::Macro(m) => {
                                m.macro_text.replace(['\r', '\n'], " ").trim().to_string()
                            }
                        };
                        let line = if is_shift {
                            format!("[shift] {} - {}", button, what)
                        } else {
                            format!("{} - {}", button, what)
                        };
                        // The active bank reads strong while shift is held.
                        if is_shift == shift_held {
                            ui.label(egui::RichText::new(line).strong());
                        } else {
                            ui.weak(line);
                        }
                    }
                });
            });
    }
}
