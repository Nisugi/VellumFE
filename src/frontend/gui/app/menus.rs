//! Popup menu and window context menu rendering for the GUI.
//!
//! Pure-move extraction from `app.rs`: the four-layer popup menu stack
//! (main/submenu/nested/deep), menu command handling, and the per-window
//! context menu (hide/detach/title bar/move).

use super::*;

#[derive(Clone, Copy, Debug)]
enum GuiMenuLayer {
    Main,
    Submenu,
    Nested,
    Deep,
}

#[derive(Clone, Debug)]
pub(super) struct GuiMenuCommand {
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
    /// The window's rendered rect at the time of the right-click; seeds the
    /// stored rect for Move mode when the window was never moved before.
    pub(super) window_rect: Rect,
}

#[derive(Clone, Debug)]
enum GuiWindowMenuCommand {
    Hide,
    Detach,
    /// Start Move mode: the window follows the cursor (title bar stays as-is)
    /// until a click places it or Esc cancels.
    StartMove,
    ToggleTitleBar,
    MoveUp,
    MoveDown,
    MoveTo(GuiShellZone),
    /// Per-window text size override; None reverts to the global size.
    /// Unlike the other commands this keeps the menu open (slider drags
    /// emit it every frame the value changes).
    SetTextSize(Option<f32>),
    /// Wrap long lines at the window edge; false = horizontal scrolling.
    SetWrapText(bool),
    /// Per-window font by system family name; None reverts to the default.
    SetFont(Option<String>),
    /// Per-window border accent color; None reverts to the theme border.
    SetAccent(Option<[u8; 3]>),
    /// Lock this window together with another one.
    GroupWith(TabKey),
    /// Remove this window from its group.
    Ungroup,
    /// Group layout: true = side by side, false = stacked.
    SetGroupOrientation(bool),
}

/// Everything the window context menu needs to render, resolved up front so
/// the menu body stays a static fn.
struct WindowMenuView<'a> {
    zone: GuiShellZone,
    allow_reorder: bool,
    title_bar_hidden: bool,
    text_size_override: Option<f32>,
    global_text_size: f32,
    /// Wrap toggle: shown only for text-list widgets; current value.
    supports_wrap: bool,
    wrap_text: bool,
    current_font: Option<&'a str>,
    accent_color: Option<Color32>,
    /// None = not grouped; Some(horizontal) = grouped with this orientation.
    group_horizontal: Option<bool>,
    /// Windows this one could be grouped with (visible, ungrouped).
    group_candidates: &'a [(TabKey, String)],
}

/// Preset border accent colors offered in the window context menu.
const ACCENT_PALETTE: [[u8; 3]; 8] = [
    [0xcd, 0x4d, 0x4d], // red
    [0xc0, 0x7f, 0x3f], // orange
    [0xcb, 0xa9, 0x42], // gold
    [0x55, 0xb8, 0x6c], // green
    [0x3f, 0xa7, 0xa0], // teal
    [0x47, 0x84, 0xd9], // blue
    [0x8f, 0x6f, 0xd0], // purple
    [0xd0, 0x6f, 0xa8], // pink
];

impl VellumGuiApp {
    pub(super) fn close_all_popup_menus(&mut self) {
        self.app_core.ui_state.popup_menu = None;
        self.app_core.ui_state.submenu = None;
        self.app_core.ui_state.nested_submenu = None;
        self.app_core.ui_state.deep_submenu = None;
        self.popup_menu_host = None;
    }

