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
enum FireMode {
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
                // debounce window has elapsed.
                let ui = self.gp_wheel.take().expect("just matched Some");
                if let Some(display) = ui.aimed {
                    let view = self.wheel_view(&ui.key, &ui.path);
                    // `view.slices` is the DISPLAY-ordered (Back-injected,
                    // rotated) ring, so index it by `display`, not the real
                    // index — indexing by `real` double-applied the rotation
                    // and fired the wrong slice inside a rotated folder.
                    if let Some(Some(_real)) = view.as_ref().and_then(|v| v.real(display)) {
                        if let Some(slice) =
                            view.as_ref().and_then(|v| v.slices.get(display)).cloned()
                        {
                            if !slice.is_folder() && !slice.command.is_empty() {
                                self.wheel_fire(slice.command);
                            }
                        }
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
        Some(WheelView::build(&real, !path.is_empty(), anchor))
    }

    /// Advance the dwell state machine for one frame while the wheel is
    /// up and the aim stick is at `(x, y_up)`. Tracks the candidate slice
    /// and its dwell time; commits a leaf (arming release-fire), auto-
    /// descends a folder, or auto-ascends via the Back slice once the
    /// relevant dwell elapses. The re-arm block prevents a still-deflected
    /// stick from chaining through nested levels.
    fn wheel_aim(&mut self, x: f32, y_up: f32) {
        let (key, path) = {
            let ui = self.gp_wheel.as_ref().expect("wheel active");
            (ui.key.clone(), ui.path.clone())
        };
        let Some(view) = self.wheel_view(&key, &path) else {
            return;
        };
        let deadzone = self.wheel_deadzone();
        let aim_ms = self.app_core.config.controller_tuning.aim_dwell_ms as u128;
        let nav_ms = self.app_core.config.controller_tuning.nav_dwell_ms as u128;
        let fire_mode = self.wheel_fire_mode();
        let magnitude = (x * x + y_up * y_up).sqrt();
        let candidate = wheel_slice_at(x, y_up, view.len(), deadzone);
        // The stick is "neutral" when it has fallen back inside the dead
        // zone (candidate None). Re-neutralizing is the ONLY thing that
        // clears the re-arm latch — keyed on physical stick state, not a
        // display index (indices are meaningless across a level change,
        // since seat counts differ).
        let centered = candidate.is_none();
        let latched = self
            .gp_wheel
            .as_ref()
            .map(|ui| ui.rearm_until_center)
            .unwrap_or(true);
        let (latch_after, may_dwell) = dwell_rearm_step(latched, centered);

        if let Some(ui) = self.gp_wheel.as_mut() {
            ui.rearm_until_center = latch_after;
            // Track the candidate and restart its dwell clock whenever it
            // changes. Any change also drops a prior commit — release only
            // fires a leaf the stick is *currently* dwelling, never a stale
            // one it has already moved off (whether to center or to another
            // slice).
            if ui.candidate != candidate {
                ui.candidate = candidate;
                ui.candidate_since = Some(std::time::Instant::now());
                ui.aimed = None;
                ui.peak_magnitude = 0.0;
                self.app_core.needs_render = true;
            }
        }

        // While the latch is up (stick hasn't re-neutralized since the last
        // auto descend/ascend), no dwell may accrue — this is what stops a
        // still-deflected stick from chaining through nested folders.
        if !may_dwell {
            return;
        }

        // Nothing under the stick: nothing to dwell.
        let Some(display) = candidate else { return };
        let dwelt = self
            .gp_wheel
            .as_ref()
            .and_then(|ui| ui.candidate_since)
            .map(|t| t.elapsed().as_millis())
            .unwrap_or(0);

        match view.real(display) {
            // Back slice: auto-ascend once the nav dwell elapses.
            Some(None) => {
                if dwelt >= nav_ms {
                    if let Some(ui) = self.gp_wheel.as_mut() {
                        ui.path.pop();
                        ui.aimed = None;
                        ui.candidate = None;
                        ui.candidate_since = None;
                        ui.rearm_until_center = true;
                        self.app_core.needs_render = true;
                    }
                }
            }
            // A real slice.
            Some(Some(real)) => {
                let is_folder = view.slices.get(real).map(|s| s.is_folder()).unwrap_or(false);
                if is_folder {
                    // Folders always descend on dwell — never fired by edge
                    // or retract (Niffy's invariant).
                    if dwelt >= nav_ms {
                        if let Some(ui) = self.gp_wheel.as_mut() {
                            ui.path.push(real);
                            ui.aimed = None;
                            ui.candidate = None;
                            ui.candidate_since = None;
                            ui.rearm_until_center = true;
                            ui.peak_magnitude = 0.0;
                            self.app_core.needs_render = true;
                        }
                    }
                    return;
                }

                // Leaf, by fire mode.
                match fire_mode {
                    FireMode::Edge => {
                        // Fire the moment deflection crosses the threshold —
                        // no dwell. `may_dwell` gated us here, so the rearm
                        // latch already blocks a still-deflected stick from
                        // refiring across slices until it recenters.
                        if edge_should_fire(magnitude, self.wheel_edge_threshold()) {
                            self.wheel_fire_committed_leaf(&key, &path, display);
                        }
                    }
                    FireMode::Retract => {
                        // Dwell to commit, then track the deflection peak and
                        // fire once it falls retract_delta below that peak.
                        if dwelt >= aim_ms {
                            if let Some(ui) = self.gp_wheel.as_mut() {
                                if ui.aimed != Some(display) {
                                    ui.aimed = Some(display);
                                    ui.peak_magnitude = magnitude;
                                    self.app_core.needs_render = true;
                                }
                                ui.peak_magnitude = ui.peak_magnitude.max(magnitude);
                            }
                            let committed = self
                                .gp_wheel
                                .as_ref()
                                .map(|ui| (ui.aimed == Some(display), ui.peak_magnitude))
                                .unwrap_or((false, 0.0));
                            if committed.0
                                && retract_should_fire(
                                    magnitude,
                                    committed.1,
                                    self.wheel_retract_delta(),
                                )
                            {
                                self.wheel_fire_committed_leaf(&key, &path, display);
                            }
                        }
                    }
                    FireMode::Release => {
                        // Commit (arm release-fire); the release arm in
                        // poll_gamepad does the firing.
                        if dwelt >= aim_ms {
                            if let Some(ui) = self.gp_wheel.as_mut() {
                                if ui.aimed != Some(display) {
                                    ui.aimed = Some(display);
                                    self.app_core.needs_render = true;
                                }
                            }
                        }
                    }
                }
            }
            None => {}
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

    /// Fire the committed leaf at display index `display` and close the
    /// wheel, exactly as the South accelerator does. Used by the `edge` and
    /// `retract` fire modes, which fire a leaf mid-hold instead of waiting
    /// for the wheel button to come up. Folders are never passed here.
    fn wheel_fire_committed_leaf(&mut self, key: &str, path: &[usize], display: usize) {
        let Some(view) = self.wheel_view(key, path) else {
            return;
        };
        let Some(slice) = view.slices.get(display).cloned() else {
            return;
        };
        if slice.is_folder() || slice.command.is_empty() {
            return;
        }
        self.gp_wheel = None;
        self.gp_wheel_fired = true;
        self.gp_aim_recenter_needed = true;
        self.wheel_fire(slice.command);
        self.app_core.needs_render = true;
    }

    /// Wheel dead zone as a 0.0–1.0 stick magnitude (percent in config).
    fn wheel_deadzone(&self) -> f32 {
        (self.app_core.config.controller_tuning.deadzone as f32 / 100.0).clamp(0.0, 0.99)
    }

    /// South accelerator while the wheel is up: act on whatever the stick
    /// is over right now (the committed slice, or the live candidate if a
    /// dwell hasn't landed yet), skipping the dwell wait. Fires a leaf,
    /// descends a folder, or ascends via the Back slice.
    fn wheel_accelerator_south(&mut self) {
        // Act on the slice the stick is over *right now* — the live
        // candidate wins over any earlier commit, so South never fires a
        // slice the stick has already moved off.
        let (key, path, target) = {
            let ui = self.gp_wheel.as_ref().expect("wheel active");
            (ui.key.clone(), ui.path.clone(), ui.candidate.or(ui.aimed))
        };
        let Some(display) = target else { return };
        let Some(view) = self.wheel_view(&key, &path) else {
            return;
        };
        match view.real(display) {
            Some(None) => {
                // Back slice.
                if let Some(ui) = self.gp_wheel.as_mut() {
                    if ui.path.pop().is_some() {
                        ui.aimed = None;
                        ui.candidate = None;
                        ui.candidate_since = None;
                        ui.rearm_until_center = true;
                        self.app_core.needs_render = true;
                    }
                }
            }
            Some(Some(real)) => {
                // Look the slice up by DISPLAY index (view.slices is the
                // rotated ring); `real` is only for path.push on descend.
                let slice = match view.slices.get(display) {
                    Some(s) => s.clone(),
                    None => return,
                };
                if slice.is_folder() {
                    if let Some(ui) = self.gp_wheel.as_mut() {
                        ui.path.push(real);
                        ui.aimed = None;
                        ui.candidate = None;
                        ui.candidate_since = None;
                        ui.rearm_until_center = true;
                        self.app_core.needs_render = true;
                    }
                } else if !slice.command.is_empty() {
                    self.gp_wheel = None;
                    self.gp_wheel_fired = true;
                    self.gp_aim_recenter_needed = true;
                    self.wheel_fire(slice.command);
                    self.app_core.needs_render = true;
                }
            }
            None => {}
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
                let step = std::f32::consts::TAU / slices.len() as f32;

                // Wedge fills: the whole pie piece carries the slice's
                // color (dim at rest, bright while aimed); colorless
                // slices highlight with the theme selection fill.
                for (i, slice) in slices.iter().enumerate() {
                    let center_angle = i as f32 * step - std::f32::consts::FRAC_PI_2;
                    let is_selected = selected == Some(i);
                    let tint = slice
                        .color
                        .as_deref()
                        .and_then(super::theme::resolve_color);
                    let fill = match (tint, is_selected) {
                        (Some(c), true) => c.gamma_multiply(0.85),
                        (Some(c), false) => c.gamma_multiply(0.22),
                        (None, true) => ui.visuals().selection.bg_fill,
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
                    let center_angle = i as f32 * step - std::f32::consts::FRAC_PI_2;
                    if slices.len() > 1 {
                        let a0 = center_angle - step / 2.0;
                        let dir = egui::vec2(a0.cos(), a0.sin());
                        painter.line_segment(
                            [center + dir * hub, center + dir * outer],
                            egui::Stroke::new(1.0, ui.visuals().window_stroke.color),
                        );
                    }
                    let pos = center + egui::vec2(center_angle.cos(), center_angle.sin()) * radius;
                    let is_selected = selected == Some(i);
                    let (color, size) = if is_selected {
                        (ui.visuals().strong_text_color(), 18.0)
                    } else {
                        (ui.visuals().text_color(), 14.0)
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

/// Which wheel slice the stick is aiming at: slice 0 centered at the top,
/// clockwise. None inside the dead zone (`deadzone`, 0.0–1.0) or with no
/// slices.
fn wheel_slice_at(x: f32, y_up: f32, count: usize, deadzone: f32) -> Option<usize> {
    if count == 0 || (x * x + y_up * y_up).sqrt() < deadzone {
        return None;
    }
    let step = 360.0 / count as f32;
    let angle = x.atan2(y_up).to_degrees();
    Some((((angle + 360.0 + step / 2.0) / step) as usize) % count)
}

/// Display index (0 = top, clockwise) whose seat center lies nearest a
/// screen-anchor word, given `count` evenly-spaced seats. Used to place
/// the reserved Back slice at its configured side.
fn anchor_display_index(anchor: &str, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    // Anchor screen direction as a (x, y_up) unit-ish vector, y_up
    // positive = up (matching the stick convention wheel_slice_at reads).
    let (ax, ay) = match anchor {
        "up" => (0.0, 1.0),
        "down" => (0.0, -1.0),
        "left" => (-1.0, 0.0),
        "right" => (1.0, 0.0),
        "up-left" => (-1.0, 1.0),
        "up-right" => (1.0, 1.0),
        "down-left" => (-1.0, -1.0),
        "down-right" => (1.0, -1.0),
        _ => (0.0, -1.0), // default: down
    };
    wheel_slice_at(ax, ay, count, 0.0).unwrap_or(0)
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
}

/// Sentinel command marking the injected Back slice.
const BACK_COMMAND: &str = "\u{0}__wheel_back__";

impl WheelView {
    /// Build the display ring. `in_folder` injects the Back slice at the
    /// anchor; the top level (no parent) shows only real slices.
    fn build(real: &[WheelSlice], in_folder: bool, back_anchor: &str) -> Self {
        if !in_folder {
            return Self {
                slices: real.to_vec(),
                real_index: (0..real.len()).map(Some).collect(),
            };
        }
        // Displayed count includes the Back seat; rotate the assembled
        // ring so Back lands nearest the anchor while the real slices stay
        // in order around it.
        let count = real.len() + 1;
        let back = WheelSlice {
            label: "◂ Back".to_string(),
            command: BACK_COMMAND.to_string(),
            color: None,
            slices: Vec::new(),
        };
        // Assemble as [real..., Back] then rotate right so Back moves from
        // the last seat to the anchor seat.
        let target = anchor_display_index(back_anchor, count);
        let mut slices: Vec<WheelSlice> = real.to_vec();
        slices.push(back);
        let mut real_index: Vec<Option<usize>> = (0..real.len()).map(Some).collect();
        real_index.push(None);
        // Back currently at index count-1; rotate so it sits at `target`.
        let shift = (target + count - (count - 1)) % count; // = (target + 1) % count
        slices.rotate_right(shift);
        real_index.rotate_right(shift);
        Self { slices, real_index }
    }

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

    fn leaf(label: &str) -> WheelSlice {
        WheelSlice {
            label: label.to_string(),
            command: label.to_string(),
            color: None,
            slices: Vec::new(),
        }
    }

    #[test]
    fn top_level_view_has_no_back_slice() {
        let real = vec![leaf("a"), leaf("b"), leaf("c")];
        let view = WheelView::build(&real, false, "down");
        assert_eq!(view.len(), 3);
        // Every display index maps to a real slice.
        for i in 0..3 {
            assert_eq!(view.real(i), Some(Some(i)));
        }
    }

    #[test]
    fn folder_view_injects_back_at_anchor() {
        let real = vec![leaf("a"), leaf("b"), leaf("c")];
        // 4 seats once Back is added; "down" anchor => display index 2.
        let view = WheelView::build(&real, true, "down");
        assert_eq!(view.len(), 4);
        let back_at = anchor_display_index("down", 4);
        assert_eq!(view.real(back_at), Some(None), "Back sits at its anchor");
        assert_eq!(view.slices[back_at].command, BACK_COMMAND);
        // The three real slices are all present exactly once.
        let mut reals: Vec<usize> = (0..4).filter_map(|i| view.real(i).flatten()).collect();
        reals.sort();
        assert_eq!(reals, vec![0, 1, 2]);
    }

    #[test]
    fn back_anchor_lands_on_the_named_side() {
        // With 4 seats: up=0, right=1, down=2, left=3.
        assert_eq!(anchor_display_index("up", 4), 0);
        assert_eq!(anchor_display_index("right", 4), 1);
        assert_eq!(anchor_display_index("down", 4), 2);
        assert_eq!(anchor_display_index("left", 4), 3);
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
