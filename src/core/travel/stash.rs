//! The hands stow/retrieve service — a tick-based port of Lich's
//! `Lich::Stash` (`lib/stash.rb`, `empty_hands`/`fill_hands` in
//! `global_defs.rb:1902-1950`).
//!
//! Lich's `stash_hands`/`equip_hands` are blocking loops that send a command
//! and busy-wait on `GameObj` until the item leaves/returns the hand.
//! VellumFE has no blocking primitive, so this is the same logic as an
//! event-driven state machine: [`StashTask::tick`] is called with a snapshot
//! of the world (hands + READY/STOW state + fallback containers) and answers
//! with commands to send, confirming each step by the hand state *changing*
//! on a later tick — the same shape as the travel executor's `AwaitArrival`.
//!
//! Stow destination follows Lich's exact per-hand cascade
//! (`stash.rb:171-257`):
//!   worn shield/bow  → `wear #id`   (retrieve: `remove #id`)
//!   1. ready store-rule → sheath / second sheath / default stow
//!   2. weapon + second_sheath set → second_sheath
//!   3. weapon + weaponsack set     → weaponsack
//!   4. lootsack set                → lootsack
//!   5. any inventory container     → first that accepts it
//!
//! Retrieval replays a LIFO stack of remembered stows (Lich's
//! `$fill_hands_actions`), so `fill_hands` puts back exactly what `empty_hands`
//! took, in reverse order.

use crate::core::game_objects::{GameItem, Hand, ReadyStow};

/// Worn items that go to inventory with `wear`, not into a container — the
/// noun set from `stash.rb:173`.
fn is_worn_gear(noun: &str) -> bool {
    matches!(
        noun,
        "shield"
            | "buckler"
            | "targe"
            | "heater"
            | "parma"
            | "aegis"
            | "scutum"
            | "greatshield"
            | "mantlet"
            | "pavis"
            | "arbalest"
            | "bow"
            | "crossbow"
            | "yumi"
    )
}

/// What the stash task sees each tick.
#[derive(Clone, Copy)]
pub struct StashContext<'a> {
    pub left_hand: Option<&'a GameItem>,
    pub right_hand: Option<&'a GameItem>,
    pub ready_stow: &'a ReadyStow,
    /// Fallback containers (Lich `UserVars.weaponsack`/`lootsack`), by the id
    /// to use in game commands. `None` when unset in config.
    pub weaponsack: Option<&'a str>,
    pub lootsack: Option<&'a str>,
    /// Any other inventory container ids, tried last (Lich's
    /// `inventory containers` scan). Ordered; first that works wins.
    pub other_containers: &'a [String],
    /// Bandolier bag id for the currently-held weapon, if the caller resolved
    /// one (Lich's `find_bandolier_bag`). A bandolier weapon retrieves with
    /// `rub #bag` and re-forms in hand, so we don't drag it to a container.
    /// Indexed by hand: (left, right).
    pub left_bandolier: Option<&'a str>,
    pub right_bandolier: Option<&'a str>,
    /// True when an item's classified type contains "weapon" — the caller
    /// resolves this via gameobj-data (we don't carry type on GameItem).
    /// Indexed by hand: (left_is_weapon, right_is_weapon).
    pub left_is_weapon: bool,
    pub right_is_weapon: bool,
    pub now_ms: u64,
}

/// A command to send, produced by a tick.
#[derive(Debug, Clone, PartialEq)]
pub enum StashEvent {
    Send(String),
    /// The requested empty/fill finished (hands in the desired state).
    Done,
    /// Gave up (a stow/retrieve never confirmed). Carries a reason.
    Failed(String),
}

/// How a stowed item is retrieved, remembered so `fill_hands` can replay it.
#[derive(Clone, Debug, PartialEq)]
pub enum Retrieve {
    /// Normal container item: `get #id`.
    Get(String),
    /// Worn gear put to inventory: `remove #id`.
    Remove(String),
    /// Ethereal item (a tattoo): `rub <noun> tattoo` (stash.rb:184-186).
    /// The item dissolves on stow and re-forms on rub, so it's identified by
    /// noun, not id.
    RubTattoo(String),
    /// Bandolier weapon: `rub #<bandolier-bag-id>` (stash.rb:187-189). The
    /// bag id is resolved by find_bandolier_bag at empty time.
    RubBandolier { bag_id: String },
}

