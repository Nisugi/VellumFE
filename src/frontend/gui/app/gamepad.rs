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
use crate::config::WheelSlice;
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

/// How a committed leaf slice fires. See `[controller_tuning] fire_mode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FireMode {
    /// Fire when the wheel button is released (the default, and the only
    /// mode before this existed).
    Release,
    /// Fire the instant deflection crosses `edge_threshold`, no dwell.
    Edge,
    /// Dwell to commit, then fire when deflection drops `retract_delta`
    /// below its tracked peak — an inward flick without recentering.
    Retract,
}

impl FireMode {
    /// Parse the config string; anything unrecognized falls back to the
    /// safe default (`Release`) so a typo never leaves the wheel unable to
    /// fire.
    fn from_str(s: &str) -> Self {
        match s {
            "edge" => Self::Edge,
            "retract" => Self::Retract,
            _ => Self::Release,
        }
    }
}

/// Edge mode: a leaf under the stick fires the instant deflection reaches
/// `threshold` (both 0.0–1.0). No dwell.
fn edge_should_fire(magnitude: f32, threshold: f32) -> bool {
    magnitude >= threshold
}

/// Retract mode: a committed leaf fires once deflection falls `delta`
/// below its tracked `peak` (all 0.0–1.0). On the commit frame peak equals
/// magnitude, so a positive delta means it never fires without a genuine
/// inward pull; delta 0 fires on any retraction. A tiny epsilon keeps the
/// exact-boundary case from being lost to f32 rounding (e.g. 0.7 - 0.1
/// lands just under 0.6).
fn retract_should_fire(magnitude: f32, peak: f32, delta: f32) -> bool {
    magnitude <= peak - delta + 1e-4
}

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

        // Read both sticks, then assign roles from [controller_tuning]
        // movement_stick: the movement stick walks the compass; the other
        // ("aim" stick) aims the wheel, scrolls the story, and does the
        // interact-mode cycling. y_up follows gilrs (positive = up).
        let (left_xy, right_xy) = self
            .gamepad
            .as_ref()
            .and_then(|g| g.gamepads().next())
            .map(|(_, pad)| {
                (
                    (
                        pad.value(gilrs::Axis::LeftStickX),
                        pad.value(gilrs::Axis::LeftStickY),
                    ),
                    (
                        pad.value(gilrs::Axis::RightStickX),
                        pad.value(gilrs::Axis::RightStickY),
                    ),
                )
            })
            .unwrap_or(((0.0, 0.0), (0.0, 0.0)));
        let move_on_right = self.app_core.config.controller_tuning.movement_stick == "right";
        // `stick` is the movement stick; `aim_x/aim_y` the default aiming
        // stick (the non-movement one). A wheel whose meta names a `stick`
        // overrides the aim stick for its duration (resolved below); when
        // that override picks the movement stick, movement is silenced by
        // the wheel-owns-move guard, so aiming never also walks.
        let stick = Some(if move_on_right { right_xy } else { left_xy });
        // Which stick aims: a held wheel's `stick` override wins (true =
        // right), else the default aim stick (the non-movement one).
        let wheel_override = self
            .held_wheel_key()
            .and_then(|key| self.wheel_aim_stick(&key));
        let aim_on_right = resolve_aim_stick(move_on_right, wheel_override);
        let (aim_x, aim_y) = if aim_on_right { right_xy } else { left_xy };
        // True when the in-effect aim stick is also the movement stick, so
        // movement must be silenced while that wheel is open.
        let aim_is_move_stick = aim_on_right == move_on_right;

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

        // Radial wheel: while a wheel button is held, the aim stick points
        // at a slice and a dwell commits it. Dwelling a folder auto-
        // descends; dwelling the reserved Back slice auto-ascends;
        // releasing fires the committed leaf. Returning the stick to
        // center (nothing committed) then releasing cancels. The wheel
        // owns the aim stick while up (that stick's normal job — movement
        // if it is the movement stick, or scroll — is silenced) and
        // swallows unrelated buttons.
        let held_key = self.held_wheel_key();
        if held_key.is_none() && self.gp_wheel_fired {
            // Fresh hold re-arms after a fired leaf; seed the movement
            // hysteresis so a still-deflected movement stick doesn't walk
            // on the release frame.
            self.gp_wheel_fired = false;
            self.gp_stick_sector =
                stick.and_then(|(x, y_up)| stick_sector(x, y_up, self.gp_stick_sector));
        }
        match (self.gp_wheel.is_some(), held_key) {
            (false, Some(key)) => {
                // A fired leaf keeps the wheel closed for the rest of this
                // hold — otherwise it would instantly reopen with the stick
                // still aimed and could fire again.
                if !self.gp_wheel_fired {
                    self.gp_wheel = Some(WheelUi {
                        key,
                        path: Vec::new(),
                        aimed: None,
                        candidate: None,
                        candidate_since: None,
                        rearm_until_center: false,
                        peak_magnitude: 0.0,
                    });
                    self.app_core.needs_render = true;
                }
            }
            (true, None) => {
                // Release: fire the committed leaf, if any and if the
                // debounce window has elapsed. `wheel_release_command` is
                // the machine-side decision (display-indexed, Back/folder/
                // empty-guarded via `leaf_command_at`).
                let ui = self.gp_wheel.take().expect("just matched Some");
                if let Some(view) = self.wheel_view(&ui.key, &ui.path) {
                    if let Some(command) = wheel_release_command(&ui, &view) {
                        self.wheel_fire(command);
                    }
                }
                // The aim stick is usually still deflected on release; if it
                // is also the movement stick, seed the hysteresis so firing
                // doesn't also walk. And require the aim stick to return to
                // center before its scroll / interact-cycle function resumes,
                // so the leftover deflection can't scroll or cycle.
                self.gp_stick_sector =
                    stick.and_then(|(x, y_up)| stick_sector(x, y_up, self.gp_stick_sector));
                self.gp_aim_recenter_needed = true;
                self.app_core.needs_render = true;
            }
            (true, Some(_)) => {
                self.wheel_aim(aim_x, aim_y);
            }
            (false, None) => {}
        }

        // Compass movement. Suppressed while a wheel owns the aim stick
        // *and* that aim stick is the movement stick (aim_is_move_stick),
        // plus the fired-hold tail. When the wheel aims with the other
        // stick, walking stays live even with the wheel up — matching
        // Niffy's "movement stays on the left" case (e.g. an exits wheel
        // that aims with the right stick).
        let wheel_up = self.gp_wheel.is_some() || self.gp_wheel_fired;
        let wheel_owns_move = wheel_up && aim_is_move_stick;
        if let Some((x, y_up)) = stick.filter(|_| !wheel_owns_move) {
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

        // Aim stick (the in-effect aiming stick — the non-movement one by
        // default, or a wheel's `stick` override): in interact mode it
        // cycles the focus — up/down switch categories, left/right step
        // entities, one step per deflection with the movement hysteresis.
        // Otherwise up/down scrolls the main story window; quadratic curve
        // so small deflections creep and full tilt flies. Stick up scrolls
        // up (negative offset delta). Silenced while it is aiming an open
        // wheel so aiming never also scrolls or cycles.
        // A wheel that just closed leaves the stick deflected; hold the
        // aim stick's normal function until it returns to center once.
        if self.gp_aim_recenter_needed && aim_stick_centered(aim_x, aim_y) {
            self.gp_aim_recenter_needed = false;
        }
        let aim_owned_by_wheel =
            self.gp_wheel.is_some() || self.gp_wheel_fired || self.gp_aim_recenter_needed;
        if aim_owned_by_wheel {
            // Keep the interact hysteresis in sync with the deflected stick
            // so resuming doesn't fire a stale cycle step.
            self.gp_right_dir = four_way(aim_x, aim_y, self.gp_right_dir);
        } else if self.app_core.ui_state.input_mode == InputMode::Interact {
            let dir = four_way(aim_x, aim_y, self.gp_right_dir);
            if dir != self.gp_right_dir {
                match dir {
                    Some(FourWay::Up) => self.app_core.interact_category_move(-1),
                    Some(FourWay::Down) => self.app_core.interact_category_move(1),
                    Some(FourWay::Left) => self.app_core.interact_move(-1),
                    Some(FourWay::Right) => self.app_core.interact_move(1),
                    None => {}
                }
                self.gp_right_dir = dir;
            }
        } else if aim_y.abs() > 0.25 && self.app_core.ui_state.input_mode != InputMode::Menu {
            let delta = -aim_y.signum() * aim_y * aim_y * 40.0;
            ctx.data_mut(|d| {
                d.insert_temp(
                    egui::Id::new(("text_scroll_pending", "main")),
                    (0u8, delta),
                )
            });
        }

        // Haptics: core detects the transitions (RT end, stun, death);
        // we map them to rumble patterns. Drain even when disabled or
        // padless so the queue can't grow.
        self.app_core.poll_haptics();
        let events = self.app_core.drain_haptics();
        self.gp_rumble
            .retain(|(_, expiry)| *expiry > std::time::Instant::now());
        if any_connected && self.app_core.config.controller_rumble.enabled {
            for event in events {
                let rumble = &self.app_core.config.controller_rumble;
                let pattern = match event {
                    crate::core::app_core::HapticEvent::RoundtimeEnd => {
                        rumble.roundtime_end.clone()
                    }
                    crate::core::app_core::HapticEvent::Stunned => rumble.stunned.clone(),
                    crate::core::app_core::HapticEvent::Death => rumble.death.clone(),
                };
                self.play_rumble(&pattern);
            }
        }

        // gilrs events arrive outside egui's own event loop; without a
        // wake-up an idle GUI would stop polling. Cheap while connected.
        if any_connected {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    /// Play a rumble pattern on every connected pad. Effects are kept
    /// alive until expiry (gilrs stops them on drop).
    fn play_rumble(&mut self, pattern: &str) {
        use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};

        let (magnitude, ms, pulses) = match pattern {
            "short" => (0.5_f32, 160_u32, 1_u32),
            "long" => (0.9, 450, 1),
            "double" => (0.8, 180, 2),
            _ => return, // "off" and anything unknown
        };
        let Some(gilrs) = self.gamepad.as_mut() else {
            return;
        };
        let ids: Vec<gilrs::GamepadId> = gilrs
            .gamepads()
            .filter(|(_, pad)| pad.is_ff_supported())
            .map(|(id, _)| id)
            .collect();
        if ids.is_empty() {
            return;
        }
        let strength = (magnitude.clamp(0.0, 1.0) * u16::MAX as f32) as u16;
        let gap = 120_u32;
        let mut builder = EffectBuilder::new();
        for pulse in 0..pulses {
            builder.add_effect(BaseEffect {
                kind: BaseEffectType::Strong { magnitude: strength },
                scheduling: Replay {
                    after: Ticks::from_ms(pulse * (ms + gap)),
                    play_for: Ticks::from_ms(ms),
                    with_delay: Ticks::from_ms(0),
                },
                envelope: Default::default(),
            });
        }
        for id in &ids {
            builder.add_gamepad(&gilrs.gamepad(*id));
        }
        match builder.finish(gilrs) {
            Ok(effect) => {
                if let Err(err) = effect.play() {
                    tracing::debug!("rumble play failed: {}", err);
                    return;
                }
                let total = std::time::Duration::from_millis((pulses * (ms + gap)) as u64 + 100);
                self.gp_rumble
                    .push((effect, std::time::Instant::now() + total));
            }
            Err(err) => tracing::debug!("rumble effect build failed: {}", err),
        }
    }

    fn handle_gamepad_button(&mut self, button: gilrs::Button, ctx: &egui::Context) {
        use gilrs::Button;

        // The wheel owns the pad while it is up. Dwell drives navigation
        // and commit on its own; South/East stay as optional accelerators
        // for anyone who wants to skip the wait — South fires the aimed
        // leaf now (or descends the aimed folder), East backs up a level.
        // Everything else is swallowed. (These are hardwired accelerators,
        // not the [controller] binds, so a user can rebind those buttons
        // freely for use outside the wheel.)
        if self.gp_wheel.is_some() {
            match button {
                Button::South => self.wheel_accelerator_south(),
                Button::East => {
                    if let Some(ui) = self.gp_wheel.as_mut() {
                        if ui.path.pop().is_some() {
                            ui.aimed = None;
                            ui.candidate = None;
                            ui.candidate_since = None;
                            // Stick is still deflected after backing out;
                            // require re-neutralize before the parent level
                            // can navigate again.
                            ui.rearm_until_center = true;
                            self.app_core.needs_render = true;
                        }
                    }
                }
                _ => {}
            }
            return;
        }

        let shift_held = self.gamepad_shift_held();
        let button_name = gamepad_button_name(button);
        // The action this button resolves to in [controller] (base layer),
        // if any — used to drive the configurable menu/interact nav below.
        let bound_action = button_name.and_then(|n| self.controller_base_action(n));

        // While a popup menu is up, navigation/confirm/cancel resolve from
        // the bindable menu_* / interact_select actions when the user has
        // assigned them; otherwise the physical d-pad/South/East fall back
        // to their historical roles, so menus are drivable out of the box
        // even though those buttons carry movement/look macros elsewhere.
        // East ALWAYS cancels as a hard fallback so a menu can never be
        // undrivable no matter how things are rebound. Shift always means
        // "the other bank", so it bypasses this.
        if self.app_core.ui_state.input_mode == InputMode::Menu && !shift_held {
            let code = match bound_action.as_deref() {
                Some("menu_up") => Some(KeyCode::Up),
                Some("menu_down") => Some(KeyCode::Down),
                Some("menu_left") => Some(KeyCode::Left),
                Some("menu_right") => Some(KeyCode::Right),
                Some("interact_select") => Some(KeyCode::Enter),
                Some("menu_cancel") => Some(KeyCode::Esc),
                // Physical fallbacks: a d-pad/South button navigates the
                // menu only when the matching menu action isn't bound to
                // some OTHER button (so an explicit rebind wins and frees
                // the physical button). East is a hard cancel regardless.
                _ => match button {
                    Button::DPadUp if !self.controller_action_bound_anywhere("menu_up") => Some(KeyCode::Up),
                    Button::DPadDown if !self.controller_action_bound_anywhere("menu_down") => Some(KeyCode::Down),
                    Button::DPadLeft if !self.controller_action_bound_anywhere("menu_left") => Some(KeyCode::Left),
                    Button::DPadRight if !self.controller_action_bound_anywhere("menu_right") => Some(KeyCode::Right),
                    Button::South if !self.controller_action_bound_anywhere("interact_select") => Some(KeyCode::Enter),
                    Button::East => Some(KeyCode::Esc), // hard fallback, always
                    _ => None,
                },
            };
            if let Some(code) = code {
                let key = KeyEvent::new(code, KeyModifiers::NONE);
                self.handle_modal_nav_key(&key, ctx);
                return;
            }
            // Other buttons still dispatch so modes can be toggled off
            // from the pad.
        }

        // Interact mode: the bindable interact_select action activates the
        // focus (walk an exit, open a creature/object menu); if nothing is
        // bound to interact_select, South falls back to it. The right stick
        // cycles focus (see poll_gamepad); every other button stays on its
        // binds so West/North/East can carry <target_id> attack macros.
        // Exit via the interact_mode toggle (which always works — it runs
        // through normal action dispatch below) or by walking an exit.
        let interact_select = bound_action.as_deref() == Some("interact_select")
            || (button == Button::South
                && !self.controller_action_bound_anywhere("interact_select"));
        if self.app_core.ui_state.input_mode == InputMode::Interact
            && !shift_held
            && interact_select
        {
            let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
            self.handle_modal_nav_key(&key, ctx);
            return;
        }

        let Some(name) = button_name else {
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

    /// The base-layer `[controller]` action-name bound to a button, if it
    /// is a plain action (not a macro). Lets the menu/interact handlers
    /// resolve configurable nav (`menu_up`, `interact_select`, …) from the
    /// user's binds instead of hardwired buttons.
    fn controller_base_action(&self, button_name: &str) -> Option<String> {
        match self.app_core.config.controller_binds.get(button_name) {
            Some(crate::config::KeyBindAction::Action(name)) => Some(name.clone()),
            _ => None,
        }
    }

    /// True if any base-layer `[controller]` button is bound to the named
    /// action. Used to decide whether a physical fallback (e.g. South =
    /// select) applies: it does not once the user has assigned that action
    /// to some button explicitly.
    fn controller_action_bound_anywhere(&self, action: &str) -> bool {
        self.app_core.config.controller_binds.values().any(|b| {
            matches!(b, crate::config::KeyBindAction::Action(name) if name == action)
        })
    }

    /// True while any button bound to `controller_shift` is held.
    fn gamepad_shift_held(&self) -> bool {
        self.gamepad_action_button_held("controller_shift")
    }

    /// The wheel key of the wheel button currently held, if any:
    /// "" for `controller_wheel` (the default wheel), "<name>" for
    /// `controller_wheel:<name>`.
    fn held_wheel_key(&self) -> Option<String> {
        let gilrs = self.gamepad.as_ref()?;
        for (button_name, action) in &self.app_core.config.controller_binds {
            let crate::config::KeyBindAction::Action(name) = action else {
                continue;
            };
            let key = if name == "controller_wheel" {
                ""
            } else if let Some(rest) = name.strip_prefix("controller_wheel:") {
                rest
            } else {
                continue;
            };
            let Some(button) = gamepad_button_from_name(button_name) else {
                continue;
            };
            if gilrs.gamepads().any(|(_, pad)| pad.is_pressed(button)) {
                return Some(key.to_string());
            }
        }
        None
    }

    /// The slice list at a folder path within a named wheel; canonical
    /// lookup lives on AppCore (shared with remote wheel picks), which
    /// also builds the dynamic `portals` wheel from the current room.
    fn wheel_level_slices(
        &self,
        key: &str,
        path: &[usize],
    ) -> Option<Vec<crate::config::WheelSlice>> {
        self.app_core.wheel_slices(key, path)
    }

    /// Per-wheel aim-stick override for `key`: `Some(true)` = right stick,
    /// `Some(false)` = left, `None` = no override (caller uses the default
    /// non-movement stick). Reads `[controller_wheels_meta.<name>].stick`;
    /// the default wheel uses the "default" key.
    fn wheel_aim_stick(&self, key: &str) -> Option<bool> {
        let name = if key.is_empty() { "default" } else { key };
        let stick = self
            .app_core
            .config
            .controller_wheels_meta
            .get(name)?
            .stick
            .as_deref()?;
        match stick {
            "right" => Some(true),
            "left" => Some(false),
            _ => None,
        }
    }

    /// The displayed ring at the wheel's current level: the real slices
    /// plus, inside a folder, the reserved Back slice at its anchor.
    pub(super) fn wheel_view(&self, key: &str, path: &[usize]) -> Option<WheelView> {
        let real = self.wheel_level_slices(key, path)?;
        let anchor = &self.app_core.config.controller_tuning.back_slice;
        Some(WheelView::build(&real, !path.is_empty(), anchor, self.wheel_start(key)))
    }

    /// The ring rotation in degrees for wheel `key` (0 = up), from
    /// `[controller_wheels_meta.<name>].start`; 0 when unset. The default
    /// wheel uses the "default" key.
    fn wheel_start(&self, key: &str) -> f32 {
        let name = if key.is_empty() { "default" } else { key };
        self.app_core
            .config
            .controller_wheels_meta
            .get(name)
            .and_then(|m| m.start)
            .unwrap_or(0.0)
    }

    /// Advance the dwell state machine for one frame while the wheel is
    /// up and the aim stick is at `(x, y_up)`. Thin adapter over the
    /// app-independent `wheel_aim_step` machine: builds the view and the
    /// tuning snapshot, injects the clock, then applies the outcome
    /// (repaint and/or a mid-hold edge/retract fire).
    fn wheel_aim(&mut self, x: f32, y_up: f32) {
        let (key, path) = {
            let ui = self.gp_wheel.as_ref().expect("wheel active");
            (ui.key.clone(), ui.path.clone())
        };
        let Some(view) = self.wheel_view(&key, &path) else {
            return;
        };
        let timing = self.wheel_timing();
        let outcome = {
            let ui = self.gp_wheel.as_mut().expect("wheel active");
            wheel_aim_step(ui, &view, &timing, x, y_up, std::time::Instant::now())
        };
        if outcome.render {
            self.app_core.needs_render = true;
        }
        if let Some(display) = outcome.fire {
            self.wheel_close_and_fire(&view, display);
        }
    }

    /// Snapshot of `[controller_tuning]` in the units the wheel state
    /// machine consumes. A plain struct so tests can drive the machine
    /// with arbitrary feels and no app.
    fn wheel_timing(&self) -> WheelTiming {
        let t = &self.app_core.config.controller_tuning;
        WheelTiming {
            deadzone: self.wheel_deadzone(),
            aim_ms: t.aim_dwell_ms as u128,
            nav_ms: t.nav_dwell_ms as u128,
            fire_mode: self.wheel_fire_mode(),
            edge_threshold: self.wheel_edge_threshold(),
            retract_delta: self.wheel_retract_delta(),
        }
    }

    /// Dispatch a wheel command, honoring the fire debounce. A small floor
    /// is always applied even when fire_debounce_ms is 0, so a bounced
    /// button (release+repress inside one frame or two) can't double-send
    /// the same fire; the configured value extends it further.
    fn wheel_fire(&mut self, command: String) {
        const DEBOUNCE_FLOOR_MS: u128 = 50;
        let debounce =
            (self.app_core.config.controller_tuning.fire_debounce_ms as u128).max(DEBOUNCE_FLOOR_MS);
        if let Some(last) = self.gp_wheel_last_fire {
            if last.elapsed().as_millis() < debounce {
                return;
            }
        }
        // Resolve <target_id>/<target_noun> against the interact focus, so
        // a combat wheel slice like `cast at <target_id>` fires at the
        // selected creature. Slices without placeholders pass straight
        // through; a placeholder with nothing focused drops the fire
        // (rather than sending the literal text) — same contract as bound
        // interact macros.
        let Some(command) = self.app_core.substitute_interact_placeholders(command) else {
            self.app_core
                .add_system_message("Wheel slice needs an interact-mode target (focus something first)");
            return;
        };
        self.gp_wheel_last_fire = Some(std::time::Instant::now());
        self.dispatch_command(command);
    }

    /// The configured leaf fire mode.
    fn wheel_fire_mode(&self) -> FireMode {
        FireMode::from_str(&self.app_core.config.controller_tuning.fire_mode)
    }

    /// `edge_threshold` as a 0.0–1.0 stick magnitude.
    fn wheel_edge_threshold(&self) -> f32 {
        (self.app_core.config.controller_tuning.edge_threshold as f32 / 100.0).clamp(0.0, 1.0)
    }

    /// `retract_delta` as a 0.0–1.0 magnitude drop.
    fn wheel_retract_delta(&self) -> f32 {
        (self.app_core.config.controller_tuning.retract_delta as f32 / 100.0).clamp(0.0, 1.0)
    }

    /// Close the wheel and fire the leaf at display seat `display`, if it
    /// is a real, non-folder, non-empty seat (`leaf_command_at` is the one
    /// shared guard). Used by the edge/retract mid-hold fires and the South
    /// accelerator; release goes through `wheel_release_command` instead
    /// (same guard, no close needed — the wheel is already down).
    fn wheel_close_and_fire(&mut self, view: &WheelView, display: usize) {
        let Some(command) = leaf_command_at(view, display) else {
            return;
        };
        self.gp_wheel = None;
        self.gp_wheel_fired = true;
        self.gp_aim_recenter_needed = true;
        self.wheel_fire(command);
        self.app_core.needs_render = true;
    }

    /// Wheel dead zone as a 0.0–1.0 stick magnitude (percent in config).
    fn wheel_deadzone(&self) -> f32 {
        (self.app_core.config.controller_tuning.deadzone as f32 / 100.0).clamp(0.0, 0.99)
    }

    /// South accelerator while the wheel is up: act on whatever the stick
    /// is over right now (the committed slice, or the live candidate if a
    /// dwell hasn't landed yet), skipping the dwell wait. Thin adapter over
    /// `wheel_south_step`: fires a leaf, descends a folder, or ascends via
    /// the Back slice.
    fn wheel_accelerator_south(&mut self) {
        let (key, path) = {
            let ui = self.gp_wheel.as_ref().expect("wheel active");
            (ui.key.clone(), ui.path.clone())
        };
        let Some(view) = self.wheel_view(&key, &path) else {
            return;
        };
        let outcome = {
            let ui = self.gp_wheel.as_mut().expect("wheel active");
            wheel_south_step(ui, &view)
        };
        if outcome.render {
            self.app_core.needs_render = true;
        }
        if let Some(display) = outcome.fire {
            self.wheel_close_and_fire(&view, display);
        }
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

/// Live radial-wheel state: which named wheel is up, the folder path
/// descended so far, and the aimed slice at the current level.
///
/// `aimed` is the *committed* display slice — the one a dwell has settled
/// on and that release will fire. `candidate` is the display slice the
/// stick is currently over but that has not yet dwelt long enough to
/// commit; `candidate_since` stamps when it was first seen so the dwell
/// gate can measure the hold. `rearm_until_center` latches on an auto
/// descend/ascend: after the level changes the stick is still deflected,
/// so no new dwell may start until the stick returns to (below) the dead
/// zone. Keyed on physical stick neutrality — NOT a display index, which
/// is meaningless across a level change because seat counts differ — so a
/// still-deflected stick can't chain through nested folders.
///
/// Indices here are into the *displayed* ring (Back slice injected,
/// rotated to its anchor) — see `WheelView`.
pub(super) struct WheelUi {
    pub(super) key: String,
    pub(super) path: Vec<usize>,
    pub(super) aimed: Option<usize>,
    pub(super) candidate: Option<usize>,
    pub(super) candidate_since: Option<std::time::Instant>,
    pub(super) rearm_until_center: bool,
    /// Peak aim-stick magnitude (0.0–1.0) seen since the current leaf
    /// committed, for `fire_mode = "retract"`: the leaf fires once
    /// deflection falls `retract_delta` below this peak. Reset whenever the
    /// committed slice changes; only meaningful while `aimed` is Some.
    pub(super) peak_magnitude: f32,
}

impl VellumGuiApp {
    /// Draw the radial command wheel while its button is held: a ring of
    /// slice labels around the screen center, the aimed slice highlighted.
    pub(super) fn render_controller_wheel(&mut self, ctx: &egui::Context) {
        let Some((key, path, selected)) = self
            .gp_wheel
            .as_ref()
            .map(|ui| (ui.key.clone(), ui.path.clone(), ui.aimed))
        else {
            return;
        };
        let Some(view) = self.wheel_view(&key, &path) else {
            return;
        };
        let slices = &view.slices;
        if slices.is_empty() {
            return;
        }
        let in_folder = !path.is_empty();
        let global_dz = self.wheel_deadzone();
        let center = ctx.content_rect().center();
        let radius = (ctx.content_rect().height() * 0.28).clamp(90.0, 220.0);

        egui::Area::new(egui::Id::new("controller_wheel"))
            .order(egui::Order::Foreground)
            .fixed_pos(center - egui::vec2(radius + 70.0, radius + 40.0))
            .interactable(false)
            .show(ctx, |ui| {
                let painter = ui.painter();
                let bg = ui.visuals().window_fill.gamma_multiply(0.92);
                let outer = radius + 34.0;
                let hub = 46.0;
                painter.circle_filled(center, outer, bg);
                painter.circle_stroke(
                    center,
                    outer,
                    egui::Stroke::new(1.0, ui.visuals().window_stroke.color),
                );
                paint_wheel_ring(
                    painter,
                    ui.visuals(),
                    center,
                    outer,
                    hub,
                    radius,
                    slices,
                    &view.layout,
                    selected,
                    global_dz,
                );

                // Center hub hosts the hint text. `selected` is the
                // committed slice (post-dwell); until a dwell lands it is
                // None and the prompt explains the gesture.
                painter.circle_filled(center, hub, bg);
                let stick_word = if self.app_core.config.controller_tuning.movement_stick == "right"
                {
                    "left stick"
                } else {
                    "right stick"
                };
                let hint = match selected.and_then(|i| slices.get(i)) {
                    Some(slice) if slice.command == BACK_COMMAND => "dwell to go back".to_string(),
                    Some(slice) if slice.is_folder() => format!("{}: dwell to open", slice.label),
                    Some(slice) => format!("release to fire: {}", slice.command),
                    None if in_folder => "dwell a slice · release fires · center to cancel".to_string(),
                    None => format!("aim with the {stick_word}, dwell, release to fire"),
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

/// Draw a wheel ring's wedges onto `painter`: color fills, separator
/// lines, slice labels, and the per-slice inner-floor arcs. Shared by the
/// live controller wheel and the visual wheel designer so the two renders
/// can never drift. The caller owns the backdrop circle, the center hub,
/// and any hint text. `label_radius` is where slice labels sit;
/// `global_deadzone` suppresses floor arcs that aren't deeper than the
/// default reach.
pub(super) fn paint_wheel_ring(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    center: egui::Pos2,
    outer: f32,
    hub: f32,
    label_radius: f32,
    slices: &[WheelSlice],
    layout: &ResolvedLayout,
    selected: Option<usize>,
    global_deadzone: f32,
) {
    let seats = &layout.seats;
    // Screen radians for a seat center: the aim convention is 0 = up,
    // clockwise (degrees); screen is 0 = +x with y down, so up is
    // -90°. screen = aim - 90°.
    let seat_center_screen =
        |i: usize| seats[i].center_deg().to_radians() - std::f32::consts::FRAC_PI_2;
    let seat_span_rad = |i: usize| seats[i].span_deg.to_radians();

    // Wedge fills: the whole pie piece carries the slice's
    // color (dim at rest, bright while aimed); colorless
    // slices highlight with the theme selection fill.
    for (i, slice) in slices.iter().enumerate() {
        let center_angle = seat_center_screen(i);
        let step = seat_span_rad(i);
        let is_selected = selected == Some(i);
        let tint = slice
            .color
            .as_deref()
            .and_then(super::theme::resolve_color);
        let fill = match (tint, is_selected) {
            (Some(c), true) => c.gamma_multiply(0.85),
            (Some(c), false) => c.gamma_multiply(0.22),
            (None, true) => visuals.selection.bg_fill,
            (None, false) => egui::Color32::TRANSPARENT,
        };
        if fill != egui::Color32::TRANSPARENT {
            if slices.len() == 1 {
                painter.circle_filled(center, outer, fill);
            } else {
                let a0 = center_angle - step / 2.0;
                let a1 = center_angle + step / 2.0;
                let mut points = vec![center];
                const ARC_STEPS: usize = 16;
                for k in 0..=ARC_STEPS {
                    let a = a0 + (a1 - a0) * k as f32 / ARC_STEPS as f32;
                    points.push(center + egui::vec2(a.cos(), a.sin()) * outer);
                }
                painter.add(egui::Shape::convex_polygon(
                    points,
                    fill,
                    egui::Stroke::NONE,
                ));
            }
        }
    }

    // Wedge separators + labels over the fills.
    for (i, slice) in slices.iter().enumerate() {
        let center_angle = seat_center_screen(i);
        if slices.len() > 1 {
            let a0 = center_angle - seat_span_rad(i) / 2.0;
            let dir = egui::vec2(a0.cos(), a0.sin());
            painter.line_segment(
                [center + dir * hub, center + dir * outer],
                egui::Stroke::new(1.0, visuals.window_stroke.color),
            );
        }
        let pos = center + egui::vec2(center_angle.cos(), center_angle.sin()) * label_radius;
        let is_selected = selected == Some(i);
        let (color, size) = if is_selected {
            (visuals.strong_text_color(), 18.0)
        } else {
            (visuals.text_color(), 14.0)
        };
        let label = if slice.is_folder() {
            format!("{} ▸", slice.label)
        } else {
            slice.label.clone()
        };
        painter.text(
            pos,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(size),
            color,
        );
    }

    // Per-slice inner floor: a slice with its own `inner` above
    // the global dead zone needs a deeper throw, so draw a faint
    // arc across its wedge at that floor radius — the deflection
    // you must cross before it registers. Slices on the default
    // floor get no extra line (keeps the common case clean).
    let floor_stroke = egui::Stroke::new(
        1.5,
        visuals.warn_fg_color.gamma_multiply(0.8),
    );
    for (i, slice) in slices.iter().enumerate() {
        let Some(inner_pct) = slice.inner else { continue };
        let frac = (inner_pct as f32 / 100.0).clamp(0.0, 1.0);
        if frac <= global_deadzone + 1e-3 {
            continue; // not harder than the default reach
        }
        let r = hub + (outer - hub) * frac;
        let center_angle = seat_center_screen(i);
        let half = seat_span_rad(i) / 2.0;
        let a0 = center_angle - half;
        let a1 = center_angle + half;
        const ARC_STEPS: usize = 12;
        let pts: Vec<egui::Pos2> = (0..=ARC_STEPS)
            .map(|k| {
                let a = a0 + (a1 - a0) * k as f32 / ARC_STEPS as f32;
                center + egui::vec2(a.cos(), a.sin()) * r
            })
            .collect();
        painter.add(egui::Shape::line(pts, floor_stroke));
    }
}

/// Dominant-axis four-way stick read, used by the interact-mode right
/// stick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FourWay {
    Up,
    Down,
    Left,
    Right,
}

/// Read a stick as a four-way direction with the movement hysteresis:
/// one direction per deflection, re-armed by returning toward center.
fn four_way(x: f32, y_up: f32, previous: Option<FourWay>) -> Option<FourWay> {
    let magnitude = (x * x + y_up * y_up).sqrt();
    let threshold = if previous.is_some() {
        STICK_RELEASE
    } else {
        STICK_DEFLECT
    };
    if magnitude < threshold {
        return None;
    }
    Some(if x.abs() > y_up.abs() {
        if x > 0.0 {
            FourWay::Right
        } else {
            FourWay::Left
        }
    } else if y_up > 0.0 {
        FourWay::Up
    } else {
        FourWay::Down
    })
}

/// The minimum wedge width the resolver will produce; a slice narrower
/// than this is hard to hit, so explicit spans are clamped up and the
/// remainder scaled to keep the ring closed. Shared with the config-layer
/// validator so the resolver and the warnings can't disagree.
use crate::config::WHEEL_MIN_SPAN_DEG as MIN_SPAN_DEG;

/// One seat in a resolved ring: its clockwise span and the angle of its
/// leading edge, both in degrees in the AIM convention (0 = up, clockwise).
/// Consecutive seats abut, wrapping the full 360.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Seat {
    /// Leading (counter-clockwise) edge of the seat, degrees from up.
    pub(super) start_deg: f32,
    pub(super) span_deg: f32,
}

impl Seat {
    pub(super) fn center_deg(&self) -> f32 {
        self.start_deg + self.span_deg / 2.0
    }
}

/// A ring laid out into concrete angular seats — the single source of
/// truth the aim lookup, the render, and the Back placement all read, so
/// they can never disagree about where a wedge sits. Built by
/// `resolve_spans` from the per-slice `span` list plus the wheel's `start`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ResolvedLayout {
    pub(super) seats: Vec<Seat>,
}

impl ResolvedLayout {
    pub(super) fn len(&self) -> usize {
        self.seats.len()
    }
}

/// Turn a per-slice span list (`None` = take an even share of the leftover)
/// into concrete abutting seats. Seat 0 is *centered* at `start_deg` (not
/// started there), matching the historical convention where slice 0 sits
/// at the top (`start_deg == 0`); the rest follow clockwise.
///
/// Resolution: explicit spans are taken as-is (clamped up to `MIN_SPAN_DEG`),
/// and whatever remains of 360° is split evenly among the span-less slices.
/// If the totals can't fit — explicit spans clamped up overshoot, or there
/// are no free slices to absorb a shortfall/overflow — every seat is scaled
/// proportionally so the ring still closes exactly. The runtime is thus
/// always forgiving (never a gap, never a panic); the editor/load warnings
/// (B5) are where the user is told to fix numbers that had to be adjusted.
///
/// With all spans `None` and `start_deg == 0` this reproduces the old
/// `360/count` ring with seat 0 centered at the top — the byte-for-byte
/// backward-compatible path, pinned by the golden vectors.
pub(super) fn resolve_spans(spans: &[Option<f32>], start_deg: f32) -> ResolvedLayout {
    let n = spans.len();
    if n == 0 {
        return ResolvedLayout { seats: Vec::new() };
    }

    // Each explicit span, floored at the minimum; None marks a free seat.
    let explicit: Vec<Option<f32>> = spans
        .iter()
        .map(|s| s.map(|v| v.max(MIN_SPAN_DEG)))
        .collect();
    let explicit_sum: f32 = explicit.iter().flatten().sum();
    let free_count = explicit.iter().filter(|s| s.is_none()).count();

    // Provisional width for each seat before the final normalize.
    let free_each = if free_count > 0 {
        ((360.0 - explicit_sum) / free_count as f32).max(0.0)
    } else {
        0.0
    };
    let mut widths: Vec<f32> = explicit
        .iter()
        .map(|s| s.unwrap_or(free_each))
        .collect();

    // Close the ring exactly: scale to 360 whenever the provisional widths
    // don't already sum there (explicit overflow, or no free seats to take
    // up the slack). A degenerate all-zero total falls back to even.
    let total: f32 = widths.iter().sum();
    if total <= f32::EPSILON {
        widths = vec![360.0 / n as f32; n];
    } else if (total - 360.0).abs() > 1e-3 {
        let scale = 360.0 / total;
        for w in &mut widths {
            *w *= scale;
        }
    }

    // Lay the widths out into abutting seats. Seat 0 is CENTERED at
    // `start_deg`, so its leading edge sits half its own width earlier;
    // the rest abut clockwise from there.
    let mut seats = Vec::with_capacity(n);
    let mut edge = start_deg - widths[0] / 2.0;
    for w in widths {
        seats.push(Seat { start_deg: edge, span_deg: w });
        edge += w;
    }
    ResolvedLayout { seats }
}

/// Which seat the stick is aiming at, by cumulative-range lookup over a
/// resolved layout: the seat whose `[start, start+span)` arc contains the
/// stick angle. None inside the dead zone (`deadzone`, 0.0–1.0) or with no
/// seats. Angle uses the aim convention (0 = up, clockwise), normalized
/// relative to seat 0's start so wrap is automatic.
fn seat_at(x: f32, y_up: f32, layout: &ResolvedLayout, deadzone: f32) -> Option<usize> {
    if (x * x + y_up * y_up).sqrt() < deadzone {
        return None;
    }
    seat_index_at_angle(x, y_up, layout)
}

/// Pure angular resolution: which seat's arc the stick points at, ignoring
/// magnitude. None only when there are no seats. The radial floor (global
/// dead zone or a per-slice `inner`) is applied by the caller.
pub(super) fn seat_index_at_angle(x: f32, y_up: f32, layout: &ResolvedLayout) -> Option<usize> {
    if layout.len() == 0 {
        return None;
    }
    let start = layout.seats[0].start_deg;
    let rel = (x.atan2(y_up).to_degrees() - start).rem_euclid(360.0);
    let mut cum = 0.0;
    for (i, seat) in layout.seats.iter().enumerate() {
        cum += seat.span_deg;
        if rel < cum {
            return Some(i);
        }
    }
    // Float slop at the wrap: the last seat owns the boundary.
    Some(layout.len() - 1)
}

/// The seat under the stick honoring per-slice `inner`: resolve the angle,
/// then apply that seat's radial floor — its own `inner` (percent of full
/// deflection) if set, else the global `deadzone`. Below the floor the seat
/// isn't aimable, so this returns None (the stick reads as centered). This
/// gates aiming/commit only; firing stays with the active fire mode.
fn seat_at_with_inner(
    x: f32,
    y_up: f32,
    view: &WheelView,
    deadzone: f32,
) -> Option<usize> {
    let seat = seat_index_at_angle(x, y_up, &view.layout)?;
    let floor = view
        .slices
        .get(seat)
        .and_then(|s| s.inner)
        .map(|pct| pct as f32 / 100.0)
        .unwrap_or(deadzone);
    if (x * x + y_up * y_up).sqrt() < floor {
        return None;
    }
    Some(seat)
}

/// Even-ring seat lookup by count — the backward-compatible shim used by
/// call sites (and tests) that don't have a layout: build the all-even
/// layout and delegate to `seat_at`. Equivalent to the old
/// `wheel_slice_at`.
fn wheel_slice_at(x: f32, y_up: f32, count: usize, deadzone: f32) -> Option<usize> {
    let layout = resolve_spans(&vec![None; count], 0.0);
    seat_at(x, y_up, &layout, deadzone)
}

/// The aim-convention angle (degrees, 0 = up, clockwise) of a screen
/// anchor word, used to place the reserved Back slice at its side.
/// Unknown words (and "none", handled earlier) fall back to down.
pub(super) fn anchor_angle_deg(anchor: &str) -> f32 {
    match anchor {
        "up" => 0.0,
        "up-right" => 45.0,
        "right" => 90.0,
        "down-right" => 135.0,
        "down" => 180.0,
        "down-left" => 225.0,
        "left" => 270.0,
        "up-left" => 315.0,
        _ => 180.0, // default: down
    }
}

/// Smallest absolute difference between two angles in degrees (0..=180).
fn angular_gap(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// One step of the wheel re-arm gate. Given whether the latch is up and
/// whether the stick is currently neutral (inside the dead zone, i.e. no
/// candidate), returns `(latch_after, may_dwell)`:
///   - re-neutralizing (`centered`) is the only thing that lowers the
///     latch — never a display index, which is meaningless across a level
///     change;
///   - no dwell may accrue while the latch is up, so a still-deflected
///     stick can't chain through nested folders.
fn dwell_rearm_step(latched: bool, centered: bool) -> (bool, bool) {
    let latch_after = latched && !centered;
    let may_dwell = !latch_after && !centered;
    (latch_after, may_dwell)
}

/// Snapshot of `[controller_tuning]` in machine units: deadzone and
/// thresholds as 0.0–1.0 magnitudes, dwells in ms. App-independent so
/// scenario tests can drive the machine with arbitrary feels.
pub(super) struct WheelTiming {
    pub(super) deadzone: f32,
    pub(super) aim_ms: u128,
    pub(super) nav_ms: u128,
    pub(super) fire_mode: FireMode,
    pub(super) edge_threshold: f32,
    pub(super) retract_delta: f32,
}

/// One frame's outcome from the wheel state machine: the display seat of a
/// leaf that must fire NOW (edge/retract fire mid-hold; South fires on
/// press), and whether the frame changed anything visible.
#[derive(Debug, Default, PartialEq)]
pub(super) struct WheelStepOutcome {
    pub(super) fire: Option<usize>,
    pub(super) render: bool,
}

/// The command of the real, non-folder, non-empty leaf at a DISPLAY seat,
/// if that seat holds one. The single guard shared by every fire path
/// (release, edge, retract, South) so they can't drift: Back seats
/// (`real == None`), folders, and empty commands all return None.
fn leaf_command_at(view: &WheelView, display: usize) -> Option<String> {
    match view.real(display) {
        Some(Some(_)) => {}
        _ => return None,
    }
    let slice = view.slices.get(display)?;
    if slice.is_folder() || slice.command.is_empty() {
        return None;
    }
    Some(slice.command.clone())
}

/// Release-fire decision: the committed leaf's command, if the stick is
/// still dwelling one when the wheel button comes up.
fn wheel_release_command(ui: &WheelUi, view: &WheelView) -> Option<String> {
    leaf_command_at(view, ui.aimed?)
}

/// Advance the dwell/commit/fire state machine one frame. App-independent:
/// takes the current display ring, a tuning snapshot, the aim-stick sample,
/// and an injected clock, and mutates only `ui`. Descend/ascend happen
/// in place (path push/pop + rearm latch); a leaf that must fire NOW
/// (edge/retract modes) is returned for the app layer to dispatch.
/// Release-mode firing stays with the caller via `wheel_release_command`.
///
/// The re-arm latch semantics: the stick is "neutral" when it has fallen
/// back inside the dead zone (candidate None), and re-neutralizing is the
/// ONLY thing that clears the latch — keyed on physical stick state, not a
/// display index (indices are meaningless across a level change, since
/// seat counts differ).
fn wheel_aim_step(
    ui: &mut WheelUi,
    view: &WheelView,
    timing: &WheelTiming,
    x: f32,
    y_up: f32,
    now: std::time::Instant,
) -> WheelStepOutcome {
    let mut render = false;
    let magnitude = (x * x + y_up * y_up).sqrt();
    // Per-slice `inner` gates aiming: a seat below its own floor reads as
    // no candidate (and thus as "centered", clearing the rearm latch).
    let candidate = seat_at_with_inner(x, y_up, view, timing.deadzone);
    let centered = candidate.is_none();
    let (latch_after, may_dwell) = dwell_rearm_step(ui.rearm_until_center, centered);
    ui.rearm_until_center = latch_after;

    // Track the candidate and restart its dwell clock whenever it changes.
    // Any change also drops a prior commit — release only fires a leaf the
    // stick is *currently* dwelling, never a stale one it has already moved
    // off (whether to center or to another slice).
    if ui.candidate != candidate {
        ui.candidate = candidate;
        ui.candidate_since = Some(now);
        ui.aimed = None;
        ui.peak_magnitude = 0.0;
        render = true;
    }

    // While the latch is up (stick hasn't re-neutralized since the last
    // auto descend/ascend), no dwell may accrue — this is what stops a
    // still-deflected stick from chaining through nested folders.
    if !may_dwell {
        return WheelStepOutcome { fire: None, render };
    }

    // Nothing under the stick: nothing to dwell.
    let Some(display) = candidate else {
        return WheelStepOutcome { fire: None, render };
    };
    let dwelt = ui
        .candidate_since
        .map(|since| now.saturating_duration_since(since).as_millis())
        .unwrap_or(0);

    match view.real(display) {
        // Back slice: auto-ascend once the nav dwell elapses.
        Some(None) => {
            if dwelt >= timing.nav_ms {
                ui.path.pop();
                ui.aimed = None;
                ui.candidate = None;
                ui.candidate_since = None;
                ui.rearm_until_center = true;
                render = true;
            }
        }
        // A real slice. Folder-ness is read from the DISPLAY seat —
        // view.slices is the rotated ring, so indexing it by `real`
        // classified the wrong slice inside a rotated folder (the same
        // display-vs-real trap 0fb3431 fixed for firing); `real` is only
        // for path.push on descend.
        Some(Some(real)) => {
            let is_folder = view
                .slices
                .get(display)
                .map(|s| s.is_folder())
                .unwrap_or(false);
            if is_folder {
                // Folders always descend on dwell — never fired by edge
                // or retract (Niffy's invariant).
                if dwelt >= timing.nav_ms {
                    ui.path.push(real);
                    ui.aimed = None;
                    ui.candidate = None;
                    ui.candidate_since = None;
                    ui.rearm_until_center = true;
                    ui.peak_magnitude = 0.0;
                    render = true;
                }
                return WheelStepOutcome { fire: None, render };
            }

            // Leaf, by fire mode.
            match timing.fire_mode {
                FireMode::Edge => {
                    // Fire the moment deflection crosses the threshold —
                    // no dwell. `may_dwell` gated us here, so the rearm
                    // latch already blocks a still-deflected stick from
                    // refiring across slices until it recenters.
                    if edge_should_fire(magnitude, timing.edge_threshold) {
                        return WheelStepOutcome {
                            fire: Some(display),
                            render,
                        };
                    }
                }
                FireMode::Retract => {
                    // Dwell to commit, then track the deflection peak and
                    // fire once it falls retract_delta below that peak.
                    if dwelt >= timing.aim_ms {
                        if ui.aimed != Some(display) {
                            ui.aimed = Some(display);
                            ui.peak_magnitude = magnitude;
                            render = true;
                        }
                        ui.peak_magnitude = ui.peak_magnitude.max(magnitude);
                        if retract_should_fire(magnitude, ui.peak_magnitude, timing.retract_delta)
                        {
                            return WheelStepOutcome {
                                fire: Some(display),
                                render,
                            };
                        }
                    }
                }
                FireMode::Release => {
                    // Commit (arm release-fire); the release arm in
                    // poll_gamepad does the firing.
                    if dwelt >= timing.aim_ms && ui.aimed != Some(display) {
                        ui.aimed = Some(display);
                        render = true;
                    }
                }
            }
        }
        None => {}
    }
    WheelStepOutcome { fire: None, render }
}

/// South accelerator step: act on the seat the stick is over *right now*
/// (the live candidate wins over an earlier commit, so South never fires a
/// slice the stick has already moved off), skipping the dwell wait.
/// Descends/ascends by mutating `ui`; a leaf to fire is returned as its
/// display seat. Slices are looked up by DISPLAY index (view.slices is the
/// rotated ring); `real` is only for `path.push` on descend.
fn wheel_south_step(ui: &mut WheelUi, view: &WheelView) -> WheelStepOutcome {
    let Some(display) = ui.candidate.or(ui.aimed) else {
        return WheelStepOutcome::default();
    };
    match view.real(display) {
        Some(None) => {
            // Back slice.
            if ui.path.pop().is_some() {
                ui.aimed = None;
                ui.candidate = None;
                ui.candidate_since = None;
                ui.rearm_until_center = true;
                return WheelStepOutcome {
                    fire: None,
                    render: true,
                };
            }
            WheelStepOutcome::default()
        }
        Some(Some(real)) => {
            let Some(slice) = view.slices.get(display) else {
                return WheelStepOutcome::default();
            };
            if slice.is_folder() {
                ui.path.push(real);
                ui.aimed = None;
                ui.candidate = None;
                ui.candidate_since = None;
                ui.rearm_until_center = true;
                WheelStepOutcome {
                    fire: None,
                    render: true,
                }
            } else if !slice.command.is_empty() {
                WheelStepOutcome {
                    fire: Some(display),
                    render: false,
                }
            } else {
                WheelStepOutcome::default()
            }
        }
        None => WheelStepOutcome::default(),
    }
}

/// True when the aim stick has returned close enough to center to clear
/// the post-wheel recenter latch (reuses the movement release threshold).
fn aim_stick_centered(x: f32, y_up: f32) -> bool {
    (x * x + y_up * y_up).sqrt() < STICK_RELEASE
}

/// Which physical stick aims the wheel (true = right). A per-wheel
/// override (`Some(true/false)`) wins; otherwise the default aim stick is
/// the one that isn't the movement stick.
fn resolve_aim_stick(move_on_right: bool, wheel_override: Option<bool>) -> bool {
    wheel_override.unwrap_or(!move_on_right)
}

/// The displayed ring for a wheel level: the real slices plus, inside a
/// folder, a synthetic Back slice reserved at the configured screen
/// anchor. `real_index` maps a displayed index back to the real slice
/// list (`None` marks the Back slice). Built in the GUI layer only so the
/// shared/remote wheel definitions stay untouched.
pub(super) struct WheelView {
    pub(super) slices: Vec<WheelSlice>,
    real_index: Vec<Option<usize>>,
    /// Concrete angular seats for `slices`, in display order — the single
    /// geometry the aim lookup and the render both read.
    pub(super) layout: ResolvedLayout,
}

/// Sentinel command marking the injected Back slice.
const BACK_COMMAND: &str = "\u{0}__wheel_back__";

impl WheelView {
    /// Build the display ring.
    ///
    /// Top level (not `in_folder`): the real slices, laid out by their spans
    /// and rotated by the wheel's `start` (0 = up, clockwise).
    ///
    /// Inside a folder: a reserved Back slice is appended as the LAST seat
    /// (real slices keep their order 0..n-1 — no array rotation, so display
    /// index n is always Back), then the whole ring is rotated by ANGLE so
    /// Back's seat center lands at the `back_anchor` direction. Back keeps
    /// the same screen side at every level, as before, but now that works
    /// with uneven seats too. `back_anchor == "none"` skips Back entirely —
    /// the folder ring is just the real slices (rotated by `start`), and
    /// you ascend with the East/B accelerator.
    fn build(real: &[WheelSlice], in_folder: bool, back_anchor: &str, start: f32) -> Self {
        let real_spans = || -> Vec<Option<f32>> { real.iter().map(|s| s.span).collect() };

        if !in_folder || back_anchor == "none" {
            let slices = real.to_vec();
            let layout = resolve_spans(&real_spans(), start);
            return Self {
                real_index: (0..real.len()).map(Some).collect(),
                layout,
                slices,
            };
        }

        // [real..., Back] — Back is always the last display seat.
        let mut slices: Vec<WheelSlice> = real.to_vec();
        slices.push(WheelSlice {
            label: "◂ Back".to_string(),
            command: BACK_COMMAND.to_string(),
            ..Default::default()
        });
        let mut real_index: Vec<Option<usize>> = (0..real.len()).map(Some).collect();
        real_index.push(None);

        // Back participates as a span-less seat in the remainder split (no
        // magic width — an even ring keeps today's geometry). Lay it out at
        // start 0 to read Back's center, then rotate so that center lands on
        // the anchor direction. (This ignores a user `start` inside folders,
        // by design: Back-position consistency across levels wins there.)
        let mut spans = real_spans();
        spans.push(None);
        let back_idx = slices.len() - 1;
        let at_zero = resolve_spans(&spans, 0.0);
        let rotation = anchor_angle_deg(back_anchor) - at_zero.seats[back_idx].center_deg();
        let layout = resolve_spans(&spans, rotation);
        Self { slices, real_index, layout }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.slices.len()
    }

    /// Real slice index for a displayed index; None = the Back slice.
    fn real(&self, display: usize) -> Option<Option<usize>> {
        self.real_index.get(display).copied()
    }
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
    fn four_way_picks_dominant_axis_with_hysteresis() {
        assert_eq!(four_way(0.0, 1.0, None), Some(FourWay::Up));
        assert_eq!(four_way(0.0, -1.0, None), Some(FourWay::Down));
        assert_eq!(four_way(-1.0, 0.0, None), Some(FourWay::Left));
        assert_eq!(four_way(1.0, 0.0, None), Some(FourWay::Right));
        // Dominant axis wins on diagonals.
        assert_eq!(four_way(0.9, 0.5, None), Some(FourWay::Right));
        assert_eq!(four_way(0.4, -0.8, None), Some(FourWay::Down));
        // Dead zone and hysteresis mirror the movement stick.
        assert_eq!(four_way(0.3, 0.3, None), None);
        assert_eq!(four_way(0.0, 0.5, Some(FourWay::Up)), Some(FourWay::Up));
        assert_eq!(four_way(0.0, 0.2, Some(FourWay::Up)), None);
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
        // 8 slices: up = 0, right = 2, down = 4, left = 6. Default 0.5 dz.
        assert_eq!(wheel_slice_at(0.0, 1.0, 8, 0.5), Some(0));
        assert_eq!(wheel_slice_at(1.0, 0.0, 8, 0.5), Some(2));
        assert_eq!(wheel_slice_at(0.0, -1.0, 8, 0.5), Some(4));
        assert_eq!(wheel_slice_at(-1.0, 0.0, 8, 0.5), Some(6));
        // Dead zone and empty wheel.
        assert_eq!(wheel_slice_at(0.2, 0.2, 8, 0.5), None);
        assert_eq!(wheel_slice_at(0.0, 1.0, 0, 0.5), None);
        // Odd slice counts still cover the full circle.
        assert_eq!(wheel_slice_at(0.0, 1.0, 3, 0.5), Some(0));
    }

    #[test]
    fn deadzone_is_configurable() {
        // A deflection of 0.4 registers with a low dead zone, not a high one.
        assert_eq!(wheel_slice_at(0.0, 0.4, 8, 0.25), Some(0));
        assert_eq!(wheel_slice_at(0.0, 0.4, 8, 0.5), None);
        // Zero dead zone registers any nonzero deflection.
        assert_eq!(wheel_slice_at(0.0, 0.05, 8, 0.0), Some(0));
    }

    fn spans(v: &[Option<f32>]) -> Vec<f32> {
        resolve_spans(v, 0.0).seats.iter().map(|s| s.span_deg).collect()
    }

    #[test]
    fn resolve_all_none_is_the_even_ring_centered_at_top() {
        // Backward-compat property: no spans + start 0 == the old geometry,
        // seat 0 centered at the top (leading edge half a step early).
        let layout = resolve_spans(&[None, None, None, None], 0.0);
        for s in &layout.seats {
            assert!((s.span_deg - 90.0).abs() < 1e-3, "even 90 each");
        }
        assert!((layout.seats[0].start_deg - -45.0).abs() < 1e-3, "seat 0 centered at up");
        assert!((layout.seats[0].center_deg() - 0.0).abs() < 1e-3);
        // And the seat lookup matches the compass exactly (up/right/down/left).
        assert_eq!(seat_at(0.0, 1.0, &layout, 0.5), Some(0));
        assert_eq!(seat_at(1.0, 0.0, &layout, 0.5), Some(1));
        assert_eq!(seat_at(0.0, -1.0, &layout, 0.5), Some(2));
        assert_eq!(seat_at(-1.0, 0.0, &layout, 0.5), Some(3));
    }

    #[test]
    fn resolve_explicit_spans_take_value_remainder_splits_evenly() {
        // One 120° slice; the other three share the remaining 240° = 80 each.
        assert_eq!(spans(&[Some(120.0), None, None, None]), vec![120.0, 80.0, 80.0, 80.0]);
        // Two explicit; two free share what's left.
        assert_eq!(spans(&[Some(100.0), Some(60.0), None, None]), vec![100.0, 60.0, 100.0, 100.0]);
    }

    #[test]
    fn resolve_clamps_min_span() {
        // A 10° request is floored to the 30° minimum; the free slice takes
        // the rest so the ring still closes.
        assert_eq!(spans(&[Some(10.0), None]), vec![30.0, 330.0]);
    }

    #[test]
    fn resolve_scales_overflow_to_close_the_ring() {
        // Explicit spans summing past 360 with no free seats scale down
        // proportionally to exactly 360, keeping their ratio (2:1).
        let got = spans(&[Some(400.0), Some(200.0)]);
        let sum: f32 = got.iter().sum();
        assert!((sum - 360.0).abs() < 1e-3, "closes at 360: {got:?}");
        assert!((got[0] / got[1] - 2.0).abs() < 1e-3, "ratio kept: {got:?}");
    }

    #[test]
    fn resolve_no_free_seats_underfill_scales_up() {
        // Explicit spans under 360 with nothing free are scaled UP to fill,
        // so there is never an unhittable gap.
        let got = spans(&[Some(60.0), Some(60.0)]);
        assert_eq!(got, vec![180.0, 180.0]);
    }

    #[test]
    fn resolve_start_rotates_the_ring() {
        // start = 90 puts seat 0's center on the right; the lookup follows.
        let layout = resolve_spans(&[None, None, None, None], 90.0);
        assert!((layout.seats[0].center_deg() - 90.0).abs() < 1e-3);
        assert_eq!(seat_at(1.0, 0.0, &layout, 0.5), Some(0), "seat 0 now points right");
        assert_eq!(seat_at(0.0, -1.0, &layout, 0.5), Some(1), "seat 1 points down");
    }

    #[test]
    fn variable_span_lookup_hits_the_wide_slice_across_its_arc() {
        // A 180° slice at seat 0 (start 0): its arc is [-90, 90) in aim
        // degrees, so anything from left-of-up to right-of-up lands on it,
        // while straight down falls to one of the narrow slices.
        let layout = resolve_spans(&[Some(180.0), None, None], 0.0);
        assert_eq!(seat_at(0.0, 1.0, &layout, 0.5), Some(0), "up");
        assert_eq!(seat_at(0.7, 0.7, &layout, 0.5), Some(0), "up-right still seat 0");
        assert_eq!(seat_at(-0.7, 0.7, &layout, 0.5), Some(0), "up-left still seat 0");
        assert_ne!(seat_at(0.0, -1.0, &layout, 0.5), Some(0), "down is a narrow slice");
    }

    fn leaf(label: &str) -> WheelSlice {
        WheelSlice {
            label: label.to_string(),
            command: label.to_string(),
            ..Default::default()
        }
    }

    fn folder(label: &str, children: Vec<WheelSlice>) -> WheelSlice {
        WheelSlice {
            label: label.to_string(),
            command: String::new(),
            slices: children,
            ..Default::default()
        }
    }

    /// A fresh wheel state as poll_gamepad opens it.
    fn fresh_ui() -> WheelUi {
        WheelUi {
            key: String::new(),
            path: Vec::new(),
            aimed: None,
            candidate: None,
            candidate_since: None,
            rearm_until_center: false,
            peak_magnitude: 0.0,
        }
    }

    /// The shipped default feel with a chosen fire mode: 50% dead zone,
    /// 150 ms dwells, 90% edge threshold, 10% retract delta.
    fn feel(fire_mode: FireMode) -> WheelTiming {
        WheelTiming {
            deadzone: 0.5,
            aim_ms: 150,
            nav_ms: 150,
            fire_mode,
            edge_threshold: 0.9,
            retract_delta: 0.10,
        }
    }

    #[test]
    fn top_level_view_has_no_back_slice() {
        let real = vec![leaf("a"), leaf("b"), leaf("c")];
        let view = WheelView::build(&real, false, "down", 0.0);
        assert_eq!(view.len(), 3);
        // Every display index maps to a real slice.
        for i in 0..3 {
            assert_eq!(view.real(i), Some(Some(i)));
        }
    }

    #[test]
    fn folder_view_appends_back_as_last_seat() {
        let real = vec![leaf("a"), leaf("b"), leaf("c")];
        let view = WheelView::build(&real, true, "down", 0.0);
        assert_eq!(view.len(), 4);
        // Back is always the LAST display seat now (no array rotation); the
        // real slices keep their order at display indices 0..n-1.
        assert_eq!(view.real(0), Some(Some(0)));
        assert_eq!(view.real(1), Some(Some(1)));
        assert_eq!(view.real(2), Some(Some(2)));
        assert_eq!(view.real(3), Some(None), "Back is the last seat");
        assert_eq!(view.slices[3].command, BACK_COMMAND);
    }

    #[test]
    fn back_center_lands_on_the_named_side() {
        // Back is positioned by ANGLE now: its seat center sits at the
        // anchor direction (up=0, right=90, down=180, left=270 in the aim
        // convention), whatever the seat count or spans.
        let real = vec![leaf("a"), leaf("b"), leaf("c")];
        for (anchor, want) in [("up", 0.0), ("right", 90.0), ("down", 180.0), ("left", 270.0)] {
            let view = WheelView::build(&real, true, anchor, 0.0);
            let back = *view.layout.seats.last().unwrap();
            assert!(
                angular_gap(back.center_deg(), want) < 1e-3,
                "Back center for {anchor}: {} vs {want}",
                back.center_deg()
            );
        }
    }

    #[test]
    fn back_slice_none_skips_the_back_seat() {
        // back_anchor "none": a folder ring is just the real slices, no
        // reserved Back seat — you ascend via the East/B accelerator.
        let real = vec![leaf("a"), leaf("b"), leaf("c")];
        let view = WheelView::build(&real, true, "none", 0.0);
        assert_eq!(view.len(), 3, "no Back seat");
        for i in 0..3 {
            assert_eq!(view.real(i), Some(Some(i)));
        }
    }

    fn leaf_inner(label: &str, inner: u8) -> WheelSlice {
        WheelSlice {
            label: label.to_string(),
            command: label.to_string(),
            inner: Some(inner),
            ..Default::default()
        }
    }

    #[test]
    fn per_slice_inner_gates_aiming_above_and_below_its_floor() {
        // Seat 0 (up) demands a 65% throw; seat 1 (right) an easy 20%.
        // Global dead zone 50%.
        let mut a = leaf_inner("hard", 65);
        a.span = Some(120.0);
        let real = vec![a, leaf_inner("easy", 20), leaf("plain")];
        let view = WheelView::build(&real, false, "down", 0.0);
        let dz = 0.5;

        // Aiming up at 0.55: past the global dead zone but under seat 0's
        // 65% floor, so it does NOT register.
        assert_eq!(seat_at_with_inner(0.0, 0.55, &view, dz), None, "under hard floor");
        // At 0.70 it clears the floor.
        assert_eq!(seat_at_with_inner(0.0, 0.70, &view, dz), Some(0), "over hard floor");

        // Seat 1 (right) registers at just 0.25 — below the global dead
        // zone — because its own inner (20%) is lower.
        assert_eq!(seat_at_with_inner(1.0, 0.0, &view, dz), Some(1));
        assert_eq!(
            seat_at_with_inner(0.25, 0.0, &view, dz).map(|s| s),
            Some(1),
            "easy slice reachable below the global dead zone"
        );

        // The plain slice (no inner) still uses the global dead zone: seat 2
        // is down (180). At 0.4 it's under 0.5 -> None; at 0.6 -> Some(2).
        assert_eq!(seat_at_with_inner(0.0, -0.4, &view, dz), None, "plain under global dz");
        assert_eq!(seat_at_with_inner(0.0, -0.6, &view, dz), Some(2), "plain over global dz");
    }

    #[test]
    fn inner_gates_commit_but_not_the_fire_mode() {
        // A high-inner leaf under a moderate throw never commits (the dwell
        // machine sees no candidate); pushing past the floor commits and
        // release then fires it — inner gates aiming, not the fire path.
        let real = vec![leaf_inner("drop", 65), leaf("b"), leaf("c")];
        let view = WheelView::build(&real, false, "down", 0.0);
        let t = feel(FireMode::Release);
        let mut ui = fresh_ui();
        let t0 = Instant::now();

        // Dwell up at 0.55 (under the 65% floor): no candidate ever, no
        // commit, so release fires nothing.
        wheel_aim_step(&mut ui, &view, &t, 0.0, 0.55, t0);
        wheel_aim_step(&mut ui, &view, &t, 0.0, 0.55, steps(t0, 300));
        assert_eq!(ui.candidate, None, "sub-floor seat is not a candidate");
        assert_eq!(ui.aimed, None);
        assert_eq!(wheel_release_command(&ui, &view), None);

        // Push past the floor and dwell: now it commits and release fires.
        wheel_aim_step(&mut ui, &view, &t, 0.0, 0.9, steps(t0, 400));
        wheel_aim_step(&mut ui, &view, &t, 0.0, 0.9, steps(t0, 600));
        assert_eq!(ui.aimed, Some(0), "clears the floor -> commits");
        assert_eq!(wheel_release_command(&ui, &view), Some("drop".to_string()));
    }

    #[test]
    fn aim_stick_resolution_honors_override_then_default() {
        // No override: aim stick is the one that isn't the movement stick.
        assert_eq!(resolve_aim_stick(false, None), true); // move left -> aim right
        assert_eq!(resolve_aim_stick(true, None), false); // move right -> aim left
        // Per-wheel override wins regardless of movement stick — including
        // Niffy's combat-on-the-movement-stick case (move left, aim left).
        assert_eq!(resolve_aim_stick(false, Some(false)), false); // aim left
        assert_eq!(resolve_aim_stick(false, Some(true)), true); // aim right
        assert_eq!(resolve_aim_stick(true, Some(true)), true);
    }

    #[test]
    fn aim_recenter_latch_clears_only_near_center() {
        // Fully deflected: still latched (function stays suppressed).
        assert!(!aim_stick_centered(0.0, 1.0));
        assert!(!aim_stick_centered(0.8, 0.0));
        // Just above the release threshold: still latched.
        assert!(!aim_stick_centered(0.0, STICK_RELEASE + 0.01));
        // Back inside the release threshold: latch clears.
        assert!(aim_stick_centered(0.0, 0.0));
        assert!(aim_stick_centered(0.1, 0.1));
    }

    #[test]
    fn rearm_latch_only_clears_on_recenter_and_blocks_chaining() {
        // Latched + stick still deflected (has a candidate): stays latched,
        // no dwell — this is the anti-chaining guard. Crucially it does NOT
        // depend on any display index, so a level change (different seat
        // count) can't clear it.
        assert_eq!(dwell_rearm_step(true, false), (true, false));
        // Latched + stick recentered (neutral): latch clears, but no dwell
        // this frame (nothing under the stick yet).
        assert_eq!(dwell_rearm_step(true, true), (false, false));
        // Unlatched + deflected: normal dwell may accrue.
        assert_eq!(dwell_rearm_step(false, false), (false, true));
        // Unlatched + neutral: nothing to dwell.
        assert_eq!(dwell_rearm_step(false, true), (false, false));
    }

    #[test]
    fn fire_mode_parses_with_release_fallback() {
        assert_eq!(FireMode::from_str("release"), FireMode::Release);
        assert_eq!(FireMode::from_str("edge"), FireMode::Edge);
        assert_eq!(FireMode::from_str("retract"), FireMode::Retract);
        // Unknown / empty / legacy configs fall back to the safe default so
        // the wheel can always still fire.
        assert_eq!(FireMode::from_str(""), FireMode::Release);
        assert_eq!(FireMode::from_str("nonsense"), FireMode::Release);
    }

    #[test]
    fn edge_fires_at_or_above_threshold() {
        // 90% threshold: below doesn't fire, at/above does.
        assert!(!edge_should_fire(0.89, 0.90));
        assert!(edge_should_fire(0.90, 0.90));
        assert!(edge_should_fire(1.00, 0.90));
    }

    #[test]
    fn retract_fires_only_after_pulling_back_from_peak() {
        // peak 1.0, delta 0.10: fires once magnitude drops to <= 0.90.
        assert!(!retract_should_fire(1.00, 1.00, 0.10)); // still at peak
        assert!(!retract_should_fire(0.95, 1.00, 0.10)); // not far enough in
        assert!(retract_should_fire(0.90, 1.00, 0.10)); // exactly delta in
        assert!(retract_should_fire(0.50, 1.00, 0.10)); // well past
        // A shallower peak needs a correspondingly shallow retraction. The
        // exact boundary (0.70 - 0.10 = 0.60) must fire despite f32 rounding.
        assert!(!retract_should_fire(0.65, 0.70, 0.10));
        assert!(retract_should_fire(0.60, 0.70, 0.10));
        // A hair above the boundary still holds (the epsilon is far smaller
        // than the 1% config granularity, so it won't fire early).
        assert!(!retract_should_fire(0.605, 0.70, 0.10));
    }

    // ---- whole-interaction scenarios against the extracted machine ----
    //
    // These drive wheel_aim_step / wheel_release_command / wheel_south_step
    // across frames with a synthetic clock — the wheel's behavior without a
    // controller in hand. Directions: (0,1)=up, (1,0)=right, (0,-1)=down.

    use std::time::{Duration, Instant};

    fn steps(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn scenario_release_dwell_commits_then_release_fires() {
        let view = WheelView::build(&[leaf("a"), leaf("b"), leaf("c")], false, "down", 0.0);
        let t = feel(FireMode::Release);
        let mut ui = fresh_ui();
        let t0 = Instant::now();

        // Frame 1: stick lands on seat 0 — candidate, no commit yet.
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, t0);
        assert_eq!((ui.candidate, ui.aimed, out.fire), (Some(0), None, None));
        // Releasing mid-sweep (before the dwell) fires nothing.
        assert_eq!(wheel_release_command(&ui, &view), None);

        // Frame 2, 160 ms later: dwell elapsed — committed, still no fire.
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, steps(t0, 160));
        assert_eq!((ui.aimed, out.fire), (Some(0), None));
        // Release now fires the committed leaf.
        assert_eq!(wheel_release_command(&ui, &view), Some("a".to_string()));
    }

    #[test]
    fn scenario_sweeping_across_slices_never_commits() {
        let view = WheelView::build(&[leaf("a"), leaf("b"), leaf("c")], false, "down", 0.0);
        let t = feel(FireMode::Release);
        let mut ui = fresh_ui();
        let t0 = Instant::now();

        // Sweep seat 0 -> 1 -> 2, each hop under the 150 ms dwell: every
        // hop restarts the clock, so nothing ever commits.
        wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, t0);
        wheel_aim_step(&mut ui, &view, &t, 1.0, 0.0, steps(t0, 100));
        assert_eq!(ui.aimed, None);
        wheel_aim_step(&mut ui, &view, &t, 0.0, -1.0, steps(t0, 200));
        assert_eq!((ui.candidate, ui.aimed), (Some(2), None));
        assert_eq!(wheel_release_command(&ui, &view), None);
    }

    #[test]
    fn scenario_center_then_release_cancels() {
        let view = WheelView::build(&[leaf("a"), leaf("b"), leaf("c")], false, "down", 0.0);
        let t = feel(FireMode::Release);
        let mut ui = fresh_ui();
        let t0 = Instant::now();

        // Commit seat 0, then return to center: the commit is dropped, so
        // releasing fires nothing (the cancel gesture).
        wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, t0);
        wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, steps(t0, 200));
        assert_eq!(ui.aimed, Some(0));
        wheel_aim_step(&mut ui, &view, &t, 0.0, 0.0, steps(t0, 300));
        assert_eq!(ui.aimed, None);
        assert_eq!(wheel_release_command(&ui, &view), None);
    }

    #[test]
    fn scenario_folder_descends_and_rearm_blocks_chaining() {
        // Top level: [folder, leaf] — two seats, folder at the top.
        let top = WheelView::build(
            &[folder("f", vec![leaf("a"), leaf("b")]), leaf("x")],
            false,
            "down",
            0.0,
        );
        let t = feel(FireMode::Release);
        let mut ui = fresh_ui();
        let t0 = Instant::now();

        // Dwell the folder: path descends and the rearm latch goes up.
        wheel_aim_step(&mut ui, &top, &t, 0.0, 1.0, t0);
        wheel_aim_step(&mut ui, &top, &t, 0.0, 1.0, steps(t0, 160));
        assert_eq!(ui.path, vec![0]);
        assert!(ui.rearm_until_center);

        // Still fully deflected inside the folder ring: the latch blocks
        // all dwell — no chained descend, no commit — for as long as the
        // stick stays out.
        let inner = WheelView::build(&[leaf("a"), leaf("b")], true, "down", 0.0);
        wheel_aim_step(&mut ui, &inner, &t, 0.0, 1.0, steps(t0, 170));
        wheel_aim_step(&mut ui, &inner, &t, 0.0, 1.0, steps(t0, 600));
        assert_eq!(ui.path, vec![0], "latched stick must not chain");
        assert_eq!(ui.aimed, None);
        assert!(ui.rearm_until_center);

        // Re-center once: latch clears. Re-aim and dwell: now it commits.
        wheel_aim_step(&mut ui, &inner, &t, 0.0, 0.0, steps(t0, 700));
        assert!(!ui.rearm_until_center);
        wheel_aim_step(&mut ui, &inner, &t, 0.0, 1.0, steps(t0, 800));
        wheel_aim_step(&mut ui, &inner, &t, 0.0, 1.0, steps(t0, 1000));
        assert!(ui.aimed.is_some());
    }

    #[test]
    fn scenario_back_dwell_ascends() {
        // Inside a folder: 3 real slices + Back appended last and anchored
        // down. Back is display index 3 (the last seat); aiming down lands
        // on it by angle.
        let view = WheelView::build(&[leaf("a"), leaf("b"), leaf("c")], true, "down", 0.0);
        let back_at = 3;
        assert_eq!(view.real(back_at), Some(None), "Back is the last seat");

        let t = feel(FireMode::Release);
        let mut ui = fresh_ui();
        ui.path = vec![0];
        let t0 = Instant::now();

        wheel_aim_step(&mut ui, &view, &t, 0.0, -1.0, t0);
        assert_eq!(ui.candidate, Some(back_at), "aiming down hits Back");
        wheel_aim_step(&mut ui, &view, &t, 0.0, -1.0, steps(t0, 160));
        assert_eq!(ui.path, Vec::<usize>::new(), "Back dwell ascends");
        assert!(ui.rearm_until_center);
    }

    #[test]
    fn scenario_edge_fires_at_threshold_without_dwell() {
        let view = WheelView::build(&[leaf("a"), leaf("b"), leaf("c")], false, "down", 0.0);
        let t = feel(FireMode::Edge);
        let mut ui = fresh_ui();
        let t0 = Instant::now();

        // Below the 90% threshold: aimable but never fires, no matter how
        // long it dwells (edge mode doesn't commit).
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 0.7, t0);
        assert_eq!(out.fire, None);
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 0.7, steps(t0, 500));
        assert_eq!((out.fire, ui.aimed), (None, None));

        // Cross the threshold: fires the same frame — zero dwell.
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 0.95, steps(t0, 510));
        assert_eq!(out.fire, Some(0));
    }

    #[test]
    fn scenario_edge_respects_rearm_latch() {
        let view = WheelView::build(&[leaf("a"), leaf("b"), leaf("c")], false, "down", 0.0);
        let t = feel(FireMode::Edge);
        let mut ui = fresh_ui();
        // As after an auto descend: stick still out, latch up.
        ui.rearm_until_center = true;
        let t0 = Instant::now();

        // Full deflection cannot fire through the latch.
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, t0);
        assert_eq!(out.fire, None);
        // Re-center clears it; the next full deflection fires.
        wheel_aim_step(&mut ui, &view, &t, 0.0, 0.0, steps(t0, 100));
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, steps(t0, 200));
        assert_eq!(out.fire, Some(0));
    }

    #[test]
    fn scenario_retract_tracks_peak_and_fires_on_inward_flick() {
        let view = WheelView::build(&[leaf("a"), leaf("b"), leaf("c")], false, "down", 0.0);
        let t = feel(FireMode::Retract);
        let mut ui = fresh_ui();
        let t0 = Instant::now();

        // Dwell at full throw: commits, peak = 1.0, no fire yet.
        wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, t0);
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, steps(t0, 200));
        assert_eq!((ui.aimed, out.fire), (Some(0), None));
        assert!((ui.peak_magnitude - 1.0).abs() < 1e-6);

        // Easing to 0.95 is inside the 10% delta: still holding.
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 0.95, steps(t0, 250));
        assert_eq!(out.fire, None);
        // Flick inward past peak - delta (0.90): fires.
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 0.88, steps(t0, 300));
        assert_eq!(out.fire, Some(0));
    }

    #[test]
    fn scenario_retract_snap_to_center_cancels() {
        let view = WheelView::build(&[leaf("a"), leaf("b"), leaf("c")], false, "down", 0.0);
        let t = feel(FireMode::Retract);
        let mut ui = fresh_ui();
        let t0 = Instant::now();

        // Commit at full throw, then snap inside the dead zone in one
        // frame: the candidate drops before the retract check can run, so
        // it cancels instead of firing.
        wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, t0);
        wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, steps(t0, 200));
        assert_eq!(ui.aimed, Some(0));
        let out = wheel_aim_step(&mut ui, &view, &t, 0.0, 0.2, steps(t0, 250));
        assert_eq!((out.fire, ui.aimed), (None, None));
        assert_eq!(wheel_release_command(&ui, &view), None);
    }

    #[test]
    fn scenario_folder_ring_with_back_descends_and_commits_correctly() {
        // A Back-bearing folder ring: children [a(leaf), inner(folder),
        // c(leaf)] + Back appended last. Append-not-rotate means real slices
        // sit at their OWN display index (0..n-1) and Back is last, so the
        // display-vs-real divergence can no longer occur by construction —
        // this test locks that in while still exercising folder-descend and
        // leaf-commit in a ring that has a Back seat.
        //
        // 4 even seats anchored "down": Back(3) is placed at 180 by a -90
        // rotation, so the reals land at up=seat1(inner), left=seat0(a),
        // right=seat2(c).
        let children = vec![leaf("a"), folder("inner", vec![leaf("b")]), leaf("c")];
        let view = WheelView::build(&children, true, "down", 0.0);
        assert_eq!(view.real(0), Some(Some(0)), "reals keep their own index");
        assert_eq!(view.real(1), Some(Some(1)));
        assert_eq!(view.real(3), Some(None), "Back last");

        let t = feel(FireMode::Release);
        let t0 = Instant::now();

        // Up aims the inner folder (seat 1) — dwelling descends.
        let mut ui = fresh_ui();
        ui.path = vec![0];
        wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, t0);
        wheel_aim_step(&mut ui, &view, &t, 0.0, 1.0, steps(t0, 160));
        assert_eq!(ui.path, vec![0, 1], "folder seat descends");
        assert_eq!(ui.aimed, None);

        // Left aims leaf a (seat 0) — dwelling commits, never descends.
        let mut ui = fresh_ui();
        ui.path = vec![0];
        wheel_aim_step(&mut ui, &view, &t, -1.0, 0.0, t0);
        wheel_aim_step(&mut ui, &view, &t, -1.0, 0.0, steps(t0, 160));
        assert_eq!(ui.path, vec![0], "leaf seat must not descend");
        assert_eq!(ui.aimed, Some(0), "leaf seat commits");
    }

    /// The golden-vector parity harness: one truth table
    /// (tests/data/wheel_golden.json) driven against this machine here and
    /// against the shipped JS core via `node tests/wheel_parity.cjs`. A
    /// geometry or state-machine change on one side only turns the other
    /// side's run red instead of the phone firing a different slice than
    /// the desktop. Keep the two runners semantically identical.
    #[test]
    fn golden_vectors_match_the_rust_machine() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../../../../tests/data/wheel_golden.json"))
                .expect("wheel_golden.json parses");

        for case in data["geometry"].as_array().unwrap() {
            let got = wheel_slice_at(
                case["x"].as_f64().unwrap() as f32,
                case["yUp"].as_f64().unwrap() as f32,
                case["count"].as_u64().unwrap() as usize,
                case["deadzone"].as_f64().unwrap() as f32,
            );
            assert_eq!(
                got.map(|v| v as u64),
                case["expect"].as_u64(),
                "geometry {case}"
            );
        }

        // Back placement is checked by ANGLE (Rust-only until B7): Back is
        // the last seat, and its center sits at the anchor direction.
        for case in data["back_placement"].as_array().unwrap() {
            let n = case["realCount"].as_u64().unwrap() as usize;
            let real: Vec<WheelSlice> = (0..n).map(|i| leaf(&format!("s{i}"))).collect();
            let view = WheelView::build(&real, true, case["anchor"].as_str().unwrap(), 0.0);
            let back = *view.layout.seats.last().unwrap();
            assert_eq!(view.real(view.len() - 1), Some(None), "Back is last: {case}");
            let want = case["expectBackCenterDeg"].as_f64().unwrap() as f32;
            assert!(
                angular_gap(back.center_deg(), want) < 1e-2,
                "back angle {case}: got {}",
                back.center_deg()
            );
        }

        // Every scenario runs here AND in node/wheel-core.js — one shared
        // contract now that the phone has the same geometry.
        let run_scenario = |sc: &serde_json::Value| {
            let name = sc["name"].as_str().unwrap();
            let ring = &sc["ring"];
            let folders: Vec<usize> = ring["folders"]
                .as_array()
                .map(|a| a.iter().map(|v| v.as_u64().unwrap() as usize).collect())
                .unwrap_or_default();
            let spans = ring["spans"].as_object();
            let inners = ring["inner"].as_object();
            let real: Vec<WheelSlice> = ring["labels"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let label = l.as_str().unwrap();
                    let mut slice = if folders.contains(&i) {
                        folder(label, vec![leaf("child")])
                    } else {
                        leaf(label)
                    };
                    let key = i.to_string();
                    if let Some(span) = spans.and_then(|m| m.get(&key)).and_then(|v| v.as_f64()) {
                        slice.span = Some(span as f32);
                    }
                    if let Some(inner) = inners.and_then(|m| m.get(&key)).and_then(|v| v.as_u64()) {
                        slice.inner = Some(inner as u8);
                    }
                    slice
                })
                .collect();
            let view = WheelView::build(
                &real,
                ring["inFolder"].as_bool().unwrap_or(false),
                ring["anchor"].as_str().unwrap_or("down"),
                0.0,
            );
            let timing = WheelTiming {
                deadzone: 0.5,
                aim_ms: 150,
                nav_ms: 150,
                fire_mode: FireMode::from_str(sc["fireMode"].as_str().unwrap_or("release")),
                edge_threshold: 0.9,
                retract_delta: 0.10,
            };
            let mut ui = fresh_ui();
            if let Some(p) = sc["initialPath"].as_array() {
                ui.path = p.iter().map(|v| v.as_u64().unwrap() as usize).collect();
            }
            if sc["initialRearm"].as_bool() == Some(true) {
                ui.rearm_until_center = true;
            }

            let t0 = Instant::now();
            let mut fired: Option<usize> = None;
            for frame in sc["frames"].as_array().unwrap() {
                let out = wheel_aim_step(
                    &mut ui,
                    &view,
                    &timing,
                    frame["x"].as_f64().unwrap() as f32,
                    frame["y"].as_f64().unwrap() as f32,
                    steps(t0, frame["t"].as_u64().unwrap()),
                );
                if let Some(display) = out.fire {
                    fired = Some(display);
                    break; // the wheel closes on a mid-hold fire
                }
            }

            let expect = sc["expect"].as_object().unwrap();
            if let Some(want) = expect.get("fired") {
                assert_eq!(fired.map(|v| v as u64), want.as_u64(), "{name}: fired");
            }
            if let Some(want) = expect.get("aimed") {
                assert_eq!(ui.aimed.map(|v| v as u64), want.as_u64(), "{name}: aimed");
            }
            if let Some(want) = expect.get("path") {
                let want: Vec<usize> = want
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as usize)
                    .collect();
                assert_eq!(ui.path, want, "{name}: path");
            }
            if let Some(want) = expect.get("releaseReal") {
                // The real index release would fire, or None — matches the
                // JS side's leafRealAt(view, aimed) (phone picks are
                // real-index paths; commands resolve host-side).
                let got = ui.aimed.and_then(|display| {
                    leaf_command_at(&view, display)
                        .and_then(|_| view.real(display).flatten())
                });
                assert_eq!(got.map(|v| v as u64), want.as_u64(), "{name}: releaseReal");
            }
        };

        for sc in data["scenarios"].as_array().unwrap() {
            run_scenario(sc);
        }
    }

    #[test]
    fn scenario_south_fires_leaf_descends_folder_pops_back() {
        let t0_view = WheelView::build(
            &[folder("f", vec![leaf("a")]), leaf("x")],
            false,
            "down",
            0.0,
        );
        // Leaf under the stick: South returns it to fire.
        let mut ui = fresh_ui();
        ui.candidate = Some(1);
        let out = wheel_south_step(&mut ui, &t0_view);
        assert_eq!(out.fire, Some(1));

        // Folder under the stick: South descends and re-arms.
        let mut ui = fresh_ui();
        ui.candidate = Some(0);
        let out = wheel_south_step(&mut ui, &t0_view);
        assert_eq!((out.fire, ui.path.as_slice()), (None, &[0][..]));
        assert!(ui.rearm_until_center);

        // Back under the stick inside a folder: South ascends. Back is the
        // last display seat now.
        let inner = WheelView::build(&[leaf("a"), leaf("b"), leaf("c")], true, "down", 0.0);
        let back_at = inner.slices.len() - 1;
        assert_eq!(inner.real(back_at), Some(None));
        let mut ui = fresh_ui();
        ui.path = vec![0];
        ui.candidate = Some(back_at);
        let out = wheel_south_step(&mut ui, &inner);
        assert_eq!((out.fire, ui.path.len()), (None, 0));
    }
}

