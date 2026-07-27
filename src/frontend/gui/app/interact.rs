//! GUI side of interact mode: modal arrow/enter/escape handling and the
//! status overlay. The focus/cycling logic lives in core
//! (`core/app_core/interact.rs`); popup-menu keyboard navigation lives
//! with the rest of the menu logic in `menus.rs`.

use super::VellumGuiApp;
use crate::data::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::data::ui_state::InputMode;
use eframe::egui;

impl VellumGuiApp {
    /// Handle plain (unmodified) navigation keys for interact mode and the
    /// popup-menu stack. Returns true when the key was consumed.
    pub(super) fn handle_modal_nav_key(&mut self, key: &KeyEvent, ctx: &egui::Context) -> bool {
        if key.modifiers != KeyModifiers::NONE {
            return false;
        }
        match self.app_core.ui_state.input_mode {
            InputMode::Menu => self.handle_menu_nav_key(key.code),
            InputMode::Interact => self.handle_interact_key(key.code, ctx),
            _ => false,
        }
    }

    fn handle_interact_key(&mut self, code: KeyCode, ctx: &egui::Context) -> bool {
        match code {
            KeyCode::Up => {
                self.app_core.interact_move(-1);
                true
            }
            KeyCode::Down => {
                self.app_core.interact_move(1);
                true
            }
            KeyCode::Left => {
                self.app_core.interact_category_move(-1);
                true
            }
            KeyCode::Right => {
                self.app_core.interact_category_move(1);
                true
            }
            KeyCode::Enter => {
                let anchor = Self::interact_menu_anchor(ctx);
                if let Some(outbound) = self.app_core.interact_activate(anchor) {
                    self.dispatch_raw_command(outbound);
                }
                true
            }
            KeyCode::Esc => {
                self.app_core.exit_interact_mode();
                true
            }
            _ => false,
        }
    }

    /// Where the server context menu should open when activation came from
    /// the keyboard: roughly screen center (no cursor position to anchor to).
    fn interact_menu_anchor(ctx: &egui::Context) -> (u16, u16) {
        let center = ctx.content_rect().center();
        (
            center.x.clamp(0.0, u16::MAX as f32) as u16,
            center.y.clamp(0.0, u16::MAX as f32) as u16,
        )
    }

    /// Mode to land in when the popup-menu stack closes: back to interact
    /// mode if it is still active underneath, else normal input.
    pub(super) fn input_mode_after_menu_close(&self) -> InputMode {
        if self.app_core.ui_state.interact.is_some() {
            InputMode::Interact
        } else {
            InputMode::Normal
        }
    }

    /// Bottom-center status overlay while interact mode is active.
    pub(super) fn render_interact_overlay(&mut self, ctx: &egui::Context) {
        if self.app_core.ui_state.input_mode != InputMode::Interact {
            return;
        }
        let line = self
            .app_core
            .interact_overlay_line()
            .unwrap_or_else(|| "Nothing here".to_string());

        egui::Area::new(egui::Id::new("interact_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -48.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Interact");
                        ui.separator();
                        ui.label(line);
                        ui.separator();
                        // Plain words only: the arrow glyphs (U+2190..)
                        // tofu in the shipped fonts. Covers both inputs —
                        // keyboard arrows and the pad's right stick.
                        ui.weak("arrows / right stick cycle · Enter or A select · Esc exit");
                    });
                });
            });
    }
}
