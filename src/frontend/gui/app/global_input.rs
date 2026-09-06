//! Global keyboard dispatch: collecting key presses (including Windows
//! numpad capture), resolving macro/core-action/app-shortcut targets, and
//! the egui<->frontend key/modifier conversions.

use super::*;

impl VellumGuiApp {
    pub(super) fn handle_global_input(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let numpad_presses = Self::collect_numpad_key_events(frame);

        // Stash numpad presses for keybind editors, which render later this frame and
        // have no access to `&Frame` to read the numpad channel themselves.
        self.frame_numpad_presses.clear();
        self.frame_numpad_presses
            .extend(numpad_presses.iter().map(|press| press.key_event));

        let mut key_presses = numpad_presses;
        key_presses.extend(Self::collect_pressed_key_events(ctx));
        if key_presses.is_empty() {
            return;
        }

        let suppress_macro_dispatch = self.should_suppress_macro_dispatch();
        let mut consumed_keyboard_input = false;

        for key_press in key_presses {
            // Plain Tab in a focused command input, mirroring the TUI's rule:
            //
            // - Dot input: dot-command / window-name completion advances
            //   first (classic candidate cycling); once it has nothing new,
            //   Tab accepts the history ghost. `.la` Tab Tab → ".launch" →
            //   ".launch nisugi". Handled inline here (needs &mut self), and
            //   the key is consumed so the TextEdit doesn't insert a tab.
            // - Plain input with a visible ghost: leave the event unconsumed
            //   so the TextEdit accepts the suggestion later this frame.
            // - Otherwise: normal Tab keybind dispatch (switch window).
            if key_press.key_event.code == crate::data::input::KeyCode::Tab
                && key_press.key_event.modifiers == crate::data::input::KeyModifiers::NONE
            {
                if self.command_input.starts_with('.')
                    && Self::command_completion_cursor_ready(
                        ctx,
                        self.command_input.chars().count(),
                    )
                {
                    if self.advance_input_completion(ctx) {
                        continue;
                    }
                } else if self.command_completion_ready(ctx) {
                    continue;
                }
            }

            // Esc cancels an active .go2 trip from anywhere in the GUI. Gated
            // on the same text-capture modes as macro dispatch so an editor
            // that owns the keyboard keeps its Esc semantics.
            if key_press.key_event.code == crate::data::input::KeyCode::Esc
                && key_press.key_event.modifiers == crate::data::input::KeyModifiers::NONE
                && !suppress_macro_dispatch
                && self.app_core.travel.is_traveling()
            {
                self.app_core.stop_travel();
                consumed_keyboard_input = true;
                ctx.input_mut(|input| {
                    if let Some(logical_key) = key_press.logical_key {
                        input.consume_key(key_press.modifiers, logical_key);
                    }
                    if let Some(physical_key) = key_press.physical_key {
                        input.consume_key(key_press.modifiers, physical_key);
                    }
                });
                continue;
            }

            // Interact mode and popup-menu keyboard navigation take plain
            // arrows/enter/escape before keybinds or text input see them.
            // Window-move and window-context-menu keep their own Esc
            // semantics (handled later this frame).
            if !suppress_macro_dispatch
                && self.window_move_state.is_none()
                && self.window_context_menu.is_none()
                && self.handle_modal_nav_key(&key_press.key_event, ctx)
            {
                consumed_keyboard_input = true;
                ctx.input_mut(|input| {
                    if let Some(logical_key) = key_press.logical_key {
                        input.consume_key(key_press.modifiers, logical_key);
                    }
                    if let Some(physical_key) = key_press.physical_key {
                        input.consume_key(key_press.modifiers, physical_key);
                    }
                });
                continue;
            }

            // A key the user bound to a command-input action belongs to the
            // input widget while it has focus — the widget consumes it before
            // its TextEdit runs. Skip global dispatch entirely so an [app]
            // shortcut sharing the key (close_window defaults to Esc, the
            // usual cursor_clear_line binding) cannot shadow the explicit
            // binding. With the input unfocused the shortcut applies as usual.
            if ctx.memory(|memory| {
                memory.focused() == Some(egui::Id::new(super::COMMAND_INPUT_EDIT_ID))
            }) && matches!(
                self.app_core.keybind_map.get(&key_press.key_event),
                Some(KeyBindAction::Action(name)) if Self::is_command_input_owned_action(name)
            ) {
                continue;
            }

            let target = Self::resolve_global_dispatch_target(
                key_press.key_event,
                &self.app_core.keybind_map,
                &self.app_core.config.app_keybinds,
                suppress_macro_dispatch,
            );
            let Some(target) = target else {
                // Diagnostic: a bound key that resolves to no GUI target is
                // either widget-owned (fine) or a dispatch gap (a bug like
                // the scroll actions had) — name it in the log either way.
                if let Some(binding) = self.app_core.keybind_map.get(&key_press.key_event) {
                    tracing::debug!(
                        "keybind {:?} -> {:?} resolved to no global GUI dispatch target",
                        key_press.key_event,
                        binding
                    );
                }
                continue;
            };

            consumed_keyboard_input = true;
            self.execute_global_dispatch_target(target, ctx);

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
            // Strip keyboard/text events so focused text widgets don't also
            // process a consumed key. This MUST retain on the PROCESSED
            // `input.events`, not `input.raw.events`: egui clones raw.events
            // into events at begin_pass (before update() runs), and the
            // command-input TextEdit reads the processed vector. Filtering
            // raw.events here is a no-op for the current frame -- which is why
            // alt+key / shift+key leaked their printable Event::Text into the
            // input line even though the macro fired (consume_key removes only
            // the Event::Key, never the Event::Text). ctrl+key never leaked
            // because the OS emits no printable text for control chords.
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
        }
    }