    fn apply_window_menu_command(
        &mut self,
        request: &GuiWindowMenuRequest,
        command: GuiWindowMenuCommand,
    ) {
        match command {
            GuiWindowMenuCommand::Hide => {
                // Hiding a grouped window hides the whole group; otherwise
                // the group would keep rendering without its leader.
                let members = self
                    .group_for_tab(&request.tab_key)
                    .map(|group| group.members.clone());
                match members {
                    Some(members) => {
                        for member in members {
                            self.hide_tab(member);
                        }
                    }
                    None => self.hide_tab(request.tab_key.clone()),
                }
            }
            GuiWindowMenuCommand::Detach => self.detach_tab(request.tab_key.clone()),
            GuiWindowMenuCommand::StartMove => {
                // Windows that were never repositioned have no stored rect;
                // seed it from where the window actually rendered.
                self.main_window_rects
                    .entry(request.tab_key.clone())
                    .or_insert(Self::rect_to_snapshot(request.window_rect));
                self.window_move_state = Some(GuiWindowMoveState {
                    tab_key: request.tab_key.clone(),
                    original_rect: self.main_window_rects.get(&request.tab_key).copied(),
                    original_order: None,
                    just_started: true,
                });
            }
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
            GuiWindowMenuCommand::SetTextSize(size) => {
                self.tab_settings
                    .entry(request.tab_key.clone())
                    .or_default()
                    .text_size = size;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetWrapText(wrap) => {
                self.tab_settings
                    .entry(request.tab_key.clone())
                    .or_default()
                    .wrap_text = wrap;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetFont(name) => {
                self.tab_settings
                    .entry(request.tab_key.clone())
                    .or_default()
                    .font_primary = match name {
                    Some(name) => FontRef::Named(name),
                    None => FontRef::SystemDefault,
                };
                // Rebuild font definitions so the new family is registered.
                self.fonts_applied = false;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetAccent(color) => {
                self.tab_settings
                    .entry(request.tab_key.clone())
                    .or_default()
                    .accent_color =
                    color.map(|[r, g, b]| format!("#{:02x}{:02x}{:02x}", r, g, b));
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::GroupWith(other) => {
                self.group_tabs(&request.tab_key.clone(), other);
            }
            GuiWindowMenuCommand::Ungroup => self.ungroup_tab(&request.tab_key.clone()),
            GuiWindowMenuCommand::SetGroupOrientation(horizontal) => {
                if let Some(group) = self
                    .tab_groups
                    .iter_mut()
                    .find(|group| group.members.contains(&request.tab_key))
                {
                    if group.horizontal != horizontal {
                        group.horizontal = horizontal;
                        self.layout_dirty = true;
                    }
                }
            }
        }
    }

    pub(super) fn render_window_context_popup(&mut self, ctx: &egui::Context) {
        let Some(request) = self.window_context_menu.clone() else {
            return;
        };

        let text_size_override = self
            .tab_settings
            .get(&request.tab_key)
            .and_then(|settings| settings.text_size);
        let current_font = self
            .tab_settings
            .get(&request.tab_key)
            .and_then(|settings| match &settings.font_primary {
                FontRef::Named(name) => Some(name.clone()),
                _ => None,
            });
        let detached_tabs = self.detached_tab_keys();
        let mut group_candidates: Vec<(TabKey, String)> = self
            .available_tabs
            .iter()
            .filter(|(key, _)| **key != request.tab_key)
            .filter(|(key, _)| !self.hidden_tabs.contains(*key))
            .filter(|(key, _)| !detached_tabs.contains(*key))
            .filter(|(key, _)| self.group_for_tab(key).is_none())
            .map(|(key, tab)| (key.clone(), tab.id.title.clone()))
            .collect();
        group_candidates.sort_by(|a, b| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase()));
        let supports_wrap = self
            .available_tabs
            .get(&request.tab_key)
            .and_then(|tab| self.app_core.ui_state.windows.get(&tab.window_name))
            .map(|window| {
                matches!(
                    window.widget_type,
                    WidgetType::Text
                        | WidgetType::TabbedText
                        | WidgetType::Inventory
                        | WidgetType::Spells
                        | WidgetType::Container
                )
            })
            .unwrap_or(false);
        let wrap_text = self
            .tab_settings
            .get(&request.tab_key)
            .map(|settings| settings.wrap_text)
            .unwrap_or(true);
        let view = WindowMenuView {
            zone: request.zone,
            allow_reorder: request.allow_reorder,
            title_bar_hidden: request.title_bar_hidden,
            text_size_override,
            global_text_size: self.ui_settings.text_size,
            supports_wrap,
            wrap_text,
            current_font: current_font.as_deref(),
            accent_color: self.accent_color_for_tab(&request.tab_key),
            group_horizontal: self
                .group_for_tab(&request.tab_key)
                .map(|group| group.horizontal),
            group_candidates: &group_candidates,
        };

        let mut selected_command: Option<GuiWindowMenuCommand> = None;
        let area_response = egui::Area::new(egui::Id::new("gui_window_context_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(request.position)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    selected_command = Self::render_window_context_menu(ui, &view);
                });
            });

        if let Some(command) = selected_command {
            // Live-adjustable settings keep the menu open so their controls
            // stay usable across repeated changes.
            let keep_open = matches!(
                command,
                GuiWindowMenuCommand::SetTextSize(_)
                    | GuiWindowMenuCommand::SetWrapText(_)
                    | GuiWindowMenuCommand::SetAccent(_)
                    | GuiWindowMenuCommand::SetGroupOrientation(_)
            );
            self.apply_window_menu_command(&request, command);
            if !keep_open {
                self.window_context_menu = None;
                return;
            }
        }

        // The click that opened the menu is still visible in this frame's
        // input; never let it count as a click-outside.
        if std::mem::take(&mut self.window_context_menu_just_opened) {
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

    pub(super) fn should_close_popup_menus_on_outside_click(
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

        if let Some(widget_type) = command.strip_prefix("__ADD_CUSTOM__") {
            // "Custom (blank)" menu items carry a widget type, not a template
            // name. Route to the matching `*_custom` blank template — its
            // add path drops the user into the window editor to configure it.
            let template = crate::config::Config::list_window_templates()
                .into_iter()
                .find(|name| {
                    name.ends_with("_custom")
                        && crate::config::Config::get_window_template(name)
                            .is_some_and(|t| t.widget_type().eq_ignore_ascii_case(widget_type))
                });
            match template {
                Some(template) => self.add_window_from_template(&template),
                None => self.app_core.add_system_message(&format!(
                    "No blank '{}' template exists; use .addwindow <name> {} <x> <y> <width> [height].",
                    widget_type, widget_type
                )),
            }
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

    /// Render the four popup-menu layers against `ctx`'s current viewport.
    /// Immutable so it can run inside a detached viewport's pass; returns
    /// the clicked command and whether an outside click should close the
    /// stack. Callers apply both with `&mut self`.
    pub(super) fn render_popup_menu_layers(
        ctx: &egui::Context,
        app_core: &AppCore,
    ) -> (Option<GuiMenuCommand>, bool) {
        let main = app_core.ui_state.popup_menu.as_ref();
        let submenu = app_core.ui_state.submenu.as_ref();
        let nested = app_core.ui_state.nested_submenu.as_ref();
        let deep = app_core.ui_state.deep_submenu.as_ref();

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

        if clicked_command.is_some() {
            return (clicked_command, false);
        }

        let should_close = ctx.input(|input| {
            Self::should_close_popup_menus_on_outside_click(
                input.pointer.any_click(),
                input.pointer.latest_pos(),
                &menu_rects,
            )
        });
        (None, should_close)
    }

    pub(super) fn apply_popup_menu_layer_result(
        &mut self,
        clicked_command: Option<GuiMenuCommand>,
        should_close: bool,
    ) {
        if let Some(command) = clicked_command {
            self.handle_popup_menu_command(command);
            return;
        }
        if should_close {
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
        }
    }

    pub(super) fn render_popup_menus(&mut self, ctx: &egui::Context) {
        // Menus requested from a detached window render inside that
        // viewport's pass (see render_detached_viewport_contents), at the
        // click position in that window's own coordinates.
        if let Some(host) = &self.popup_menu_host {
            if self.detached_tabs.contains_key(host) {
                return;
            }
        }
        let (clicked_command, should_close) =
            Self::render_popup_menu_layers(ctx, &self.app_core);
        self.apply_popup_menu_layer_result(clicked_command, should_close);
    }

    fn render_window_context_menu(
        ui: &mut egui::Ui,
        view: &WindowMenuView<'_>,
    ) -> Option<GuiWindowMenuCommand> {
        if ui.button("Hide").clicked() {
            return Some(GuiWindowMenuCommand::Hide);
        }
        if ui.button("Detach").clicked() {
            return Some(GuiWindowMenuCommand::Detach);
        }
        if ui.button("Move Window").clicked() {
            return Some(GuiWindowMenuCommand::StartMove);
        }
        if ui
            .button(if view.title_bar_hidden {
                "Show Title Bar"
            } else {
                "Hide Title Bar"
            })
            .clicked()
        {
            return Some(GuiWindowMenuCommand::ToggleTitleBar);
        }
        if view.allow_reorder {
            ui.separator();
            if ui.button("Move Up").clicked() {
                return Some(GuiWindowMenuCommand::MoveUp);
            }
            if ui.button("Move Down").clicked() {
                return Some(GuiWindowMenuCommand::MoveDown);
            }
        }
        ui.separator();
        let mut settings_command = None;
        let mut override_enabled = view.text_size_override.is_some();
        if ui
            .checkbox(&mut override_enabled, "Custom text size")
            .changed()
        {
            settings_command = Some(GuiWindowMenuCommand::SetTextSize(if override_enabled {
                Some(view.text_size_override.unwrap_or(view.global_text_size))
            } else {
                None
            }));
        }
        if let Some(current) = view.text_size_override {
            let mut value = current;
            if ui
                .add(egui::Slider::new(&mut value, 8.0..=32.0).step_by(0.5))
                .changed()
            {
                settings_command = Some(GuiWindowMenuCommand::SetTextSize(Some(value)));
            }
        }
        if view.supports_wrap {
            let mut wrap = view.wrap_text;
            if ui.checkbox(&mut wrap, "Word wrap").changed() {
                settings_command = Some(GuiWindowMenuCommand::SetWrapText(wrap));
            }
        }
        ui.collapsing("Font", |ui| {
            // Filter box: system font lists run to hundreds of families.
            let filter_id = egui::Id::new("gui_window_font_filter");
            let mut filter: String = ui.data_mut(|data| data.get_temp(filter_id).unwrap_or_default());
            if ui
                .add(egui::TextEdit::singleline(&mut filter).hint_text("Filter fonts"))
                .changed()
            {
                ui.data_mut(|data| data.insert_temp(filter_id, filter.clone()));
            }
            let filter_lower = filter.to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("gui_window_font_list")
                .max_height(180.0)
                .show(ui, |ui| {
                    if ui
                        .selectable_label(view.current_font.is_none(), "Default")
                        .clicked()
                    {
                        settings_command = Some(GuiWindowMenuCommand::SetFont(None));
                    }
                    for family in theme::system_font_families() {
                        if !filter_lower.is_empty()
                            && !family.to_lowercase().contains(&filter_lower)
                        {
                            continue;
                        }
                        let selected = view.current_font == Some(family.as_str());
                        if ui.selectable_label(selected, family).clicked() {
                            settings_command =
                                Some(GuiWindowMenuCommand::SetFont(Some(family.clone())));
                        }
                    }
                });
        });
        ui.separator();
        ui.label("Accent color");
        ui.horizontal(|ui| {
            for color in ACCENT_PALETTE {
                let fill = Color32::from_rgb(color[0], color[1], color[2]);
                let selected = view.accent_color == Some(fill);
                let mut button = egui::Button::new("  ").fill(fill);
                if selected {
                    button = button.stroke(egui::Stroke::new(2.0, ui.visuals().text_color()));
                }
                if ui.add(button).clicked() {
                    settings_command = Some(GuiWindowMenuCommand::SetAccent(Some(color)));
                }
            }
            if view.accent_color.is_some() && ui.small_button("✕").clicked() {
                settings_command = Some(GuiWindowMenuCommand::SetAccent(None));
            }
        });
        ui.separator();
        if let Some(horizontal) = view.group_horizontal {
            ui.label("Grouped window");
            ui.horizontal(|ui| {
                if ui.selectable_label(!horizontal, "Stacked").clicked() && horizontal {
                    settings_command = Some(GuiWindowMenuCommand::SetGroupOrientation(false));
                }
                if ui.selectable_label(horizontal, "Side by side").clicked() && !horizontal {
                    settings_command = Some(GuiWindowMenuCommand::SetGroupOrientation(true));
                }
            });
            if ui.button("Ungroup").clicked() {
                return Some(GuiWindowMenuCommand::Ungroup);
            }
        }
        if !view.group_candidates.is_empty() {
            let mut group_command = None;
            ui.collapsing("Group with", |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("gui_window_group_list")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for (key, title) in view.group_candidates {
                            if ui.button(title).clicked() {
                                group_command =
                                    Some(GuiWindowMenuCommand::GroupWith(key.clone()));
                            }
                        }
                    });
            });
            if group_command.is_some() {
                return group_command;
            }
        }
        ui.separator();
        ui.label("Move to");
        for target in GuiShellZone::all() {
            let is_current = target == view.zone;
            let label = if is_current {
                format!("{} (current)", target.label())
            } else {
                target.label().to_string()
            };
            if ui.selectable_label(is_current, label).clicked() {
                return Some(GuiWindowMenuCommand::MoveTo(target));
            }
        }
        // Returned last (not early) so the menu keeps its full layout while
        // the slider or palette is being used.
        settings_command
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
