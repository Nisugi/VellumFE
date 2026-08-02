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
    /// Send this floating window behind any windows it overlaps in its
    /// zone, so a covered window can be reached. Live-session only, like
    /// egui's built-in click-to-raise.
    SendToBack,
    /// Start Move mode: the window follows the cursor (title bar stays as-is)
    /// until a click places it or Esc cancels.
    StartMove,
    /// Toggle the per-window position/size lock: locked windows ignore
    /// drag and resize gestures (Move Window above stays available).
    ToggleLock,
    ToggleTitleBar,
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
    /// Per-window frame corner radius; None reverts to the global setting.
    /// Keeps the menu open like SetTextSize.
    SetCornerRadius(Option<f32>),
    /// Per-window skin frame: a named `[frames.*]` entry, "none" for no
    /// frame, or None to revert to the skin's own per-window mapping.
    SetSkinFrame(Option<String>),
    /// Global injury doll image override (pool-relative path); None reverts
    /// to the active skin's doll.
    SetDollImage(Option<String>),
    /// Open the doll calibrator (injury doll windows).
    CalibrateDoll,
    /// Open the hand-icons editor (hand windows): status-driven icon
    /// states (empty hand, held weapon, prepared spell, ...).
    EditHandIcons,
    /// Open the dashboard editor (dashboard windows): which status ids to
    /// show plus layout/spacing/hide-inactive.
    EditDashboard,
    /// Render doll art in grayscale (dots keep their colors).
    SetDollGrayscale(bool),
    /// Global compass art set from the pool; None reverts to the skin's.
    SetCompassSet(Option<String>),
    /// Icon for one hand window (LEFTHAND/RIGHTHAND/SPELLHAND); None
    /// reverts to the skin default. Stored as a per-indicator override so
    /// the whole IconRef machinery (pool images, sheet cells) applies.
    SetHandIcon {
        hand_id: String,
        icon: Option<crate::data::IconRef>,
    },
    /// Per-window background: pool image path, "none" for no background,
    /// or None to revert to the skin's per-window mapping.
    SetBackground(Option<String>),
    /// Per-window title bar height; None reverts to the global setting.
    /// Keeps the menu open like SetTextSize.
    SetTitleBarHeight(Option<f32>),
    /// Per-window title alignment ("left" | "center" | "right"); None
    /// reverts to the global setting.
    SetTitleBarAlign(Option<String>),
    /// Border on/off for the shared layout def (also drives the TUI).
    SetShowBorder(bool),
    /// Border style name for the shared layout def ("single", "double", ...).
    SetBorderStyle(String),
    /// Which edges draw a border, for the shared layout def.
    SetBorderSides(crate::config::BorderSides),
    /// Content alignment for the shared layout def ("center", "bottom-left",
    /// ...); None reverts to the default top-left flow.
    SetContentAlign(Option<String>),
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
    /// Toggle whether a member shares its predecessor's slot (stacking
    /// along the perpendicular axis) instead of opening its own.
    ToggleMemberMerge(TabKey),
    /// Toggle whether the slot this member opens anchors its content to
    /// the end of the perpendicular axis (bottom of a column).
    ToggleSlotAnchor(TabKey),
    /// Set a member's relative size weight along the group's stack axis.
    SetMemberWeight { member: TabKey, weight: f32 },
}

/// Everything the window context menu needs to render, resolved up front so
/// the menu body stays a static fn.
struct WindowMenuView<'a> {
    zone: GuiShellZone,
    /// Position/size lock state, rendered as a checked row.
    locked: bool,
    appearance: WindowAppearanceView,
    /// None = not grouped; Some(horizontal) = grouped with this orientation.
    group_horizontal: Option<bool>,
    /// Members of this window's group in render order (empty when ungrouped).
    group_members: &'a [(TabKey, String)],
    /// Members sharing their predecessor's slot (subset of group_members).
    group_merged: &'a [TabKey],
    /// Slot-opening members whose slot content anchors to the end.
    group_end_anchored: &'a [TabKey],
    /// Per-member relative size weights (defaults to 1.0 when absent).
    group_weights: &'a [(TabKey, f32)],
    /// Windows this one could be grouped with (visible, ungrouped).
    group_candidates: &'a [(TabKey, String)],
}

/// A picker row with a fixed-width preview slot before the label: the
/// thumbnail paints centered in the slot when loaded; the slot is
/// reserved either way so labels align while thumbs stream in (the
/// SkinState cache decodes a few per frame). Returns true on click.
fn thumbed_row(
    ui: &mut egui::Ui,
    selected: bool,
    stem: &str,
    thumb: Option<(egui::TextureId, egui::Vec2)>,
) -> bool {
    const EDGE: f32 = 20.0;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(EDGE), egui::Sense::hover());
        if let Some((id, size)) = thumb {
            let scale = (EDGE / size.x.max(size.y)).min(1.0);
            let draw_rect = egui::Rect::from_center_size(rect.center(), size * scale);
            ui.painter().image(
                id,
                draw_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        ui.selectable_label(selected, stem).clicked()
    })
    .inner
}

