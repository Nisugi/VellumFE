//! Native go2 travel: the per-frame walk-executor tick, urchin/day-pass
//! pre-flight scans, foreach ticking, trip start/stop, the outbound
//! command queue, and mapdb download management.

use super::*;

impl AppCore {
    /// Advance the walk executor against the latest world state. Called
    /// after every processed network line and once per frontend frame (the
    /// frame tick covers time-based waits like roundtime when the game is
    /// quiet).
    /// Resolve a deferred `.go2` waiting on `urchin status` (see start_travel).
    /// Once the reply has parsed (access now valid) or the deadline passes, the
    /// trip is re-planned. Mirrors Lich's `update_urchin_expire` pre-flight.
    pub(super) fn tick_urchin_refresh(&mut self) {
        let Some((destination, deadline)) = self.pending_urchin_refresh else {
            return;
        };
        let now_epoch = chrono::Utc::now().timestamp();
        let now_valid = self.game_state.character.urchins_valid(
            now_epoch,
            self.game_state.status.hidden(),
            self.game_state.status.invisible(),
        );
        let expired = std::time::Instant::now() >= deadline;
        if now_valid || expired {
            // Clear the pending marker FIRST so the re-invoked start_travel
            // sees `is_none()` and proceeds to planning instead of re-deferring.
            self.pending_urchin_refresh = None;
            if !now_valid {
                self.add_system_message(
                    "[go2] no active urchin access - routing without urchin guides",
                );
            }
            self.start_travel(destination);
        }
    }

    /// Lich's bounty guard on the day-pass sweep: skip the scan while a
    /// child-contact bounty is live, or a protective-escort bounty that isn't
    /// in its WAIT stage — stray commands there can break the task.
    /// (`bounty? !~ /^You have made contact with the child/ and
    /// (bounty? !~ /provide a protective escort/ or bounty? =~ /WAIT/)`.)
    pub(super) fn day_pass_scan_blocked_by_bounty(&self) -> bool {
        let b = &self.game_state.bounty.raw_text;
        b.starts_with("You have made contact with the child")
            || (b.contains("provide a protective escort") && !b.contains("WAIT"))
    }

    /// The day-pass sack's command-target, the ids of Chronomage day passes in
    /// it the cache hasn't learned, and whether ANY pass is visible in it.
    /// (None target = no sack configured/resolved.)
    pub(super) fn day_pass_scan_targets(&self) -> (Option<String>, Vec<String>, bool) {
        let name = self.config.go2.day_pass_sack.trim();
        if name.is_empty() {
            return (None, Vec::new(), false);
        }
        let Some(sack) = self.game_state.objects.find_container(name) else {
            return (None, Vec::new(), false);
        };
        let target = sack.command_target();
        let mut unknown = Vec::new();
        let mut any_pass = false;
        for item in self.game_state.objects.items_in(&sack.id) {
            if item.name.contains("Chronomage day pass") {
                any_pass = true;
                if !self.game_state.day_passes.contains(&item.id) {
                    unknown.push(item.id.clone());
                }
            }
        }
        (Some(target), unknown, any_pass)
    }

    /// Drop cached day passes whose id is no longer in the sack (given away,
    /// sold, lost): Lich's sweep prunes `\$mapdb_day_passes` the same way.
    /// Without this a vanished pass keeps routing a pass edge that then
    /// fails at `raise` every trip.
    pub(super) fn prune_missing_day_passes(&mut self) {
        let name = self.config.go2.day_pass_sack.trim();
        if name.is_empty() {
            return;
        }
        let Some(sack) = self.game_state.objects.find_container(name) else {
            return;
        };
        let present: std::collections::HashSet<&str> = self
            .game_state
            .objects
            .items_in(&sack.id)
            .into_iter()
            .map(|i| i.id.as_str())
            .collect();
        let gone: Vec<String> = self
            .game_state
            .day_passes
            .ids()
            .filter(|id| !present.contains(id.as_str()))
            .cloned()
            .collect();
        for id in gone {
            self.game_state.day_passes.forget(&id);
        }
    }

