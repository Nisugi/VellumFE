//! The unified "known windows" view — U3's replacement for the separate
//! `window_offers` registry. Every window the client knows about (from
//! the layout, plus session-only ephemeral windows like containers and
//! dialog panels) is enumerated here as a single list the Windows menu
//! renders, with per-row show/hide. There is no parallel offer registry:
//! a window's binding + visibility on the layout (or its ephemeral
//! runtime state) is the single source of truth.
//!
//! This module holds the pure descriptor + classification; the AppCore
//! methods that enumerate and toggle live in `app_core::state` since they
//! need both the layout and the ui_state.

/// What kind of source a known window came from — drives grouping in the
/// menu and how show/hide is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownWindowKind {
    /// A persistent layout widget (template or custom), possibly bound to
    /// a game dialog/stream feed.
    Layout,
    /// A game dialog panel (combat, befriend, ...) — resident, dockable.
    Dialog,
    /// A game text stream (thoughts, loot, bounty, ...).
    Stream,
    /// A container (bag/ground/worn) — session-only, wiped on relog.
    Container,
}

impl KnownWindowKind {
    pub fn label(&self) -> &'static str {
        match self {
            KnownWindowKind::Layout => "window",
            KnownWindowKind::Dialog => "dialog",
            KnownWindowKind::Stream => "stream",
            KnownWindowKind::Container => "container",
        }
    }

    /// Stable group order for the menu.
    pub const MENU_ORDER: [KnownWindowKind; 4] = [
        KnownWindowKind::Layout,
        KnownWindowKind::Dialog,
        KnownWindowKind::Stream,
        KnownWindowKind::Container,
    ];
}

/// One row in the unified known-windows list.
#[derive(Debug, Clone)]
pub struct KnownWindow {
    /// The window's layout name (persistent) or ephemeral runtime name —
    /// the key show/hide is applied against.
    pub name: String,
    /// Human-facing title (falls back to name).
    pub title: String,
    pub kind: KnownWindowKind,
    /// Widget type string (for grouping by WidgetCategory in the UI).
    pub widget_type: String,
    /// Whether it is currently rendered.
    pub shown: bool,
    /// Whether it lives only for the session (never persisted).
    pub ephemeral: bool,
}
