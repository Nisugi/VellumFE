//! Popup menu and window context menu rendering for the GUI.
//!
//! Pure-move extraction from `app.rs`: the four-layer popup menu stack
//! (main/submenu/nested/deep), menu command handling, and the per-window
//! context menu (hide/eject/title bar/move).

use super::*;

#[derive(Clone, Copy, Debug)]
enum GuiMenuLayer {
    Main,
    Submenu,
    Nested,
    Deep,
}

#[derive(Clone, Debug)]
struct GuiMenuCommand {
    layer: GuiMenuLayer,
    command: String,
}

#[derive(Clone, Debug)]
pub(super) struct GuiWindowMenuRequest {
    pub(super) tab_key: TabKey,
    pub(super) zone: GuiShellZone,
    pub(super) allow_reorder: bool,
    pub(super) title_bar_hidden: bool,
    pub(super) position: Pos2,
}

#[derive(Clone, Copy, Debug)]
enum GuiWindowMenuCommand {
    Hide,
    Eject,
    ToggleTitleBar,
    MoveUp,
    MoveDown,
    MoveTo(GuiShellZone),
}

impl VellumGuiApp {
    pub(super) fn close_all_popup_menus(&mut self) {
        self.app_core.ui_state.popup_menu = None;
        self.app_core.ui_state.submenu = None;
        self.app_core.ui_state.nested_submenu = None;
        self.app_core.ui_state.deep_submenu = None;
    }

    fn apply_window_menu_command(
        &mut self,
        request: &GuiWindowMenuRequest,
        command: GuiWindowMenuCommand,
    ) {
        match command {
            GuiWindowMenuCommand::Hide => self.hide_tab(request.tab_key.clone()),
            GuiWindowMenuCommand::Eject => self.detach_tab(request.tab_key.clone()),
            GuiWindowMenuCommand::ToggleTitleBar => self.toggle_title_bar(request.tab_key.clone()),
            GuiWindowMenuCommand::MoveUp => {
                if request.allow_reorder {
                    self.move_tab_within_zone(&request.tab_key, request.zone, true);
                }
            }
            GuiWindowMenuCommand::MoveDown => {
                if request.allow_reorder {
                    self.move_tab_within_zone(&request.tab_key, request.zone, false);
                }
            }
            GuiWindowMenuCommand::MoveTo(target) => {
                if target != request.zone {
                    self.set_tab_zone(request.tab_key.clone(), target);
                }
            }
        }
    }

