//! True OS-window detach support via egui multi-viewport.
//!
//! Each detached tab renders in its own native window through
//! `Context::show_viewport_immediate`, so it can be dragged to another
//! monitor. Geometry is captured per frame into `ViewportState` (values are
//! egui points despite the `_px` field names) and persisted in
//! `layout_v1.json`'s `detached_viewports` map. Closing the native window
//! reattaches the tab to its previous zone; `tab_zones` and
//! `main_window_rects` keep their entries while a tab is detached, so
//! reattach is just removing the key from `detached_tabs`.

use super::menus::GuiMenuCommand;
use super::*;
use eframe::egui::{ViewportCommand, ViewportId};

pub(super) struct DetachedWindowState {
    /// Stable id derived from the tab key; hashing the key directly matches
    /// the zone-window id convention and avoids per-frame string allocation.
    pub(super) viewport_id: ViewportId,
    /// Geometry the `ViewportBuilder` is seeded with. Stable after creation:
    /// eframe re-applies the builder every call, so feeding live geometry
    /// back in would fight user drags with SetOuterPosition commands.
    pub(super) initial: ViewportState,
    /// Live geometry captured each frame from `ViewportInfo`; persisted.
    pub(super) current: ViewportState,
    /// Frames rendered so far; the off-screen safety check runs once the
    /// window has reported real geometry (frame 2).
    pub(super) frames_rendered: u32,
}

