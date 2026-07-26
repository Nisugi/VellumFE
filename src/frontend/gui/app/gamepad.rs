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

        // Radial wheel: while the wheel button is held, the left stick
        // aims a slice; releasing fires it. Owns the stick while active
        // (no movement) and swallows other buttons.
        let wheel_held = self.gamepad_action_button_held("controller_wheel");
        match (self.gp_wheel.is_some(), wheel_held) {
            (false, true) => {
                self.gp_wheel = Some(None);
                self.app_core.needs_render = true;
            }
            (true, false) => {
                let selected = self.gp_wheel.take().flatten();
                if let Some(index) = selected {
                    if let Some(slice) = self.app_core.config.controller_wheel.get(index).cloned()
                    {
                        self.dispatch_command(slice.command);
                    }
                }
                self.app_core.needs_render = true;
            }
            (true, true) => {
                if let (Some((x, y_up)), Some(slot)) = (stick, self.gp_wheel.as_mut()) {
                    let count = self.app_core.config.controller_wheel.len();
                    let aimed = wheel_slice_at(x, y_up, count);
                    if aimed.is_some() && *slot != aimed {
                        *slot = aimed;
                        self.app_core.needs_render = true;
                    }
                }
            }
            (false, false) => {}
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

        // The wheel owns the pad while it is up: buttons neither navigate
        // nor dispatch (release of the wheel button fires the slice).
        if self.gp_wheel.is_some() {
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

impl VellumGuiApp {
    /// Draw the radial command wheel while its button is held: a ring of
    /// slice labels around the screen center, the aimed slice highlighted.
    pub(super) fn render_controller_wheel(&mut self, ctx: &egui::Context) {
        let Some(selected) = self.gp_wheel else {
            return;
        };
        let slices = &self.app_core.config.controller_wheel;
        if slices.is_empty() {
            return;
        }
        let center = ctx.content_rect().center();
        let radius = (ctx.content_rect().height() * 0.28).clamp(90.0, 220.0);

        egui::Area::new(egui::Id::new("controller_wheel"))
            .order(egui::Order::Foreground)
            .fixed_pos(center - egui::vec2(radius + 70.0, radius + 40.0))
            .interactable(false)
            .show(ctx, |ui| {
                let painter = ui.painter();
                let bg = ui.visuals().window_fill.gamma_multiply(0.92);
                painter.circle_filled(center, radius + 34.0, bg);
                painter.circle_stroke(
                    center,
                    radius + 34.0,
                    egui::Stroke::new(1.0, ui.visuals().window_stroke.color),
                );
                let step = std::f32::consts::TAU / slices.len() as f32;
                for (i, slice) in slices.iter().enumerate() {
                    // Slice 0 at the top, clockwise (matches wheel_slice_at).
                    let angle = i as f32 * step - std::f32::consts::FRAC_PI_2;
                    let pos = center + egui::vec2(angle.cos(), angle.sin()) * radius;
                    let is_selected = selected == Some(i);
                    let (color, size) = if is_selected {
                        (ui.visuals().selection.stroke.color, 18.0)
                    } else {
                        (ui.visuals().text_color(), 14.0)
                    };
                    if is_selected {
                        painter.circle_filled(pos, 26.0, ui.visuals().selection.bg_fill);
                    }
                    painter.text(
                        pos,
                        egui::Align2::CENTER_CENTER,
                        &slice.label,
                        egui::FontId::proportional(size),
                        color,
                    );
                }
                let hint = match selected {
                    Some(i) => slices
                        .get(i)
                        .map(|s| s.command.clone())
                        .unwrap_or_default(),
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