/// True for an ethereal tattoo item (Lich's `/^ethereal \w+$/` on the name).
fn is_ethereal(name: &str) -> bool {
    let rest = match name.strip_prefix("ethereal ") {
        Some(r) => r,
        None => return false,
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphabetic())
}

/// One remembered stow: which hand it came from and how to get it back.
/// Opaque payload the executor carries from an Empty task to its later Fill.
#[derive(Clone, Debug, PartialEq)]
pub struct Stowed {
    hand: Hand,
    item: GameItem,
    retrieve: Retrieve,
}

/// Direction of the current operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StashOp {
    Empty,
    Fill,
}

/// Confirmation budget: how long a single stow/get may take to reflect in the
/// hand before we give up on it. Lich busy-waits 20×0.1s (~2s).
const CONFIRM_TIMEOUT_MS: u64 = 3_000;

#[derive(Debug)]
enum Phase {
    /// Deciding the next hand to stow / item to retrieve.
    Idle,
    /// A command was sent; waiting for the hand to change.
    Await {
        /// The hand we expect to become empty (Empty) or filled (Fill).
        hand: Hand,
        /// The item id we expect to leave (Empty) or return (Fill).
        item_id: String,
        sent_ms: u64,
        /// (Empty only) we already fell back to a bare `stow` after the primary
        /// container command timed out — the next timeout is a real failure.
        fallback_tried: bool,
    },
}

/// The running empty/fill operation.
#[derive(Debug)]
pub struct StashTask {
    op: StashOp,
    phase: Phase,
    /// LIFO stack of what `Empty` stowed, replayed by `Fill`.
    stack: Vec<Stowed>,
    /// For Fill: items still to retrieve (popped from the stack).
    to_retrieve: Vec<Stowed>,
}

impl StashTask {
    /// Begin emptying both hands.
    pub fn empty() -> Self {
        StashTask {
            op: StashOp::Empty,
            phase: Phase::Idle,
            stack: Vec::new(),
            to_retrieve: Vec::new(),
        }
    }

    /// Begin refilling from a prior empty's stack (LIFO). `stack` is the value
    /// returned by [`StashTask::take_stack`] on the matching empty task.
    /// `to_retrieve` is popped from the end, so the last item stowed is the
    /// first retrieved — no pre-reversal needed.
    pub fn fill(stack: Vec<Stowed>) -> Self {
        let to_retrieve = stack.clone();
        StashTask {
            op: StashOp::Fill,
            phase: Phase::Idle,
            stack,
            to_retrieve,
        }
    }

    /// The LIFO stow stack (moved out for a later Fill). Empty after this.
    pub fn take_stack(&mut self) -> Vec<Stowed> {
        std::mem::take(&mut self.stack)
    }

    pub fn op(&self) -> StashOp {
        self.op
    }

    /// Advance the machine. Returns commands to send / completion.
    pub fn tick(&mut self, ctx: StashContext) -> Vec<StashEvent> {
        let mut events = Vec::new();
        match self.op {
            StashOp::Empty => self.tick_empty(ctx, &mut events),
            StashOp::Fill => self.tick_fill(ctx, &mut events),
        }
        events
    }

