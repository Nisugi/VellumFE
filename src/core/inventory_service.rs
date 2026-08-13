//! Continuation-following for the extended feed's `<inventoryManager>`
//! (Saga protocol). Big containers arrive paginated: the initial response
//! carries `<continuation root last>` cursors, each answered by
//! `_inventory manager <token> continue <room> <root> <last>` with a fresh
//! request token. This service owns the request tokens, keeps at most
//! [`MAX_IN_FLIGHT`] continuation requests outstanding, merges chunks into
//! one snapshot, and publishes it only when every cursor has been drained.
//!
//! Failure discipline (mirrors Saga's client):
//! - `state="stale"` = the server invalidated a cursor; the whole load is
//!   torn down and re-requested from scratch (bounded restarts).
//! - Any other non-empty `state` (including the parser-synthesized
//!   `malformed` for prompt-torn captures) fails the load the same way.
//! - A continuation response that never arrives times out, failing the load.
//! - Repeated cursors and duplicate item ids are dropped, not re-requested.
//!
//! The service never sends anything itself: `tick()` returns the commands
//! that are due and the caller (AppCore's tick) queues them to the game —
//! every send stays observable and testable, per the command-gate lesson
//! from the travel engine.

use crate::core::state::{ManagedInventoryItem, ManagedInventoryState};
use std::collections::{HashMap, HashSet, VecDeque};

/// Saga's continuation fan-out cap.
const MAX_IN_FLIGHT: usize = 4;
/// How long a single request may stay unanswered before the load fails.
const REQUEST_TIMEOUT_MS: u64 = 10_000;
/// How many times a stale/failed load may restart before giving up until
/// the next explicit refresh.
const MAX_RESTARTS: u32 = 2;

/// One outstanding request (initial or continuation).
#[derive(Debug, Clone)]
struct InFlight {
    /// Cursor this request asked for; None = the initial load request.
    cursor: Option<(String, String)>,
    deadline_ms: u64,
}

/// An in-progress paginated load.
#[derive(Debug, Default)]
struct ActiveLoad {
    room: String,
    items: Vec<ManagedInventoryItem>,
    item_ids: HashSet<String>,
    /// Cursors waiting for a free in-flight slot.
    queued: VecDeque<(String, String)>,
    /// token -> outstanding request.
    in_flight: HashMap<String, InFlight>,
    /// Every cursor ever seen this load (repeat suppression).
    seen_cursors: HashSet<(String, String)>,
}

/// What `on_response` decided; the caller applies state changes.
#[derive(Debug, PartialEq)]
pub enum ResponseOutcome {
    /// Snapshot finished (single-response or final continuation) — publish.
    Publish(ManagedInventoryState),
    /// Chunk absorbed; more continuations outstanding.
    Absorbed,
    /// Not one of our tokens (e.g. a manual `_inventory manager imtest1`).
    /// Complete foreign responses are still published for parity with the
    /// pre-service behavior; paginated foreign responses publish incomplete.
    Foreign,
    /// Load failed (stale cursor / error state); a restart may have been
    /// scheduled (visible via `tick()`).
    Failed,
}

#[derive(Debug, Default)]
pub struct InventoryService {
    active: Option<ActiveLoad>,
    /// Commands ready to go out, drained by `tick()`.
    outbox: Vec<String>,
    /// Monotonic token counter (uniqueness within the session).
    counter: u64,
    /// Restarts consumed by the current refresh attempt.
    restarts: u32,
    generation: u64,
}

impl InventoryService {
    pub fn new() -> Self {
        Self::default()
    }

    /// True while a load is being assembled.
    pub fn loading(&self) -> bool {
        self.active.is_some()
    }

    /// Begin (or restart) a full inventory load. No-op while one is already
    /// running — the extended feed keys responses by token, so overlapping
    /// loads would only race each other.
    pub fn request_refresh(&mut self, now_ms: u64) {
        if self.active.is_some() {
            return;
        }
        self.restarts = 0;
        self.start_load(now_ms);
    }

