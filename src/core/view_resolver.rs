//! Presentation resolver — `resolve_view(binding, override) -> ViewKind`.
//!
//! Window-system redesign Phase 1 (spec:
//! `.beads/artifacts/window-system-redesign/spec.md`): a PURE FAÇADE over
//! the template catalog's three id-mapping functions. Nothing in
//! production calls it yet (dark); the equivalence tests below prove it
//! reproduces the catalog exactly, pinned by the same probe inventory as
//! the Phase 0 golden fixture. Phase 3 re-points the creation and
//! enumeration paths here while these tests stay green; Phase 4 swaps
//! the internals from the catalog delegation to one `DEDICATED_VIEWS`
//! table — behind this unchanged signature — and the "must agree"
//! invariant currently split across three functions becomes true by
//! construction.
//!
//! Three layers, first hit wins:
//!   1. per-window `view` override (serde-optional field, arrives with
//!      Phase 3; the parameter exists now so the signature never changes)
//!   2. dedicated views (today: delegation to the catalog id-maps)
//!   3. kind fallback: Stream → Text, Dialog → DialogPanel,
//!      Container → Container

use crate::config::{Config, WindowBinding};
use crate::data::view_kind::ViewKind;

/// Stream ids with a DEDICATED widget view (Phase 6: formerly
/// `stream_id_to_template`). Everything else is generic text.
fn dedicated_stream_view(id: &str) -> Option<&'static str> {
    match id {
        "Spells" => Some("spells"),
        "inv" => Some("inventory"),
        "reserve" => Some("reserve"),
        "room" => Some("room"),
        _ => None,
    }
}

/// Dialog id → catalog seed key when they differ (Phase 6: formerly
/// `dialog_id_to_template`). Most dialogs share their id with the key;
/// the active-effects dialogs arrive capitalized and expr maps to the
/// GS4 experience widget.
pub fn dialog_seed_alias(id: &str) -> &str {
    match id {
        "expr" => "gs4_experience",
        "Buffs" => "buffs",
        "Debuffs" => "debuffs",
        "Cooldowns" => "cooldowns",
        "Active Spells" | "ActiveSpells" => "active_spells",
        other => other,
    }
}