    fn tick_empty(&mut self, ctx: StashContext, events: &mut Vec<StashEvent>) {
        // Confirm an in-flight stow first.
        if let Phase::Await {
            hand,
            item_id,
            sent_ms,
            fallback_tried,
        } = &self.phase
        {
            let hand = *hand;
            let item_id = item_id.clone();
            let fallback_tried = *fallback_tried;
            let still_held = ctx.hand(hand).map(|i| i.id == item_id).unwrap_or(false);
            if !still_held {
                // Hand cleared — the stow landed. Move on.
                self.phase = Phase::Idle;
            } else if ctx.now_ms.saturating_sub(*sent_ms) > CONFIRM_TIMEOUT_MS {
                if fallback_tried {
                    // The bare-`stow` fallback also failed — give up on this
                    // hand (the item stays held; the caller decides what next).
                    events.push(StashEvent::Failed(format!(
                        "couldn't stow the item in your {} hand",
                        hand_name(hand)
                    )));
                    return;
                }
                // The primary container command failed (e.g. a bad `_direct_`
                // bag id, or a container that isn't reachable). Fall back to a
                // bare `stow`, which uses the game's own default container.
                events.push(StashEvent::Send(format!("stow #{item_id}")));
                self.phase = Phase::Await {
                    hand,
                    item_id,
                    sent_ms: ctx.now_ms,
                    fallback_tried: true,
                };
                return;
            } else {
                return; // still waiting
            }
        }

        // Pick the next non-empty hand and stow it.
        for hand in [Hand::Left, Hand::Right] {
            if let Some(item) = ctx.hand(hand) {
                let is_weapon = ctx.is_weapon(hand);
                let (command, retrieve) = plan_stow(item, is_weapon, ctx.bandolier(hand), ctx);
                self.stack.push(Stowed {
                    hand,
                    item: item.clone(),
                    retrieve,
                });
                events.push(StashEvent::Send(command));
                self.phase = Phase::Await {
                    hand,
                    item_id: item.id.clone(),
                    sent_ms: ctx.now_ms,
                    fallback_tried: false,
                };
                return;
            }
        }
        // Both hands empty — done.
        events.push(StashEvent::Done);
    }

    fn tick_fill(&mut self, ctx: StashContext, events: &mut Vec<StashEvent>) {
        if let Phase::Await {
            hand,
            item_id,
            sent_ms,
            ..
        } = &self.phase
        {
            let hand = *hand;
            // Retrieved if the item is now in EITHER hand (it may land in the
            // wrong one; a swap fixes that, but for "filled" either counts).
            let back = ctx
                .left_hand
                .map(|i| &i.id == item_id)
                .unwrap_or(false)
                || ctx.right_hand.map(|i| &i.id == item_id).unwrap_or(false);
            if back {
                // If it landed in the wrong hand, swap it back (Lich's
                // `dothistimeout 'swap'`).
                let wrong_hand = ctx.hand(hand).map(|i| &i.id != item_id).unwrap_or(true);
                if wrong_hand
                    && ctx
                        .hand(other(hand))
                        .map(|i| &i.id == item_id)
                        .unwrap_or(false)
                {
                    events.push(StashEvent::Send("swap".to_string()));
                }
                self.phase = Phase::Idle;
            } else if ctx.now_ms.saturating_sub(*sent_ms) > CONFIRM_TIMEOUT_MS {
                events.push(StashEvent::Failed(
                    "couldn't retrieve a stowed item".to_string(),
                ));
                return;
            } else {
                return;
            }
        }

        // Retrieve the next item (LIFO).
        if let Some(stowed) = self.to_retrieve.pop() {
            let command = match &stowed.retrieve {
                Retrieve::Get(id) => format!("get #{id}"),
                Retrieve::Remove(id) => format!("remove #{id}"),
                Retrieve::RubTattoo(noun) => format!("rub {noun} tattoo"),
                Retrieve::RubBandolier { bag_id } => format!("rub #{bag_id}"),
            };
            events.push(StashEvent::Send(command));
            self.phase = Phase::Await {
                hand: stowed.hand,
                item_id: stowed.item.id.clone(),
                sent_ms: ctx.now_ms,
                fallback_tried: false,
            };
            return;
        }
        events.push(StashEvent::Done);
    }
}