    fn start_load(&mut self, now_ms: u64) {
        let token = self.next_token(now_ms);
        let mut load = ActiveLoad::default();
        load.in_flight.insert(
            token.clone(),
            InFlight {
                cursor: None,
                deadline_ms: now_ms + REQUEST_TIMEOUT_MS,
            },
        );
        self.active = Some(load);
        self.outbox.push(format!("_inventory manager {token}"));
    }

    /// Saga-style token: `im` + base36 time + base36 counter, ≤ 24 chars.
    fn next_token(&mut self, now_ms: u64) -> String {
        self.counter += 1;
        let mut t = format!("im{}{}", base36(now_ms), base36(self.counter));
        t.truncate(24);
        t
    }

    /// Feed one `<inventoryManager>` response through the state machine.
    #[allow(clippy::too_many_arguments)]
    pub fn on_response(
        &mut self,
        token: &str,
        room: &str,
        state: Option<&str>,
        items: Vec<ManagedInventoryItem>,
        continuations: &[(String, String)],
        now_ms: u64,
    ) -> ResponseOutcome {
        let Some(load) = self.active.as_mut() else {
            return ResponseOutcome::Foreign;
        };
        let Some(req) = load.in_flight.remove(token) else {
            return ResponseOutcome::Foreign;
        };

        // Error/stale states fail the whole load; stale means the server
        // discarded our cursor mid-walk, so partial data can't be trusted
        // to still describe one coherent moment.
        if let Some(s) = state.filter(|s| !s.is_empty()) {
            tracing::warn!(
                "inventoryManager load failed (token={token}, state={s}); restarting"
            );
            self.fail_load(now_ms);
            return ResponseOutcome::Failed;
        }

        // First chunk pins the room; later chunks must agree (a room change
        // mid-walk means the snapshot is torn).
        if load.room.is_empty() {
            load.room = room.to_string();
        } else if load.room != room {
            tracing::warn!(
                "inventoryManager continuation for room {room} but load began in {}; restarting",
                load.room
            );
            self.fail_load(now_ms);
            return ResponseOutcome::Failed;
        }
        let _ = req;

        for item in items {
            if load.item_ids.insert(item.id.clone()) {
                load.items.push(item);
            }
        }
        for cursor in continuations {
            if load.seen_cursors.insert(cursor.clone()) {
                load.queued.push_back(cursor.clone());
            }
        }
        self.pump(now_ms);

        let load = self.active.as_ref().expect("load still active");
        if load.queued.is_empty() && load.in_flight.is_empty() {
            let load = self.active.take().expect("load present");
            self.generation += 1;
            ResponseOutcome::Publish(ManagedInventoryState {
                token: token.to_string(),
                room: load.room,
                items: load.items,
                complete: true,
                generation: self.generation,
            })
        } else {
            ResponseOutcome::Absorbed
        }
    }

    /// Tear down the active load and schedule a restart if budget remains.
    fn fail_load(&mut self, now_ms: u64) {
        self.active = None;
        if self.restarts < MAX_RESTARTS {
            self.restarts += 1;
            self.start_load(now_ms);
        } else {
            tracing::warn!("inventoryManager load abandoned after {MAX_RESTARTS} restarts");
        }
    }

    /// Fill free in-flight slots from the cursor queue.
    fn pump(&mut self, now_ms: u64) {
        // Collect sends outside the borrow of `active`.
        let mut sends: Vec<(String, String, String, String)> = Vec::new();
        {
            let Some(load) = self.active.as_mut() else {
                return;
            };
            // Sends collected here haven't hit in_flight yet - count them
            // against the cap or the whole queue drains in one burst.
            while load.in_flight.len() + sends.len() < MAX_IN_FLIGHT {
                let Some((root, last)) = load.queued.pop_front() else {
                    break;
                };
                sends.push((load.room.clone(), root, last, String::new()));
            }
        }
        for (room, root, last, _) in sends {
            let token = self.next_token(now_ms);
            if let Some(load) = self.active.as_mut() {
                load.in_flight.insert(
                    token.clone(),
                    InFlight {
                        cursor: Some((root.clone(), last.clone())),
                        deadline_ms: now_ms + REQUEST_TIMEOUT_MS,
                    },
                );
            }
            self.outbox
                .push(format!("_inventory manager {token} continue {room} {root} {last}"));
        }
    }

