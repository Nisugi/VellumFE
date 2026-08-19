//! Verified item moves over the extended feed's hidden `_drag` verb
//! (Saga's transfer discipline). Every move declares the hand-state change
//! that proves it worked and is confirmed against the `<left>`/`<right>`
//! feed within a timeout — never by matching prose. One move at a time;
//! the caller drains outbound commands through the same observable path
//! as travel (the command-gate lesson).
//!
//! Command vocabulary (GS):
//! - retrieve to hand: `_drag #{id} LEFT` / `_drag #{id} RIGHT`
//! - drop:             `_drag #{id} DROP`
//! - wear:             `WEAR #{id}`
//! - place at feet:    `PLACE FEET #{id}`
//! - into/onto:        `put #{id} {in|on|behind|underneath} {#dest|selector}`

/// How long a move may wait for its confirming hand event (Saga uses 8s).
const MOVE_TIMEOUT_MS: u64 = 8_000;

/// What to do with the item.
#[derive(Debug, Clone, PartialEq)]
pub enum MoveKind {
    ToLeftHand,
    ToRightHand,
    Drop,
    Wear,
    PlaceFeet,
    /// `put` into/onto a destination container. `selector` (from the
    /// destination's `in_selector`, e.g. lockers) replaces the `#id` path
    /// when present.
    PutIn {
        dest: String,
        relation: String,
        selector: Option<String>,
    },
}

/// Exist ids currently in each hand (None = empty).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandsView {
    pub left: Option<String>,
    pub right: Option<String>,
}

impl HandsView {
    fn holds(&self, exist: &str) -> bool {
        self.left.as_deref() == Some(exist) || self.right.as_deref() == Some(exist)
    }
}

/// The hand-state change that proves the move happened.
#[derive(Debug, Clone, PartialEq)]
enum Expect {
    AppearIn {
        left: bool,
    },
    LeaveHands,
    /// Item wasn't in a hand when the move fired (`_drag` handles
    /// container-direct moves server-side in one motion). Hand events can
    /// only confirm if the item transits a hand; otherwise the move
    /// completes as "sent" when the timeout window closes quietly.
    Unverifiable,
}

#[derive(Debug, Clone)]
struct Active {
    item: String,
    expect: Expect,
    deadline_ms: u64,
    /// Human-readable description for failure echoes
    desc: String,
}

#[derive(Debug, PartialEq)]
pub enum MoveOutcome {
    Succeeded {
        desc: String,
    },
    /// The command went out but nothing observable can confirm it
    /// (container-direct move that never transited a hand).
    Sent {
        desc: String,
    },
    Failed {
        desc: String,
        reason: String,
    },
}

#[derive(Debug, Default)]
pub struct ItemMover {
    active: Option<Active>,
    outbox: Vec<String>,
}

impl ItemMover {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn busy(&self) -> bool {
        self.active.is_some()
    }