    /// Resolve a deferred `.go2` waiting on the day-pass sack scan (see
    /// start_travel). Done when every looked-at pass has parsed into the cache
    /// (or, for the contents probe, a pass is now visible in the sack), or the
    /// deadline passes — then the trip is re-planned. Lich's
    /// `mapdb_find_day_pass` sweep, deferred-trip style like tick_urchin_refresh.
    pub(super) fn tick_day_pass_scan(&mut self) {
        if self.pending_day_pass_scan.is_none() {
            return;
        }
        // "That is already open" landed since the scan's `open` — the sack was
        // open before we touched it; don't close it at completion. Peeked (not
        // drained) here: tick_travel drains the queue right after us.
        if self
            .game_state
            .move_feedback
            .iter()
            .any(|(_, f)| matches!(f, crate::core::move_feedback::MoveFeedback::ContainerAlreadyOpen))
        {
            if let Some(open) = self.day_pass_scan_open.as_mut() {
                open.1 = true;
            }
        }
        let Some((destination, deadline, ids)) = self.pending_day_pass_scan.clone() else {
            return;
        };
        let learned = !ids.is_empty()
            && ids.iter().all(|id| self.game_state.day_passes.contains(id));
        // Probe round (no ids): done as soon as the contents line shows ANY
        // pass — unknown ones get their look round on the re-plan; already-
        // known ones proceed straight to planning (don't sit out the
        // deadline). An empty sack still waits the deadline (we can't tell
        // "no passes" from "contents not seen yet").
        let (_, _, probe_any) = self.day_pass_scan_targets();
        let probe_done = ids.is_empty() && probe_any;
        let expired = std::time::Instant::now() >= deadline;
        if learned || probe_done || expired {
            // Clear FIRST so the re-invoked start_travel plans (or queues the
            // next scan round) instead of re-deferring.
            self.pending_day_pass_scan = None;
            self.start_travel(destination);
            // Only when start_travel queued NO further round is the scan
            // truly over — then restore the sack to how we found it (close
            // only if OUR open opened it). Closing between rounds churned
            // the sack: every round closed it and the next reopened it.
            if self.pending_day_pass_scan.is_none() {
                if let Some((sack, was_open)) = self.day_pass_scan_open.take() {
                    if !was_open {
                        self.travel.queue_command(format!("close #{sack}"));
                    }
                }
            }
        }
    }

