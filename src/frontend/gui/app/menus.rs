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
    pub(super) position: Pos2,
    /// The window's rendered rect at the time of the right-click; seeds the
    /// stored rect for Move mode when the window was never moved before.
    pub(super) window_rect: Rect,
}

#[derive(Clone, Debug)]
pub(super) enum GuiWindowMenuCommand {
    /// Open the Window Editor on this window (title, streams, feed ids).
    Edit,
    Hide,
    Detach,
    /// Send this floating (Center-zone) window behind any windows it
    /// overlaps, so a covered window can be reached. Live-session only,
    /// like egui's built-in click-to-raise.
    SendToBack,
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
    /// Remove one specific member from this window's group.
    UngroupMember(TabKey),
    /// Break up this window's whole group.
    DissolveGroup,
    /// Open the Map Explorer native window (map widgets only).
    OpenMapExplorer,
    /// Mini map zoom in px per cell; None reverts to the widget default.
    /// Keeps the menu open like SetTextSize.
    SetMapZoom(Option<f32>),
    /// Group layout: true = side by side, false = stacked.
    SetGroupOrientation(bool),
    /// Move a group member one step up/down in the group's render order.
    MoveGroupMember { member: TabKey, up: bool },
}

/// Everything the window context menu needs to render, resolved up front so
/// the menu body stays a static fn.
struct WindowMenuView<'a> {
    zone: GuiShellZone,
    allow_reorder: bool,
    appearance: WindowAppearanceView,
    /// None = not grouped; Some(horizontal) = grouped with this orientation.
    group_horizontal: Option<bool>,
    /// Members of this window's group in render order (empty when ungrouped).
    group_members: &'a [(TabKey, String)],
    /// Windows this one could be grouped with (visible, ungrouped).
    group_candidates: &'a [(TabKey, String)],
}

/// Per-window appearance state, shared between the context menu's Appearance
/// section and the Window Editor (see `appearance_view_for_tab`).
pub(super) struct WindowAppearanceView {
    title_bar_hidden: bool,
    text_size_override: Option<f32>,
    global_text_size: f32,
    /// Wrap toggle: shown only for text-list widgets; current value.
    supports_wrap: bool,
    /// Map widget: offer "Open Map Explorer" and the zoom override.
    is_map: bool,
    /// Mini map zoom override (px per cell), when is_map.
    map_zoom: Option<f32>,
    wrap_text: bool,
    current_font: Option<String>,
    accent_color: Option<Color32>,
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

    /// Close the menu stack and drop to Normal input, abandoning any
    /// interact-mode flow (used when a menu command opens an editor or
    /// other UI that takes over input).
    fn close_menus_to_normal(&mut self) {
        self.close_all_popup_menus();
        self.app_core.ui_state.interact = None;
        self.app_core.ui_state.input_mode = InputMode::Normal;
    }

    /// Close the menu stack and return to whatever was underneath: interact
    /// mode if it is still active (the menu was opened from it), else Normal.
    fn close_menus_restore(&mut self) {
        self.close_all_popup_menus();
        self.app_core.ui_state.input_mode = self.input_mode_after_menu_close();
    }

    /// Arrow/enter/escape navigation over the deepest open popup menu.
    /// Returns true when the key was consumed.
    pub(super) fn handle_menu_nav_key(&mut self, code: crate::data::input::KeyCode) -> bool {
        use crate::data::input::KeyCode;

        let deepest = {
            let ui = &self.app_core.ui_state;
            if ui.deep_submenu.is_some() {
                Some(GuiMenuLayer::Deep)
            } else if ui.nested_submenu.is_some() {
                Some(GuiMenuLayer::Nested)
            } else if ui.submenu.is_some() {
                Some(GuiMenuLayer::Submenu)
            } else if ui.popup_menu.is_some() {
                Some(GuiMenuLayer::Main)
            } else {
                None
            }
        };
        let Some(layer) = deepest else {
            return false;
        };

        match code {
            KeyCode::Up => {
                if let Some(menu) = self.popup_menu_layer_mut(layer) {
                    menu.select_prev();
                }
                self.app_core.needs_render = true;
                true
            }
            KeyCode::Down => {
                if let Some(menu) = self.popup_menu_layer_mut(layer) {
                    menu.select_next();
                }
                self.app_core.needs_render = true;
                true
            }
            KeyCode::Enter => {
                let command = self.popup_menu_layer_mut(layer).and_then(|menu| {
                    menu.selected_item()
                        .filter(|item| !item.disabled)
                        .map(|item| item.command.clone())
                });
                if let Some(command) = command {
                    self.handle_popup_menu_command(GuiMenuCommand { layer, command });
                }
                true
            }
            KeyCode::Esc => {
                self.close_menus_restore();
                self.app_core.needs_render = true;
                true
            }
            _ => false,
        }
    }

