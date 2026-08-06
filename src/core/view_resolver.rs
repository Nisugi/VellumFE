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
    // Layer 2 (Phase 1: catalog delegation) + layer 3 (kind fallback).
    match binding {
        WindowBinding::Stream(id) => {
            let template = Config::stream_id_to_template(id);
            if template == "text_custom" {
                ViewKind::Text
            } else {
                ViewKind::Dedicated(template.to_string())
            }
        }
        WindowBinding::Dialog(id) => {
            if Config::id_has_widget_template(id) {
                ViewKind::Dedicated(Config::dialog_id_to_template(id).to_string())
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

    /// The façade must agree with the raw catalog maps for every id in
    /// the Phase 0 probe inventory — the equivalence proof that lets
    /// Phase 3 consumers move here without behavior change. When Phase 4
    /// replaces the delegation with the DEDICATED_VIEWS table, THIS test
    /// (plus the Phase 0 golden) is what proves the table complete.
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
            // Streams: dedicated iff the catalog maps to a non-text
            // template; the dedicated key IS that template.
            let stream = resolve_view(&WindowBinding::Stream(id.to_string()), None);
            let stream_template = Config::stream_id_to_template(id);
            match &stream {
                ViewKind::Text => assert_eq!(stream_template, "text_custom", "{id}"),
                ViewKind::Dedicated(key) => assert_eq!(key, stream_template, "{id}"),
                other => panic!("stream {id} resolved to {other:?}"),
            }

            // Dialogs: dedicated iff id_has_widget_template; the key IS
            // dialog_id_to_template's answer.
            let dialog = resolve_view(&WindowBinding::Dialog(id.to_string()), None);
            match &dialog {
                ViewKind::Dedicated(key) => {
                    assert!(Config::id_has_widget_template(id), "{id}");
                    assert_eq!(key, Config::dialog_id_to_template(id), "{id}");
                }
                ViewKind::DialogPanel => {
                    assert!(!Config::id_has_widget_template(id), "{id}")
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