    pub fn tick_travel(&mut self) {
        if !self.travel.is_traveling() {
            // Not walking: don't let feedback accumulate unboundedly.
            self.game_state.move_feedback.clear();
            // Awaits are the only consumer of raw lines; stop copying every
            // game line into the ring once travel ends, and drop what's left.
            if self.message_processor.capture_recent_lines {
                self.message_processor.capture_recent_lines = false;
                self.game_state.recent_lines.clear();
            }
            return;
        }
        // Travelling: a scripted edge may arm an `Await` at any point, and an
        // await must be able to see lines that landed before it armed, so the
        // capture has to be running for the whole trip rather than switched on
        // when a step needs it.
        self.message_processor.capture_recent_lines = true;
        let Some(db) = self.map.mapdb().cloned() else {
            return;
        };
        // Drain the move-feedback queue for this tick (edge-triggered events,
        // each consumed exactly once — §09).
        let feedback: Vec<(u64, crate::core::move_feedback::MoveFeedback)> =
            self.game_state.move_feedback.drain(..).collect();
        // Raw lines for `Await` steps. Copied (not drained): the ring must
        // outlive this tick so an await arming now still sees earlier lines.
        // A VecDeque isn't contiguous, and `as_slices().0` would silently drop
        // the wrapped half, so flatten it into an owned Vec.
        let recent_lines: Vec<(u64, String)> =
            self.game_state.recent_lines.iter().cloned().collect();
        // Active spell numbers for scripted-edge checkspell branches.
        let active_spells: Vec<u16> = self
            .game_state
            .effects
            .get("ActiveSpells")
            .map(|content| {
                content
                    .effects
                    .iter()
                    .filter_map(|e| e.id.trim().parse::<u16>().ok())
                    .collect()
            })
            .unwrap_or_default();

        // Assemble the hands stow inputs into OWNED locals, so `ctx` doesn't
        // hold a borrow of self.game_state across the &mut self.travel tick.
        // Resolve the configured weaponsack/lootsack names to container
        // command-ids, gather the other tracked containers as last-resort
        // stow targets, and classify each hand's item as a weapon.
        use crate::core::game_objects::Hand;
        // gameobj_data() takes &mut self (lazy load), so resolve it before we
        // hold the immutable objects borrow.
        let gameobj_data = self.gameobj_data();
        let objects = &self.game_state.objects;
        let resolve_bag = |name: &str| -> Option<String> {
            if name.trim().is_empty() {
                return None;
            }
            objects.find_container(name).map(|c| c.command_target())
        };
        // Four Winds trinket: resolve the configured name to a live exist id
        // and the container to put it back in. Done here (not in the
        // executor) so the executor stays a state machine over plain values.
        // The registry already tracks containment, so the crossing never has
        // to scrape `<a exist=...>` links the way the mapdb proc does.
        // Rogue Guild password + platinum flag feed the transpiler's
        // UserVars store, so the guild-door recognizer sees them the same
        // way Lich's proc saw `UserVars.rogue_password` / `$platinum`.
        {
            use crate::core::pathing::transpile::set_mapdb_var;
            let pw = self.config.go2.rogue_password.trim();
            set_mapdb_var("rogue_password", (!pw.is_empty()).then(|| pw.to_string()));
            let plat = self
                .config
                .connection
                .game
                .as_deref()
                .is_some_and(|g| g.contains("plat"));
            set_mapdb_var("platinum", plat.then(|| "true".into()));
        }
        // go2 treats the literal "off" the same as unset (go2.lic:448-449) —
        // its UI's documented way to disable the trinket without clearing it.
        let fwi_setting = self.config.go2.fwi_trinket.trim();
        let fwi = (!fwi_setting.is_empty() && !fwi_setting.eq_ignore_ascii_case("off"))
            .then(|| objects.find_item(fwi_setting))
            .flatten()
            .map(|(item, loc)| {
                use crate::core::game_objects::Location;
                let in_hand = matches!(loc, Location::Hand(_));
                let return_to = match &loc {
                    Location::Container(id) => objects
                        .container(id)
                        .map(|c| c.command_target()),
                    // Worn / at-feet / held: nothing to return it to.
                    _ => None,
                };
                (item.id.clone(), return_to, in_hand)
            });
        // Every item name we can reach, for `Cond::HasItem` (keyed doors).
        // Carried plus container contents — a key in your bag still opens it.
        let carried_names: Vec<String> = objects
            .carried()
            .into_iter()
            .map(|i| i.name.clone())
            .chain(
                objects
                    .containers()
                    .flat_map(|c| c.items.iter().map(|i| i.name.clone())),
            )
            .collect();
        let weaponsack = resolve_bag(&self.config.go2.weaponsack);
        let lootsack = resolve_bag(&self.config.go2.lootsack);
        let day_pass_sack = resolve_bag(&self.config.go2.day_pass_sack);
        let reserved: std::collections::HashSet<&str> = weaponsack
            .as_deref()
            .into_iter()
            .chain(lootsack.as_deref())
            .collect();
        let other_containers: Vec<String> = objects
            .containers()
            .map(|c| c.command_target())
            .filter(|id| !reserved.contains(id.as_str()))
            .collect();
        let is_weapon = |item: Option<&crate::core::game_objects::GameItem>| -> bool {
            match item {
                Some(i) => gameobj_data.is_type(&i.name, &i.noun, "weapon"),
                None => false,
            }
        };
        // Confluence landmark scan reads ground loot + linked room scenery
        // (Lich's `GameObj.loot`): the tranquility point and pit appear as
        // room objects. Collect their nouns/names while we hold `objects`.
        let loot_nouns: Vec<String> = objects
            .ground()
            .iter()
            .chain(objects.room_desc().iter())
            .map(|item| item.name.clone())
            .collect();
        let left_hand = objects.hand(Hand::Left).cloned();
        let right_hand = objects.hand(Hand::Right).cloned();
        let left_is_weapon = is_weapon(left_hand.as_ref());
        let right_is_weapon = is_weapon(right_hand.as_ref());
        let ready_stow = objects.ready_stow().clone();
        let hands = crate::core::travel::executor::StashInputs {
            left_hand: left_hand.as_ref(),
            right_hand: right_hand.as_ref(),
            ready_stow: &ready_stow,
            weaponsack: weaponsack.as_deref(),
            lootsack: lootsack.as_deref(),
            other_containers: &other_containers,
            // Bandolier-bag resolution (Lich's find_bandolier_bag) is a
            // multi-container "swirling mist" look-scan we don't run yet; the
            // retrieval command is ported (rub #bag), but live bag lookup is a
            // follow-up. Ethereal items need no resolution (retrieved by noun).
            left_bandolier: None,
            right_bandolier: None,
            left_is_weapon,
            right_is_weapon,
        };

        // Live compass exits (XMLData.room_exits) for the Confluence explorer.
        let compass_dirs: Vec<String> = self.game_state.compass_dirs.clone();

        let ctx = crate::core::travel::TravelContext {
            db: &db,
            current_room: self.map.current_room_id,
            dead: self.game_state.status.dead(),
            // Lich's Status.muckled?: stunned/webbed indicators plus the
            // Bind (214) and Sleep (501) debuff-board entries (status.rb:46).
            muckled: self.game_state.status.stunned()
                || self.game_state.status.webbed()
                || self.game_state.debuff_active("Bind")
                || self.game_state.debuff_active("Sleep"),
            standing: self.game_state.status.standing(),
            sitting: self.game_state.status.sitting(),
            kneeling: self.game_state.status.kneeling(),
            hidden: self.game_state.status.hidden() || self.game_state.status.invisible(),
            citizenship: self.game_state.character.citizenship.as_deref(),
            profession: self.game_state.character.profession.as_deref(),
            society: self.game_state.character.society.as_deref(),
            active_spells: &active_spells,
            rt_remaining: self.game_state.roundtime_remaining() as f64,
            now_ms: self.travel.now_ms(),
            pathcodes: &self.config.go2.pathcodes,
            hands: Some(hands),
            feedback: &feedback,
            // Raw lines for `Await` steps. A ring, not a drained queue — see
            // GameState::recent_lines.
            recent_lines: &recent_lines,
            line_seq: self.game_state.line_seq,
            game_line_no: self.game_state.game_line_no,
            game_nav_count: self.game_state.nav_count,
            // The fallback is a Lich-only bandaid: gated on the setting AND a
            // Lich connection (a direct connection has no Lich to hand off to).
            // Gate on the connection itself, NOT on WebUI reachability — WebUI
            // is an optional Lich feature, and conflating the two left the
            // fallback permanently dead on GUI/TUI.
            lich_fallback: self.config.go2.lich_fallback && self.lich_connected(),
            funding: Some(crate::core::travel::executor::FundingInputs {
                silver: self.game_state.silver,
                silver_line_no: self.game_state.silver_line_no,
                get_silvers: self.config.go2.get_silvers,
                get_return_trip: self.config.go2.get_return_trip_silvers,
            }),
            at_pinefar_depository: self
                .game_state
                .room_name
                .as_deref()
                .is_some_and(|t| t.contains("Pinefar, Depository")),
            // Confluence explorer's live view of the shifting maze: the
            // current room's compass exits and ground-loot nouns (the
            // tranquility point / pit landmarks live in ground + room_desc).
            compass_dirs: &compass_dirs,
            carried_names: &carried_names,
            loot_nouns: &loot_nouns,
            // Day-pass crossing inputs: the resolved sack container, buy config,
            // and the live pass cache (begin_day_pass computes the per-edge
            // held-pass / buy-permission from the town pair).
            fwi_trinket: fwi.as_ref().map(|(id, return_to, in_hand)| {
                crate::core::travel::executor::TrinketInputs {
                    id,
                    return_to: return_to.as_deref(),
                    in_hand: *in_hand,
                }
            }),
            day_pass: Some(crate::core::travel::executor::DayPassInputs {
                sack_id: day_pass_sack.as_deref(),
                // A too-poor buy this session flipped the setting off in
                // memory (Lich parity); held passes still route.
                buy_day_pass: if self.game_state.day_passes.buy_disabled() {
                    ""
                } else {
                    &self.config.go2.buy_day_pass
                },
                get_silvers: self.config.go2.get_silvers,
                cache: &self.game_state.day_passes,
                now_epoch: chrono::Utc::now().timestamp(),
                hidden: self.game_state.status.hidden() || self.game_state.status.invisible(),
            }),
        };
        let events = self.travel.tick(ctx);
        for event in events {
            match event {
                crate::core::travel::TravelEvent::Status(text) => {
                    self.add_system_message(&format!("[go2] {text}"));
                }
                crate::core::travel::TravelEvent::Arrived {
                    destination,
                    seconds,
                } => {
                    self.add_system_message(&format!(
                        "[go2] arrived at room {destination} - travel time {}",
                        crate::core::travel::format_eta(seconds)
                    ));
                }
                crate::core::travel::TravelEvent::Failed(reason) => {
                    self.add_system_message(&format!("[go2] {reason}"));
                }
                crate::core::travel::TravelEvent::LichFallback { destination } => {
                    // Native travel can't cross this edge; hand off to Lich.
                    // Stop the native task and send `;go2 <dest>` — Lich walks
                    // the rest. (The event only fires on a Lich connection.)
                    self.travel.stop();
                    self.queue_timed_command(
                        std::time::Duration::ZERO,
                        format!(";go2 {destination}"),
                    );
                }
                crate::core::travel::TravelEvent::DisableDayPassBuy => {
                    // Lich parity: a too-poor buy turns the setting off for
                    // the session (config on disk stays untouched).
                    self.game_state.day_passes.disable_buy();
                }
                crate::core::travel::TravelEvent::Send(_) => unreachable!("queued by the service"),
            }
        }
    }

