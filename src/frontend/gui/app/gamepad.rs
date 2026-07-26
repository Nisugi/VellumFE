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

        // gilrs events arrive outside egui's own event loop; without a
        // wake-up an idle GUI would stop polling. Cheap while connected.
        if any_connected {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn handle_gamepad_button(&mut self, button: gilrs::Button, ctx: &egui::Context) {
        use gilrs::Button;

        // While interact mode or a popup menu is up, the d-pad and the
        // confirm/cancel face buttons are fixed navigation keys.
        let modal = matches!(
            self.app_core.ui_state.input_mode,
            InputMode::Interact | InputMode::Menu
        );
        if modal {
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
        if let Some(binding) = self.app_core.config.controller_binds.get(name).cloned() {
            self.execute_macro_keybind(&binding);
        }
    }
}