/// Resolve which renderer presents the window bound to `binding`.
/// `view_override` is the per-window user override (checked first, so a
/// user can flip any dialog to the raw dynamic rendering — or a stream
/// to plain text — without regressing defaults).
pub fn resolve_view(binding: &WindowBinding, view_override: Option<&str>) -> ViewKind {
    // Layer 1: explicit per-window override.
    if let Some(view) = view_override {
        return match view {
            "text" => ViewKind::Text,
            "dialogpanel" => ViewKind::DialogPanel,
            "container" => ViewKind::Container,
            key => ViewKind::Dedicated(key.to_string()),
        };
    }
    // Layer 2 (Phase 6: THE dedicated-view tables, formerly three
    // scattered id-map functions) + layer 3 (kind fallback). A dialog id
    // is claimed exactly when its aliased key seeds a catalog view —
    // reproducing the old id_has_widget_template contract, pinned by the
    // Phase 0 golden.
    match binding {
        WindowBinding::Stream(id) => match dedicated_stream_view(id) {
            Some(view) => ViewKind::Dedicated(view.to_string()),
            None => ViewKind::Text,
        },
        WindowBinding::Dialog(id) => {
            let key = dialog_seed_alias(id);
            if Config::get_window_template(key).is_some() {
                ViewKind::Dedicated(key.to_string())
            } else {
                ViewKind::DialogPanel
            }
        }
        WindowBinding::Container(_) => ViewKind::Container,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase 6: the id-maps are deleted; the resolver IS the
    /// implementation. The Phase 0 golden fixture (frozen text) remains
    /// the oracle proving these tables reproduce the old maps exactly.
    #[test]
    fn resolver_is_equivalent_to_the_catalog_maps() {
        let probe_ids = [
            "inv",
            "reserve",
            "room",
            "Spells",
            "thoughts",
            "speech",
            "familiar",
            "voln",
            "logons",
            "death",
            "combat",
            "charprofile",
            "minivitals",
            "expr",
            "encum",
            "stance",
            "injuries",
            "IconBAR",
            "compass",
            "Buffs",
            "Debuffs",
            "Cooldowns",
            "Active Spells",
            "bank",
            "befriend",
            "quick",
            "espMasterDialog",
            "injuries-10154507",
            "UberBar",
            "bugDialogBox",
            "no_such_id",
            "",
        ];
        for id in probe_ids {
            // Streams: dedicated iff the table names a view; otherwise
            // generic text (the old "text_custom" fallback).
            let stream = resolve_view(&WindowBinding::Stream(id.to_string()), None);
            match &stream {
                ViewKind::Text => assert!(dedicated_stream_view(id).is_none(), "{id}"),
                ViewKind::Dedicated(key) => {
                    assert_eq!(Some(key.as_str()), dedicated_stream_view(id), "{id}")
                }
                other => panic!("stream {id} resolved to {other:?}"),
            }

            // Dialogs: dedicated iff the aliased key seeds a view.
            let dialog = resolve_view(&WindowBinding::Dialog(id.to_string()), None);
            match &dialog {
                ViewKind::Dedicated(key) => {
                    assert_eq!(key, dialog_seed_alias(id), "{id}");
                    assert!(Config::get_window_template(key).is_some(), "{id}");
                }
                ViewKind::DialogPanel => {
                    assert!(
                        Config::get_window_template(dialog_seed_alias(id)).is_none(),
                        "{id}"
                    )
                }
                other => panic!("dialog {id} resolved to {other:?}"),
            }

            // Containers are always the container view.
            assert_eq!(
                resolve_view(&WindowBinding::Container(id.to_string()), None),
                ViewKind::Container
            );
        }
    }

    /// The active-effects dialogs come capitalized/spaced; each must map
    /// to a real widget seed so claims_dialog recognizes them as
    /// widget-backed (and never spawns a generic empty panel). Moved from
    /// templates.rs in Phase 6.
    #[test]
    fn effect_dialog_ids_resolve_to_their_widget_views() {
        for (id, expected) in [
            ("Buffs", "buffs"),
            ("Debuffs", "debuffs"),
            ("Cooldowns", "cooldowns"),
            ("Active Spells", "active_spells"),
            ("expr", "gs4_experience"),
        ] {
            assert_eq!(
                resolve_view(&WindowBinding::Dialog(id.to_string()), None),
                ViewKind::Dedicated(expected.to_string()),
                "dialog '{id}' should resolve to '{expected}'"
            );
        }
    }

    /// Every dedicated view a dialog/stream can resolve to must exist in
    /// the catalog — a resolver that names a missing view would create
    /// un-seedable windows.
    #[test]
    fn dedicated_views_are_seedable() {
        for id in ["inv", "reserve", "room", "Spells", "minivitals", "expr", "encum"] {
            for binding in [
                WindowBinding::Stream(id.to_string()),
                WindowBinding::Dialog(id.to_string()),
            ] {
                if let ViewKind::Dedicated(key) = resolve_view(&binding, None) {
                    assert!(
                        Config::get_window_template(&key).is_some(),
                        "resolver names view {key:?} for {binding:?} but the \
                         catalog cannot seed it"
                    );
                }
            }
        }
    }

    /// Layer 1: the per-window override beats everything, including for
    /// ids with dedicated views (the "flip minivitals to the raw skinned
    /// rendering" use case).
    #[test]
    fn view_override_wins_over_dedicated() {
        let minivitals = WindowBinding::Dialog("minivitals".to_string());
        assert!(matches!(
            resolve_view(&minivitals, None),
            ViewKind::Dedicated(_)
        ));
        assert_eq!(
            resolve_view(&minivitals, Some("dialogpanel")),
            ViewKind::DialogPanel
        );
        assert_eq!(
            resolve_view(&WindowBinding::Stream("inv".to_string()), Some("text")),
            ViewKind::Text
        );
        assert_eq!(
            resolve_view(&minivitals, Some("dashboard")),
            ViewKind::Dedicated("dashboard".to_string())
        );
    }
}