    /// Advance the `.foreach` runner. Called from the same two places as
    /// `tick_travel` (per network line + per frontend frame).
    pub fn tick_foreach(&mut self) {
        if !self.foreach.is_running() {
            return;
        }
        let ctx = crate::core::foreach::ForeachContext {
            rt_remaining: self.game_state.roundtime_remaining() as f64,
            now_ms: self.foreach.now_ms(),
            dead: self.game_state.status.dead(),
        };
        let events = self.foreach.tick(&ctx);
        for event in events {
            match event {
                crate::core::foreach::ForeachEvent::Status(text) => {
                    self.add_system_message(&format!("[foreach] {text}"));
                }
                crate::core::foreach::ForeachEvent::Done { items } => {
                    self.add_system_message(&format!(
                        "[foreach] done - {items} item{} processed.",
                        if items == 1 { "" } else { "s" }
                    ));
                }
                crate::core::foreach::ForeachEvent::Failed(reason) => {
                    self.add_system_message(&format!("[foreach] {reason}"));
                }
                crate::core::foreach::ForeachEvent::Send(_) => {
                    unreachable!("queued by the service")
                }
            }
        }
    }

    /// Drive a user-invoked `.emptyhands`/`.fillhands` StashTask - the same
    /// state machine travel uses for its stow/retrieve phases, assembled
    /// from the live registry each tick and confirmed by hand changes.
    pub fn tick_hand_stash(&mut self) {
        if self.hand_stash.is_none() {
            return;
        }
        use crate::core::game_objects::Hand;
        use crate::core::travel::stash::{StashContext, StashEvent, StashOp};
        let gameobj_data = self.gameobj_data();
        let objects = &self.game_state.objects;
        let resolve_bag = |name: &str| -> Option<String> {
            if name.trim().is_empty() {
                return None;
            }
            objects.find_container(name).map(|c| c.command_target())
        };
        let weaponsack = resolve_bag(&self.config.go2.weaponsack);
        let lootsack = resolve_bag(&self.config.go2.lootsack);
        let reserved: std::collections::HashSet<&str> = weaponsack
            .as_deref()
            .into_iter()
            .chain(lootsack.as_deref())
            .collect();
        let other_containers: Vec<String> = objects
            .containers()
            .map(|c| c.command_target())
            .filter(|id| !reserved.contains(id.as_str()))
            .collect();
        let left_hand = objects.hand(Hand::Left).cloned();
        let right_hand = objects.hand(Hand::Right).cloned();
        let is_weapon = |item: Option<&crate::core::game_objects::GameItem>| -> bool {
            item.is_some_and(|i| gameobj_data.is_type(&i.name, &i.noun, "weapon"))
        };
        let left_is_weapon = is_weapon(left_hand.as_ref());
        let right_is_weapon = is_weapon(right_hand.as_ref());
        let ready_stow = self.game_state.objects.ready_stow().clone();
        let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
        let ctx = StashContext {
            left_hand: left_hand.as_ref(),
            right_hand: right_hand.as_ref(),
            ready_stow: &ready_stow,
            weaponsack: weaponsack.as_deref(),
            lootsack: lootsack.as_deref(),
            other_containers: &other_containers,
            // Bandolier-bag live lookup is a travel follow-up too; ethereal
            // items need no resolution.
            left_bandolier: None,
            right_bandolier: None,
            left_is_weapon,
            right_is_weapon,
            now_ms,
        };
        let Some(task) = self.hand_stash.as_mut() else {
            return;
        };
        let events = task.tick(ctx);
        for event in events {
            match event {
                StashEvent::Send(cmd) => {
                    self.queue_timed_command(std::time::Duration::ZERO, cmd);
                }
                StashEvent::Done => {
                    let mut task = self.hand_stash.take().expect("task present");
                    match task.op() {
                        StashOp::Empty => {
                            let stack = task.take_stack();
                            let n = stack.len();
                            self.hand_stash_stack = stack;
                            self.add_system_message(&format!(
                                "[hands] emptied - {n} item{} stowed (.fillhands to restore).",
                                if n == 1 { "" } else { "s" }
                            ));
                        }
                        StashOp::Fill => {
                            self.hand_stash_stack.clear();
                            self.add_system_message("[hands] refilled.");
                        }
                    }
                    return;
                }
                StashEvent::Failed(reason) => {
                    self.hand_stash = None;
                    self.add_system_message(&format!("[hands] FAILED: {reason}"));
                    return;
                }
            }
        }
    }