    /// Begin a move. Fails fast (no send) when the request can't succeed:
    /// a second move while one is running, a retrieve into an occupied
    /// hand, or a hand-shedding move for an item not actually held.
    pub fn start(
        &mut self,
        item: &str,
        kind: MoveKind,
        hands: &HandsView,
        now_ms: u64,
    ) -> Result<(), String> {
        if self.active.is_some() {
            return Err("a move is already in progress".to_string());
        }
        let (cmd, expect, desc) = match &kind {
            MoveKind::ToLeftHand => {
                if hands.left.is_some() {
                    return Err("left hand is full".to_string());
                }
                (
                    format!("_drag #{item} LEFT"),
                    Expect::AppearIn { left: true },
                    format!("retrieve #{item} to left hand"),
                )
            }
            MoveKind::ToRightHand => {
                if hands.right.is_some() {
                    return Err("right hand is full".to_string());
                }
                (
                    format!("_drag #{item} RIGHT"),
                    Expect::AppearIn { left: false },
                    format!("retrieve #{item} to right hand"),
                )
            }
            // Shedding moves work from a container too - the server's _drag
            // retrieves and completes in one motion (Lich behavior). Held
            // items verify by the hand clearing; container-direct moves
            // can only be watched for a transient hand appearance.
            MoveKind::Drop => {
                let expect = if hands.holds(item) {
                    Expect::LeaveHands
                } else {
                    Expect::Unverifiable
                };
                (
                    format!("_drag #{item} DROP"),
                    expect,
                    format!("drop #{item}"),
                )
            }
            MoveKind::Wear => {
                let expect = if hands.holds(item) {
                    Expect::LeaveHands
                } else {
                    Expect::Unverifiable
                };
                (format!("WEAR #{item}"), expect, format!("wear #{item}"))
            }
            MoveKind::PlaceFeet => {
                let expect = if hands.holds(item) {
                    Expect::LeaveHands
                } else {
                    Expect::Unverifiable
                };
                (
                    format!("PLACE FEET #{item}"),
                    expect,
                    format!("place #{item} at feet"),
                )
            }
            MoveKind::PutIn {
                dest,
                relation,
                selector,
            } => {
                let expect = if hands.holds(item) {
                    Expect::LeaveHands
                } else {
                    Expect::Unverifiable
                };
                let target = match selector {
                    Some(s) => s.clone(),
                    None => format!("#{dest}"),
                };
                (
                    format!("put #{item} {relation} {target}"),
                    expect,
                    format!("put #{item} {relation} {target}"),
                )
            }
        };
        // Unverifiable moves only wait long enough to catch a transient
        // hand appearance before reporting "sent".
        let window = if expect == Expect::Unverifiable {
            2_000
        } else {
            MOVE_TIMEOUT_MS
        };
        self.active = Some(Active {
            item: item.to_string(),
            expect,
            deadline_ms: now_ms + window,
            desc,
        });
        self.outbox.push(cmd);
        Ok(())
    }