    pub(super) fn collect_pressed_key_events(ctx: &egui::Context) -> Vec<GuiKeyPress> {
        ctx.input(|input| {
            input
                .raw
                .events
                .iter()
                .filter_map(|event| {
                    let egui::Event::Key {
                        key,
                        physical_key,
                        pressed,
                        repeat,
                        modifiers,
                    } = event
                    else {
                        return None;
                    };

                    if !pressed || *repeat {
                        return None;
                    }

                    let key_event = Self::egui_key_to_frontend_event(*key, *modifiers)?;
                    Some(GuiKeyPress {
                        key_event,
                        logical_key: Some(*key),
                        physical_key: *physical_key,
                        modifiers: *modifiers,
                    })
                })
                .collect()
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn collect_numpad_key_events(frame: &eframe::Frame) -> Vec<GuiKeyPress> {
        frame
            .numpad_keys()
            .iter()
            .filter_map(|numpad_key| {
                if !numpad_key.pressed || numpad_key.repeat {
                    return None;
                }

                // keybind_name() is Some only for events eframe consumed (egui
                // never saw them), so dispatch can't double-act with text input.
                let code = Self::numpad_binding_name_to_frontend_code(numpad_key.keybind_name()?)?;
                let modifiers = numpad_key.modifiers;
                let key_event = crate::data::input::KeyEvent::new(
                    code,
                    Self::egui_modifiers_to_frontend(modifiers),
                );

                Some(GuiKeyPress {
                    key_event,
                    logical_key: None,
                    physical_key: None,
                    modifiers,
                })
            })
            .collect()
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn collect_numpad_key_events(_frame: &eframe::Frame) -> Vec<GuiKeyPress> {
        Vec::new()
    }

    /// Tell eframe which numpad keys actually have bindings, so unbound keys
    /// keep their native behavior (typing digits, NumpadEnter submitting text).
    /// Cheap when nothing changed; call every frame so edits from the keybind
    /// editor and dot-commands are picked up wherever they happen.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn sync_numpad_capture_keys(&mut self, frame: &mut eframe::Frame) {
        let keys = self.bound_numpad_capture_keys();
        if self.numpad_capture_keys.as_ref() != Some(&keys) {
            frame.set_numpad_capture_keys(Some(keys.clone()));
            self.numpad_capture_keys = Some(keys);
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn sync_numpad_capture_keys(&mut self, _frame: &mut eframe::Frame) {}

    /// Numpad keybind names ("num_1", "num_plus", …) with a user binding or an
    /// app shortcut, i.e. the keys `handle_global_input` can actually dispatch.
    ///
    /// While a keybind editor is waiting for a key press, every numpad key is
    /// captured instead: an unbound key would otherwise keep its native behavior and
    /// never surface as an event, so it could never be recorded — you could only
    /// rebind numpad keys that were already bound.
    pub(super) fn bound_numpad_capture_keys(&self) -> HashSet<String> {
        if self.keybind_capture_armed()
            || self.menu_keybind_capture_armed()
            || self.hotbar_capture_armed()
        {
            return crate::data::input::keypad_names()
                .map(str::to_string)
                .collect();
        }

        let mut keys: HashSet<String> = self
            .app_core
            .keybind_map
            .keys()
            .filter_map(|event| Self::frontend_code_to_numpad_binding_name(event.code))
            .map(str::to_string)
            .collect();

        let app_keybinds = &self.app_core.config.app_keybinds;
        for binding in [
            &app_keybinds.quit,
            &app_keybinds.start_search,
            &app_keybinds.close_window,
        ] {
            if let Some((code, _)) = crate::config::parse_key_string(binding) {
                if let Some(name) = Self::frontend_code_to_numpad_binding_name(code) {
                    keys.insert(name.to_string());
                }
            }
        }
        keys
    }

    /// The eframe capture name for a numpad key. eframe uses the same canonical word
    /// form as `keybinds.toml`, so this is the shared table in both directions.
    pub(super) fn frontend_code_to_numpad_binding_name(
        code: crate::data::input::KeyCode,
    ) -> Option<&'static str> {
        code.keypad_name()
    }

    pub(super) fn numpad_binding_name_to_frontend_code(
        binding: &str,
    ) -> Option<crate::data::input::KeyCode> {
        crate::data::input::KeyCode::from_keypad_name(binding)
    }

    pub(super) fn resolve_global_dispatch_target(
        key_event: crate::data::input::KeyEvent,
        keybind_map: &HashMap<crate::data::input::KeyEvent, KeyBindAction>,
        app_keybinds: &AppKeybinds,
        suppress_macro_dispatch: bool,
    ) -> Option<GlobalDispatchTarget> {
        if !suppress_macro_dispatch {
            match keybind_map.get(&key_event) {
                Some(binding @ KeyBindAction::Macro(_)) => {
                    return Some(GlobalDispatchTarget::Macro(binding.clone()));
                }
                // Action bindings split three ways:
                //  - core-global (mode/system toggles, TTS) → run in AppCore;
                //  - GUI-widget actions (history, tab nav, search, window
                //    switch) → run in the GUI via try_gui_command_action so a
                //    REBOUND key reaches them (the command-input widget still
                //    consumes the default Enter/↑/↓ before this path);
                //  - scroll actions → run through execute_macro_keybind, whose
                //    try_gui_scroll_action applies them to the GUI scroll model
                //    (this is also the controller path, so keyboard and pad
                //    binds share one implementation);
                //  - everything else stays widget-local (cursor/clipboard are
                //    egui-native).
                Some(binding @ KeyBindAction::Action(name)) => {
                    if crate::config::KeyAction::from_str(name)
                        .is_some_and(Self::is_core_global_action)
                        || Self::is_gui_scroll_action(name)
                    {
                        return Some(GlobalDispatchTarget::Macro(binding.clone()));
                    }
                    if Self::is_gui_command_action(name) {
                        return Some(GlobalDispatchTarget::GuiCommandAction(name.clone()));
                    }
                }
                None => {}
            }
        }

        Self::app_shortcut_for_key(key_event, app_keybinds).map(GlobalDispatchTarget::Shortcut)
    }

    /// Action keybinds dispatched globally in the GUI to `try_gui_command_action`.
    ///
    /// This is the single dispatch point for actions that had NO working GUI
    /// path before (parity with the TUI's keybind router). It deliberately
    /// excludes actions already owned by another single owner, to avoid
    /// double-firing on their default keys:
    ///  - send_command / previous_command / next_command / cursor_clear_line →
    ///    owned by the command-input widget, which reads the keybind config
    ///    itself (`command_input_action_for_key`), so rebinding still works;
    ///  - switch_current_window (Tab) and clear_search (Esc) → owned by the
    ///    modal-nav / search-close paths on their default keys.
    pub(super) fn is_gui_command_action(name: &str) -> bool {
        matches!(
            name,
            "send_last_command"
                | "send_second_last_command"
                | "next_tab"
                | "prev_tab"
                | "next_unread_tab"
                | "switch_current_window"
                | "start_search"
                | "next_search_match"
                | "prev_search_match"
        )
    }

    /// Action keybinds owned by the command-input widget (which consumes
    /// their bound keys itself, before its TextEdit runs — see
    /// `CommandInputKeys`). While the input has focus these must never be
    /// dispatched or shadowed globally.
    pub(super) fn is_command_input_owned_action(name: &str) -> bool {
        matches!(
            name,
            "send_command"
                | "previous_command"
                | "next_command"
                | "cursor_left"
                | "cursor_right"
                | "cursor_word_left"
                | "cursor_word_right"
                | "cursor_home"
                | "cursor_end"
                | "cursor_backspace"
                | "cursor_delete"
                | "cursor_delete_word"
                | "cursor_clear_line"
                | "select_all"
                | "copy"
                | "paste"
        )
    }

    /// Scroll actions applied to the GUI's own scroll model — must match the
    /// names `try_gui_scroll_action` handles, so a hit here is guaranteed to
    /// be consumed there and never fall through to the core scroll code
    /// (which only the TUI renders).
    pub(super) fn is_gui_scroll_action(name: &str) -> bool {
        matches!(
            name,
            "scroll_current_window_up_page"
                | "scroll_current_window_down_page"
                | "scroll_current_window_up_one"
                | "scroll_current_window_down_one"
                | "scroll_current_window_home"
                | "scroll_current_window_end"
        )
    }

    /// Action keybinds that execute fully inside AppCore, safe to dispatch
    /// globally in the GUI (everything else is widget-level and handled by
    /// the GUI's own input paths).
    pub(super) fn is_core_global_action(action: crate::config::KeyAction) -> bool {
        use crate::config::KeyAction;
        matches!(
            action,
            KeyAction::InteractMode
                | KeyAction::StopTravel
                | KeyAction::TargetNext
                | KeyAction::TargetPrevious
                | KeyAction::ToggleSounds
                | KeyAction::TogglePerformanceStats
                | KeyAction::TtsNext
                | KeyAction::TtsPrevious
                | KeyAction::TtsNextUnread
                | KeyAction::TtsStop
                | KeyAction::TtsMuteToggle
                | KeyAction::TtsIncreaseRate
                | KeyAction::TtsDecreaseRate
                | KeyAction::TtsIncreaseVolume
                | KeyAction::TtsDecreaseVolume
        )
    }

    pub(super) fn app_shortcut_for_key(
        key_event: crate::data::input::KeyEvent,
        app_keybinds: &AppKeybinds,
    ) -> Option<AppShortcut> {
        if Self::binding_matches_key_event(&app_keybinds.quit, key_event) {
            return Some(AppShortcut::Quit);
        }
        if Self::binding_matches_key_event(&app_keybinds.start_search, key_event) {
            return Some(AppShortcut::StartSearch);
        }
        // The [app] section's own match-step fields (the [user] action
        // versions route separately via GuiCommandAction).
        if Self::binding_matches_key_event(&app_keybinds.next_search_match, key_event) {
            return Some(AppShortcut::NextSearchMatch);
        }
        if Self::binding_matches_key_event(&app_keybinds.prev_search_match, key_event) {
            return Some(AppShortcut::PrevSearchMatch);
        }
        if Self::binding_matches_key_event(&app_keybinds.close_window, key_event) {
            return Some(AppShortcut::CloseWindow);
        }
        None
    }

    pub(super) fn binding_matches_key_event(
        binding: &str,
        key_event: crate::data::input::KeyEvent,
    ) -> bool {
        crate::config::parse_key_string(binding)
            .map(|(code, modifiers)| crate::data::input::KeyEvent::new(code, modifiers))
            .is_some_and(|candidate| candidate == key_event)
    }

    pub(super) fn should_suppress_macro_dispatch(&self) -> bool {
        matches!(
            self.app_core.ui_state.input_mode,
            InputMode::KeybindForm | InputMode::Search
        )
    }

    pub(super) fn execute_global_dispatch_target(
        &mut self,
        target: GlobalDispatchTarget,
        ctx: &egui::Context,
    ) {
        match target {
            GlobalDispatchTarget::Macro(action) => self.execute_macro_keybind(&action, ctx),
            GlobalDispatchTarget::Shortcut(shortcut) => self.execute_app_shortcut(shortcut, ctx),
            GlobalDispatchTarget::GuiCommandAction(name) => {
                self.try_gui_command_action(&name, ctx);
            }
        }
    }

    /// Scroll actions bound to keys or controller buttons, applied to the
    /// GUI's own scroll model — the core implementations move
    /// `content.scroll_offset`, which only the TUI renders. v1 targets the
    /// "main" story window. Returns true when the action was a scroll.
    pub(super) fn try_gui_scroll_action(
        &mut self,
        action: &KeyBindAction,
        ctx: &egui::Context,
    ) -> bool {
        let KeyBindAction::Action(name) = action else {
            return false;
        };
        // Which window to scroll, in order of how visible the choice is to
        // the user (core focus has NO indicator in the GUI and Tab moves it
        // silently, so it must never win over what the user can see):
        //  1. the text window under the pointer (stashed fresh each pass);
        //  2. the core-focused window, if it is actually visible;
        //  3. "main".
        let hovered: Option<String> = ctx
            .data_mut(|d| d.get_temp::<(u64, String)>(egui::Id::new("text_scroll_hovered")))
            .filter(|(pass, _)| pass + 1 >= ctx.cumulative_pass_nr())
            .map(|(_, id)| id);
        let focused = self.app_core.get_focused_window_name();
        let focused_visible = self
            .app_core
            .ui_state
            .windows
            .get(&focused)
            .is_some_and(|window| window.visible);
        let target: String = hovered.unwrap_or(if focused_visible && !focused.is_empty() {
            focused
        } else {
            "main".to_string()
        });
        // A window NAME is not always a SCROLL id: tabbed windows render
        // each tab under "{name}::tab{N}" (independent scroll state per
        // tab), so a tabbed target resolves to its active tab's pane. The
        // hovered stash already carries a real scroll id and passes through
        // unchanged (it never names a tabbed window bare).
        let scroll_id: String = match self
            .app_core
            .ui_state
            .windows
            .get(&target)
            .map(|window| &window.content)
        {
            Some(crate::data::WindowContent::TabbedText(tabbed)) => {
                format!("{}::tab{}", target, tabbed.active_tab_index)
            }
            _ => target,
        };
        let scroll_id: &str = &scroll_id;
        let view_h: f32 = ctx
            .data_mut(|d| d.get_temp(egui::Id::new(("text_scroll_view_h", scroll_id))))
            .unwrap_or(400.0);
        // (kind, value): 0 = relative px, 1 = home, 2 = end
        let request: (u8, f32) = match name.as_str() {
            "scroll_current_window_up_page" => (0, -(view_h * 0.85)),
            "scroll_current_window_down_page" => (0, view_h * 0.85),
            "scroll_current_window_up_one" => (0, -48.0),
            "scroll_current_window_down_one" => (0, 48.0),
            "scroll_current_window_home" => (1, 0.0),
            "scroll_current_window_end" => (2, 0.0),
            _ => return false,
        };
        // Diagnostic (scroll-to-top hunt): every programmatic scroll names
        // its action so a runaway producer shows in vellum-fe.log.
        tracing::info!("scrollreq action={name} window={scroll_id} req={request:?}");
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(("text_scroll_pending", scroll_id)), request));
        ctx.request_repaint();
        true
    }

    pub(super) fn execute_macro_keybind(&mut self, action: &KeyBindAction, ctx: &egui::Context) {
        if self.try_gui_scroll_action(action, ctx) {
            return;
        }
        #[cfg(feature = "gamepad")]
        if matches!(action, KeyBindAction::Action(name) if name == "controller_overlay") {
            self.gp_overlay = !self.gp_overlay;
            ctx.request_repaint();
            return;
        }
        match self.app_core.execute_keybind_action(action) {
            Ok(outcomes) => {
                for outcome in outcomes {
                    match outcome {
                        crate::data::CommandOutcome::Game(outbound) => {
                            let sent = if Self::should_send_to_network(&outbound) {
                                self.app_core
                                    .perf_stats
                                    .record_bytes_sent((outbound.len() + 1) as u64);
                                self.command_tx.send(outbound.clone()).is_ok()
                            } else {
                                false
                            };
                            self.app_core.finish_game_command_send(&outbound, sent);
                        }
                        crate::data::CommandOutcome::Handled => {}
                        // A macro bound to a dot-command that opens an
                        // editor: perform it here.
                        crate::data::CommandOutcome::Ui(ui) => self.handle_ui_action(ui),
                    }
                }
            }
            Err(err) => {
                self.app_core
                    .add_system_message(&format!("Keybind error: {}", err));
            }
        }

        if !self.app_core.running {
            self.close_requested = true;
        }
    }

    pub(super) fn execute_app_shortcut(&mut self, shortcut: AppShortcut, ctx: &egui::Context) {
        match shortcut {
            AppShortcut::Quit => {
                self.app_core.quit();
                self.close_requested = true;
            }
            AppShortcut::NextSearchMatch => self.step_search_match(true, ctx),
            AppShortcut::PrevSearchMatch => self.step_search_match(false, ctx),
            AppShortcut::StartSearch => {
                self.app_core.start_search_mode();
                self.search_bar_needs_focus = true;
            }
            AppShortcut::CloseWindow => self.handle_close_window_shortcut(),
        }
    }

    /// Run a GUI-widget keybind action (see `is_gui_command_action`). Reached
    /// from a globally-dispatched key so a REBOUND binding works; the default
    /// Enter/↑/↓ are still handled first by the focused command-input widget.
    pub(super) fn try_gui_command_action(&mut self, name: &str, ctx: &egui::Context) {
        match name {
            "send_last_command" => self.resend_history_command(0),
            "send_second_last_command" => self.resend_history_command(1),
            "next_tab" => self.cycle_tabbed_tabs(true),
            "prev_tab" => self.cycle_tabbed_tabs(false),
            "next_unread_tab" => self.goto_unread_tab(),
            "switch_current_window" => self.cycle_focused_window(),
            "start_search" => {
                self.app_core.start_search_mode();
                self.search_bar_needs_focus = true;
                self.search_match_index = None;
                self.search_match_window = None;
            }
            "next_search_match" => self.step_search_match(true, ctx),
            "prev_search_match" => self.step_search_match(false, ctx),
            _ => {}
        }
        ctx.request_repaint();
    }

    /// Re-send a past command by history index (0 = most recent). Used by
    /// send_last_command / send_second_last_command.
    pub(super) fn resend_history_command(&mut self, index: usize) {
        if let Some(cmd) = self.command_history.get(index).cloned() {
            self.dispatch_command(cmd);
        }
    }

    /// Every window a search can target, as `(label, scroll_id)` pairs in
    /// display order. The label is what the Find bar shows; the scroll id is
    /// what the scroll-pending protocol keys on (`name`, or `name::tabN` for
    /// the active tab of a tabbed window). One list, so the dropdown can
    /// never offer a target that match-nav can't actually search.
    ///
    /// Every scrollable text-ish content participates, not just plain Text:
    /// inventory/reserve/spells windows highlight matches too, so they must
    /// be navigable as well.
    pub(super) fn search_targets(app_core: &AppCore) -> Vec<(String, String)> {
        let mut targets: Vec<(String, String)> = app_core
            .ui_state
            .windows
            .values()
            .filter_map(|window| match &window.content {
                WindowContent::Text(_)
                | WindowContent::Inventory(_)
                | WindowContent::Reserve(_)
                | WindowContent::Spells(_) => Some((window.name.clone(), window.name.clone())),
                WindowContent::TabbedText(tabbed) => {
                    let tab = tabbed.tabs.get(tabbed.active_tab_index)?;
                    // Name the tab, not just the window: "thoughts" is what
                    // the user is looking at, "tabs::tab2" is bookkeeping.
                    Some((
                        format!("{} ▸ {}", window.name, tab.definition.name),
                        format!("{}::tab{}", window.name, tabbed.active_tab_index),
                    ))
                }
                _ => None,
            })
            .collect();
        // `windows` is a HashMap; sort so the dropdown holds still.
        targets.sort();
        targets
    }

    /// Resolve a scroll id to the buffer it names, following `name::tabN`
    /// into the tab's own content.
    pub(super) fn search_content<'a>(
        app_core: &'a AppCore,
        scroll_id: &str,
    ) -> Option<&'a crate::data::TextContent> {
        let (window_name, tab_index) = match scroll_id.split_once("::tab") {
            Some((name, index)) => (name, index.parse::<usize>().ok()),
            None => (scroll_id, None),
        };
        let window = app_core
            .ui_state
            .windows
            .values()
            .find(|window| window.name == window_name)?;
        match (&window.content, tab_index) {
            (WindowContent::TabbedText(tabbed), Some(index)) => {
                Some(&tabbed.tabs.get(index)?.content)
            }
            (
                WindowContent::Text(content)
                | WindowContent::Inventory(content)
                | WindowContent::Reserve(content)
                | WindowContent::Spells(content),
                _,
            ) => Some(content),
            _ => None,
        }
    }