/// The Lich stow cascade for one item. Returns the command to send and how to
/// retrieve it later. `bandolier` is the resolved bandolier-bag id for this
/// item, if any.
fn plan_stow(
    item: &GameItem,
    is_weapon: bool,
    bandolier: Option<&str>,
    ctx: StashContext,
) -> (String, Retrieve) {
    // Ethereal tattoo item: it dissolves when stowed and re-forms on
    // `rub <noun> tattoo`. Stow it normally (into the cascade destination);
    // only the retrieval differs. Checked before worn-gear because an
    // "ethereal shield" is a tattoo, not a worn shield.
    let ethereal = is_ethereal(&item.name);
    // Worn shields/bows (non-ethereal): wear to inventory, retrieve `remove`.
    if !ethereal && is_worn_gear(&item.noun) {
        return (format!("wear #{}", item.id), Retrieve::Remove(item.id.clone()));
    }
    // Bandolier weapon: stow normally, but retrieve with `rub #bag`.
    let retrieve_override = if ethereal {
        Some(Retrieve::RubTattoo(item.noun.clone()))
    } else {
        bandolier.map(|bag| Retrieve::RubBandolier {
            bag_id: bag.trim_start_matches('#').to_string(),
        })
    };

    let rs = ctx.ready_stow;
    // The stow command follows the cascade; the retrieve is the ethereal/
    // bandolier override if any, else a plain `get`.
    let stow_command = stow_destination(item, is_weapon, ctx, rs);
    let retrieve = retrieve_override.unwrap_or_else(|| Retrieve::Get(item.id.clone()));
    (stow_command, retrieve)
}

/// Pick the stow command per Lich's per-hand cascade (stash.rb:171-257).
fn stow_destination(
    item: &GameItem,
    is_weapon: bool,
    ctx: StashContext,
    rs: &ReadyStow,
) -> String {
    // 1. This item has a ready store-rule → sheath / 2nd sheath / default.
    if let Some(mode) = rs.store_mode_for(item) {
        let bag = match mode {
            "put in sheath" => rs.sheath(),
            "put in secondary sheath" => rs.second_sheath(),
            // "stowed" / "worn if possible, stowed otherwise"
            _ => rs.default_stow(),
        };
        if let Some(bag) = bag {
            return drag_cmd(item, &bag.id);
        }
    }
    // 2. Weapon + a secondary sheath is set.
    if is_weapon {
        if let Some(sheath) = rs.second_sheath() {
            return drag_cmd(item, &sheath.id);
        }
        // 3. Weapon + a configured weaponsack.
        if let Some(sack) = ctx.weaponsack {
            return drag_cmd(item, sack);
        }
    }
    // 4. A configured lootsack.
    if let Some(sack) = ctx.lootsack {
        return drag_cmd(item, sack);
    }
    // 5. Any other inventory container.
    if let Some(bag) = ctx.other_containers.first() {
        return drag_cmd(item, bag);
    }
    // Nothing configured: fall back to a bare `stow`, which uses the game's
    // own default stow container.
    format!("stow #{}", item.id)
}

/// `_drag #item #bag` — Lich's `add_to_bag` command. When the bag id isn't a
/// usable game id (empty, or the parser's `_direct_` marker for a container
/// reached by a direct command rather than an object id), a `_drag #… #_direct_`
/// just errors — so fall back to a bare `stow`, which uses the game's own
/// default stow container.
fn drag_cmd(item: &GameItem, bag_id: &str) -> String {
    let bag = bag_id.trim_start_matches('#').trim();
    if bag.is_empty() || bag == "_direct_" {
        return format!("stow #{}", item.id);
    }
    format!("_drag #{} #{}", item.id, bag)
}

fn other(hand: Hand) -> Hand {
    match hand {
        Hand::Left => Hand::Right,
        Hand::Right => Hand::Left,
    }
}

fn hand_name(hand: Hand) -> &'static str {
    match hand {
        Hand::Left => "left",
        Hand::Right => "right",
    }
}