impl DetachedWindowState {
    pub(super) fn new(key: &TabKey, viewport: ViewportState) -> Self {
        Self {
            viewport_id: ViewportId::from_hash_of(("vellum_detached", key)),
            initial: viewport.clone(),
            current: viewport,
            frames_rendered: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct DetachedMenuState {
    pub(super) tab_key: TabKey,
    pub(super) pos: Pos2,
}

/// Everything a detached viewport's render pass wants to change on the app.
/// Collected inside the viewport closure (which only borrows `AppCore`) and
/// applied with `&mut self` after the closure returns.
#[derive(Default)]
struct DetachedFrameOutput {
    link_click: Option<GuiLinkClick>,
    open_menu_at: Option<Pos2>,
    close_menu: bool,
    reattach: bool,
    hide: bool,
    /// Command picked in this window's context menu; applied with `&mut
    /// self` after the viewport closure returns.
    menu_command: Option<super::menus::GuiWindowMenuCommand>,
    dispatch_targets: Vec<GlobalDispatchTarget>,
    typed_text: String,
    backspaces: usize,
    submit_command: bool,
    history_prev: bool,
    history_next: bool,
    clear_line: bool,
    popup_command: Option<GuiMenuCommand>,
    popup_should_close: bool,
    info: Option<egui::ViewportInfo>,
}

impl VellumGuiApp {
    pub(super) fn detach_tab(&mut self, key: TabKey) {
        if self.detached_tabs.contains_key(&key) || !self.available_tabs.contains_key(&key) {
            return;
        }
        // A detached tab lives in its own OS window; it can't stay grouped.
        self.ungroup_tab(&key);
        let bounds = self
            .last_monitor_bounds
            .unwrap_or([0.0, 0.0, 1200.0, 800.0]);
        // Seed size from the tab's current in-window rect when known.
        let size = self
            .main_window_rects
            .get(&key)
            .map(|rect| {
                [
                    rect[2].max(MIN_VIEWPORT_WIDTH),
                    rect[3].max(MIN_VIEWPORT_HEIGHT),
                ]
            })
            .unwrap_or([
                bounds[2].min(640.0).max(320.0),
                bounds[3].min(480.0).max(240.0),
            ]);
        let viewport = ViewportState::new(key.clone(), [bounds[0] + 120.0, bounds[1] + 120.0], size);
        self.detached_tabs
            .insert(key.clone(), DetachedWindowState::new(&key, viewport));
        self.layout_dirty = true;
    }

    pub(super) fn reattach_tab(&mut self, key: TabKey) {
        if self.detached_tabs.remove(&key).is_some() {
            if self
                .detached_context_menu
                .as_ref()
                .is_some_and(|menu| menu.tab_key == key)
            {
                self.detached_context_menu = None;
            }
            self.close_popup_menus_if_host_gone();
            self.layout_dirty = true;
        }
    }

    /// Popup menus hosted by a window that is no longer detached would
    /// reappear in the root window at foreign coordinates; close them.
    fn close_popup_menus_if_host_gone(&mut self) {
        if self
            .popup_menu_host
            .as_ref()
            .is_some_and(|host| !self.detached_tabs.contains_key(host))
        {
            self.close_all_popup_menus();
            self.app_core.ui_state.input_mode = InputMode::Normal;
        }
    }

    /// Drop detached entries whose tab disappeared or was hidden. Hiding a
    /// detached tab closes its native window; restoring it later returns it
    /// to a zone.
    pub(super) fn prune_detached_tabs(&mut self) {
        let available_tabs = &self.available_tabs;
        let hidden_tabs = &self.hidden_tabs;
        let before = self.detached_tabs.len();
        self.detached_tabs
            .retain(|key, _| available_tabs.contains_key(key) && !hidden_tabs.contains(key));
        if self.detached_tabs.len() != before {
            self.close_popup_menus_if_host_gone();
            self.layout_dirty = true;
        }
    }

    pub(super) fn detached_tab_keys(&self) -> HashSet<TabKey> {
        self.detached_tabs.keys().cloned().collect()
    }

    /// Replace non-finite values and enforce minimum sizes. Restored
    /// positions are deliberately NOT clamped to the root monitor: viewport
    /// positions are global desktop coordinates and a second monitor
    /// legitimately lies outside the primary's rect. A deferred off-screen
    /// check (`rect_within_virtual_desktop`) catches truly lost windows.
    pub(super) fn sanitize_viewport_state(state: &ViewportState) -> ViewportState {
        let mut viewport = state.clone();
        if !viewport.outer_pos_px[0].is_finite() {
            viewport.outer_pos_px[0] = 120.0;
        }
        if !viewport.outer_pos_px[1].is_finite() {
            viewport.outer_pos_px[1] = 120.0;
        }
        if !viewport.outer_size_px[0].is_finite() {
            viewport.outer_size_px[0] = 640.0;
        }
        if !viewport.outer_size_px[1].is_finite() {
            viewport.outer_size_px[1] = 480.0;
        }
        viewport.outer_size_px[0] = viewport.outer_size_px[0].max(MIN_VIEWPORT_WIDTH);
        viewport.outer_size_px[1] = viewport.outer_size_px[1].max(MIN_VIEWPORT_HEIGHT);
        viewport
    }

    /// Generous virtual-desktop estimate around the root window's monitor:
    /// a window intersecting it is reachable; one outside it (e.g. saved on
    /// a monitor that no longer exists) gets repositioned. `root_bounds` is
    /// `[x, y, monitor_width, monitor_height]` from `monitor_bounds_from_ctx`.
    fn rect_within_virtual_desktop(rect: Rect, root_bounds: [f32; 4]) -> bool {
        if !rect.is_finite() || !root_bounds.iter().all(|value| value.is_finite()) {
            return true;
        }
        let [x, y, w, h] = root_bounds;
        let w = w.max(1.0);
        let h = h.max(1.0);
        let virtual_desktop = Rect::from_min_max(
            Pos2::new(x - 2.0 * w, y - 2.0 * h),
            Pos2::new(x + 3.0 * w, y + 3.0 * h),
        );
        rect.intersects(virtual_desktop)
    }

    /// Fold a child viewport's reported geometry into the persisted state.
    /// Returns true when something changed beyond the 0.5pt epsilon.
    fn update_viewport_state_from_info(
        current: &mut ViewportState,
        info: &egui::ViewportInfo,
    ) -> bool {
        if info.minimized == Some(true) {
            return false;
        }
        let mut changed = false;
        let maximized = info.maximized == Some(true);
        if current.maximized != maximized {
            current.maximized = maximized;
            changed = true;
        }
        // While maximized, keep the last floating geometry so un-maximizing
        // after a restart returns to a sane rect.
        if !maximized {
            if let Some(rect) = info.outer_rect.filter(|rect| rect.is_finite()) {
                let pos = [rect.min.x, rect.min.y];
                let size = [rect.width().max(1.0), rect.height().max(1.0)];
                if (current.outer_pos_px[0] - pos[0]).abs() > 0.5
                    || (current.outer_pos_px[1] - pos[1]).abs() > 0.5
                {
                    current.outer_pos_px = pos;
                    changed = true;
                }
                if (current.outer_size_px[0] - size[0]).abs() > 0.5
                    || (current.outer_size_px[1] - size[1]).abs() > 0.5
                {
                    current.outer_size_px = size;
                    changed = true;
                }
            }
        }
        if let Some(scale) = info.native_pixels_per_point {
            if current
                .scale_hint
                .is_none_or(|existing| (existing - scale).abs() > 0.01)
            {
                current.scale_hint = Some(scale);
                changed = true;
            }
        }
        if let Some(monitor) = info.monitor_size {
            let hint = format!("{}x{}", monitor.x.round() as i64, monitor.y.round() as i64);
            if current.monitor_hint.as_deref() != Some(hint.as_str()) {
                current.monitor_hint = Some(hint);
                changed = true;
            }
        }
        changed
    }

    /// Render every detached tab in its own native window. Must run after
    /// the main panels so the immediate child passes don't interleave with
    /// panel layout. Returns link clicks tagged with the tab they came from.
    pub(super) fn render_detached_viewports(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<(TabKey, GuiLinkClick)> {
        if self.detached_tabs.is_empty() {
            return Vec::new();
        }
        let mut keys: Vec<TabKey> = self.detached_tabs.keys().cloned().collect();
        keys.sort_by_key(|key| key.short_id());
        let suppress_macro_dispatch = self.should_suppress_macro_dispatch();

        let mut results: Vec<(TabKey, DetachedFrameOutput)> = Vec::new();
        for key in keys {
            let Some(tab) = self.available_tabs.get(&key).cloned() else {
                continue;
            };
            let Some(state) = self.detached_tabs.get(&key) else {
                continue;
            };
            let viewport_id = state.viewport_id;
            let initial = state.initial.clone();
            let builder = egui::ViewportBuilder::default()
                .with_title(format!("VellumFE - {}", tab.id.title))
                .with_position(Pos2::new(initial.outer_pos_px[0], initial.outer_pos_px[1]))
                .with_inner_size(Vec2::new(initial.outer_size_px[0], initial.outer_size_px[1]))
                .with_min_inner_size(Vec2::new(MIN_VIEWPORT_WIDTH, MIN_VIEWPORT_HEIGHT))
                .with_maximized(initial.maximized);
            let menu = self
                .detached_context_menu
                .clone()
                .filter(|menu| menu.tab_key == key);
            // Menu view data needs &mut self (thumbnail decode cache), so
            // resolve it here — the viewport closure only borrows AppCore.
            let menu_data = menu
                .as_ref()
                .map(|_| self.detached_window_menu_data(ctx, &key));
            let hosts_popup_menus = self.popup_menu_host.as_ref() == Some(&key);
            let render_settings = self.widget_render_settings(&tab.id.key);
            let command_completion_end = (key == TabKey::CommandInput
                && render_settings.command_input_completion.is_some())
                .then(|| {
                    render_settings
                        .command_input_seed
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .count()
                });
            let app_core = &self.app_core;
            // The viewport callback is FnMut to egui but runs exactly once
            // per show call; take() lets the owned menu data move through.
            let mut menu_data = menu_data;
            let out = ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
                let mut out = DetachedFrameOutput::default();
                Self::render_detached_viewport_contents(
                    app_core,
                    ui,
                    &tab,
                    render_settings.clone(),
                    menu.as_ref(),
                    menu_data.take(),
                    hosts_popup_menus,
                    suppress_macro_dispatch,
                    command_completion_end,
                    &mut out,
                );
                out
            });
            results.push((key, out));
        }

        let mut link_clicks = Vec::new();
        for (key, out) in results {
            if let Some(click) = out.link_click {
                link_clicks.push((key.clone(), click));
            }
            for target in out.dispatch_targets {
                self.execute_global_dispatch_target(target, ctx);
            }
            if out.popup_command.is_some() || out.popup_should_close {
                self.apply_popup_menu_layer_result(out.popup_command, out.popup_should_close);
            }
            if out.clear_line {
                self.command_input.clear();
            }
            for _ in 0..out.backspaces {
                self.command_input.pop();
            }
            if !out.typed_text.is_empty() {
                self.command_input.push_str(&out.typed_text);
            }
            if out.history_prev {
                self.history_previous();
            }
            if out.history_next {
                self.history_next();
            }
            if out.submit_command {
                self.submit_command();
            }
            if let Some(pos) = out.open_menu_at {
                self.close_all_popup_menus();
                self.window_context_menu = None;
                self.detached_context_menu = Some(DetachedMenuState {
                    tab_key: key.clone(),
                    pos,
                });
            } else if out.close_menu
                && self
                    .detached_context_menu
                    .as_ref()
                    .is_some_and(|menu| menu.tab_key == key)
            {
                self.detached_context_menu = None;
            }
            if let Some(command) = out.menu_command {
                let keep_open = Self::window_menu_command_keeps_open(&command);
                let menu_pos = self
                    .detached_context_menu
                    .as_ref()
                    .filter(|menu| menu.tab_key == key)
                    .map(|menu| menu.pos)
                    .unwrap_or_default();
                // Rect only seeds Move mode, which the detached menu never
                // offers; the viewport's size is a faithful stand-in.
                let window_rect = self
                    .detached_tabs
                    .get(&key)
                    .map(|state| {
                        Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(
                                state.current.outer_size_px[0],
                                state.current.outer_size_px[1],
                            ),
                        )
                    })
                    .unwrap_or(Rect::ZERO);
                let request = super::menus::GuiWindowMenuRequest {
                    tab_key: key.clone(),
                    zone: self.zone_for_tab(&key),
                    position: menu_pos,
                    window_rect,
                };
                self.apply_window_menu_command(ctx, &request, command);
                if !keep_open
                    && self
                        .detached_context_menu
                        .as_ref()
                        .is_some_and(|menu| menu.tab_key == key)
                {
                    self.detached_context_menu = None;
                }
            }
            if out.hide {
                // Hide = the Windows-window uncheck (core visibility); the
                // next available-tabs refresh prunes this viewport.
                self.core_hide_tab(&key);
                continue;
            }
            if out.reattach {
                self.reattach_tab(key);
                continue;
            }

            if let Some(state) = self.detached_tabs.get_mut(&key) {
                state.frames_rendered = state.frames_rendered.saturating_add(1);
                if let Some(info) = &out.info {
                    if Self::update_viewport_state_from_info(&mut state.current, info) {
                        self.layout_dirty = true;
                    }
                    if state.frames_rendered == 2 {
                        if let (Some(rect), Some(bounds)) =
                            (info.outer_rect, self.last_monitor_bounds)
                        {
                            if !Self::rect_within_virtual_desktop(rect, bounds) {
                                ctx.send_viewport_cmd_to(
                                    state.viewport_id,
                                    ViewportCommand::OuterPosition(Pos2::new(
                                        bounds[0] + 120.0,
                                        bounds[1] + 120.0,
                                    )),
                                );
                            }
                        }
                    }
                }
            }
        }
        link_clicks
    }

    /// Body of one detached viewport pass. Runs inside the child viewport,
    /// so all `ctx.input` reads see the child window's input.
    #[allow(clippy::too_many_arguments)]
    fn render_detached_viewport_contents(
        app_core: &AppCore,
        ui: &mut egui::Ui,
        tab: &GuiTab,
        render_settings: WidgetRenderSettings,
        menu: Option<&DetachedMenuState>,
        menu_data: Option<super::menus::DetachedWindowMenuData>,
        hosts_popup_menus: bool,
        suppress_macro_dispatch: bool,
        command_completion_end: Option<usize>,
        out: &mut DetachedFrameOutput,
    ) {
        let ctx = ui.ctx().clone();

        // Keybinds first, mirroring handle_global_input's ordering: consumed
        // keys must not reach widgets or the command-line forwarding below.
        // While this window's context menu is open, skip the pipeline
        // entirely: the menu hosts text fields (title, streams, feed ids)
        // and forwarding that typing to the root command line would send it
        // to the game.
        if menu.is_none() {
            Self::forward_detached_input(
                &ctx,
                app_core,
                suppress_macro_dispatch,
                command_completion_end,
                out,
            );
        }

        egui::CentralPanel::default().show(ui, |ui| {
            // Body drag moves the OS window, mirroring the docked windows'
            // drag-anywhere. Registered BEFORE the content so interactive
            // content (links, compass arrows, scrollbars) still wins
            // hit-testing; dragging empty space falls through to this and
            // hands the gesture to the OS.
            let drag_bg = ui.interact(
                ui.max_rect(),
                ui.id().with("detached_body_drag"),
                egui::Sense::drag(),
            );
            if drag_bg.drag_started_by(egui::PointerButton::Primary) {
                ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            }
            ui.push_id(&tab.id.key, |ui| {
                if let Some(click) =
                    Self::render_window_content(app_core, ui, tab, render_settings)
                {
                    out.link_click = Some(click);
                }
            });
        });

        let secondary_click_pos = ctx.input(|input| {
            if input.pointer.secondary_clicked() {
                input.pointer.interact_pos()
            } else {
                None
            }
        });
        if let Some(pos) = secondary_click_pos {
            out.open_menu_at = Some(pos);
        }

        if let (Some(menu), Some(menu_data)) = (menu, menu_data) {
            let area_response = egui::Area::new(egui::Id::new("gui_detached_context_menu"))
                .order(egui::Order::Foreground)
                .fixed_pos(menu.pos)
                .interactable(true)
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(220.0);
                        out.menu_command = Self::render_detached_window_menu(ui, menu_data);
                    });
                });
            // A command consumed this frame's click; skip the click-outside
            // check so picking from a combo popup (which renders outside the
            // menu rect) doesn't also close the menu.
            if out.menu_command.is_none() && out.open_menu_at.is_none() {
                let menu_rect = area_response.response.rect;
                let should_close = ctx.input(|input| {
                    Self::should_close_popup_menus_on_outside_click(
                        input.pointer.any_click(),
                        input.pointer.latest_pos(),
                        &[menu_rect],
                    )
                });
                if should_close {
                    out.close_menu = true;
                }
            }
        }

        // Game popup menus requested from this window render here, at the
        // click position in this window's own coordinates.
        if hosts_popup_menus {
            let (popup_command, popup_should_close) =
                Self::render_popup_menu_layers(&ctx, app_core);
            out.popup_command = popup_command;
            out.popup_should_close = popup_should_close;
        }

        // The native close button reattaches: for non-root viewports egui
        // expects the app to stop showing the viewport next frame.
        if ctx.input(|input| input.viewport().close_requested()) {
            out.reattach = true;
        }

        out.info = Some(ctx.input(|input| input.viewport().clone()));
    }

    /// Run the global keybind pipeline against the child viewport's input
    /// and forward plain typing to the root command line.
    fn forward_detached_input(
        ctx: &egui::Context,
        app_core: &AppCore,
        suppress_macro_dispatch: bool,
        command_completion_end: Option<usize>,
        out: &mut DetachedFrameOutput,
    ) {
        let key_presses = Self::collect_pressed_key_events(ctx);
        let mut consumed_keyboard_input = false;
        for key_press in key_presses {
            // The detached command-input TextEdit owns plain Tab when it can
            // accept a visible history suggestion, matching the root window.
            if key_press.key_event.code == crate::data::input::KeyCode::Tab
                && key_press.key_event.modifiers == crate::data::input::KeyModifiers::NONE
                && command_completion_end
                    .is_some_and(|end| Self::command_completion_cursor_ready(ctx, end))
            {
                continue;
            }

            let target = Self::resolve_global_dispatch_target(
                key_press.key_event,
                &app_core.keybind_map,
                &app_core.config.app_keybinds,
                suppress_macro_dispatch,
            );
            let Some(target) = target else {
                continue;
            };
            consumed_keyboard_input = true;
            out.dispatch_targets.push(target);
            ctx.input_mut(|input| {
                if let Some(logical_key) = key_press.logical_key {
                    input.consume_key(key_press.modifiers, logical_key);
                }
                if let Some(physical_key) = key_press.physical_key {
                    input.consume_key(key_press.modifiers, physical_key);
                }
            });
        }

        if consumed_keyboard_input {
            // Retain on PROCESSED input.events (what widgets read this frame),
            // not raw.events -- see the matching note in app.rs
            // handle_global_input. Kept consistent with the root path even
            // though detached windows host no text widgets today.
            ctx.input_mut(|input| {
                input.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Key { .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                            | egui::Event::Copy
                            | egui::Event::Cut
                    )
                });
            });
            return;
        }

        // No text widgets live in detached windows, so unconsumed typing
        // routes to the root command input (a MUD client should accept
        // commands no matter which of its windows is focused). Submit /
        // history / clear-line honor the SAME per-frame key stash as the
        // root input widget, so rebinds work here too (the detached
        // viewport shares the root egui context, and with it the stash).
        let keys = ctx
            .data(|data| data.get_temp::<super::CommandInputKeys>(super::CommandInputKeys::id()))
            .unwrap_or_else(|| super::CommandInputKeys {
                submit: vec![egui::Key::Enter],
                history_prev: vec![egui::Key::ArrowUp],
                history_next: vec![egui::Key::ArrowDown],
                ..Default::default()
            });
        ctx.input(|input| {
            for event in &input.raw.events {
                match event {
                    egui::Event::Text(text) => out.typed_text.push_str(text),
                    egui::Event::Paste(text) => out.typed_text.push_str(text),
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } if modifiers.is_none() && keys.submit.contains(key) => {
                        out.submit_command = true;
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.is_none() && keys.history_prev.contains(key) => {
                        out.history_prev = true;
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.is_none() && keys.history_next.contains(key) => {
                        out.history_next = true;
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if keys
                        .clear_line
                        .iter()
                        .any(|(k, m)| k == key && *m == *modifiers) =>
                    {
                        out.clear_line = true;
                    }
                    egui::Event::Key {
                        key: egui::Key::Backspace,
                        pressed: true,
                        ..
                    } => out.backspaces += 1,
                    _ => {}
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_id_stable_for_same_key() {
        let a = DetachedWindowState::new(
            &TabKey::Vitals,
            ViewportState::new(TabKey::Vitals, [0.0, 0.0], [400.0, 300.0]),
        );
        let b = DetachedWindowState::new(
            &TabKey::Vitals,
            ViewportState::new(TabKey::Vitals, [50.0, 50.0], [500.0, 350.0]),
        );
        let other = DetachedWindowState::new(
            &TabKey::Compass,
            ViewportState::new(TabKey::Compass, [0.0, 0.0], [400.0, 300.0]),
        );
        assert_eq!(a.viewport_id, b.viewport_id);
        assert_ne!(a.viewport_id, other.viewport_id);
    }

    #[test]
    fn test_sanitize_viewport_state_fixes_non_finite_and_min_size() {
        let viewport = ViewportState::new(
            TabKey::Vitals,
            [f32::NAN, f32::INFINITY],
            [20.0, f32::NAN],
        );
        let sanitized = VellumGuiApp::sanitize_viewport_state(&viewport);
        assert_eq!(sanitized.outer_pos_px, [120.0, 120.0]);
        assert!(sanitized.outer_size_px[0] >= MIN_VIEWPORT_WIDTH);
        assert!(sanitized.outer_size_px[1] >= MIN_VIEWPORT_HEIGHT);
    }

    #[test]
    fn test_sanitize_viewport_state_keeps_second_monitor_positions() {
        // Positions outside the primary monitor are legitimate (multi-monitor).
        let viewport = ViewportState::new(TabKey::Vitals, [-1800.0, 200.0], [640.0, 480.0]);
        let sanitized = VellumGuiApp::sanitize_viewport_state(&viewport);
        assert_eq!(sanitized.outer_pos_px, [-1800.0, 200.0]);
    }

    #[test]
    fn test_rect_within_virtual_desktop() {
        let bounds = [0.0, 0.0, 1920.0, 1080.0];
        let on_primary = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(640.0, 480.0));
        assert!(VellumGuiApp::rect_within_virtual_desktop(on_primary, bounds));
        let on_second_monitor =
            Rect::from_min_size(Pos2::new(-1900.0, 0.0), Vec2::new(640.0, 480.0));
        assert!(VellumGuiApp::rect_within_virtual_desktop(
            on_second_monitor,
            bounds
        ));
        let lost = Rect::from_min_size(Pos2::new(50_000.0, 50_000.0), Vec2::new(640.0, 480.0));
        assert!(!VellumGuiApp::rect_within_virtual_desktop(lost, bounds));
    }

    #[test]
    fn test_update_viewport_state_skips_minimized() {
        let mut current = ViewportState::new(TabKey::Vitals, [10.0, 10.0], [400.0, 300.0]);
        let info = egui::ViewportInfo {
            minimized: Some(true),
            outer_rect: Some(Rect::from_min_size(
                Pos2::new(999.0, 999.0),
                Vec2::new(100.0, 100.0),
            )),
            ..Default::default()
        };
        assert!(!VellumGuiApp::update_viewport_state_from_info(
            &mut current,
            &info
        ));
        assert_eq!(current.outer_pos_px, [10.0, 10.0]);
    }

    #[test]
    fn test_update_viewport_state_keeps_floating_rect_while_maximized() {
        let mut current = ViewportState::new(TabKey::Vitals, [10.0, 10.0], [400.0, 300.0]);
        let info = egui::ViewportInfo {
            maximized: Some(true),
            outer_rect: Some(Rect::from_min_size(
                Pos2::new(0.0, 0.0),
                Vec2::new(1920.0, 1080.0),
            )),
            ..Default::default()
        };
        assert!(VellumGuiApp::update_viewport_state_from_info(
            &mut current,
            &info
        ));
        assert!(current.maximized);
        assert_eq!(current.outer_pos_px, [10.0, 10.0]);
        assert_eq!(current.outer_size_px, [400.0, 300.0]);
    }

    #[test]
    fn test_update_viewport_state_captures_geometry_with_epsilon() {
        let mut current = ViewportState::new(TabKey::Vitals, [10.0, 10.0], [400.0, 300.0]);
        let unchanged = egui::ViewportInfo {
            outer_rect: Some(Rect::from_min_size(
                Pos2::new(10.2, 10.0),
                Vec2::new(400.0, 300.0),
            )),
            ..Default::default()
        };
        assert!(!VellumGuiApp::update_viewport_state_from_info(
            &mut current,
            &unchanged
        ));

        let moved = egui::ViewportInfo {
            outer_rect: Some(Rect::from_min_size(
                Pos2::new(250.0, 80.0),
                Vec2::new(640.0, 480.0),
            )),
            ..Default::default()
        };
        assert!(VellumGuiApp::update_viewport_state_from_info(
            &mut current,
            &moved
        ));
        assert_eq!(current.outer_pos_px, [250.0, 80.0]);
        assert_eq!(current.outer_size_px, [640.0, 480.0]);
    }
}