    /// Absolute line number of buffer index `index`: the value `generation`
    /// held when that line was appended. Stable as the ring buffer drops
    /// lines off the front, which plain indices are not.
    pub(super) fn absolute_line(content: &crate::data::TextContent, index: usize) -> u64 {
        // The newest line (index len-1) was the `generation`-th appended.
        content
            .generation
            .wrapping_sub((content.lines.len().saturating_sub(index + 1)) as u64)
    }

    /// Inverse of `absolute_line`: where that line sits in the buffer now,
    /// or None once it has scrolled off the front.
    pub(super) fn line_index_of(
        content: &crate::data::TextContent,
        absolute: u64,
    ) -> Option<usize> {
        // `age` = how many lines have been appended since this one.
        let age = content.generation.checked_sub(absolute)? as usize;
        let len = content.lines.len();
        // Older than the buffer keeps: it has been dropped off the front.
        len.checked_sub(age + 1)
    }

    /// Resolve a scroll id to its buffer and the line indices matching
    /// `query` (already lowercased). None when the id names nothing
    /// searchable; an empty vec when it searches clean.
    pub(super) fn search_hits_in(
        app_core: &AppCore,
        scroll_id: &str,
        query: &str,
    ) -> Option<(String, Vec<usize>)> {
        // Split `name::tabN` back into window + tab index.
        let (window_name, tab_index) = match scroll_id.split_once("::tab") {
            Some((name, index)) => (name, index.parse::<usize>().ok()),
            None => (scroll_id, None),
        };
        let window = app_core
            .ui_state
            .windows
            .values()
            .find(|window| window.name == window_name)?;
        let content = match (&window.content, tab_index) {
            (WindowContent::TabbedText(tabbed), Some(index)) => &tabbed.tabs.get(index)?.content,
            (
                WindowContent::Text(content)
                | WindowContent::Inventory(content)
                | WindowContent::Reserve(content)
                | WindowContent::Spells(content),
                _,
            ) => content,
            _ => return None,
        };
        let hits = content
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.segments
                    .iter()
                    .any(|segment| Self::find_ascii_ci(&segment.text, query, 0).is_some())
            })
            .map(|(index, _)| index)
            .collect();
        Some((scroll_id.to_string(), hits))
    }

    /// Step search match-navigation through the window the user selected in
    /// the Find bar (falling back to the keyboard focus until they pick).
    /// Moves the match cursor forward/back (wrapping) and queues a scroll
    /// that brings the matching line into view. No-op with an empty query.
    pub(super) fn step_search_match(&mut self, forward: bool, ctx: &egui::Context) {
        let query = self
            .app_core
            .ui_state
            .search_input
            .trim()
            .to_ascii_lowercase();
        if query.is_empty() {
            return;
        }
        // Which window to search is the user's choice, never a guess: the
        // search bar's target dropdown owns it (`search_target`). Until they
        // pick, fall back to the keyboard focus if it happens to hold text —
        // and if it doesn't (it is normally `command_input`, which has no
        // buffer at all), leave the target unset and let the bar prompt.
        let target = self
            .search_target
            .clone()
            .unwrap_or_else(|| self.app_core.get_focused_window_name());
        let Some((scroll_id, matches)) = Self::search_hits_in(&self.app_core, &target, &query)
        else {
            self.app_core.add_system_message(&format!(
                "'{target}' has no searchable text — choose a window in the Find bar."
            ));
            return;
        };
        // The window was chosen because it has hits, so this can't be empty;
        // guard anyway rather than indexing into nothing.
        if matches.is_empty() {
            return;
        }
        let Some(content) = Self::search_content(&self.app_core, &scroll_id) else {
            return;
        };

        // Where the cursor sits now, as a buffer index. The cursor is an
        // absolute line number, so this survives lines scrolling off the
        // top while the user pages through a live window. A cursor whose
        // line has already scrolled away resolves to None and restarts.
        let current_index = self
            .search_match_index
            .and_then(|absolute| Self::line_index_of(content, absolute));
        // Advance within the match list (wrapping).
        let next = match current_index {
            None => {
                if forward {
                    0
                } else {
                    matches.len() - 1
                }
            }
            Some(current) => {
                // The cursor line may no longer be a match (edited buffer)
                // or may sit between matches; land on the nearest one in the
                // direction of travel rather than snapping to the start.
                if forward {
                    matches.iter().position(|&m| m > current).unwrap_or(0)
                } else {
                    matches
                        .iter()
                        .rposition(|&m| m < current)
                        .unwrap_or(matches.len() - 1)
                }
            }
        };
        let target_line = matches[next];
        self.search_match_index = Some(Self::absolute_line(content, target_line));
        // Request a scroll-to-line for the chosen window (see the
        // scroll-pending protocol in widgets.rs, kind 3 = absolute line
        // index). The scroll id is the window name, or `name::tabN`.
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new(("text_scroll_pending", scroll_id.as_str())),
                (3u8, target_line as f32),
            )
        });
        // Deliberately silent: the search bar renders "N of M in <window>"
        // every frame, so echoing each step into the story window would
        // bury the very text the user is searching through.
        self.search_match_window = Some(scroll_id);
        self.app_core.needs_render = true;
    }

    /// Advance the "current window" (switch_current_window), the same
    /// core-owned focus the TUI uses (`ui_state.focused_window`) — search
    /// match-nav and scroll target it. Raises the newly-focused window's tab
    /// to the front as a visual cue.
    pub(super) fn cycle_focused_window(&mut self) {
        self.app_core.cycle_focused_window();
        // Reset the match cursor: a new window means a fresh match list.
        self.search_match_index = None;
        self.search_match_window = None;
        // Bring the focused window's tab forward, if it maps to one.
        let focused = self.app_core.get_focused_window_name();
        if let Some(key) = self
            .available_tabs
            .iter()
            .find(|(_, tab)| tab.window_name == focused)
            .map(|(key, _)| key.clone())
        {
            self.pending_raise_tab = Some(key);
        }
        self.app_core.needs_render = true;
    }

    pub(super) fn handle_close_window_shortcut(&mut self) {
        // Move mode owns Esc: the move overlay cancels and restores the
        // window's original position later this frame.
        if self.window_move_state.is_some() {
            return;
        }
        if self.window_context_menu.is_some() {
            self.window_context_menu = None;
            return;
        }
        if self.app_core.ui_state.input_mode == InputMode::Search {
            self.app_core.clear_search_mode();
            return;
        }

        if !matches!(self.app_core.ui_state.input_mode, InputMode::Normal) {
            self.app_core.ui_state.input_mode = InputMode::Normal;
            self.app_core.ui_state.popup_menu = None;
            self.app_core.ui_state.submenu = None;
            self.app_core.ui_state.nested_submenu = None;
            self.app_core.ui_state.deep_submenu = None;
            self.app_core.ui_state.active_dialog = None;
            self.app_core.needs_render = true;
        }
    }

    pub(super) fn egui_key_to_frontend_event(
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) -> Option<crate::data::input::KeyEvent> {
        let code = Self::egui_key_to_frontend_code(key, modifiers)?;
        let modifiers = Self::egui_modifiers_to_frontend(modifiers);
        Some(crate::data::input::KeyEvent::new(code, modifiers))
    }

    /// Inverse of `egui_key_to_frontend_code` for the keys that can bind to a
    /// command-input action. Lets the command-input widget resolve which egui
    /// keys are bound to submit/history/clear-line from the config, so those
    /// actions honor rebinding (single source of truth). Returns None for keys
    /// that can't be a command-input default (the widget just won't match them).
    pub(super) fn frontend_keycode_to_egui(code: crate::data::input::KeyCode) -> Option<egui::Key> {
        use crate::data::input::KeyCode;
        Some(match code {
            KeyCode::Up => egui::Key::ArrowUp,
            KeyCode::Down => egui::Key::ArrowDown,
            KeyCode::Left => egui::Key::ArrowLeft,
            KeyCode::Right => egui::Key::ArrowRight,
            KeyCode::Enter => egui::Key::Enter,
            KeyCode::Esc => egui::Key::Escape,
            KeyCode::Tab => egui::Key::Tab,
            KeyCode::Home => egui::Key::Home,
            KeyCode::End => egui::Key::End,
            KeyCode::PageUp => egui::Key::PageUp,
            KeyCode::PageDown => egui::Key::PageDown,
            KeyCode::Backspace => egui::Key::Backspace,
            KeyCode::Delete => egui::Key::Delete,
            KeyCode::F(n) => match n {
                1 => egui::Key::F1,
                2 => egui::Key::F2,
                3 => egui::Key::F3,
                4 => egui::Key::F4,
                5 => egui::Key::F5,
                6 => egui::Key::F6,
                7 => egui::Key::F7,
                8 => egui::Key::F8,
                9 => egui::Key::F9,
                10 => egui::Key::F10,
                11 => egui::Key::F11,
                12 => egui::Key::F12,
                _ => return None,
            },
            KeyCode::Char(c) => return egui::Key::from_name(&c.to_uppercase().to_string()),
            _ => return None,
        })
    }

    /// Resolve which egui keys are bound to the command-input actions and stash
    /// them for the widget (see `CommandInputKeys`). The hardcoded defaults
    /// (Enter/↑/↓) are seeded first so the input never dies on a partial config;
    /// any additional bound keys are appended.
    pub(super) fn stash_command_input_keys(&self, ctx: &egui::Context) {
        let mut keys = CommandInputKeys {
            submit: vec![egui::Key::Enter],
            history_prev: vec![egui::Key::ArrowUp],
            history_next: vec![egui::Key::ArrowDown],
            ..Default::default()
        };
        for (event, action) in &self.app_core.keybind_map {
            let KeyBindAction::Action(name) = action else {
                continue;
            };
            let Some(key) = Self::frontend_keycode_to_egui(event.code) else {
                continue;
            };
            let modifiers = Self::frontend_modifiers_to_egui(event.modifiers);
            match name.as_str() {
                "send_command" if modifiers.is_none() && !keys.submit.contains(&key) => {
                    keys.submit.push(key)
                }
                "previous_command" if modifiers.is_none() && !keys.history_prev.contains(&key) => {
                    keys.history_prev.push(key)
                }
                "next_command" if modifiers.is_none() && !keys.history_next.contains(&key) => {
                    keys.history_next.push(key)
                }
                "cursor_clear_line" => keys.clear_line.push((key, modifiers)),
                "cursor_left" => keys.cursor_left.push((key, modifiers)),
                "cursor_right" => keys.cursor_right.push((key, modifiers)),
                "cursor_word_left" => keys.cursor_word_left.push((key, modifiers)),
                "cursor_word_right" => keys.cursor_word_right.push((key, modifiers)),
                "cursor_home" => keys.cursor_home.push((key, modifiers)),
                "cursor_end" => keys.cursor_end.push((key, modifiers)),
                "cursor_backspace" => keys.cursor_backspace.push((key, modifiers)),
                "cursor_delete" => keys.cursor_delete.push((key, modifiers)),
                "cursor_delete_word" => keys.cursor_delete_word.push((key, modifiers)),
                "select_all" => keys.select_all.push((key, modifiers)),
                "copy" => keys.copy.push((key, modifiers)),
                "paste" => keys.paste.push((key, modifiers)),
                _ => {}
            }
        }
        ctx.data_mut(|data| data.insert_temp(CommandInputKeys::id(), keys));
    }

    pub(super) fn frontend_modifiers_to_egui(
        modifiers: crate::data::input::KeyModifiers,
    ) -> egui::Modifiers {
        egui::Modifiers {
            alt: modifiers.alt,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            mac_cmd: false,
            command: modifiers.ctrl,
        }
    }

    pub(super) fn egui_modifiers_to_frontend(
        modifiers: egui::Modifiers,
    ) -> crate::data::input::KeyModifiers {
        crate::data::input::KeyModifiers {
            ctrl: modifiers.ctrl || modifiers.command,
            shift: modifiers.shift,
            alt: modifiers.alt,
        }
    }

    pub(super) fn egui_key_to_frontend_code(
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) -> Option<crate::data::input::KeyCode> {
        let code = match key {
            egui::Key::ArrowDown => crate::data::input::KeyCode::Down,
            egui::Key::ArrowLeft => crate::data::input::KeyCode::Left,
            egui::Key::ArrowRight => crate::data::input::KeyCode::Right,
            egui::Key::ArrowUp => crate::data::input::KeyCode::Up,
            egui::Key::Escape => crate::data::input::KeyCode::Esc,
            egui::Key::Tab => {
                if modifiers.shift {
                    crate::data::input::KeyCode::BackTab
                } else {
                    crate::data::input::KeyCode::Tab
                }
            }
            egui::Key::Backspace => crate::data::input::KeyCode::Backspace,
            egui::Key::Enter => crate::data::input::KeyCode::Enter,
            egui::Key::Space => crate::data::input::KeyCode::Char(' '),
            egui::Key::Insert => crate::data::input::KeyCode::Insert,
            egui::Key::Delete => crate::data::input::KeyCode::Delete,
            egui::Key::Home => crate::data::input::KeyCode::Home,
            egui::Key::End => crate::data::input::KeyCode::End,
            egui::Key::PageUp => crate::data::input::KeyCode::PageUp,
            egui::Key::PageDown => crate::data::input::KeyCode::PageDown,
            egui::Key::Num0 => crate::data::input::KeyCode::Char('0'),
            egui::Key::Num1 => crate::data::input::KeyCode::Char('1'),
            egui::Key::Num2 => crate::data::input::KeyCode::Char('2'),
            egui::Key::Num3 => crate::data::input::KeyCode::Char('3'),
            egui::Key::Num4 => crate::data::input::KeyCode::Char('4'),
            egui::Key::Num5 => crate::data::input::KeyCode::Char('5'),
            egui::Key::Num6 => crate::data::input::KeyCode::Char('6'),
            egui::Key::Num7 => crate::data::input::KeyCode::Char('7'),
            egui::Key::Num8 => crate::data::input::KeyCode::Char('8'),
            egui::Key::Num9 => crate::data::input::KeyCode::Char('9'),
            egui::Key::A => crate::data::input::KeyCode::Char('a'),
            egui::Key::B => crate::data::input::KeyCode::Char('b'),
            egui::Key::C => crate::data::input::KeyCode::Char('c'),
            egui::Key::D => crate::data::input::KeyCode::Char('d'),
            egui::Key::E => crate::data::input::KeyCode::Char('e'),
            egui::Key::F => crate::data::input::KeyCode::Char('f'),
            egui::Key::G => crate::data::input::KeyCode::Char('g'),
            egui::Key::H => crate::data::input::KeyCode::Char('h'),
            egui::Key::I => crate::data::input::KeyCode::Char('i'),
            egui::Key::J => crate::data::input::KeyCode::Char('j'),
            egui::Key::K => crate::data::input::KeyCode::Char('k'),
            egui::Key::L => crate::data::input::KeyCode::Char('l'),
            egui::Key::M => crate::data::input::KeyCode::Char('m'),
            egui::Key::N => crate::data::input::KeyCode::Char('n'),
            egui::Key::O => crate::data::input::KeyCode::Char('o'),
            egui::Key::P => crate::data::input::KeyCode::Char('p'),
            egui::Key::Q => crate::data::input::KeyCode::Char('q'),
            egui::Key::R => crate::data::input::KeyCode::Char('r'),
            egui::Key::S => crate::data::input::KeyCode::Char('s'),
            egui::Key::T => crate::data::input::KeyCode::Char('t'),
            egui::Key::U => crate::data::input::KeyCode::Char('u'),
            egui::Key::V => crate::data::input::KeyCode::Char('v'),
            egui::Key::W => crate::data::input::KeyCode::Char('w'),
            egui::Key::X => crate::data::input::KeyCode::Char('x'),
            egui::Key::Y => crate::data::input::KeyCode::Char('y'),
            egui::Key::Z => crate::data::input::KeyCode::Char('z'),
            egui::Key::F1 => crate::data::input::KeyCode::F(1),
            egui::Key::F2 => crate::data::input::KeyCode::F(2),
            egui::Key::F3 => crate::data::input::KeyCode::F(3),
            egui::Key::F4 => crate::data::input::KeyCode::F(4),
            egui::Key::F5 => crate::data::input::KeyCode::F(5),
            egui::Key::F6 => crate::data::input::KeyCode::F(6),
            egui::Key::F7 => crate::data::input::KeyCode::F(7),
            egui::Key::F8 => crate::data::input::KeyCode::F(8),
            egui::Key::F9 => crate::data::input::KeyCode::F(9),
            egui::Key::F10 => crate::data::input::KeyCode::F(10),
            egui::Key::F11 => crate::data::input::KeyCode::F(11),
            egui::Key::F12 => crate::data::input::KeyCode::F(12),
            // Punctuation → UNSHIFTED base char, matching the keybind map /
            // keybinds.toml convention (parse_key_string stores a single char
            // as Char, key_event_to_string lowercases). Shift stays a modifier
            // rather than folding into ':' / '?' / '{' etc. Without these arms
            // Capture ignored the press and the live matcher never fired the
            // binding (bug: punctuation keys can't be bound in the GUI).
            egui::Key::Semicolon => crate::data::input::KeyCode::Char(';'),
            egui::Key::Comma => crate::data::input::KeyCode::Char(','),
            egui::Key::Period => crate::data::input::KeyCode::Char('.'),
            egui::Key::Slash => crate::data::input::KeyCode::Char('/'),
            egui::Key::Minus => crate::data::input::KeyCode::Char('-'),
            egui::Key::Equals => crate::data::input::KeyCode::Char('='),
            egui::Key::Backtick => crate::data::input::KeyCode::Char('`'),
            egui::Key::OpenBracket => crate::data::input::KeyCode::Char('['),
            egui::Key::CloseBracket => crate::data::input::KeyCode::Char(']'),
            egui::Key::Backslash => crate::data::input::KeyCode::Char('\\'),
            egui::Key::Quote => crate::data::input::KeyCode::Char('\''),
            // Fork also exposes the shifted-glyph variants directly (some
            // layouts/IMEs report ':' as Colon rather than Shift+Semicolon).
            // Map them to the same base char so a binding matches regardless of
            // which the platform emits.
            egui::Key::Colon => crate::data::input::KeyCode::Char(';'),
            egui::Key::Pipe => crate::data::input::KeyCode::Char('\\'),
            egui::Key::Questionmark => crate::data::input::KeyCode::Char('/'),
            egui::Key::Plus => crate::data::input::KeyCode::Char('='),
            _ => return None,
        };
        Some(code)
    }
}