impl StashContext<'_> {
    fn hand(&self, hand: Hand) -> Option<&GameItem> {
        match hand {
            Hand::Left => self.left_hand,
            Hand::Right => self.right_hand,
        }
    }
    fn is_weapon(&self, hand: Hand) -> bool {
        match hand {
            Hand::Left => self.left_is_weapon,
            Hand::Right => self.right_is_weapon,
        }
    }
    fn bandolier(&self, hand: Hand) -> Option<&str> {
        match hand {
            Hand::Left => self.left_bandolier,
            Hand::Right => self.right_bandolier,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::game_objects::GameObjects;

    fn ctx<'a>(
        objs: &'a GameObjects,
        weaponsack: Option<&'a str>,
        lootsack: Option<&'a str>,
        others: &'a [String],
        now_ms: u64,
    ) -> StashContext<'a> {
        StashContext {
            left_hand: objs.hand(Hand::Left),
            right_hand: objs.hand(Hand::Right),
            ready_stow: objs.ready_stow(),
            weaponsack,
            lootsack,
            other_containers: others,
            left_bandolier: None,
            right_bandolier: None,
            left_is_weapon: false,
            right_is_weapon: false,
            now_ms,
        }
    }

    #[test]
    fn empty_hands_stows_both_then_fill_retrieves_in_reverse() {
        // A player holding a torch (left) and a rock (right), lootsack set.
        let mut objs = GameObjects::default();
        objs.set_hand(Hand::Left, Some(GameItem::new("10", "torch", "a torch")));
        objs.set_hand(Hand::Right, Some(GameItem::new("20", "rock", "a rock")));
        let others: Vec<String> = vec![];

        let mut empty = StashTask::empty();
        // Tick 1: stow left (torch) into lootsack.
        let e = empty.tick(ctx(&objs, None, Some("99"), &others, 0));
        assert_eq!(e, vec![StashEvent::Send("_drag #10 #99".into())]);
        // Confirm: left hand clears.
        objs.set_hand(Hand::Left, None);
        // Tick 2: stow right (rock).
        let e = empty.tick(ctx(&objs, None, Some("99"), &others, 100));
        assert_eq!(e, vec![StashEvent::Send("_drag #20 #99".into())]);
        objs.set_hand(Hand::Right, None);
        // Tick 3: both empty → Done.
        let e = empty.tick(ctx(&objs, None, Some("99"), &others, 200));
        assert_eq!(e, vec![StashEvent::Done]);

        // Fill replays LIFO: rock first, then torch.
        let stack = empty.take_stack();
        let mut fill = StashTask::fill(stack);
        let e = fill.tick(ctx(&objs, None, Some("99"), &others, 300));
        assert_eq!(e, vec![StashEvent::Send("get #20".into())]);
        objs.set_hand(Hand::Right, Some(GameItem::new("20", "rock", "a rock")));
        let e = fill.tick(ctx(&objs, None, Some("99"), &others, 400));
        assert_eq!(e, vec![StashEvent::Send("get #10".into())]);
        objs.set_hand(Hand::Left, Some(GameItem::new("10", "torch", "a torch")));
        let e = fill.tick(ctx(&objs, None, Some("99"), &others, 500));
        assert_eq!(e, vec![StashEvent::Done]);
    }

    #[test]
    fn worn_shield_uses_wear_and_remove() {
        let mut objs = GameObjects::default();
        objs.set_hand(Hand::Left, Some(GameItem::new("5", "shield", "a targe")));
        let others: Vec<String> = vec![];
        let mut empty = StashTask::empty();
        let e = empty.tick(ctx(&objs, None, Some("99"), &others, 0));
        assert_eq!(e, vec![StashEvent::Send("wear #5".into())]);
        objs.set_hand(Hand::Left, None);
        empty.tick(ctx(&objs, None, Some("99"), &others, 100));
        // Retrieval uses remove, not get.
        let stack = empty.take_stack();
        let mut fill = StashTask::fill(stack);
        let e = fill.tick(ctx(&objs, None, Some("99"), &others, 200));
        assert_eq!(e, vec![StashEvent::Send("remove #5".into())]);
    }

    #[test]
    fn no_containers_falls_back_to_bare_stow() {
        let mut objs = GameObjects::default();
        objs.set_hand(Hand::Right, Some(GameItem::new("7", "gem", "a gem")));
        let others: Vec<String> = vec![];
        let mut empty = StashTask::empty();
        let e = empty.tick(ctx(&objs, None, None, &others, 0));
        assert_eq!(e, vec![StashEvent::Send("stow #7".into())]);
    }

    #[test]
    fn ethereal_item_retrieves_with_rub_tattoo() {
        let mut objs = GameObjects::default();
        // Ethereal items are named "ethereal <noun>".
        objs.set_hand(
            Hand::Left,
            Some(GameItem::new("3", "shield", "ethereal shield")),
        );
        let others: Vec<String> = vec![];
        let mut empty = StashTask::empty();
        // Stow is still a normal cascade command (lootsack here)...
        let e = empty.tick(ctx(&objs, None, Some("99"), &others, 0));
        assert_eq!(e, vec![StashEvent::Send("_drag #3 #99".into())]);
        objs.set_hand(Hand::Left, None);
        empty.tick(ctx(&objs, None, Some("99"), &others, 100));
        // ...but retrieval rubs the tattoo by noun, not `get`.
        let stack = empty.take_stack();
        let mut fill = StashTask::fill(stack);
        let e = fill.tick(ctx(&objs, None, Some("99"), &others, 200));
        assert_eq!(e, vec![StashEvent::Send("rub shield tattoo".into())]);
    }

    #[test]
    fn bandolier_weapon_retrieves_with_rub_bag() {
        let mut objs = GameObjects::default();
        objs.set_hand(Hand::Right, Some(GameItem::new("7", "sword", "a war sword")));
        let others: Vec<String> = vec![];
        // Caller resolved a bandolier bag (#500) for the right hand.
        let mut c = ctx(&objs, None, Some("99"), &others, 0);
        c.right_bandolier = Some("500");
        let mut empty = StashTask::empty();
        let e = empty.tick(c);
        assert_eq!(e, vec![StashEvent::Send("_drag #7 #99".into())]);
        objs.set_hand(Hand::Right, None);
        let mut c = ctx(&objs, None, Some("99"), &others, 100);
        c.right_bandolier = Some("500");
        empty.tick(c);
        // Retrieval rubs the bandolier bag.
        let stack = empty.take_stack();
        let mut fill = StashTask::fill(stack);
        let e = fill.tick(ctx(&objs, None, Some("99"), &others, 200));
        assert_eq!(e, vec![StashEvent::Send("rub #500".into())]);
    }

    #[test]
    fn stow_timeout_falls_back_to_bare_stow_then_fails() {
        let mut objs = GameObjects::default();
        objs.set_hand(Hand::Left, Some(GameItem::new("1", "torch", "a torch")));
        let others: Vec<String> = vec![];
        let mut empty = StashTask::empty();
        // Tick 1: the primary stow (into the lootsack).
        let e = empty.tick(ctx(&objs, None, Some("99"), &others, 0));
        assert_eq!(e, vec![StashEvent::Send("_drag #1 #99".into())]);
        // Hand never clears; past the timeout → fall back to a bare `stow`
        // rather than giving up (the item might still land in the default bag).
        let e = empty.tick(ctx(&objs, None, Some("99"), &others, CONFIRM_TIMEOUT_MS + 1));
        assert_eq!(e, vec![StashEvent::Send("stow #1".into())], "falls back to bare stow");
        // The fallback ALSO times out → now it's a real failure.
        let e = empty.tick(ctx(&objs, None, Some("99"), &others, 2 * CONFIRM_TIMEOUT_MS + 2));
        assert!(matches!(e.as_slice(), [StashEvent::Failed(_)]), "gives up after the fallback: {e:?}");
    }

    #[test]
    fn bad_direct_bag_id_uses_bare_stow() {
        use crate::core::game_objects::{Container, GameObjects};
        // A container whose command target is the parser's `_direct_` marker
        // (a container reached by a direct command, not a real object id). A
        // `_drag #item #_direct_` errors, so the plan must use a bare `stow`.
        let mut objs = GameObjects::default();
        objs.set_hand(Hand::Left, Some(GameItem::new("1", "torch", "a torch")));
        // A registered container whose target is `_direct_`.
        objs.register_container(
            "direct".to_string(),
            "a dwarf skin backpack".to_string(),
            Some("_direct_".to_string()),
        );
        let target = objs
            .find_container("dwarf skin backpack")
            .map(Container::command_target);
        // command_target must NOT hand back `_direct_`.
        assert_ne!(target.as_deref(), Some("_direct_"));

        // And a stow planned into a `_direct_` bag id becomes a bare stow.
        let others: Vec<String> = vec![];
        let mut empty = StashTask::empty();
        // Force the lootsack to the bad id: the plan should reject it.
        let e = empty.tick(ctx(&objs, None, Some("_direct_"), &others, 0));
        assert_eq!(
            e,
            vec![StashEvent::Send("stow #1".into())],
            "a `_direct_` bag id falls back to a bare stow, not `_drag #1 #_direct_`: {e:?}"
        );
    }
}