    fn popup_menu_layer_mut(&mut self, layer: GuiMenuLayer) -> Option<&mut PopupMenu> {
        let ui = &mut self.app_core.ui_state;
        match layer {
            GuiMenuLayer::Main => ui.popup_menu.as_mut(),
            GuiMenuLayer::Submenu => ui.submenu.as_mut(),
            GuiMenuLayer::Nested => ui.nested_submenu.as_mut(),
            GuiMenuLayer::Deep => ui.deep_submenu.as_mut(),
        }
    }

    fn apply_window_menu_command(
        &mut self,
        ctx: &egui::Context,
        request: &GuiWindowMenuRequest,
        command: GuiWindowMenuCommand,
    ) {
        match command {
            GuiWindowMenuCommand::Edit => {
                let window_name = self
                    .available_tabs
                    .get(&request.tab_key)
                    .map(|tab| tab.window_name.clone());
                if let Some(name) = window_name {
                    self.open_window_editor(Some(&name));
                }
            }
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
            GuiWindowMenuCommand::SendToBack => {
                self.send_window_to_back(ctx, &request.tab_key);
            }
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
            GuiWindowMenuCommand::ToggleTitleBar
            | GuiWindowMenuCommand::SetTextSize(_)
            | GuiWindowMenuCommand::SetWrapText(_)
            | GuiWindowMenuCommand::SetFont(_)
            | GuiWindowMenuCommand::SetAccent(_)
            | GuiWindowMenuCommand::SetMapZoom(_) => {
                let tab_key = request.tab_key.clone();
                self.apply_appearance_command(&tab_key, command);
            }
            GuiWindowMenuCommand::GroupWith(other) => {
                self.group_tabs(&request.tab_key.clone(), other);
            }
            GuiWindowMenuCommand::UngroupMember(member) => self.ungroup_tab(&member),
            GuiWindowMenuCommand::DissolveGroup => {
                if let Some(index) = self
                    .tab_groups
                    .iter()
                    .position(|group| group.members.contains(&request.tab_key))
                {
                    self.tab_groups.remove(index);
                    self.layout_dirty = true;
                }
            }
            GuiWindowMenuCommand::MoveGroupMember { member, up } => {
                if let Some(group) = self
                    .tab_groups
                    .iter_mut()
                    .find(|group| group.members.contains(&member))
                {
                    if let Some(index) = group.members.iter().position(|key| *key == member) {
                        let target = if up {
                            index.checked_sub(1)
                        } else {
                            (index + 1 < group.members.len()).then_some(index + 1)
                        };
                        if let Some(target) = target {
                            group.members.swap(index, target);
                            self.layout_dirty = true;
                        }
                    }
                }
            }
            GuiWindowMenuCommand::OpenMapExplorer => {
                self.map_explorer.open = true;
            }
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

    /// Apply an appearance-only command for `tab_key`. Split out from
    /// `apply_window_menu_command` because the Window Editor's Appearance
    /// section drives the same commands without a menu request.
    pub(super) fn apply_appearance_command(
        &mut self,
        tab_key: &TabKey,
        command: GuiWindowMenuCommand,
    ) {
        match command {
            GuiWindowMenuCommand::ToggleTitleBar => self.toggle_title_bar(tab_key.clone()),
            GuiWindowMenuCommand::SetTextSize(size) => {
                self.with_layout_def_for_tab(tab_key, |def| {
                    def.base_mut().text_size = size;
                });
            }
            GuiWindowMenuCommand::SetWrapText(wrap) => {
                if self.def_wordwrap_for_tab(tab_key).is_some() {
                    // Text-list defs carry wordwrap; store it there (shared
                    // with the TUI and layout.toml).
                    self.with_layout_def_for_tab(tab_key, |def| match def {
                        crate::config::WindowDef::Text { data, .. } => data.wordwrap = wrap,
                        crate::config::WindowDef::Inventory { data, .. }
                        | crate::config::WindowDef::Reserve { data, .. } => data.wordwrap = wrap,
                        _ => {}
                    });
                } else {
                    // Widget types with no wordwrap field (tabbedtext, spells,
                    // container) keep the per-tab GUI setting.
                    self.tab_settings
                        .entry(tab_key.clone())
                        .or_default()
                        .wrap_text = wrap;
                    self.layout_dirty = true;
                }
            }
            GuiWindowMenuCommand::SetFont(name) => {
                self.with_layout_def_for_tab(tab_key, |def| {
                    def.base_mut().font_family = name;
                });
                // Rebuild font definitions so the new family is registered.
                self.fonts_applied = false;
            }
            GuiWindowMenuCommand::SetAccent(color) => {
                self.tab_settings
                    .entry(tab_key.clone())
                    .or_default()
                    .accent_color =
                    color.map(|[r, g, b]| format!("#{:02x}{:02x}{:02x}", r, g, b));
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetMapZoom(zoom) => {
                self.tab_settings
                    .entry(tab_key.clone())
                    .or_default()
                    .map_zoom = zoom;
                self.layout_dirty = true;
            }
            _ => {}
        }
    }

    /// Resolve the current appearance state for `tab_key`, shared by the
    /// context menu and the Window Editor.
    pub(super) fn appearance_view_for_tab(&self, tab_key: &TabKey) -> WindowAppearanceView {
        let current_font = self.font_ref_for_tab(tab_key).and_then(|font| match font {
            FontRef::Named(name) => Some(name),
            _ => None,
        });
        let widget_type = self
            .available_tabs
            .get(tab_key)
            .and_then(|tab| self.app_core.ui_state.windows.get(&tab.window_name))
            .map(|window| window.widget_type.clone());
        let supports_wrap = matches!(
            widget_type,
            Some(
                WidgetType::Text
                    | WidgetType::TabbedText
                    | WidgetType::Inventory
                    | WidgetType::Reserve
                    | WidgetType::Spells
                    | WidgetType::Container
            )
        );
        WindowAppearanceView {
            title_bar_hidden: self.title_bar_hidden(tab_key),
            text_size_override: self.text_size_override_for_tab(tab_key),
            global_text_size: self.ui_settings.text_size,
            supports_wrap,
            is_map: widget_type == Some(WidgetType::Map),
            map_zoom: self
                .tab_settings
                .get(tab_key)
                .and_then(|settings| settings.map_zoom),
            wrap_text: self.effective_wrap_text(tab_key),
            current_font,
            accent_color: self.accent_color_for_tab(tab_key),
        }
    }

    pub(super) fn render_window_context_popup(&mut self, ctx: &egui::Context) {
        let Some(request) = self.window_context_menu.clone() else {
            return;
        };

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
        // Group members in render order, for the reorder controls.
        let group_members: Vec<(TabKey, String)> = self
            .group_for_tab(&request.tab_key)
            .map(|group| {
                group
                    .members
                    .iter()
                    .filter_map(|key| {
                        self.available_tabs
                            .get(key)
                            .map(|tab| (key.clone(), tab.id.title.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let view = WindowMenuView {
            zone: request.zone,
            allow_reorder: request.allow_reorder,
            appearance: self.appearance_view_for_tab(&request.tab_key),
            group_horizontal: self
                .group_for_tab(&request.tab_key)
                .map(|group| group.horizontal),
            group_members: &group_members,
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
                    | GuiWindowMenuCommand::SetMapZoom(_)
                    | GuiWindowMenuCommand::SetWrapText(_)
                    | GuiWindowMenuCommand::SetAccent(_)
                    | GuiWindowMenuCommand::ToggleTitleBar
                    | GuiWindowMenuCommand::SetGroupOrientation(_)
                    | GuiWindowMenuCommand::MoveGroupMember { .. }
                    | GuiWindowMenuCommand::UngroupMember(_)
            );
            self.apply_window_menu_command(ctx, &request, command);
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
        keyboard_active: bool,
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
                    let selected_index = menu.get_selected_index();
                    // Flat full-width rows like the settings window, not
                    // framed buttons; the keyboard cursor uses the same
                    // selection highlight as everywhere else.
                    ui.with_layout(
                        egui::Layout::top_down_justified(egui::Align::Min),
                        |ui| {
                            for (index, item) in menu.get_items().iter().enumerate() {
                                let selected = keyboard_active && index == selected_index;
                                let row = egui::Button::selectable(selected, item.text.as_str());
                                let response = ui
                                    .add_enabled(!item.disabled, row)
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if response.clicked() {
                                    clicked_command = Some(item.command.clone());
                                }
                            }
                        },
                    );
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
            // Right-click context menus stash their children in
            // menu_categories; the .menu main-menu submenus (colors/
            // highlights/keybinds/layouts/windows) are built on demand by
            // build_submenu. Try the cache first, then fall back — without
            // the fallback the whole .menu tree is dead in the GUI.
            let items = self
                .app_core
                .menu_categories
                .get(category)
                .cloned()
                .unwrap_or_else(|| self.app_core.build_submenu(category));
            if items.is_empty() {
                tracing::warn!("Missing GUI menu category: {}", category);
            } else {
                self.open_child_menu_for_layer(menu_command.layer, items);
            }
            return;
        }

        if let Some(submenu) = command.strip_prefix("menu:") {
            let items = self.app_core.build_submenu(submenu);
            if items.is_empty() {
                self.app_core
                    .add_system_message(&format!("Menu '{}' has no entries.", submenu));
                self.close_menus_restore();
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
            // Actions open editors/panels that own input next — leave
            // interact mode rather than fighting them for the keyboard.
            self.close_menus_to_normal();
            return;
        }

        if command == "__INDICATOR_EDITOR" {
            self.open_indicator_templates_editor();
            self.close_menus_to_normal();
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
            self.close_menus_to_normal();
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
            self.close_menus_to_normal();
            return;
        }

        // Known-windows list: flip this offer's show/hide, then re-open the
        // list (refreshed marks) so several toggle in one pass.
        if let Some(offer_id) = command.strip_prefix("__TOGGLE_OFFER__") {
            let offer_id = offer_id.to_string();
            self.app_core.toggle_window_offer(&offer_id);
            let items = self.app_core.build_known_windows_menu();
            self.open_child_menu_for_layer(menu_command.layer, items);
            return;
        }

        // Internal (double-underscore) menu commands must never reach the server.
        if command.starts_with("__") {
            self.app_core
                .add_system_message(&format!("GUI menu command not implemented yet: {}", command));
            self.close_menus_restore();
            return;
        }

        // A menu leaf command. Dot-commands (.settings, .colors, ...) are
        // CLIENT commands — process them locally (open the editor), never
        // send them to the game. Everything else is a context-menu game
        // verb. This is why the .menu tree was dead in the GUI: its leaves
        // are dot-commands and they were being sent to the network.
        if command.starts_with('.') {
            self.dispatch_command(command);
        } else {
            self.dispatch_raw_command(command);
        }
        self.close_menus_restore();
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

        // Keyboard selection highlights only the deepest layer — the one
        // arrow keys act on.
        let deepest = if deep.is_some() {
            3
        } else if nested.is_some() {
            2
        } else if submenu.is_some() {
            1
        } else {
            0
        };

        if let Some(menu) = main {
            let (command, rect) =
                Self::render_menu_layer(ctx, GuiMenuLayer::Main, menu, deepest == 0);
            clicked_command = command;
            if let Some(rect) = rect {
                menu_rects.push(rect);
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = submenu {
                let (command, rect) =
                    Self::render_menu_layer(ctx, GuiMenuLayer::Submenu, menu, deepest == 1);
                clicked_command = command;
                if let Some(rect) = rect {
                    menu_rects.push(rect);
                }
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = nested {
                let (command, rect) =
                    Self::render_menu_layer(ctx, GuiMenuLayer::Nested, menu, deepest == 2);
                clicked_command = command;
                if let Some(rect) = rect {
                    menu_rects.push(rect);
                }
            }
        }
        if clicked_command.is_none() {
            if let Some(menu) = deep {
                let (command, rect) =
                    Self::render_menu_layer(ctx, GuiMenuLayer::Deep, menu, deepest == 3);
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
            self.close_menus_restore();
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
        // Action rows are flat selectable labels (hover highlight only);
        // framed buttons read as noisy chips here.
        if ui.selectable_label(false, "Edit Window…").clicked() {
            return Some(GuiWindowMenuCommand::Edit);
        }
        if ui.selectable_label(false, "Hide").clicked() {
            return Some(GuiWindowMenuCommand::Hide);
        }
        if ui.selectable_label(false, "Detach").clicked() {
            return Some(GuiWindowMenuCommand::Detach);
        }
        if ui.selectable_label(false, "Move Window").clicked() {
            return Some(GuiWindowMenuCommand::StartMove);
        }
        if view.appearance.is_map
            && ui.selectable_label(false, "Open Map Explorer").clicked()
        {
            return Some(GuiWindowMenuCommand::OpenMapExplorer);
        }
        ui.separator();
        // Everything below folds into sections so the menu opens short.
        // Commands are collected instead of returned early so the layout
        // stays stable while a slider or palette inside a section is in use.
        let mut command = None;
        ui.collapsing("Arrange", |ui| {
            if view.allow_reorder {
                if ui.selectable_label(false, "Move Up").clicked() {
                    command = Some(GuiWindowMenuCommand::MoveUp);
                }
                if ui.selectable_label(false, "Move Down").clicked() {
                    command = Some(GuiWindowMenuCommand::MoveDown);
                }
            }
            // Stacking only matters where windows float and overlap — the
            // Center zone. Docked strips (header/footer/sidebars) never
            // overlap.
            if view.zone == GuiShellZone::Center
                && ui
                    .selectable_label(false, "Send to Back")
                    .on_hover_text("Drop this window behind any it overlaps")
                    .clicked()
            {
                command = Some(GuiWindowMenuCommand::SendToBack);
            }
            ui.label("Move to");
            for target in GuiShellZone::all() {
                let is_current = target == view.zone;
                let label = if is_current {
                    format!("{} (current)", target.label())
                } else {
                    target.label().to_string()
                };
                if ui.selectable_label(is_current, label).clicked() {
                    command = Some(GuiWindowMenuCommand::MoveTo(target));
                }
            }
        });
        ui.collapsing("Appearance", |ui| {
            if let Some(appearance) = Self::render_appearance_controls(ui, &view.appearance) {
                command = Some(appearance);
            }
        });
        if view.group_horizontal.is_some() || !view.group_candidates.is_empty() {
            ui.collapsing("Group", |ui| {
                if let Some(horizontal) = view.group_horizontal {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(!horizontal, "Stacked").clicked() && horizontal {
                            command = Some(GuiWindowMenuCommand::SetGroupOrientation(false));
                        }
                        if ui.selectable_label(horizontal, "Side by side").clicked()
                            && !horizontal
                        {
                            command = Some(GuiWindowMenuCommand::SetGroupOrientation(true));
                        }
                    });
                    if view.group_members.len() > 1 {
                        ui.label("Order");
                        let last = view.group_members.len() - 1;
                        for (index, (key, title)) in view.group_members.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(index > 0, egui::Button::new("⬆").small())
                                    .clicked()
                                {
                                    command = Some(GuiWindowMenuCommand::MoveGroupMember {
                                        member: key.clone(),
                                        up: true,
                                    });
                                }
                                if ui
                                    .add_enabled(index < last, egui::Button::new("⬇").small())
                                    .clicked()
                                {
                                    command = Some(GuiWindowMenuCommand::MoveGroupMember {
                                        member: key.clone(),
                                        up: false,
                                    });
                                }
                                if ui
                                    .add(egui::Button::new("✕").small())
                                    .on_hover_text("Remove this window from the group")
                                    .clicked()
                                {
                                    command =
                                        Some(GuiWindowMenuCommand::UngroupMember(key.clone()));
                                }
                                ui.label(title);
                            });
                        }
                    }
                    if ui
                        .selectable_label(false, "Dissolve group")
                        .on_hover_text("Ungroup all members; every window stands alone again")
                        .clicked()
                    {
                        command = Some(GuiWindowMenuCommand::DissolveGroup);
                    }
                }
                if !view.group_candidates.is_empty() {
                    ui.label("Group with");
                    egui::ScrollArea::vertical()
                        .id_salt("gui_window_group_list")
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for (key, title) in view.group_candidates {
                                if ui.selectable_label(false, title).clicked() {
                                    command = Some(GuiWindowMenuCommand::GroupWith(key.clone()));
                                }
                            }
                        });
                }
            });
        }
        command
    }

    /// Per-window appearance controls, shared between the context menu's
    /// Appearance section and the Window Editor. Returns a command to run
    /// through `apply_appearance_command`; live controls (sliders, swatches)
    /// emit one every frame their value changes.
    pub(super) fn render_appearance_controls(
        ui: &mut egui::Ui,
        view: &WindowAppearanceView,
    ) -> Option<GuiWindowMenuCommand> {
        let mut command = None;
        let mut show_title_bar = !view.title_bar_hidden;
        if ui.checkbox(&mut show_title_bar, "Title bar").changed() {
            command = Some(GuiWindowMenuCommand::ToggleTitleBar);
        }
        let mut override_enabled = view.text_size_override.is_some();
        if ui
            .checkbox(&mut override_enabled, "Custom text size")
            .changed()
        {
            command = Some(GuiWindowMenuCommand::SetTextSize(if override_enabled {
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
                command = Some(GuiWindowMenuCommand::SetTextSize(Some(value)));
            }
        }
        if view.is_map {
            let mut has_override = view.map_zoom.is_some();
            if ui.checkbox(&mut has_override, "Custom map zoom").changed() {
                command = Some(GuiWindowMenuCommand::SetMapZoom(
                    has_override.then_some(view.map_zoom.unwrap_or(16.0)),
                ));
            }
            if let Some(zoom) = view.map_zoom {
                let mut value = zoom;
                if ui
                    .add(egui::Slider::new(&mut value, 6.0..=48.0).suffix(" px/cell"))
                    .changed()
                {
                    command = Some(GuiWindowMenuCommand::SetMapZoom(Some(value)));
                }
            }
        }
        if view.supports_wrap {
            let mut wrap = view.wrap_text;
            if ui.checkbox(&mut wrap, "Word wrap").changed() {
                command = Some(GuiWindowMenuCommand::SetWrapText(wrap));
            }
        }
        ui.collapsing("Font", |ui| {
            // Filter box: system font lists run to hundreds of families.
            // Ids derive from ui.id() so the menu's and the editor's copies
            // of this section don't share state.
            let filter_id = ui.id().with("font_filter");
            let mut filter: String =
                ui.data_mut(|data| data.get_temp(filter_id).unwrap_or_default());
            if ui
                .add(egui::TextEdit::singleline(&mut filter).hint_text("Filter fonts"))
                .changed()
            {
                ui.data_mut(|data| data.insert_temp(filter_id, filter.clone()));
            }
            let filter_lower = filter.to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("font_list")
                .max_height(180.0)
                .show(ui, |ui| {
                    if ui
                        .selectable_label(view.current_font.is_none(), "Default")
                        .clicked()
                    {
                        command = Some(GuiWindowMenuCommand::SetFont(None));
                    }
                    for family in theme::system_font_families() {
                        if !filter_lower.is_empty()
                            && !family.to_lowercase().contains(&filter_lower)
                        {
                            continue;
                        }
                        let selected = view.current_font.as_deref() == Some(family.as_str());
                        if ui.selectable_label(selected, family).clicked() {
                            command = Some(GuiWindowMenuCommand::SetFont(Some(family.clone())));
                        }
                    }
                });
        });
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
                    command = Some(GuiWindowMenuCommand::SetAccent(Some(color)));
                }
            }
            if view.accent_color.is_some() && ui.small_button("✕").clicked() {
                command = Some(GuiWindowMenuCommand::SetAccent(None));
            }
        });
        command
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