    /// Commands automation wants sent to the game; frontends drain this
    /// through the same path as typed commands. Includes macro sleep
    /// segments whose pause has elapsed.
    pub fn take_outbound(&mut self) -> Vec<String> {
        let mut commands = self.travel.take_outbound();
        commands.extend(self.foreach.take_outbound());
        // Inventory continuation-following: timeouts advance and due
        // `_inventory manager ...` requests go out with everything else.
        let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
        commands.extend(self.message_processor.inv_service.tick(now_ms));
        // Verified item moves: confirm against the current hand state,
        // surface outcomes, send whatever the mover queued.
        let hands = crate::core::item_mover::HandsView {
            left: self
                .game_state
                .objects
                .hand(crate::core::game_objects::Hand::Left)
                .map(|i| i.id.clone()),
            right: self
                .game_state
                .objects
                .hand(crate::core::game_objects::Hand::Right)
                .map(|i| i.id.clone()),
        };
        // Announce a freshly completed managed-inventory snapshot once
        // (token-keyed: probe flag updates bump generation, not token).
        if let Some(snap) = self.game_state.managed_inventory.as_ref() {
            if snap.complete && snap.token != self.last_announced_inv_token {
                let (token, count, room) =
                    (snap.token.clone(), snap.items.len(), snap.room.clone());
                self.last_announced_inv_token = token;
                self.add_system_message(&format!(
                    "[invsync] snapshot complete: {count} items (room {room})."
                ));
            }
        }
        let (mover_cmds, outcome) = self.item_mover.tick(&hands, now_ms);
        commands.extend(mover_cmds);
        match outcome {
            Some(crate::core::item_mover::MoveOutcome::Succeeded { desc }) => {
                self.add_system_message(&format!("[drag] {desc} - confirmed."));
            }
            Some(crate::core::item_mover::MoveOutcome::Failed { desc, reason }) => {
                self.add_system_message(&format!("[drag] {desc} FAILED: {reason}"));
            }
            None => {}
        }
        let now = std::time::Instant::now();
        let mut i = 0;
        while i < self.timed_commands.len() {
            if self.timed_commands[i].0 <= now {
                commands.push(self.timed_commands.remove(i).1);
            } else {
                i += 1;
            }
        }
        commands
    }

