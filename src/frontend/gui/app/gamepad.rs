//! Native gamepad input (gilrs) for the GUI — controller Tier 1, first
//! slice: a fixed mapping that drives interact mode and the popup menus.
//!
//! D-pad = arrows, South (A/cross) = Enter, East (B/circle) = Esc while
//! interact mode or a menu is open; Start toggles interact mode from
//! anywhere. The configurable `controllerbinds` profile and the richer
//! tiers (layers, radial wheel, rumble) build on this once the input
//! path is proven on real hardware.
//!
//! Backend notes: Windows uses Windows Gaming Input (needs a focused
//! window — true for the GUI), Linux evdev, macOS IOKit (verify on
//! hardware; may need a GameController-framework bridge).

use super::VellumGuiApp;
use crate::data::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::data::ui_state::InputMode;
use eframe::egui;

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

        // Start toggles interact mode from anywhere — the controller's
        // entry point into the pointer-free flow.
        if button == Button::Start {
            self.app_core.toggle_interact_mode();
            return;
        }

        // While interact mode or a popup menu is up, the d-pad and face
        // buttons behave like the keyboard navigation keys.
        let modal = matches!(
            self.app_core.ui_state.input_mode,
            InputMode::Interact | InputMode::Menu
        );
        if !modal {
            return;
        }
        let code = match button {
            Button::DPadUp => KeyCode::Up,
            Button::DPadDown => KeyCode::Down,
            Button::DPadLeft => KeyCode::Left,
            Button::DPadRight => KeyCode::Right,
            Button::South => KeyCode::Enter,
            Button::East => KeyCode::Esc,
            _ => return,
        };
        let key = KeyEvent::new(code, KeyModifiers::NONE);
        self.handle_modal_nav_key(&key, ctx);
    }
}
