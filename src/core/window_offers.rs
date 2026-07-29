//! The window-offer registry: one unified model of every window the game
//! has offered to show — containers, dialogs, and streams — replacing
//! three fragmented auto-spawn paths (`newly_registered_container`,
//! `pending_window_additions`/`active_dialog`, and the seen-streams
//! picker) with one list the user can see and toggle.
//!
//! The game announces window intents through three tag families
//! (`<container>`, `<openDialog>`, `<streamWindow>`), all carrying the
//! same essentials — id, title, kind, persistence hints. This registry
//! captures each as a `WindowOffer` with a per-offer **policy** (the
//! user's show/hide choice, remembered). Frontends render the offered
//! windows whose policy shows them, plus the offer list itself (the
//! "known windows" checkbox manager) — same data on every frontend, so
//! TUI/GUI/web stay in parity.
//!
//! P1 (this module) stands up the model + registration, populated
//! alongside the existing spawn paths (no behavior change yet). P2 points
//! the UI at it and retires the old paths.

use std::collections::BTreeMap;

/// What kind of window the game offered. They share the offer shape but
/// differ in how contents are sourced and rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferKind {
    /// A bag/container (`<container>`); contents from the GameObjects
    /// registry.
    Container,
    /// A game dialog (`<openDialog>`; bank, menus, ...); content from
    /// `dialogData`.
    Dialog,
    /// A named text stream (`<streamWindow>`; thoughts, loot, ...).
    Stream,
}

impl OfferKind {
    pub fn label(&self) -> &'static str {
        match self {
            OfferKind::Container => "container",
            OfferKind::Dialog => "dialog",
            OfferKind::Stream => "stream",
        }
    }
}

/// The user's show/hide decision for an offered window, remembered across
/// re-offers of the same id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    /// No explicit choice yet — the frontend applies its default (e.g.
    /// don't auto-open until the user opts in).
    #[default]
    Unset,
    /// The user wants this window shown when offered.
    Shown,
    /// The user has hidden this window; don't auto-show it.
    Hidden,
}

/// One window the game has offered to show.
#[derive(Clone, Debug)]
pub struct WindowOffer {
    pub id: String,
    pub title: String,
    pub kind: OfferKind,
    /// Game layout hint (right/left/center/quickBar/…) when known.
    pub location: Option<String>,
    /// The game asked to persist this window's position (`save='t'`).
    pub save: bool,
    /// The game marks this window resident (persistent across the session).
    pub resident: bool,
    /// User's show/hide policy, remembered.
    pub policy: Policy,
}

impl WindowOffer {
    /// Whether the frontend should show this window: an explicit Shown, or
    /// Unset for a resident window (the game intends those to persist).
    pub fn should_show(&self) -> bool {
        match self.policy {
            Policy::Shown => true,
            Policy::Hidden => false,
            Policy::Unset => self.resident,
        }
    }
}

/// The registry of offered windows, keyed by id. Ordered (BTreeMap) so the
/// "known windows" list is stable.
#[derive(Clone, Debug, Default)]
pub struct WindowOffers {
    offers: BTreeMap<String, WindowOffer>,
    /// Bumped on any change so frontends can diff cheaply.
    pub generation: u64,
}

impl WindowOffers {
    /// Register or refresh an offer. Preserves the existing user policy
    /// when the same id is re-offered (the whole point of remembering).
    /// Title/location/flags refresh from the latest offer. Returns true if
    /// anything changed.
    pub fn offer(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        kind: OfferKind,
        location: Option<String>,
        save: bool,
        resident: bool,
    ) -> bool {
        let id = id.into();
        let title = title.into();
        if let Some(existing) = self.offers.get_mut(&id) {
            let changed = existing.title != title
                || existing.kind != kind
                || existing.location != location
                || existing.save != save
                || existing.resident != resident;
            if changed {
                existing.title = title;
                existing.kind = kind;
                existing.location = location;
                existing.save = save;
                existing.resident = resident;
                self.generation += 1;
            }
            changed
        } else {
            self.offers.insert(
                id.clone(),
                WindowOffer {
                    id,
                    title,
                    kind,
                    location,
                    save,
                    resident,
                    policy: Policy::default(),
                },
            );
            self.generation += 1;
            true
        }
    }

    /// Set a user policy for an offered window (the checkbox toggle).
    /// No-op if the id isn't offered.
    pub fn set_policy(&mut self, id: &str, policy: Policy) {
        if let Some(offer) = self.offers.get_mut(id) {
            if offer.policy != policy {
                offer.policy = policy;
                self.generation += 1;
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&WindowOffer> {
        self.offers.get(id)
    }

    /// All offers in stable (id) order — the "known windows" list.
    pub fn all(&self) -> impl Iterator<Item = &WindowOffer> {
        self.offers.values()
    }

    /// Offers the frontend should currently show.
    pub fn shown(&self) -> impl Iterator<Item = &WindowOffer> {
        self.offers.values().filter(|o| o.should_show())
    }

    pub fn len(&self) -> usize {
        self.offers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.offers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_inserts_then_refreshes_preserving_policy() {
        let mut offers = WindowOffers::default();
        assert!(offers.offer("backpack", "Backpack", OfferKind::Container, Some("right".into()), false, false));
        let g = offers.generation;

        // User hides it.
        offers.set_policy("backpack", Policy::Hidden);
        assert!(offers.generation > g);
        assert_eq!(offers.get("backpack").unwrap().policy, Policy::Hidden);

        // Re-offer with a new title: title refreshes, policy is KEPT.
        offers.offer("backpack", "Sturdy Backpack", OfferKind::Container, Some("right".into()), false, false);
        let o = offers.get("backpack").unwrap();
        assert_eq!(o.title, "Sturdy Backpack");
        assert_eq!(o.policy, Policy::Hidden, "user choice remembered across re-offer");

        // Re-offer identical = no change, no generation bump.
        let g = offers.generation;
        assert!(!offers.offer("backpack", "Sturdy Backpack", OfferKind::Container, Some("right".into()), false, false));
        assert_eq!(offers.generation, g);
    }

    #[test]
    fn should_show_reflects_policy_and_residency() {
        let mut offers = WindowOffers::default();
        // Non-resident, Unset → not shown (frontend opts in).
        offers.offer("bank", "Bank", OfferKind::Dialog, None, true, false);
        assert!(!offers.get("bank").unwrap().should_show());
        // Resident, Unset → shown (game intends persistence).
        offers.offer("Buffs", "Buffs", OfferKind::Dialog, Some("right".into()), false, true);
        assert!(offers.get("Buffs").unwrap().should_show());
        // Explicit Shown wins.
        offers.set_policy("bank", Policy::Shown);
        assert!(offers.get("bank").unwrap().should_show());
        // Explicit Hidden hides a resident one.
        offers.set_policy("Buffs", Policy::Hidden);
        assert!(!offers.get("Buffs").unwrap().should_show());
    }

    #[test]
    fn shown_and_all_iterators() {
        let mut offers = WindowOffers::default();
        offers.offer("a", "A", OfferKind::Container, None, false, false);
        offers.offer("b", "B", OfferKind::Stream, None, false, false);
        offers.set_policy("a", Policy::Shown);
        assert_eq!(offers.all().count(), 2);
        let shown: Vec<&str> = offers.shown().map(|o| o.id.as_str()).collect();
        assert_eq!(shown, vec!["a"]);
    }
}