    /// Queue a command to go out after a pause (macro sleep segments).
    pub fn queue_timed_command(&mut self, delay: std::time::Duration, command: String) {
        self.timed_commands
            .push((std::time::Instant::now() + delay, command));
    }

    /// Plan and begin a trip to a mapdb room id.
    pub fn start_travel(&mut self, destination: u32) {
        // Lease gate: a different automation root (e.g. a running foreach)
        // must be stopped first; a go2-owned chain retargets as always.
        if let Some(owner) = self.automation_blocked_by("go2") {
            self.add_system_message(&format!(
                "[go2] {} is driving - .stop to cancel it first.",
                owner.desc
            ));
            return;
        }
        // Sync the gated-travel routing flags before planning (Lich's
        // $go2_use_seeking / UserVars.mapdb_use_portmasters globals).
        // Seeking only takes effect for a Voln Master, so its toggle is gated
        // on can_seek(); portmasters are open to anyone with the silver.
        crate::core::pathing::transpile::set_use_seeking(
            self.config.go2.use_seeking,
            self.game_state.character.can_seek(),
        );
        crate::core::pathing::transpile::set_use_portmasters(self.config.go2.use_portmasters);
        // Urchin access refresh (Lich's update_urchin_expire): if urchin travel
        // is enabled but the cached access is missing/expired, ask the game
        // (`urchin status`) and defer this trip until the reply parses. Without
        // this, a route would silently skip urchin edges whenever the client
        // hasn't yet seen an `urchin status` line this session.
        let now_epoch = chrono::Utc::now().timestamp();
        if self.config.go2.use_urchins
            && self.pending_urchin_refresh.is_none()
            && !self.game_state.character.urchins_valid(
                now_epoch,
                self.game_state.status.hidden(),
                self.game_state.status.invisible(),
            )
            && !self.game_state.status.hidden()
            && !self.game_state.status.invisible()
        {
            // Stale/unknown access (not merely hidden) — refresh before routing.
            self.add_system_message("[go2] checking urchin access...");
            self.travel.queue_command("urchin status".to_string());
            self.pending_urchin_refresh = Some((
                destination,
                std::time::Instant::now() + std::time::Duration::from_secs(4),
            ));
            return;
        }
        // Day-pass sack scan (Lich's `mapdb_find_day_pass` sweep): learn the
        // passes actually held BEFORE routing, so a held pair routes at 0.8
        // instead of the buy cost (or not at all). If the sack holds passes
        // the cache hasn't seen, `look` at each (the look output feeds the
        // cache via the day-pass feed) and defer planning until they parse.
        // If the sack's contents are still unknown this session, probe once
        // with open + look in — the container stream keeps them fresh after.
        if self.config.go2.use_day_pass
            && self.pending_day_pass_scan.is_none()
            && !self.day_pass_scan_blocked_by_bounty()
        {
            self.prune_missing_day_passes();
            let (target, unknown, any_pass) = self.day_pass_scan_targets();
            if let Some(target) = target {
                if !unknown.is_empty() {
                    self.add_system_message("[go2] checking your Chronomage day passes...");
                    // One `open` for the WHOLE scan (all rounds); the close
                    // is queued once by tick_day_pass_scan at the true end.
                    if self.day_pass_scan_open.is_none() {
                        self.travel.queue_command(format!("open #{target}"));
                        self.day_pass_scan_open = Some((target.clone(), false));
                    }
                    // Pace the looks: a burst trips the game's type-ahead
                    // limit ("Sorry, you may only type ahead 1 command") and
                    // the dropped look forces a whole re-scan round.
                    for (i, id) in unknown.iter().enumerate() {
                        self.queue_timed_command(
                            std::time::Duration::from_millis(700 * (i as u64 + 1)),
                            format!("look #{id}"),
                        );
                    }
                    self.pending_day_pass_scan = Some((
                        destination,
                        std::time::Instant::now()
                            + std::time::Duration::from_secs(5)
                            + std::time::Duration::from_millis(700 * unknown.len() as u64),
                        unknown,
                    ));
                    return;
                }
                if !any_pass && !self.day_pass_sack_probed {
                    // Contents never seen this session: open + look in, then
                    // re-plan (a discovered pass triggers the look round).
                    self.day_pass_sack_probed = true;
                    self.add_system_message("[go2] checking the day-pass sack...");
                    if self.day_pass_scan_open.is_none() {
                        self.travel.queue_command(format!("open #{target}"));
                        self.day_pass_scan_open = Some((target.clone(), false));
                    }
                    self.travel.queue_command(format!("look in #{target}"));
                    self.pending_day_pass_scan = Some((
                        destination,
                        std::time::Instant::now() + std::time::Duration::from_secs(4),
                        Vec::new(),
                    ));
                    return;
                }
            }
        }
        // Urchins: valid only when enabled AND access hasn't expired AND not
        // hidden/invisible (Lich's combined urchin timeto gate). Also lets
        // dijkstra route through the urchin-hideout hubs this trip.
        crate::core::pathing::transpile::set_urchins_valid(
            self.config.go2.use_urchins
                && self.game_state.character.urchins_valid(
                    now_epoch,
                    self.game_state.status.hidden(),
                    self.game_state.status.invisible(),
                ),
        );
        // Day-pass: for each of the three town pairs, decide the routable cost.
        // A held valid pass → 0.8 (use it); no pass but the buy config permits
        // the pair (with Get Silvers to cover a shortfall) → 7.4 (buy it); else
        // not routable. Off entirely unless use_day_pass is set.
        {
            use crate::core::day_pass;
            let mut routable: Vec<((String, String), f64)> = Vec::new();
            if self.config.go2.use_day_pass {
                // Each departure's destinations carry the pair + per-source buy
                // cost. A held valid pass → 0.8; else buyable (config + Get
                // Silvers) → that edge's buy cost; else not routable.
                for dep in day_pass::DEPARTURES {
                    for dest in dep.destinations {
                        let (a, b) = dest.pair;
                        // Buy routing needs only the config (Lich parity):
                        // silver on hand can cover the purchase; Get Silvers
                        // gates just the in-conversation bank detour.
                        let cost = if self.game_state.day_passes.has_valid_pass(a, b, now_epoch) {
                            Some(0.8)
                        } else if day_pass::buy_permits(&self.config.go2.buy_day_pass, a, b) {
                            Some(dest.buy_cost)
                        } else {
                            None
                        };
                        if let Some(cost) = cost {
                            routable.push(((a.to_string(), b.to_string()), cost));
                        }
                    }
                }
            }
            crate::core::pathing::transpile::set_day_pass_routable(&routable);
        }
        let Some(db) = self.map.mapdb().cloned() else {
            self.add_system_message(
                "[go2] map database not loaded - configure it in Settings > Map",
            );
            return;
        };
        let Some(current) = self.map.current_room_id else {
            self.add_system_message(
                "[go2] your current room hasn't resolved against the mapdb yet (see .room)",
            );
            return;
        };
        if current == destination {
            self.add_system_message("[go2] you're already here...");
            return;
        }
        if db.room(destination).is_none() {
            self.add_system_message(&format!("[go2] room {destination} is not in the mapdb"));
            return;
        }
        match crate::core::travel::TravelTask::start(
            &db,
            current,
            destination,
            self.travel.now_ms(),
        ) {
            Ok(task) => {
                let eta = task.eta_seconds(&db, current);
                let title = db
                    .room(destination)
                    .and_then(|r| r.title.first().cloned())
                    .unwrap_or_default();
                self.add_system_message(&format!(
                    "[go2] -> {title} ({destination}): {} rooms, ETA {}",
                    task.rooms_total(),
                    crate::core::travel::format_eta(eta)
                ));
                self.travel.last_start_room = Some(current);
                self.travel.set_task(task);
                // Fire the first move now instead of on the next frame.
                self.tick_travel();
            }
            Err(reason) => {
                self.add_system_message(&format!("[go2] {reason}"));
            }
        }
    }