    /// Advance timeouts and drain due commands. Call once per tick; the
    /// caller sends whatever comes back.
    pub fn tick(&mut self, now_ms: u64) -> Vec<String> {
        if let Some(load) = self.active.as_ref() {
            let timed_out = load
                .in_flight
                .values()
                .any(|r| now_ms >= r.deadline_ms);
            if timed_out {
                tracing::warn!("inventoryManager request timed out; restarting load");
                self.fail_load(now_ms);
            }
        }
        std::mem::take(&mut self.outbox)
    }
}

/// Lowercase base36, the alphabet Saga's `toString(36)` produces.
fn base36(mut v: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if v == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while v > 0 {
        out.push(DIGITS[(v % 36) as usize]);
        v /= 36;
    }
    out.reverse();
    String::from_utf8(out).expect("base36 digits are ascii")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> ManagedInventoryItem {
        ManagedInventoryItem {
            id: id.to_string(),
            relation: "in".to_string(),
            parent: "player".to_string(),
            noun: format!("thing{id}"),
            name: format!("a thing{id}"),
            ..Default::default()
        }
    }

    /// Pull the token out of an outbound command (last word for initial,
    /// third word for continue).
    fn token_of(cmd: &str) -> String {
        cmd.split_whitespace().nth(2).unwrap().to_string()
    }

    #[test]
    fn single_response_publishes_complete() {
        let mut svc = InventoryService::new();
        svc.request_refresh(1_000);
        let cmds = svc.tick(1_000);
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].starts_with("_inventory manager im"));
        let token = token_of(&cmds[0]);

        let out = svc.on_response(&token, "12345", None, vec![item("1")], &[], 1_500);
        let ResponseOutcome::Publish(snap) = out else {
            panic!("expected publish, got {out:?}");
        };
        assert!(snap.complete);
        assert_eq!(snap.room, "12345");
        assert_eq!(snap.items.len(), 1);
        assert!(!svc.loading());
    }

    #[test]
    fn continuations_fan_out_capped_and_merge() {
        let mut svc = InventoryService::new();
        svc.request_refresh(0);
        let token = token_of(&svc.tick(0)[0]);

        // Initial chunk: 2 items + 6 cursors -> only 4 go out.
        let cursors: Vec<(String, String)> = (0..6)
            .map(|i| (format!("r{i}"), format!("l{i}")))
            .collect();
        let out = svc.on_response(&token, "77", None, vec![item("1"), item("2")], &cursors, 10);
        assert_eq!(out, ResponseOutcome::Absorbed);
        let sent = svc.tick(10);
        assert_eq!(sent.len(), MAX_IN_FLIGHT, "fan-out capped at 4");
        for cmd in &sent {
            assert!(cmd.contains(" continue 77 "), "cursor request: {cmd}");
        }

        // Answer the four; two more cursors go out as slots free.
        for (i, cmd) in sent.iter().enumerate() {
            let t = token_of(cmd);
            let out = svc.on_response(&t, "77", None, vec![item(&format!("c{i}"))], &[], 20);
            assert_eq!(out, ResponseOutcome::Absorbed);
        }
        let sent2 = svc.tick(20);
        assert_eq!(sent2.len(), 2, "remaining cursors dispatched");

        // Final answers complete the snapshot with every chunk merged.
        let t0 = token_of(&sent2[0]);
        assert_eq!(
            svc.on_response(&t0, "77", None, vec![item("x")], &[], 30),
            ResponseOutcome::Absorbed
        );
        let t1 = token_of(&sent2[1]);
        let out = svc.on_response(&t1, "77", None, vec![item("y")], &[], 30);
        let ResponseOutcome::Publish(snap) = out else {
            panic!("expected publish, got {out:?}");
        };
        assert!(snap.complete);
        assert_eq!(snap.items.len(), 2 + 4 + 2);
    }

    #[test]
    fn repeated_cursor_and_duplicate_item_are_dropped() {
        let mut svc = InventoryService::new();
        svc.request_refresh(0);
        let token = token_of(&svc.tick(0)[0]);
        let cursor = vec![("r0".to_string(), "l0".to_string())];
        svc.on_response(&token, "1", None, vec![item("1")], &cursor, 0);
        let sent = svc.tick(0);
        assert_eq!(sent.len(), 1);
        // The continuation re-offers the same cursor and a duplicate item.
        let t = token_of(&sent[0]);
        let out = svc.on_response(&t, "1", None, vec![item("1"), item("2")], &cursor, 5);
        let ResponseOutcome::Publish(snap) = out else {
            panic!("repeat cursor must not re-queue; got {out:?}");
        };
        assert_eq!(snap.items.len(), 2, "duplicate id dropped");
        assert!(svc.tick(5).is_empty(), "no repeat request");
    }

    #[test]
    fn stale_state_restarts_from_scratch_bounded() {
        let mut svc = InventoryService::new();
        svc.request_refresh(0);
        let t1 = token_of(&svc.tick(0)[0]);
        assert_eq!(
            svc.on_response(&t1, "1", Some("stale"), vec![], &[], 10),
            ResponseOutcome::Failed
        );
        // Restart #1 goes out.
        let sent = svc.tick(10);
        assert_eq!(sent.len(), 1);
        assert!(sent[0].starts_with("_inventory manager im"));
        assert!(!sent[0].contains("continue"), "restart is a fresh initial load");

        // Two more failures exhaust the restart budget.
        let t2 = token_of(&sent[0]);
        svc.on_response(&t2, "1", Some("stale"), vec![], &[], 20);
        let t3 = token_of(&svc.tick(20)[0]);
        svc.on_response(&t3, "1", Some("malformed"), vec![], &[], 30);
        assert!(svc.tick(30).is_empty(), "restart budget exhausted");
        assert!(!svc.loading());

        // An explicit refresh starts a new budget.
        svc.request_refresh(40);
        assert_eq!(svc.tick(40).len(), 1);
    }

    #[test]
    fn timeout_restarts_load() {
        let mut svc = InventoryService::new();
        svc.request_refresh(0);
        let _ = svc.tick(0);
        // No answer within the window: next tick fails + restarts.
        let sent = svc.tick(REQUEST_TIMEOUT_MS + 1);
        assert_eq!(sent.len(), 1, "restarted initial request");
        assert!(svc.loading());
    }

    #[test]
    fn room_mismatch_mid_walk_restarts() {
        let mut svc = InventoryService::new();
        svc.request_refresh(0);
        let token = token_of(&svc.tick(0)[0]);
        let cursor = vec![("r".to_string(), "l".to_string())];
        svc.on_response(&token, "100", None, vec![item("1")], &cursor, 0);
        let t = token_of(&svc.tick(0)[0]);
        // Continuation claims a different room: torn snapshot.
        assert_eq!(
            svc.on_response(&t, "200", None, vec![item("2")], &[], 5),
            ResponseOutcome::Failed
        );
    }

    #[test]
    fn foreign_token_is_ignored_by_the_state_machine() {
        let mut svc = InventoryService::new();
        svc.request_refresh(0);
        let _ = svc.tick(0);
        assert_eq!(
            svc.on_response("imtest1", "1", None, vec![item("1")], &[], 5),
            ResponseOutcome::Foreign
        );
        assert!(svc.loading(), "active load unaffected");
    }

    #[test]
    fn refresh_while_loading_is_a_noop() {
        let mut svc = InventoryService::new();
        svc.request_refresh(0);
        assert_eq!(svc.tick(0).len(), 1);
        svc.request_refresh(1);
        assert!(svc.tick(1).is_empty(), "no second initial request");
    }

    #[test]
    fn tokens_are_unique_and_bounded() {
        let mut svc = InventoryService::new();
        let a = svc.next_token(1_755_000_000_000);
        let b = svc.next_token(1_755_000_000_000);
        assert_ne!(a, b);
        assert!(a.len() <= 24 && b.len() <= 24);
        assert!(a.starts_with("im"));
    }
}