    pub(super) fn render_window_context_popup(&mut self, ctx: &egui::Context) {
        let Some(request) = self.window_context_menu.clone() else {
            return;
        };

        let mut selected_command: Option<GuiWindowMenuCommand> = None;
        let area_response = egui::Area::new(egui::Id::new("gui_window_context_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(request.position)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    selected_command = Self::render_window_context_menu(
                        ui,
                        request.zone,
                        request.allow_reorder,
                        request.title_bar_hidden,
                    );
                });
            });

        if let Some(command) = selected_command {
            self.apply_window_menu_command(&request, command);
            self.window_context_menu = None;
            return;
        }

        let menu_rect = area_response.response.rect;
        let should_close = ctx.input(|input| {
            input.pointer.any_click()
                && input
                    .pointer
                    .latest_pos()
                    .map(|pos| !menu_rect.contains(pos))
                    .unwrap_or(false)
        });
        if should_close {
            self.window_context_menu = None;
        }
    }

    fn render_menu_layer(
        ctx: &egui::Context,
        layer: GuiMenuLayer,
        menu: &PopupMenu,
    ) -> (Option<GuiMenuCommand>, Option<Rect>) {
        let layer_id = match layer {
            GuiMenuLayer::Main => "gui_popup_menu_main",
            GuiMenuLayer::Submenu => "gui_popup_menu_submenu",
            GuiMenuLayer::Nested => "gui_popup_menu_nested",
            GuiMenuLayer::Deep => "gui_popup_menu_deep",
        };

        let mut clicked_command: Option<String> = None;
        let pos = Pos2::new(menu.position.0 as f32, menu.position.1 as f32);
        let area_response = egui::Area::new(egui::Id::new(layer_id))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    for item in menu.get_items() {
                        let button = egui::Button::new(item.text.as_str());
                        let response = ui.add_enabled(!item.disabled, button);
                        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            clicked_command = Some(item.command.clone());
                        }
                    }
                });
            });
        let layer_rect = area_response.response.rect;

        (
            clicked_command.map(|command| GuiMenuCommand { layer, command }),
            if layer_rect.is_finite() {
                Some(layer_rect)
            } else {
                None
            },
        )
    }

    fn should_close_popup_menus_on_outside_click(
        any_click: bool,
        pointer_pos: Option<Pos2>,
        menu_rects: &[Rect],
    ) -> bool {
        if !any_click || menu_rects.is_empty() {
            return false;
        }
        let Some(pointer_pos) = pointer_pos else {
            return false;
        };

        menu_rects.iter().all(|rect| !rect.contains(pointer_pos))
    }

    fn open_child_menu_for_layer(
        &mut self,
        layer: GuiMenuLayer,
        items: Vec<crate::data::ui_state::PopupMenuItem>,
    ) {
        if items.is_empty() {
            return;
        }

        let parent_pos = match layer {
            GuiMenuLayer::Main => self
                .app_core
                .ui_state
                .popup_menu
                .as_ref()
                .map(|menu| menu.get_position()),
            GuiMenuLayer::Submenu => self
                .app_core
                .ui_state
                .submenu
                .as_ref()
                .map(|menu| menu.get_position()),
            GuiMenuLayer::Nested => self
                .app_core
                .ui_state
                .nested_submenu
                .as_ref()
                .map(|menu| menu.get_position()),
            GuiMenuLayer::Deep => self
                .app_core
                .ui_state
                .deep_submenu
                .as_ref()
                .map(|menu| menu.get_position()),
        }
        .unwrap_or((40, 12));

        let child = PopupMenu::new(items, (parent_pos.0.saturating_add(24), parent_pos.1));
        match layer {
            GuiMenuLayer::Main => {
                self.app_core.ui_state.submenu = Some(child);
                self.app_core.ui_state.nested_submenu = None;
                self.app_core.ui_state.deep_submenu = None;
            }
            GuiMenuLayer::Submenu => {
                self.app_core.ui_state.nested_submenu = Some(child);
                self.app_core.ui_state.deep_submenu = None;
            }
            GuiMenuLayer::Nested | GuiMenuLayer::Deep => {
                self.app_core.ui_state.deep_submenu = Some(child)
            }
        }
        self.app_core.ui_state.input_mode = InputMode::Menu;
    }

    fn handle_popup_menu_command(&mut self, menu_command: GuiMenuCommand) {
        let command = menu_command.command;

        if let Some(category) = command.strip_prefix("__SUBMENU__") {
            if let Some(items) = self.app_core.menu_categories.get(category).cloned() {
                self.open_child_menu_for_layer(menu_command.layer, items);
            } else {
                tracing::warn!("Missing GUI menu category: {}", category);
            }
            return;
        }

        if let Some(submenu) = command.strip_prefix("menu:") {
            let items = self.app_core.build_submenu(submenu);
            if items.is_empty() {
                self.app_core
                    .add_system_message(&format!("Menu '{}' has no entries.", submenu));
                self.close_all_popup_menus();
                self.app_core.ui_state.input_mode = InputMode::Normal;
            } else {
                self.open_child_menu_for_layer(menu_command.layer, items);
            }
            return;
        }

        if command.starts_with("action:") {
            if !self.handle_action_string(&command) {
                self.app_core
                    .add_system_message(&format!("GUI action not implemented yet: {}", command));
            }
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            return;
        }

        if command == "__INDICATOR_EDITOR" {
            self.open_indicator_templates_editor();
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            return;
        }

        if let Some(category_str) = command.strip_prefix("__SUBMENU_ADD__") {
            match crate::config::WidgetCategory::from_name(category_str) {
                Some(category) => {
                    let items = self.app_core.build_add_window_category_menu(&category);
                    if items.is_empty() {
                        self.app_core
                            .add_system_message("No windows available in that category.");
                    } else {
                        self.open_child_menu_for_layer(menu_command.layer, items);
                    }
                }
                None => {
                    tracing::warn!("Unknown widget category in menu command: {}", category_str);
                }
            }
            return;
        }

        if command == "__SUBMENU_INDICATORS" {
            let templates = crate::config::Config::get_addable_templates_by_category(
                &self.app_core.layout,
                self.app_core.game_type(),
            )
            .get(&crate::config::WidgetCategory::Status)
            .cloned()
            .unwrap_or_default();
            let items = self.app_core.build_indicator_add_menu(&templates);
            if items.is_empty() {
                self.app_core
                    .add_system_message("No indicator templates available.");
            } else {
                self.open_child_menu_for_layer(menu_command.layer, items);
            }
            return;
        }

        if let Some(template) = command.strip_prefix("__ADD__") {
            let template = template.to_string();
            self.add_window_from_template(&template);
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            return;
        }

        if command.strip_prefix("__ADD_CUSTOM__").is_some() {
            self.app_core.add_system_message(
                "Custom blank windows are not supported in the GUI yet; \
                 use .addwindow <name> <type> <x> <y> <width> [height].",
            );
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            return;
        }

        // Internal (double-underscore) menu commands must never reach the server.
        if command.starts_with("__") {
            self.app_core
                .add_system_message(&format!("GUI menu command not implemented yet: {}", command));
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
            return;
        }

        self.dispatch_raw_command(command);
        self.close_all_popup_menus();
        self.app_core.ui_state.input_mode = InputMode::Normal;
    }

    pub(super) fn render_popup_menus(&mut self, ctx: &egui::Context) {
        let main = self.app_core.ui_state.popup_menu.as_ref();
        let submenu = self.app_core.ui_state.submenu.as_ref();
        let nested = self.app_core.ui_state.nested_submenu.as_ref();
        let deep = self.app_core.ui_state.deep_submenu.as_ref();

        let mut clicked_command: Option<GuiMenuCommand> = None;
        let mut menu_rects: Vec<Rect> = Vec::new();

        if let Some(menu) = main {
            let (command, rect) = Self::render_menu_layer(ctx, GuiMenuLayer::Main, menu);
            clicked_command = command;
            if let Some(rect) = rect {
                menu_rects.push(rect);
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = submenu {
                let (command, rect) = Self::render_menu_layer(ctx, GuiMenuLayer::Submenu, menu);
                clicked_command = command;
                if let Some(rect) = rect {
                    menu_rects.push(rect);
                }
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = nested {
                let (command, rect) = Self::render_menu_layer(ctx, GuiMenuLayer::Nested, menu);
                clicked_command = command;
                if let Some(rect) = rect {
                    menu_rects.push(rect);
                }
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = deep {
                let (command, rect) = Self::render_menu_layer(ctx, GuiMenuLayer::Deep, menu);
                clicked_command = command;
                if let Some(rect) = rect {
                    menu_rects.push(rect);
                }
            }
        }

        if let Some(command) = clicked_command {
            self.handle_popup_menu_command(command);
            return;
        }

        let should_close = ctx.input(|input| {
            Self::should_close_popup_menus_on_outside_click(
                input.pointer.any_click(),
                input.pointer.latest_pos(),
                &menu_rects,
            )
        });
        if should_close {
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
        }
    }

    fn render_window_context_menu(
        ui: &mut egui::Ui,
        zone: GuiShellZone,
        allow_reorder: bool,
        title_bar_hidden: bool,
    ) -> Option<GuiWindowMenuCommand> {
        if ui.button("Hide").clicked() {
            return Some(GuiWindowMenuCommand::Hide);
        }
        if ui.button("Eject").clicked() {
            return Some(GuiWindowMenuCommand::Eject);
        }
        if ui
            .button(if title_bar_hidden {
                "Show Title Bar"
            } else {
                "Hide Title Bar"
            })
            .clicked()
        {
            return Some(GuiWindowMenuCommand::ToggleTitleBar);
        }
        if allow_reorder {
            ui.separator();
            if ui.button("Move Up").clicked() {
                return Some(GuiWindowMenuCommand::MoveUp);
            }
            if ui.button("Move Down").clicked() {
                return Some(GuiWindowMenuCommand::MoveDown);
            }
        }
        ui.separator();
        ui.label("Move to");
        for target in GuiShellZone::all() {
            let is_current = target == zone;
            let label = if is_current {
                format!("{} (current)", target.label())
            } else {
                target.label().to_string()
            };
            if ui.selectable_label(is_current, label).clicked() {
                return Some(GuiWindowMenuCommand::MoveTo(target));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::VellumGuiApp;
    use eframe::egui::{Pos2, Rect};

    #[test]
    fn test_should_close_popup_menus_on_outside_click_true() {
        let menu_rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(220.0, 180.0));
        let should_close = VellumGuiApp::should_close_popup_menus_on_outside_click(
            true,
            Some(Pos2::new(50.0, 50.0)),
            &[menu_rect],
        );
        assert!(should_close);
    }

    #[test]
    fn test_should_close_popup_menus_on_outside_click_false_for_inside_click() {
        let menu_rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(220.0, 180.0));
        let should_close = VellumGuiApp::should_close_popup_menus_on_outside_click(
            true,
            Some(Pos2::new(150.0, 120.0)),
            &[menu_rect],
        );
        assert!(!should_close);
    }

    #[test]
    fn test_should_close_popup_menus_on_outside_click_false_without_click() {
        let menu_rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(220.0, 180.0));
        let should_close = VellumGuiApp::should_close_popup_menus_on_outside_click(
            false,
            Some(Pos2::new(50.0, 50.0)),
            &[menu_rect],
        );
        assert!(!should_close);
    }
}