/// Per-window appearance state for the context menu's Appearance section,
/// attached and detached alike (see `appearance_view_for_tab`).
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
    /// Per-window frame corner radius override; None follows the global.
    corner_radius_override: Option<f32>,
    global_corner_radius: f32,
    /// Named frames the active skin offers; the picker hides when the skin
    /// neither names frames nor draws a border on this window.
    skin_frames: Vec<String>,
    /// Per-window skin frame override; None follows the skin's mapping.
    skin_frame_override: Option<String>,
    /// Whether the skin resolves a border for this window by default (so
    /// "None" is worth offering even without named frames).
    has_skin_border: bool,
    /// Injury doll widget: offer the pool doll picker + calibrator.
    is_doll: bool,
    /// Pool doll images as (pool-relative path, display stem, thumbnail).
    doll_images: Vec<(String, String, Option<(egui::TextureId, egui::Vec2)>)>,
    /// Global doll override (pool-relative path); None = skin default.
    doll_override: Option<String>,
    /// Grayscale doll art toggle (global).
    doll_grayscale: bool,
    /// Compass widget: offer the pool compass-set picker.
    is_compass: bool,
    /// Dashboard widget: offer the "Edit Dashboard…" pop-out.
    is_dashboard: bool,
    /// Pool compass sets.
    compass_sets: Vec<String>,
    /// Global compass set override; None = skin default.
    compass_override: Option<String>,
    /// Hand widget: which hand this window is (LEFTHAND/RIGHTHAND/
    /// SPELLHAND); None for non-hand windows.
    hand_id: Option<String>,
    /// Pool hand images as (pool-relative path, display stem, thumbnail).
    hand_images: Vec<(String, String, Option<(egui::TextureId, egui::Vec2)>)>,
    /// This hand's current icon override; None = skin default.
    hand_icon_override: Option<crate::data::IconRef>,
    /// Pool background images as (pool-relative path, display stem,
    /// thumbnail).
    background_images: Vec<(String, String, Option<(egui::TextureId, egui::Vec2)>)>,
    /// Per-window background override; None = skin default.
    background_override: Option<String>,
    /// Whether the skin resolves a background for this window by default.
    has_skin_background: bool,
    /// Per-window title bar height override; None follows the global.
    title_bar_height_override: Option<f32>,
    /// Global title bar height; 0 = derived from the title font.
    global_title_bar_height: f32,
    /// Per-window title alignment override; None follows the global.
    title_align_override: Option<String>,
    /// Border fields from the shared layout def; None when the window has
    /// no def (border controls are hidden then).
    border: Option<BorderView>,
    /// Content alignment from the shared layout def (text-list widgets).
    content_align: Option<String>,
}