impl VellumGuiApp {
    /// Binding-legend overlay: a compact right-edge panel listing the
    /// curated entries from [controller_overlay] (base and shift/
    /// bindings), with the shift bank marked. Toggled by the
    /// controller_overlay action (Select by default).
    pub(super) fn render_controller_overlay(&mut self, ctx: &egui::Context) {
        if !self.gp_overlay {
            return;
        }
        let entries = self.app_core.config.controller_overlay.clone();
        if entries.is_empty() {
            return;
        }
        let shift_held = self.gamepad_shift_held();
        egui::Area::new(egui::Id::new("controller_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-12.0, 0.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.strong("Controller");
                    for entry in &entries {
                        let (is_shift, button) = match entry.strip_prefix("shift/") {
                            Some(rest) => (true, rest),
                            None => (false, entry.as_str()),
                        };
                        let binds = if is_shift {
                            &self.app_core.config.controller_shift_binds
                        } else {
                            &self.app_core.config.controller_binds
                        };
                        let Some(action) = binds.get(button) else {
                            continue;
                        };
                        let what = match action {
                            crate::config::KeyBindAction::Action(name) => name.clone(),
                            crate::config::KeyBindAction::Macro(m) => {
                                m.macro_text.replace(['\r', '\n'], " ").trim().to_string()
                            }
                        };
                        let line = if is_shift {
                            format!("[shift] {} - {}", button, what)
                        } else {
                            format!("{} - {}", button, what)
                        };
                        // The active bank reads strong while shift is held.
                        if is_shift == shift_held {
                            ui.label(egui::RichText::new(line).strong());
                        } else {
                            ui.weak(line);
                        }
                    }
                });
            });
    }
}
