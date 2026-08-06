//! `ViewKind` — names a PRESENTATION (which renderer draws a window),
//! decoupled from window identity (which game feed it binds to).
//!
//! Window-system redesign Phase 1 (spec:
//! `.beads/artifacts/window-system-redesign/spec.md`). The template
//! catalog conflates identity, presentation, and default geometry;
//! identity already lives in `WindowBinding`, and this type extracts
//! presentation. Dark in Phase 1: only the resolver façade
//! (`core::view_resolver`) produces it, and only tests consume it.
//! Phase 3 swaps the creation/enumeration paths onto it.

use serde::{Deserialize, Serialize};

/// Which renderer presents a bound window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    /// A dedicated widget view, named by its catalog key (today: the
    /// template name, e.g. "compass", "inventory", "gs4_experience").
    /// The key vocabulary is pinned by the Phase 0 golden fixture.
    Dedicated(String),
    /// The generic scrolling text view (unclaimed streams).
    Text,
    /// The generic dynamic dialog renderer (unclaimed dialogs — the
    /// UberBar-proven anchor-grid path; always safe because the dialog
    /// store ingests every dialog).
    DialogPanel,
    /// The container listing view.
    Container,
}

impl ViewKind {
    /// The catalog key for dedicated views; None for the generic views.
    pub fn dedicated_key(&self) -> Option<&str> {
        match self {
            ViewKind::Dedicated(key) => Some(key),
            _ => None,
        }
    }
}