    /// Check the current hand state against the active move's expectation
    /// and drain due commands. Returns an outcome exactly once per move.
    pub fn tick(&mut self, hands: &HandsView, now_ms: u64) -> (Vec<String>, Option<MoveOutcome>) {
        // A container-direct move that transits a hand upgrades to real
        // hand-clearing verification.
        if let Some(active) = self.active.as_mut() {
            if active.expect == Expect::Unverifiable && hands.holds(&active.item) {
                active.expect = Expect::LeaveHands;
                active.deadline_ms = now_ms + MOVE_TIMEOUT_MS;
            }
        }
        let outcome = match self.active.as_ref() {
            None => None,
            Some(active) => {
                let confirmed = match &active.expect {
                    Expect::AppearIn { left: true } => {
                        hands.left.as_deref() == Some(active.item.as_str())
                    }
                    Expect::AppearIn { left: false } => {
                        hands.right.as_deref() == Some(active.item.as_str())
                    }
                    Expect::LeaveHands => !hands.holds(&active.item),
                    Expect::Unverifiable => false,
                };
                if confirmed {
                    let active = self.active.take().expect("checked above");
                    Some(MoveOutcome::Succeeded { desc: active.desc })
                } else if now_ms >= active.deadline_ms {
                    let active = self.active.take().expect("checked above");
                    if active.expect == Expect::Unverifiable {
                        Some(MoveOutcome::Sent { desc: active.desc })
                    } else {
                        Some(MoveOutcome::Failed {
                            desc: active.desc,
                            reason: "no confirming hand update within 8s".to_string(),
                        })
                    }
                } else {
                    None
                }
            }
        };
        (std::mem::take(&mut self.outbox), outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hands(left: Option<&str>, right: Option<&str>) -> HandsView {
        HandsView {
            left: left.map(str::to_string),
            right: right.map(str::to_string),
        }
    }

    #[test]
    fn retrieve_confirms_on_hand_appearance() {
        let mut m = ItemMover::new();
        let empty = hands(None, None);
        m.start("42", MoveKind::ToLeftHand, &empty, 0).unwrap();
        let (cmds, out) = m.tick(&empty, 100);
        assert_eq!(cmds, vec!["_drag #42 LEFT".to_string()]);
        assert_eq!(out, None, "not confirmed yet");

        let (_, out) = m.tick(&hands(Some("42"), None), 200);
        assert!(matches!(out, Some(MoveOutcome::Succeeded { .. })));
        assert!(!m.busy());
    }

    #[test]
    fn put_uses_selector_over_id_path_and_confirms_on_leave() {
        let mut m = ItemMover::new();
        let holding = hands(None, Some("42"));
        m.start(
            "42",
            MoveKind::PutIn {
                dest: "9001".to_string(),
                relation: "in".to_string(),
                selector: Some("locker".to_string()),
            },
            &holding,
            0,
        )
        .unwrap();
        let (cmds, _) = m.tick(&holding, 1);
        assert_eq!(cmds, vec!["put #42 in locker".to_string()]);
        let (_, out) = m.tick(&hands(None, None), 500);
        assert!(matches!(out, Some(MoveOutcome::Succeeded { .. })));
    }

    #[test]
    fn timeout_fails_exactly_once() {
        let mut m = ItemMover::new();
        let holding = hands(Some("42"), None);
        m.start("42", MoveKind::Drop, &holding, 0).unwrap();
        let _ = m.tick(&holding, 1);
        let (_, out) = m.tick(&holding, MOVE_TIMEOUT_MS + 1);
        assert!(matches!(out, Some(MoveOutcome::Failed { .. })));
        let (_, out) = m.tick(&holding, MOVE_TIMEOUT_MS + 2);
        assert_eq!(out, None, "outcome reported once");
    }

    #[test]
    fn fail_fast_preconditions_send_nothing() {
        let mut m = ItemMover::new();
        // Retrieve into an occupied hand.
        assert!(m
            .start("42", MoveKind::ToLeftHand, &hands(Some("7"), None), 0)
            .is_err());
        // Second move while one runs.
        m.start("42", MoveKind::ToRightHand, &hands(None, None), 0)
            .unwrap();
        assert!(m
            .start("43", MoveKind::ToLeftHand, &hands(None, None), 0)
            .is_err());
        let (cmds, _) = m.tick(&hands(None, None), 1);
        assert_eq!(cmds.len(), 1, "only the accepted move sent");
    }

    #[test]
    fn container_direct_drop_is_allowed_and_reports_sent() {
        // The server's _drag handles container->drop in one motion (Lich
        // behavior); no hand ever changes, so the move reports Sent after
        // the transient window.
        let mut m = ItemMover::new();
        let empty = hands(None, None);
        m.start("42", MoveKind::Drop, &empty, 0).unwrap();
        let (cmds, _) = m.tick(&empty, 1);
        assert_eq!(cmds, vec!["_drag #42 DROP".to_string()]);
        let (_, out) = m.tick(&empty, 2_100);
        assert!(matches!(out, Some(MoveOutcome::Sent { .. })), "{out:?}");
    }

    #[test]
    fn container_direct_move_transiting_a_hand_upgrades_to_verified() {
        let mut m = ItemMover::new();
        let empty = hands(None, None);
        m.start("42", MoveKind::Drop, &empty, 0).unwrap();
        let _ = m.tick(&empty, 1);
        // Item flashes through the right hand mid-motion...
        let (_, out) = m.tick(&hands(None, Some("42")), 500);
        assert_eq!(out, None, "transit upgrades, not completes");
        // ...and leaves: fully verified.
        let (_, out) = m.tick(&empty, 900);
        assert!(
            matches!(out, Some(MoveOutcome::Succeeded { .. })),
            "{out:?}"
        );
    }

    #[test]
    fn drag_drop_verifies_departure_not_prose() {
        let mut m = ItemMover::new();
        let holding = hands(Some("42"), None);
        m.start("42", MoveKind::Drop, &holding, 0).unwrap();
        let (cmds, _) = m.tick(&holding, 1);
        assert_eq!(cmds, vec!["_drag #42 DROP".to_string()]);
        // Hand cleared = confirmed, regardless of what prose said.
        let (_, out) = m.tick(&hands(None, None), 50);
        assert!(matches!(out, Some(MoveOutcome::Succeeded { .. })));
    }
}