    /// Cancel the active trip (`.go2 stop`, Esc).
    pub fn stop_travel(&mut self) {
        if self.travel.stop() {
            self.add_system_message("[go2] travel stopped.");
        } else {
            self.add_system_message("[go2] not traveling.");
        }
    }

    /// Check the given GitHub repo for a mapdb release and download it if
    /// it's new. Progress lands in `map_updater.status`.
    pub fn start_mapdb_download(&mut self, repo: &str) {
        let repo = repo.trim();
        if repo.is_empty() {
            return;
        }
        self.map_updater.start(repo.to_owned());
    }

    /// Delete all downloaded mapdb versions and fall back to the Lich folder.
    pub fn remove_downloaded_mapdb(&mut self) {
        self.map_updater.remove_downloaded();
        self.refresh_map_source();
    }

    /// Push the latest stream-reported room identifiers into the map service.
    /// `nav_room_id` carries the game uid; `lich_room_id` the Lich room id.
    /// Title and obvious exits ride along — unmapped rooms are sketched as
    /// ghosts from exactly this data.
    pub(super) fn sync_map_room(&mut self) {
        let uid = self
            .nav_room_id
            .as_deref()
            .and_then(|s| s.trim().parse::<i64>().ok());
        let lich_id = self
            .lich_room_id
            .as_deref()
            .and_then(|s| s.trim().parse::<u32>().ok());
        // Plain-text "room desc" for the uid-less content fallback; lines
        // are joined with a space to mirror the single-string mapdb form.
        let description = self
            .room_components
            .get("room desc")
            .map(|lines| {
                lines
                    .iter()
                    .map(|segments| {
                        segments
                            .iter()
                            .map(|seg| seg.text.as_str())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let snapshot = crate::core::ghost_rooms::RoomSnapshot {
            title: self.game_state.room_name.clone(),
            exits: self.game_state.exits.clone(),
            description,
        };
        self.map.note_room(uid, lich_id, snapshot);
    }

}