/// Snapshot of a layout def's border configuration.
pub(super) struct BorderView {
    show_border: bool,
    style: String,
    sides: crate::config::BorderSides,
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
                // Hide IS the Windows-window uncheck (one visibility layer):
                // core WindowVisibility flips, auto-spawn is suppressed, and
                // the catalog checkbox reflects it. Hiding a grouped window
                // hides every member — same as unchecking each in the catalog
                // (which, like this, dissolves the group).
                let members = self
                    .group_for_tab(&request.tab_key)
                    .map(|group| group.members.clone());
                match members {
                    Some(members) => {
                        for member in members {
                            self.core_hide_tab(&member);
                        }
                    }
                    None => self.core_hide_tab(&request.tab_key.clone()),
                }
            }
            GuiWindowMenuCommand::Detach => self.detach_tab(request.tab_key.clone()),
            GuiWindowMenuCommand::SendToBack => {
                self.send_window_to_back(ctx, &request.tab_key);
            }
            GuiWindowMenuCommand::ToggleLock => {
                // Writes the shared layout's `locked` — the single lock
                // store `.lockwindows` / `.lockwindow` / the TUI use — so
                // a menu lock is cleared by `.unlockall` and vice versa.
                let new_state = !self.window_locked(&request.tab_key);
                let window_name = self
                    .available_tabs
                    .get(&request.tab_key)
                    .map(|tab| tab.window_name.clone());
                let mut stored = false;
                if let Some(name) = window_name {
                    if let Some(window) = self
                        .app_core
                        .layout
                        .windows
                        .iter_mut()
                        .find(|window| window.name() == name)
                    {
                        window.base_mut().locked = new_state;
                        stored = true;
                    }
                }
                if stored {
                    self.app_core.schedule_layout_autosave();
                    self.layout_dirty = true;
                } else {
                    self.app_core
                        .add_system_message("This window has no layout entry to lock.");
                }
            }
            GuiWindowMenuCommand::EditHandIcons => {
                if let Some(name) = self
                    .available_tabs
                    .get(&request.tab_key)
                    .map(|tab| tab.window_name.clone())
                {
                    self.open_hand_icons_editor(name);
                }
            }
            GuiWindowMenuCommand::EditDashboard => {
                if let Some(name) = self
                    .available_tabs
                    .get(&request.tab_key)
                    .map(|tab| tab.window_name.clone())
                {
                    self.open_dashboard_editor(name);
                }
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
                    just_started: true,
                });
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
            | GuiWindowMenuCommand::SetCornerRadius(_)
            | GuiWindowMenuCommand::SetSkinFrame(_)
            | GuiWindowMenuCommand::SetDollImage(_)
            | GuiWindowMenuCommand::CalibrateDoll
            | GuiWindowMenuCommand::SetDollGrayscale(_)
            | GuiWindowMenuCommand::SetCompassSet(_)
            | GuiWindowMenuCommand::SetHandIcon { .. }
            | GuiWindowMenuCommand::SetBackground(_)
            | GuiWindowMenuCommand::SetTitleBarHeight(_)
            | GuiWindowMenuCommand::SetTitleBarAlign(_)
            | GuiWindowMenuCommand::SetShowBorder(_)
            | GuiWindowMenuCommand::SetBorderStyle(_)
            | GuiWindowMenuCommand::SetBorderSides(_)
            | GuiWindowMenuCommand::SetContentAlign(_)
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
            GuiWindowMenuCommand::ToggleMemberMerge(member) => {
                if let Some(group) = self
                    .tab_groups
                    .iter_mut()
                    .find(|group| group.members.contains(&member))
                {
                    if let Some(index) = group.merged.iter().position(|key| *key == member) {
                        group.merged.remove(index);
                    } else {
                        group.merged.push(member);
                    }
                    self.layout_dirty = true;
                }
            }
            GuiWindowMenuCommand::ToggleSlotAnchor(member) => {
                if let Some(group) = self
                    .tab_groups
                    .iter_mut()
                    .find(|group| group.members.contains(&member))
                {
                    if let Some(index) =
                        group.end_anchored.iter().position(|key| *key == member)
                    {
                        group.end_anchored.remove(index);
                    } else {
                        group.end_anchored.push(member);
                    }
                    self.layout_dirty = true;
                }
            }
            GuiWindowMenuCommand::SetMemberWeight { member, weight } => {
                if let Some(group) = self
                    .tab_groups
                    .iter_mut()
                    .find(|group| group.members.contains(&member))
                {
                    // Weight 1.0 is the neutral default — drop the entry
                    // rather than storing it, so a reset-to-1 group serializes
                    // exactly like a never-weighted one.
                    group.weights.retain(|(key, _)| *key != member);
                    if (weight - 1.0).abs() > f32::EPSILON && weight > 0.0 {
                        group.weights.push((member, weight));
                    }
                    self.layout_dirty = true;
                }
            }
        }
    }

    /// Apply an appearance-only command for `tab_key`. Split out from
    /// `apply_window_menu_command` so callers without a menu request (the
    /// detached-window dispatch) can drive the same commands.
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
            GuiWindowMenuCommand::SetCornerRadius(radius) => {
                self.tab_settings
                    .entry(tab_key.clone())
                    .or_default()
                    .corner_radius = radius;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetSkinFrame(frame) => {
                self.tab_settings
                    .entry(tab_key.clone())
                    .or_default()
                    .skin_frame = frame;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetDollImage(image) => {
                self.set_doll_image(image);
            }
            GuiWindowMenuCommand::CalibrateDoll => {
                self.open_doll_calibration();
            }
            GuiWindowMenuCommand::SetDollGrayscale(gray) => {
                self.ui_settings.doll_grayscale = gray;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetCompassSet(set) => {
                self.ui_settings.compass_set = set;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetHandIcon { hand_id, icon } => {
                match icon {
                    Some(icon) => {
                        self.ui_settings
                            .status_icons
                            .overrides
                            .insert(hand_id, icon);
                    }
                    None => {
                        self.ui_settings.status_icons.overrides.remove(&hand_id);
                    }
                }
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetBackground(background) => {
                self.tab_settings
                    .entry(tab_key.clone())
                    .or_default()
                    .background_image = background;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetTitleBarHeight(height) => {
                self.tab_settings
                    .entry(tab_key.clone())
                    .or_default()
                    .title_bar_height = height;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetTitleBarAlign(align) => {
                self.tab_settings
                    .entry(tab_key.clone())
                    .or_default()
                    .title_bar_align = align;
                self.layout_dirty = true;
            }
            GuiWindowMenuCommand::SetShowBorder(show) => {
                self.with_layout_def_for_tab(tab_key, |def| {
                    def.base_mut().show_border = show;
                });
            }
            GuiWindowMenuCommand::SetBorderStyle(style) => {
                self.with_layout_def_for_tab(tab_key, |def| {
                    def.base_mut().border_style = style;
                });
            }
            GuiWindowMenuCommand::SetBorderSides(sides) => {
                self.with_layout_def_for_tab(tab_key, |def| {
                    def.base_mut().border_sides = sides;
                });
            }
            GuiWindowMenuCommand::SetContentAlign(align) => {
                self.with_layout_def_for_tab(tab_key, |def| {
                    def.base_mut().content_align = align.clone();
                });
                // The renderer reads the live window state each frame; push
                // the change there too so it applies without a recreate.
                if let Some(window_name) = self
                    .available_tabs
                    .get(tab_key)
                    .map(|tab| tab.window_name.clone())
                {
                    if let Some(window) =
                        self.app_core.ui_state.windows.get_mut(&window_name)
                    {
                        window.content_align = align;
                    }
                }
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

    /// Resolve the current appearance state for `tab_key` for the window
    /// context menu (attached and detached). Needs `&mut self` + ctx for
    /// the picker thumbnails (budgeted decode cache on SkinState).
    pub(super) fn appearance_view_for_tab(
        &mut self,
        ctx: &egui::Context,
        tab_key: &TabKey,
    ) -> WindowAppearanceView {
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
        let is_doll = widget_type == Some(WidgetType::InjuryDoll);
        // Pool scan only for doll windows — no disk walk for every menu.
        let doll_images = if is_doll {
            crate::config::pool::list_category("dolls")
                .iter()
                .map(|image| {
                    let thumb = self.skin_state.thumbnail(ctx, &image.pool_path);
                    (image.pool_path.clone(), image.stem().to_string(), thumb)
                })
                .collect()
        } else {
            Vec::new()
        };
        let is_compass = widget_type == Some(WidgetType::Compass);
        let compass_sets = if is_compass {
            crate::config::pool::set_names("compass")
        } else {
            Vec::new()
        };
        // Which hand a hand window is, by the same name rule the renderer
        // uses (left/right substrings, else spell).
        let hand_id = (widget_type == Some(WidgetType::Hand))
            .then(|| {
                self.available_tabs.get(tab_key).map(|tab| {
                    let name = tab.window_name.to_ascii_lowercase();
                    if name.contains("left") {
                        "LEFTHAND".to_string()
                    } else if name.contains("right") {
                        "RIGHTHAND".to_string()
                    } else {
                        "SPELLHAND".to_string()
                    }
                })
            })
            .flatten();
        let hand_images = if hand_id.is_some() {
            crate::config::pool::list_category("hands")
                .iter()
                .map(|image| {
                    let thumb = self.skin_state.thumbnail(ctx, &image.pool_path);
                    (image.pool_path.clone(), image.stem().to_string(), thumb)
                })
                .collect()
        } else {
            Vec::new()
        };
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
            corner_radius_override: self.corner_radius_override_for_tab(tab_key),
            global_corner_radius: self.ui_settings.window_corner_radius,
            skin_frames: self.skin_state.frame_names(),
            skin_frame_override: self
                .tab_settings
                .get(tab_key)
                .and_then(|settings| settings.skin_frame.clone()),
            has_skin_border: self
                .available_tabs
                .get(tab_key)
                .and_then(|tab| self.skin_state.border_for(&tab.window_name))
                .is_some(),
            is_doll,
            doll_images,
            doll_override: self.ui_settings.doll_image.clone(),
            doll_grayscale: self.ui_settings.doll_grayscale,
            is_compass,
            is_dashboard: widget_type == Some(WidgetType::Dashboard),
            compass_sets,
            compass_override: self.ui_settings.compass_set.clone(),
            hand_icon_override: hand_id.as_ref().and_then(|id| {
                self.ui_settings.status_icons.overrides.get(id).cloned()
            }),
            hand_id,
            hand_images,
            background_images: crate::config::pool::list_category("backgrounds")
                .iter()
                .map(|image| {
                    let thumb = self.skin_state.thumbnail(ctx, &image.pool_path);
                    (image.pool_path.clone(), image.stem().to_string(), thumb)
                })
                .collect(),
            background_override: self
                .tab_settings
                .get(tab_key)
                .and_then(|settings| settings.background_image.clone()),
            has_skin_background: self
                .available_tabs
                .get(tab_key)
                .and_then(|tab| self.skin_state.background_for(&tab.window_name))
                .is_some(),
            title_bar_height_override: self
                .tab_settings
                .get(tab_key)
                .and_then(|settings| settings.title_bar_height),
            global_title_bar_height: self.ui_settings.title_bar_height,
            title_align_override: self
                .tab_settings
                .get(tab_key)
                .and_then(|settings| settings.title_bar_align.clone()),
            border: self.layout_def_for_tab(tab_key).map(|def| {
                let base = def.base();
                BorderView {
                    show_border: base.show_border,
                    style: base.border_style.clone(),
                    sides: base.border_sides.clone(),
                }
            }),
            content_align: self
                .layout_def_for_tab(tab_key)
                .and_then(|def| def.base().content_align.clone()),
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
        let (group_merged, group_end_anchored, group_weights) = self
            .group_for_tab(&request.tab_key)
            .map(|group| {
                (
                    group.merged.clone(),
                    group.end_anchored.clone(),
                    group.weights.clone(),
                )
            })
            .unwrap_or_default();
        let view = WindowMenuView {
            zone: request.zone,
            locked: self.window_locked(&request.tab_key),
            appearance: self.appearance_view_for_tab(ctx, &request.tab_key),
            group_horizontal: self
                .group_for_tab(&request.tab_key)
                .map(|group| group.horizontal),
            group_members: &group_members,
            group_merged: &group_merged,
            group_end_anchored: &group_end_anchored,
            group_weights: &group_weights,
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
                    | GuiWindowMenuCommand::ToggleMemberMerge(_)
                    | GuiWindowMenuCommand::ToggleSlotAnchor(_)
                    | GuiWindowMenuCommand::SetMemberWeight { .. }
                    | GuiWindowMenuCommand::SetCornerRadius(_)
                    | GuiWindowMenuCommand::SetSkinFrame(_)
                    | GuiWindowMenuCommand::SetDollImage(_)
                    | GuiWindowMenuCommand::SetDollGrayscale(_)
                    | GuiWindowMenuCommand::SetCompassSet(_)
                    | GuiWindowMenuCommand::SetHandIcon { .. }
                    | GuiWindowMenuCommand::SetBackground(_)
                    | GuiWindowMenuCommand::SetTitleBarHeight(_)
                    | GuiWindowMenuCommand::SetTitleBarAlign(_)
                    | GuiWindowMenuCommand::SetShowBorder(_)
                    | GuiWindowMenuCommand::SetBorderStyle(_)
                    | GuiWindowMenuCommand::SetBorderSides(_)
                    | GuiWindowMenuCommand::SetContentAlign(_)
                    | GuiWindowMenuCommand::SetFont(_)
            );
            self.apply_window_menu_command(ctx, &request, command);
            if !keep_open {
                self.window_context_menu = None;
            }
            // Either way this frame's click has been consumed by a control;
            // skip the click-outside check so picking from a combo popup
            // (whose options render outside the menu rect) doesn't also
            // count as a click-outside and close the menu.
            return;
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

    /// The Layouts submenu, GUI edition: core's builder lists the TUI's
    /// TOML cell layouts, which don't apply here, so this lists the GUI's
    /// JSON checkpoints from the same shared ~/.vellum-fe/layouts/ pool.
    fn build_gui_layouts_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let mut items: Vec<crate::data::ui_state::PopupMenuItem> =
            list_named_layouts()
                .into_iter()
                .map(|name| crate::data::ui_state::PopupMenuItem {
                    text: name.clone(),
                    command: format!("action:layout:load:{}", name),
                    disabled: false,
                })
                .collect();
        if items.is_empty() {
            items.push(crate::data::ui_state::PopupMenuItem {
                text: "No layouts found (.savelayout <name>)".to_string(),
                command: String::new(),
                disabled: true,
            });
        }
        items.push(crate::data::ui_state::PopupMenuItem {
            text: "Close menu".to_string(),
            command: String::new(),
            disabled: true,
        });
        items
    }

    fn handle_popup_menu_command(&mut self, menu_command: GuiMenuCommand) {
        let command = menu_command.command;

        if let Some(category) = command.strip_prefix("__SUBMENU__") {
            // Right-click context menus stash their children in
            // menu_categories; the .menu main-menu submenus (colors/
            // highlights/keybinds/layouts/windows) are built on demand by
            // build_submenu. Try the cache first, then fall back — without
            // the fallback the whole .menu tree is dead in the GUI.
            // Layouts are frontend-owned (core would list the TUI's TOML
            // cell layouts), so the GUI builds that one itself.
            let items = if category == "layouts" {
                self.build_gui_layouts_submenu()
            } else {
                self.app_core
                    .menu_categories
                    .get(category)
                    .cloned()
                    .unwrap_or_else(|| self.app_core.build_submenu(category))
            };
            if items.is_empty() {
                tracing::warn!("Missing GUI menu category: {}", category);
            } else {
                self.open_child_menu_for_layer(menu_command.layer, items);
            }
            return;
        }

        if let Some(submenu) = command.strip_prefix("menu:") {
            // The GUI shows the show/hide list as its richer checkbox panel
            // rather than a nested menu (the TUI uses the menu form).
            if submenu == "knownwindows" {
                self.open_known_windows_editor();
                self.close_menus_to_normal();
                return;
            }
            let items = if submenu == "layouts" {
                self.build_gui_layouts_submenu()
            } else {
                self.app_core.build_submenu(submenu)
            };
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
                // Every real action parses; this is a menu-wiring bug.
                self.app_core
                    .add_system_message(&format!("Unknown action: {}", command));
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

        // Windows list: flip this window's show/hide and close the menu
        // like any other menu action. (U3: keyed on window name.)
        if let Some(name) = command.strip_prefix("__TOGGLE_WINDOW__") {
            let name = name.to_string();
            self.app_core.toggle_known_window(&name);
            self.close_menus_restore();
            self.app_core.needs_render = true;
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
            // Close the menu stack BEFORE dispatching: a dot-command may
            // itself open a menu (.knownwindows) and closing afterwards
            // wipes what it just opened.
            self.close_menus_restore();
            self.dispatch_command(command);
        } else {
            self.dispatch_raw_command(command);
            self.close_menus_restore();
        }
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
        // Widget tools sit next to Edit: they open something, unlike the
        // settings grouped under Appearance.
        if view.appearance.is_map
            && ui.selectable_label(false, "Open Map Explorer").clicked()
        {
            return Some(GuiWindowMenuCommand::OpenMapExplorer);
        }
        if view.appearance.is_doll
            && ui.selectable_label(false, "Calibrate doll…").clicked()
        {
            return Some(GuiWindowMenuCommand::CalibrateDoll);
        }
        if view.appearance.is_dashboard
            && ui
                .selectable_label(false, "Edit dashboard…")
                .on_hover_text("Which statuses to show, plus layout and spacing")
                .clicked()
        {
            return Some(GuiWindowMenuCommand::EditDashboard);
        }
        if view.appearance.hand_id.is_some()
            && ui
                .selectable_label(false, "Hand icons…")
                .on_hover_text(
                    "Status-driven icons: empty hand, held weapon, prepared spell, ...",
                )
                .clicked()
        {
            return Some(GuiWindowMenuCommand::EditHandIcons);
        }
        if ui.selectable_label(false, "Hide").clicked() {
            return Some(GuiWindowMenuCommand::Hide);
        }
        if ui.selectable_label(false, "Detach").clicked() {
            return Some(GuiWindowMenuCommand::Detach);
        }
        // Stacking matters wherever windows float freely and can overlap —
        // every zone except the packed sidebars.
        if matches!(
            view.zone,
            GuiShellZone::Center | GuiShellZone::Header | GuiShellZone::Footer
        ) && ui
            .selectable_label(false, "Send to Back")
            .on_hover_text("Drop this window behind any it overlaps")
            .clicked()
        {
            return Some(GuiWindowMenuCommand::SendToBack);
        }
        if ui
            .selectable_label(
                view.locked,
                if view.locked { "Unlock Window" } else { "Lock Window" },
            )
            .on_hover_text(
                "Locked windows ignore dragging and resizing. \
                 Arrange ▸ Move Window still works deliberately.",
            )
            .clicked()
        {
            return Some(GuiWindowMenuCommand::ToggleLock);
        }
        ui.separator();
        // Everything below folds into sections so the menu opens short.
        // Commands are collected instead of returned early so the layout
        // stays stable while a slider or palette inside a section is in use.
        let mut command = None;
        ui.collapsing("Arrange", |ui| {
            if ui.selectable_label(false, "Move Window").clicked() {
                command = Some(GuiWindowMenuCommand::StartMove);
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
                            let merged = index > 0 && view.group_merged.contains(key);
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
                                if merged {
                                    // Visual cue: this member lives in the
                                    // slot opened by the member above it.
                                    ui.label("└");
                                }
                                ui.label(title);
                            });
                            ui.indent((key, "group_slot_controls"), |ui| {
                                if index > 0 {
                                    let label = if horizontal {
                                        "Share column with the window above"
                                    } else {
                                        "Share row with the window above"
                                    };
                                    let mut flag = merged;
                                    if ui.checkbox(&mut flag, label).changed() {
                                        command = Some(
                                            GuiWindowMenuCommand::ToggleMemberMerge(key.clone()),
                                        );
                                    }
                                }
                                if horizontal && !merged {
                                    let mut anchored = view.group_end_anchored.contains(key);
                                    if ui
                                        .checkbox(&mut anchored, "Anchor column to bottom")
                                        .on_hover_text(
                                            "Leftover column space goes above these \
                                             windows instead of below them.",
                                        )
                                        .changed()
                                    {
                                        command = Some(
                                            GuiWindowMenuCommand::ToggleSlotAnchor(key.clone()),
                                        );
                                    }
                                }
                                // Relative size weight along the stack axis:
                                // a member with weight 2 takes twice the
                                // growable space of a weight-1 sibling (buffs
                                // 6 lines / cooldowns 3). Only flexible
                                // members grow, so this is inert for one-row
                                // widgets (bars/timers), which is why it's
                                // always offered — no need to know which is
                                // which. Merged members size on the OTHER
                                // axis, so hide it for them.
                                if !merged {
                                    let mut weight = view
                                        .group_weights
                                        .iter()
                                        .find(|(k, _)| k == key)
                                        .map(|(_, w)| *w)
                                        .filter(|w| *w > 0.0)
                                        .unwrap_or(1.0);
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut weight, 0.25..=4.0)
                                                .text("size weight")
                                                .step_by(0.25),
                                        )
                                        .on_hover_text(
                                            "Relative share of the group's growable \
                                             space. 2 = twice a sibling at 1. \
                                             One-row widgets ignore this.",
                                        )
                                        .changed()
                                    {
                                        command = Some(GuiWindowMenuCommand::SetMemberWeight {
                                            member: key.clone(),
                                            weight,
                                        });
                                    }
                                }
                            });
                        }
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
                // Destructive action last, set off from the rest.
                if view.group_horizontal.is_some() {
                    ui.separator();
                    if ui
                        .selectable_label(false, "Dissolve group")
                        .on_hover_text("Ungroup all members; every window stands alone again")
                        .clicked()
                    {
                        command = Some(GuiWindowMenuCommand::DissolveGroup);
                    }
                }
            });
        }
        command
    }

    /// Per-window appearance controls: widget-specific settings first (map
    /// zoom, doll, compass, hand icon), then Text / Title bar / Frame
    /// subsections. The context menu is their only home (attached and
    /// detached windows both route here). Returns a command to run through
    /// `apply_appearance_command`; live controls (sliders, swatches) emit
    /// one every frame their value changes.
    pub(super) fn render_appearance_controls(
        ui: &mut egui::Ui,
        view: &WindowAppearanceView,
    ) -> Option<GuiWindowMenuCommand> {
        let mut command = None;
        // Widget-specific settings surface first — for a map or doll window
        // they are the reason the menu was opened. General settings follow,
        // grouped into Text / Title bar / Frame subsections.
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
        if view.is_doll {
            ui.horizontal(|ui| {
                ui.label("Doll image");
                let selected_label = match view.doll_override.as_deref() {
                    None => "Skin default",
                    Some(path) if path.eq_ignore_ascii_case("none") => "None",
                    Some(path) => view
                        .doll_images
                        .iter()
                        .find(|(pool_path, _, _)| pool_path == path)
                        .map(|(_, stem, _)| stem.as_str())
                        // Stale override (image removed): show the raw path.
                        .unwrap_or(path),
                };
                egui::ComboBox::from_id_salt("gui_window_doll_image")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(view.doll_override.is_none(), "Skin default")
                            .clicked()
                        {
                            command = Some(GuiWindowMenuCommand::SetDollImage(None));
                        }
                        let none_selected = view
                            .doll_override
                            .as_deref()
                            .is_some_and(|path| path.eq_ignore_ascii_case("none"));
                        if ui
                            .selectable_label(none_selected, "None")
                            .on_hover_text("No doll art; draw the built-in vector body")
                            .clicked()
                        {
                            command = Some(GuiWindowMenuCommand::SetDollImage(Some(
                                "none".to_string(),
                            )));
                        }
                        for (path, stem, thumb) in &view.doll_images {
                            let selected = view.doll_override.as_deref() == Some(path.as_str());
                            if thumbed_row(ui, selected, stem, *thumb) {
                                command =
                                    Some(GuiWindowMenuCommand::SetDollImage(Some(path.clone())));
                            }
                        }
                    });
            });
            if view.doll_images.is_empty() {
                ui.weak("No dolls in the pool — install some with .jinx list / .jinx install");
            }
            let mut gray = view.doll_grayscale;
            if ui
                .checkbox(&mut gray, "Grayscale doll art")
                .on_hover_text(
                    "Render the doll's base and overlays desaturated; wound/scar dots keep \
                     their colors. The grayscale copy is built only while this is on.",
                )
                .changed()
            {
                command = Some(GuiWindowMenuCommand::SetDollGrayscale(gray));
            }
        }
        if view.is_compass {
            ui.horizontal(|ui| {
                ui.label("Compass art");
                let selected_label = match view.compass_override.as_deref() {
                    None => "Skin default",
                    Some(set) if set.eq_ignore_ascii_case("none") => "None",
                    Some(set) => set,
                };
                egui::ComboBox::from_id_salt("gui_window_compass_set")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(view.compass_override.is_none(), "Skin default")
                            .clicked()
                        {
                            command = Some(GuiWindowMenuCommand::SetCompassSet(None));
                        }
                        let none_selected = view
                            .compass_override
                            .as_deref()
                            .is_some_and(|set| set.eq_ignore_ascii_case("none"));
                        if ui
                            .selectable_label(none_selected, "None")
                            .on_hover_text("No compass art; draw the built-in vector rose")
                            .clicked()
                        {
                            command = Some(GuiWindowMenuCommand::SetCompassSet(Some(
                                "none".to_string(),
                            )));
                        }
                        for set in &view.compass_sets {
                            let selected = view.compass_override.as_deref() == Some(set.as_str());
                            if ui.selectable_label(selected, set).clicked() {
                                command =
                                    Some(GuiWindowMenuCommand::SetCompassSet(Some(set.clone())));
                            }
                        }
                    });
            });
            if view.compass_sets.is_empty() {
                ui.weak("No compass sets in the pool — install with .jinx");
            }
        }
        if let Some(hand_id) = &view.hand_id {
            ui.horizontal(|ui| {
                ui.label("Hand icon");
                let selected_label = match &view.hand_icon_override {
                    None => "Skin default".to_string(),
                    Some(crate::data::IconRef::Default) => "Skin default".to_string(),
                    Some(crate::data::IconRef::None) => "None".to_string(),
                    Some(crate::data::IconRef::Image { path }) => view
                        .hand_images
                        .iter()
                        .find(|(pool_path, _, _)| pool_path == path)
                        .map(|(_, stem, _)| stem.clone())
                        .unwrap_or_else(|| path.clone()),
                    Some(crate::data::IconRef::SheetCell { sheet, cell }) => {
                        format!("{sheet} #{cell}")
                    }
                };
                egui::ComboBox::from_id_salt("gui_window_hand_icon")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(view.hand_icon_override.is_none(), "Skin default")
                            .clicked()
                        {
                            command = Some(GuiWindowMenuCommand::SetHandIcon {
                                hand_id: hand_id.clone(),
                                icon: None,
                            });
                        }
                        let none_selected = matches!(
                            &view.hand_icon_override,
                            Some(crate::data::IconRef::None)
                        );
                        if ui
                            .selectable_label(none_selected, "None")
                            .on_hover_text("No icon art; show the text fallback")
                            .clicked()
                        {
                            command = Some(GuiWindowMenuCommand::SetHandIcon {
                                hand_id: hand_id.clone(),
                                icon: Some(crate::data::IconRef::None),
                            });
                        }
                        for (path, stem, thumb) in &view.hand_images {
                            let selected = matches!(
                                &view.hand_icon_override,
                                Some(crate::data::IconRef::Image { path: current })
                                    if current == path
                            );
                            if thumbed_row(ui, selected, stem, *thumb) {
                                command = Some(GuiWindowMenuCommand::SetHandIcon {
                                    hand_id: hand_id.clone(),
                                    icon: Some(crate::data::IconRef::Image {
                                        path: path.clone(),
                                    }),
                                });
                            }
                        }
                    });
            });
            if view.hand_images.is_empty() {
                ui.weak("No hand icons in the pool — install with .jinx");
            }
        }
        ui.collapsing("Text", |ui| {
            ui.collapsing("Font", |ui| {
                // Filter box: system font lists run to hundreds of families.
                // Ids derive from ui.id() so the menu's and the detached
                // menu's copies of this section don't share state.
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
                                command =
                                    Some(GuiWindowMenuCommand::SetFont(Some(family.clone())));
                            }
                        }
                    });
            });
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
            if view.supports_wrap {
                let mut wrap = view.wrap_text;
                if ui.checkbox(&mut wrap, "Word wrap").changed() {
                    command = Some(GuiWindowMenuCommand::SetWrapText(wrap));
                }
                ui.horizontal(|ui| {
                    ui.label("Content alignment");
                    const ALIGNS: [(Option<&str>, &str); 10] = [
                        (None, "Default (top left)"),
                        (Some("top-left"), "Top left"),
                        (Some("top"), "Top center"),
                        (Some("top-right"), "Top right"),
                        (Some("left"), "Middle left"),
                        (Some("center"), "Center"),
                        (Some("right"), "Middle right"),
                        (Some("bottom-left"), "Bottom left"),
                        (Some("bottom"), "Bottom center"),
                        (Some("bottom-right"), "Bottom right"),
                    ];
                    let current = view.content_align.as_deref();
                    let selected_label = ALIGNS
                        .iter()
                        .find(|(value, _)| *value == current)
                        .map(|(_, label)| *label)
                        .unwrap_or("Default (top left)");
                    egui::ComboBox::from_id_salt("gui_window_content_align")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for (value, label) in ALIGNS {
                                if ui.selectable_label(current == value, label).clicked() {
                                    command = Some(GuiWindowMenuCommand::SetContentAlign(
                                        value.map(str::to_string),
                                    ));
                                }
                            }
                        });
                });
            }
        });
        ui.collapsing("Title bar", |ui| {
            let mut show_title_bar = !view.title_bar_hidden;
            if ui.checkbox(&mut show_title_bar, "Show title bar").changed() {
                command = Some(GuiWindowMenuCommand::ToggleTitleBar);
            }
            ui.horizontal(|ui| {
                ui.label("Title alignment");
                let current = view.title_align_override.as_deref();
                let selected_label = match current {
                    Some("left") => "Left",
                    Some("center") => "Center",
                    Some("right") => "Right",
                    _ => "Default",
                };
                egui::ComboBox::from_id_salt("gui_window_title_align")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            (None, "Default"),
                            (Some("left"), "Left"),
                            (Some("center"), "Center"),
                            (Some("right"), "Right"),
                        ] {
                            if ui.selectable_label(current == value, label).clicked() {
                                command = Some(GuiWindowMenuCommand::SetTitleBarAlign(
                                    value.map(str::to_string),
                                ));
                            }
                        }
                    });
            });
            let mut tb_enabled = view.title_bar_height_override.is_some();
            if ui
                .checkbox(&mut tb_enabled, "Custom title bar height")
                .changed()
            {
                command = Some(GuiWindowMenuCommand::SetTitleBarHeight(if tb_enabled {
                    let seed = if view.global_title_bar_height > 0.0 {
                        view.global_title_bar_height
                    } else {
                        18.0
                    };
                    Some(view.title_bar_height_override.unwrap_or(seed))
                } else {
                    None
                }));
            }
            if let Some(current) = view.title_bar_height_override {
                let mut value = current;
                if ui
                    .add(egui::Slider::new(&mut value, 12.0..=32.0).step_by(1.0))
                    .changed()
                {
                    command = Some(GuiWindowMenuCommand::SetTitleBarHeight(Some(value)));
                }
            }
        });
        ui.collapsing("Frame", |ui| {
            if let Some(border) = &view.border {
                let mut show_border = border.show_border;
                if ui.checkbox(&mut show_border, "Border").changed() {
                    command = Some(GuiWindowMenuCommand::SetShowBorder(show_border));
                }
                if border.show_border {
                    const BORDER_STYLES: [(&str, &str); 6] = [
                        ("single", "Single"),
                        ("double", "Double"),
                        ("thick", "Thick"),
                        ("rounded", "Rounded"),
                        ("quadrant_inside", "Quadrant inside"),
                        ("quadrant_outside", "Quadrant outside"),
                    ];
                    let current = border.style.to_ascii_lowercase();
                    let current_label = BORDER_STYLES
                        .iter()
                        .find(|(key, _)| *key == current)
                        .map(|(_, label)| *label)
                        .unwrap_or("Single");
                    egui::ComboBox::from_id_salt("gui_window_border_style")
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            for (key, label) in BORDER_STYLES {
                                if ui.selectable_label(current == key, label).clicked() {
                                    command = Some(GuiWindowMenuCommand::SetBorderStyle(
                                        key.to_string(),
                                    ));
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        let mut sides = border.sides.clone();
                        let mut changed = false;
                        changed |= ui.checkbox(&mut sides.top, "Top").changed();
                        changed |= ui.checkbox(&mut sides.bottom, "Bottom").changed();
                        changed |= ui.checkbox(&mut sides.left, "Left").changed();
                        changed |= ui.checkbox(&mut sides.right, "Right").changed();
                        if changed {
                            command = Some(GuiWindowMenuCommand::SetBorderSides(sides));
                        }
                    });
                }
            }
            let mut radius_enabled = view.corner_radius_override.is_some();
            if ui
                .checkbox(&mut radius_enabled, "Custom corner radius")
                .changed()
            {
                command = Some(GuiWindowMenuCommand::SetCornerRadius(if radius_enabled {
                    Some(view.corner_radius_override.unwrap_or(view.global_corner_radius))
                } else {
                    None
                }));
            }
            if let Some(current) = view.corner_radius_override {
                let mut value = current;
                if ui
                    .add(egui::Slider::new(&mut value, 0.0..=12.0).step_by(0.5))
                    .changed()
                {
                    command = Some(GuiWindowMenuCommand::SetCornerRadius(Some(value)));
                }
            }
            ui.label("Accent color");
            ui.horizontal(|ui| {
                for color in ACCENT_PALETTE {
                    let fill = Color32::from_rgb(color[0], color[1], color[2]);
                    let selected = view.accent_color == Some(fill);
                    let mut button = egui::Button::new("  ").fill(fill);
                    if selected {
                        button =
                            button.stroke(egui::Stroke::new(2.0, ui.visuals().text_color()));
                    }
                    if ui.add(button).clicked() {
                        command = Some(GuiWindowMenuCommand::SetAccent(Some(color)));
                    }
                }
                if view.accent_color.is_some() && ui.small_button("✕").clicked() {
                    command = Some(GuiWindowMenuCommand::SetAccent(None));
                }
            });
            if !view.skin_frames.is_empty() || view.has_skin_border {
                const NO_FRAME: &str = crate::config::skins::NO_FRAME;
                ui.horizontal(|ui| {
                    ui.label("Skin frame");
                    let current = view.skin_frame_override.as_deref();
                    let selected_label = match current {
                        None => "Skin default",
                        Some(name) if name.eq_ignore_ascii_case(NO_FRAME) => "None",
                        Some(name) => name,
                    };
                    egui::ComboBox::from_id_salt("gui_window_skin_frame")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(current.is_none(), "Skin default")
                                .clicked()
                            {
                                command = Some(GuiWindowMenuCommand::SetSkinFrame(None));
                            }
                            let none_selected = current
                                .is_some_and(|name| name.eq_ignore_ascii_case(NO_FRAME));
                            if ui.selectable_label(none_selected, "None").clicked() {
                                command = Some(GuiWindowMenuCommand::SetSkinFrame(Some(
                                    NO_FRAME.to_string(),
                                )));
                            }
                            for name in &view.skin_frames {
                                let selected =
                                    current.is_some_and(|value| value.eq_ignore_ascii_case(name));
                                if ui.selectable_label(selected, name).clicked() {
                                    command = Some(GuiWindowMenuCommand::SetSkinFrame(Some(
                                        name.clone(),
                                    )));
                                }
                            }
                        });
                });
            }
            if !view.background_images.is_empty() || view.has_skin_background {
                ui.horizontal(|ui| {
                    ui.label("Background");
                    let selected_label = match view.background_override.as_deref() {
                        None => "Skin default",
                        Some(path) if path.eq_ignore_ascii_case("none") => "None",
                        Some(path) => view
                            .background_images
                            .iter()
                            .find(|(pool_path, _, _)| pool_path == path)
                            .map(|(_, stem, _)| stem.as_str())
                            .unwrap_or(path),
                    };
                    egui::ComboBox::from_id_salt("gui_window_background")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    view.background_override.is_none(),
                                    "Skin default",
                                )
                                .clicked()
                            {
                                command = Some(GuiWindowMenuCommand::SetBackground(None));
                            }
                            let none_selected = view
                                .background_override
                                .as_deref()
                                .is_some_and(|path| path.eq_ignore_ascii_case("none"));
                            if ui.selectable_label(none_selected, "None").clicked() {
                                command = Some(GuiWindowMenuCommand::SetBackground(Some(
                                    "none".to_string(),
                                )));
                            }
                            for (path, stem, thumb) in &view.background_images {
                                let selected =
                                    view.background_override.as_deref() == Some(path.as_str());
                                if thumbed_row(ui, selected, stem, *thumb) {
                                    command = Some(GuiWindowMenuCommand::SetBackground(Some(
                                        path.clone(),
                                    )));
                                }
                            }
                        });
                });
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
