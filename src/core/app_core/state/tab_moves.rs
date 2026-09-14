//! Tab moves: tear a tabbedtext tab out into its own text window, join a
//! text window into a tabbed one as a tab, convert a window between the two
//! kinds in place, reorder tabs — and undo any of them. Design in
//! `.beads/artifacts/tab-tear-out/spec.md` (§0–§3).
//!
//! Each operation is one atomic method on `AppCore`: a tab's streams leave
//! one subscriber and reach the other inside a single call, so no server
//! line can arrive between and fall to the fallback route. Scrollback is
//! lifted out *before* the sync that would drop it and installed afterwards
//! through [`install_text_buffer`], the only way a buffer ever enters a
//! window here. That helper also copies the resolved stream list into
//! `TextContent.streams` — the field that routes a standalone window, and
//! one a tab's buffer never had filled (tabs route by their definition).
//!
//! Undo is a snapshot restore with a dedicated inverse per operation, never
//! a replay of the opposite move (a replay would mint a fresh `custom-*`
//! name and lose the original window's settings). It applies only while the
//! layout defs still hash to what they were right after the move.

use super::AppCore;
use crate::config::{TabbedTextTab, TabbedTextWidgetData, TextWidgetData, WindowDef};
use crate::data::widget::{TabId, TabbedTextContent, TextContent};
use crate::data::{WidgetType, WindowContent};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

/// The `text_custom` seed every torn-out tab is instantiated from.
const CUSTOM_TEXT_SEED: &str = crate::core::local_catalog::CUSTOM_TEXT_SEED;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabMoveKind {
    /// A tab left its window for a new `custom-*` text window.
    TearOut,
    /// The last tab left: the window itself became a text window.
    ConvertedInPlace,
    /// A text window became a tab of a tabbed window and ceased to exist.
    Join,
    /// A text window became a one-tab tabbed window.
    WrappedInPlace,
    /// A tab moved from one tabbed window straight into another.
    Move,
    /// Tabs changed order within one window.
    Reorder,
}

/// What a move did, in the terms a frontend needs to follow it: which
/// window holds the content now, which renderer id it had and has (for
/// scroll-state migration), which windows changed, and which appeared or
/// vanished (for shell-key reconciliation and placement).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabMoveReceipt {
    pub kind: TabMoveKind,
    /// Window now holding the moved content.
    pub window: String,
    /// The tab now holding it, if it lives in a tabbed window.
    pub tab_id: Option<TabId>,
    pub old_scroll_id: String,
    pub new_scroll_id: String,
    /// Every window whose definition or content changed.
    pub affected_windows: Vec<String>,
    /// `(window, from, to)` for every `buffer_size` the move changed.
    pub capacity_changed: Vec<(String, usize, usize)>,
    /// A window that no longer exists after this move.
    pub removed_window: Option<String>,
    /// A window that exists only because of this move.
    pub created_window: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabMoveError {
    NoSuchWindow(String),
    NoSuchTab {
        window: String,
        tab_id: TabId,
    },
    NotTabbed(String),
    NotText(String),
    SameWindow(String),
    /// Undo of a join: the source's original name is in use again.
    NameTaken(String),
    /// Undo: the layout defs changed since the move.
    LayoutChanged,
    NothingToUndo,
    /// A layout operation failed (message from the layout layer).
    Layout(String),
}

impl fmt::Display for TabMoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchWindow(name) => write!(f, "no window named '{name}'"),
            Self::NoSuchTab { window, tab_id } => {
                write!(f, "window '{window}' has no tab with id {tab_id}")
            }
            Self::NotTabbed(name) => write!(f, "'{name}' is not a tabbed text window"),
            Self::NotText(name) => write!(f, "'{name}' is not a text window"),
            Self::SameWindow(name) => write!(f, "cannot join '{name}' into itself"),
            Self::NameTaken(name) => write!(f, "a window named '{name}' already exists"),
            Self::LayoutChanged => write!(f, "can't undo — the layout changed"),
            Self::NothingToUndo => write!(f, "nothing to undo"),
            Self::Layout(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for TabMoveError {}

/// The snapshot each move leaves behind so its inverse can restore the
/// original state exactly — original definitions, the tab's position, and
/// the selection that was showing.
#[derive(Clone, Debug)]
pub enum UndoTabMove {
    TearOut {
        target: String,
        tab_def: TabbedTextTab,
        index: usize,
        prev_active: Option<TabId>,
        new_window: String,
    },
    ConvertedInPlace {
        window: String,
        tabbed_def: WindowDef,
    },
    WrappedInPlace {
        window: String,
        text_def: WindowDef,
    },
    Join {
        source_def: WindowDef,
        target: String,
        tab_id: TabId,
        prev_active: Option<TabId>,
        prev_buffer_size: usize,
    },
    Reorder {
        window: String,
        tab_ids_before: Vec<TabId>,
        prev_active: Option<TabId>,
    },
    Move {
        /// The source window's whole definition before the move (still
        /// holding the tab); re-inserted wholesale when the move emptied
        /// and removed the window.
        source_def: WindowDef,
        source_removed: bool,
        /// The tab as it was in the source, and where.
        tab_def: TabbedTextTab,
        source_index: usize,
        prev_active_source: Option<TabId>,
        target: String,
        /// The id the tab was given in the target.
        target_tab_id: TabId,
        prev_active_target: Option<TabId>,
        prev_target_buffer_size: usize,
    },
}

#[derive(Clone, Debug)]
pub struct UndoRecord {
    pub op: UndoTabMove,
    /// Hash of every layout def right after the move; undo applies only
    /// while the current defs still hash to this.
    pub layout_hash: u64,
    /// Opaque placement the GUI stashes (rect, zone, anchors, group …) for
    /// the window this move removed or re-placed. Core never reads it; it
    /// is handed back on undo. The TUI leaves it `Null`.
    pub gui: serde_json::Value,
}

/// What an undo produced: the receipt describing the restoring move, plus
/// the GUI placement blob that was stashed with the record.
#[derive(Clone, Debug, PartialEq)]
pub struct UndoOutcome {
    pub receipt: TabMoveReceipt,
    pub gui: serde_json::Value,
}

/// Renderer id of a standalone text window's content.
pub fn scroll_id_for_window(window: &str) -> String {
    window.to_string()
}

/// Renderer id of one tab's content, keyed by the tab's stable id so a
/// removal or reorder never hands one tab's scroll state to a neighbour.
pub fn scroll_id_for_tab(window: &str, tab_id: TabId) -> String {
    format!("{window}::tab#{tab_id}")
}

/// Capacity a window may hold after a move: the definition's own value,
/// raised to fit `len` lines already present. `0` (unlimited) stays `0`.
/// `add_line` prunes at most one line per push, so a buffer longer than its
/// `max_lines` would otherwise stay oversized for good; the move keeps the
/// invariant `len <= max_lines` by growing the cap, never by trimming.
pub fn reconcile_capacity(def_buffer_size: usize, len: usize) -> usize {
    if def_buffer_size == 0 {
        0
    } else {
        def_buffer_size.max(len)
    }
}

/// The one way a lifted buffer enters a window. Sets the routing stream
/// list, title and capacity; leaves `lines`, `scroll_offset` and
/// `generation` exactly as they were.
pub fn install_text_buffer(
    content: &mut TextContent,
    streams: &[String],
    title: &str,
    max_lines: usize,
) {
    content.streams = streams.to_vec();
    content.title = title.to_string();
    content.max_lines = max_lines;
}

/// Copy the parts of a window's `base` that make it *look* like itself —
/// border, colours, title chrome — from `from` onto `to`, leaving name,
/// geometry, binding and visibility alone.
fn copy_appearance(from: &crate::config::WindowBase, to: &mut crate::config::WindowBase) {
    to.show_border = from.show_border;
    to.border_style = from.border_style.clone();
    to.border_sides = from.border_sides.clone();
    to.border_color = from.border_color.clone();
    to.placement.show_title = from.placement.show_title;
    to.placement.title_position = from.placement.title_position.clone();
    to.background_color = from.background_color.clone();
    to.text_color = from.text_color.clone();
    to.transparent_background = from.transparent_background;
}

impl AppCore {
    // ---- shared plumbing ---------------------------------------------------

    /// Hash of every layout definition (ids and counters included). Any def
    /// mutation through any path changes it, which is what lets undo refuse
    /// safely without every editor knowing undo exists.
    pub fn layout_hash(&self) -> u64 {
        let bytes = serde_json::to_vec(&self.layout.windows).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        hasher.finish()
    }

    /// Forget the pending undo. Called from `init_windows` (every wholesale
    /// layout replacement rebuilds the buffers undo would restore onto).
    pub fn invalidate_tab_undo(&mut self) {
        self.last_tab_move = None;
    }

    /// Attach the GUI's placement snapshot to the record the last move left.
    pub fn set_tab_undo_gui(&mut self, gui: serde_json::Value) {
        if let Some(record) = self.last_tab_move.as_mut() {
            record.gui = gui;
        }
    }

    fn record_tab_undo(&mut self, op: UndoTabMove) {
        self.last_tab_move = Some(UndoRecord {
            op,
            layout_hash: self.layout_hash(),
            gui: serde_json::Value::Null,
        });
    }

    fn finish_tab_move(&mut self) {
        self.message_processor
            .update_text_stream_subscribers(&self.ui_state);
        self.schedule_layout_autosave();
        self.needs_render = true;
    }

    fn tabbed_def_mut(&mut self, name: &str) -> Result<&mut TabbedTextWidgetData, TabMoveError> {
        match self.layout.windows.iter_mut().find(|w| w.name() == name) {
            Some(WindowDef::TabbedText { data, .. }) => Ok(data),
            Some(_) => Err(TabMoveError::NotTabbed(name.to_string())),
            None => Err(TabMoveError::NoSuchWindow(name.to_string())),
        }
    }

    fn live_tabbed_mut(&mut self, name: &str) -> Result<&mut TabbedTextContent, TabMoveError> {
        match self.ui_state.windows.get_mut(name) {
            Some(window) => match &mut window.content {
                WindowContent::TabbedText(tabbed) => Ok(tabbed),
                _ => Err(TabMoveError::NotTabbed(name.to_string())),
            },
            None => Err(TabMoveError::NoSuchWindow(name.to_string())),
        }
    }

    fn live_text(&self, name: &str) -> Result<&TextContent, TabMoveError> {
        match self.ui_state.windows.get(name) {
            Some(window) => match &window.content {
                WindowContent::Text(content) => Ok(content),
                _ => Err(TabMoveError::NotText(name.to_string())),
            },
            None => Err(TabMoveError::NoSuchWindow(name.to_string())),
        }
    }

    /// Swap a window's kind without touching its name, geometry or
    /// placement: definition, `WindowState.widget_type` and content all
    /// change together, the widget-type index and routing are rebuilt, and
    /// the frontends are told to drop their typed caches for the name.
    fn retype_window_in_place(
        &mut self,
        name: &str,
        new_def: WindowDef,
        new_content: WindowContent,
    ) -> Result<(), TabMoveError> {
        let slot = self
            .layout
            .windows
            .iter_mut()
            .find(|w| w.name() == name)
            .ok_or_else(|| TabMoveError::NoSuchWindow(name.to_string()))?;
        let window = self
            .ui_state
            .windows
            .get_mut(name)
            .ok_or_else(|| TabMoveError::NoSuchWindow(name.to_string()))?;
        window.widget_type = WidgetType::from_str(new_def.widget_type());
        window.content = new_content;
        *slot = new_def;
        self.ui_state.rebuild_widget_index();
        self.ui_state.widgets_to_reset.push(name.to_string());
        self.finish_tab_move();
        Ok(())
    }

    // ---- the moves ---------------------------------------------------------

    /// Move one tab out of `window` into its own text window — or, when it
    /// is the last tab, turn `window` itself into a text window. Scrollback,
    /// scroll position and unread state travel with the tab.
    pub fn tear_out_tab(
        &mut self,
        window: &str,
        tab_id: TabId,
    ) -> Result<TabMoveReceipt, TabMoveError> {
        let global_ts = self.config.ui.timestamp_position;

        // Definition side: locate the tab, remember what undo needs.
        let (index, tab_def, buffer_size, base, tab_count, tabbed_def) = {
            let def = self
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == window)
                .ok_or_else(|| TabMoveError::NoSuchWindow(window.to_string()))?;
            let snapshot = def.clone();
            let WindowDef::TabbedText { base, data } = def else {
                return Err(TabMoveError::NotTabbed(window.to_string()));
            };
            data.normalize_tab_ids();
            let index = data.index_of_tab(tab_id).ok_or(TabMoveError::NoSuchTab {
                window: window.to_string(),
                tab_id,
            })?;
            (
                index,
                data.tabs[index].clone(),
                data.buffer_size,
                base.clone(),
                data.tabs.len(),
                snapshot,
            )
        };

        // Live side: lift the buffer before anything can drop it.
        let (lifted, prev_active) = {
            let tabbed = self.live_tabbed_mut(window)?;
            let prev_active = tabbed.active_tab_id();
            let live_index = tabbed.index_of(tab_id).ok_or(TabMoveError::NoSuchTab {
                window: window.to_string(),
                tab_id,
            })?;
            (tabbed.tabs[live_index].clone(), prev_active)
        };
        let streams = tab_def.get_streams();
        let spec = tab_def.to_spec(global_ts);
        let old_scroll_id = scroll_id_for_tab(window, tab_id);

        if tab_count == 1 {
            // Last tab: the window becomes a text window in place.
            let title = base.title.clone().unwrap_or_else(|| window.to_string());
            let mut buffer = lifted.content;
            install_text_buffer(&mut buffer, &streams, &title, buffer_size);
            buffer.compact = false;
            let new_def = WindowDef::Text {
                base,
                data: TextWidgetData {
                    streams,
                    buffer_size,
                    wordwrap: true,
                    show_timestamps: spec.show_timestamps,
                    timestamp_position: tab_def.timestamp_position,
                    compact: false,
                },
            };
            self.retype_window_in_place(window, new_def, WindowContent::Text(buffer))?;
            self.record_tab_undo(UndoTabMove::ConvertedInPlace {
                window: window.to_string(),
                tabbed_def,
            });
            return Ok(TabMoveReceipt {
                kind: TabMoveKind::ConvertedInPlace,
                window: window.to_string(),
                tab_id: None,
                old_scroll_id,
                new_scroll_id: scroll_id_for_window(window),
                affected_windows: vec![window.to_string()],
                capacity_changed: Vec::new(),
                removed_window: None,
                created_window: None,
            });
        }

        // Ordinary tear-out: drop the tab from the def, seed a new window.
        self.tabbed_def_mut(window)?.tabs.remove(index);
        let seeded = self
            .layout
            .add_window(CUSTOM_TEXT_SEED)
            .map_err(|e| TabMoveError::Layout(e.to_string()))?;
        let new_index = self
            .layout
            .windows
            .iter()
            .position(|w| w.name() == seeded)
            .ok_or_else(|| TabMoveError::Layout(format!("seed '{seeded}' vanished")))?;
        let new_name = {
            let WindowDef::Text {
                base: new_base,
                data: new_data,
            } = &mut self.layout.windows[new_index]
            else {
                return Err(TabMoveError::Layout(format!(
                    "seed '{CUSTOM_TEXT_SEED}' is not a text window"
                )));
            };
            new_data.streams = streams.clone();
            new_data.buffer_size = buffer_size;
            new_data.wordwrap = true;
            new_data.show_timestamps = spec.show_timestamps;
            new_data.timestamp_position = tab_def.timestamp_position;
            new_data.compact = false;
            new_base.title = Some(tab_def.name.clone());
            copy_appearance(&base, new_base);
            new_base.name.clone()
        };
        let new_def = self.layout.windows[new_index].clone();
        self.add_new_window(&new_def, 0, 0);

        // Install the lifted buffer into the new window.
        {
            let target = self
                .ui_state
                .windows
                .get_mut(&new_name)
                .ok_or_else(|| TabMoveError::Layout(format!("'{new_name}' was not created")))?;
            let mut buffer = lifted.content;
            install_text_buffer(&mut buffer, &streams, &tab_def.name, buffer_size);
            buffer.compact = false;
            target.content = WindowContent::Text(buffer);
        }

        // Rebuild the source's live tabs, keeping what was showing.
        self.sync_tabbed_window_tabs_keep(window, prev_active);
        self.finish_tab_move();
        self.record_tab_undo(UndoTabMove::TearOut {
            target: window.to_string(),
            tab_def,
            index,
            prev_active,
            new_window: new_name.clone(),
        });
        Ok(TabMoveReceipt {
            kind: TabMoveKind::TearOut,
            window: new_name.clone(),
            tab_id: None,
            old_scroll_id,
            new_scroll_id: scroll_id_for_window(&new_name),
            affected_windows: vec![window.to_string(), new_name.clone()],
            capacity_changed: Vec::new(),
            removed_window: None,
            created_window: Some(new_name),
        })
    }

    /// Move a text window into `target` as a new tab (at `insert_at`, or
    /// the end) and remove it. The tab keeps the window's scrollback; the
    /// target's capacity grows if needed so nothing is trimmed.
    pub fn join_window_as_tab(
        &mut self,
        source: &str,
        target: &str,
        insert_at: Option<usize>,
    ) -> Result<TabMoveReceipt, TabMoveError> {
        if source == target {
            return Err(TabMoveError::SameWindow(source.to_string()));
        }

        // Source definition and live buffer.
        let source_def = self
            .layout
            .windows
            .iter()
            .find(|w| w.name() == source)
            .cloned()
            .ok_or_else(|| TabMoveError::NoSuchWindow(source.to_string()))?;
        let WindowDef::Text {
            base: source_base,
            data: source_data,
        } = &source_def
        else {
            return Err(TabMoveError::NotText(source.to_string()));
        };
        let lifted = self.live_text(source)?.clone();
        let streams = source_data.streams.clone();
        let tab_name = source_base
            .title
            .clone()
            .unwrap_or_else(|| source.to_string());
        let show_timestamps = source_data.show_timestamps;
        let timestamp_position = source_data.timestamp_position;

        // Target: allocate the tab, reconcile capacity.
        let prev_active = self.live_tabbed_mut(target)?.active_tab_id();
        let (tab_id, prev_buffer_size, new_buffer_size) = {
            let data = self.tabbed_def_mut(target)?;
            data.normalize_tab_ids();
            let tab_id = data.alloc_tab_id();
            let at = insert_at.unwrap_or(data.tabs.len()).min(data.tabs.len());
            data.tabs.insert(
                at,
                TabbedTextTab {
                    id: tab_id,
                    name: tab_name.clone(),
                    stream: None,
                    streams: streams.clone(),
                    show_timestamps: Some(show_timestamps),
                    ignore_activity: None,
                    timestamp_position,
                },
            );
            let prev = data.buffer_size;
            data.buffer_size = reconcile_capacity(prev, lifted.lines.len());
            (tab_id, prev, data.buffer_size)
        };

        // The source ceases to exist.
        self.layout.windows.retain(|w| w.name() != source);
        self.remove_window(source);

        // Materialise the tab, then hand it the lifted buffer.
        self.sync_tabbed_window_tabs_keep(target, Some(tab_id));
        {
            let tabbed = self.live_tabbed_mut(target)?;
            let live_index = tabbed
                .index_of(tab_id)
                .ok_or_else(|| TabMoveError::Layout(format!("tab {tab_id} was not created")))?;
            let mut buffer = lifted;
            install_text_buffer(&mut buffer, &streams, &tab_name, new_buffer_size);
            buffer.compact = false;
            tabbed.tabs[live_index].content = buffer;
        }
        self.finish_tab_move();
        self.record_tab_undo(UndoTabMove::Join {
            source_def,
            target: target.to_string(),
            tab_id,
            prev_active,
            prev_buffer_size,
        });

        let capacity_changed = if new_buffer_size != prev_buffer_size {
            vec![(target.to_string(), prev_buffer_size, new_buffer_size)]
        } else {
            Vec::new()
        };
        Ok(TabMoveReceipt {
            kind: TabMoveKind::Join,
            window: target.to_string(),
            tab_id: Some(tab_id),
            old_scroll_id: scroll_id_for_window(source),
            new_scroll_id: scroll_id_for_tab(target, tab_id),
            affected_windows: vec![target.to_string()],
            capacity_changed,
            removed_window: Some(source.to_string()),
            created_window: None,
        })
    }

    /// Turn a text window into a one-tab tabbed window in place.
    pub fn wrap_text_as_tabbed(&mut self, window: &str) -> Result<TabMoveReceipt, TabMoveError> {
        let text_def = self
            .layout
            .windows
            .iter()
            .find(|w| w.name() == window)
            .cloned()
            .ok_or_else(|| TabMoveError::NoSuchWindow(window.to_string()))?;
        let WindowDef::Text { base, data } = &text_def else {
            return Err(TabMoveError::NotText(window.to_string()));
        };
        let lifted = self.live_text(window)?.clone();
        let tab_name = base.title.clone().unwrap_or_else(|| window.to_string());

        let mut tabbed_data = TabbedTextWidgetData {
            tabs: vec![TabbedTextTab {
                id: 0,
                name: tab_name.clone(),
                stream: None,
                streams: data.streams.clone(),
                show_timestamps: Some(data.show_timestamps),
                ignore_activity: None,
                timestamp_position: data.timestamp_position,
            }],
            next_tab_id: 0,
            buffer_size: data.buffer_size,
            tab_bar_position: "top".to_string(),
            tab_separator: false,
            tab_active_color: None,
            tab_inactive_color: None,
            tab_unread_color: None,
            tab_unread_prefix: None,
        };
        tabbed_data.normalize_tab_ids();
        let tab_id = tabbed_data.tabs[0].id;

        let mut content = TabbedTextContent::from_specs(
            tabbed_data.tab_specs(self.config.ui.timestamp_position),
            data.buffer_size,
        );
        let mut buffer = lifted;
        install_text_buffer(&mut buffer, &data.streams, &tab_name, data.buffer_size);
        content.tabs[0].content = buffer;

        let new_def = WindowDef::TabbedText {
            base: base.clone(),
            data: tabbed_data,
        };
        self.retype_window_in_place(window, new_def, WindowContent::TabbedText(content))?;
        self.record_tab_undo(UndoTabMove::WrappedInPlace {
            window: window.to_string(),
            text_def,
        });
        Ok(TabMoveReceipt {
            kind: TabMoveKind::WrappedInPlace,
            window: window.to_string(),
            tab_id: Some(tab_id),
            old_scroll_id: scroll_id_for_window(window),
            new_scroll_id: scroll_id_for_tab(window, tab_id),
            affected_windows: vec![window.to_string()],
            capacity_changed: Vec::new(),
            removed_window: None,
            created_window: None,
        })
    }

    /// Move one tab from `source` straight into `target` (spec §1, the
    /// tab-to-tab gesture). The tab keeps its buffer, scroll position and
    /// settings and gets a fresh id in the target. When it was the source's
    /// last tab the source window ceases to exist, as a join's source does.
    pub fn move_tab(
        &mut self,
        source: &str,
        tab_id: TabId,
        target: &str,
        insert_at: Option<usize>,
    ) -> Result<TabMoveReceipt, TabMoveError> {
        if source == target {
            return Err(TabMoveError::SameWindow(source.to_string()));
        }
        // Both ends must be tabbed before anything is touched.
        let (source_def, source_index, tab_def, source_count) = {
            let def = self
                .layout
                .windows
                .iter_mut()
                .find(|w| w.name() == source)
                .ok_or_else(|| TabMoveError::NoSuchWindow(source.to_string()))?;
            let WindowDef::TabbedText { data, .. } = def else {
                return Err(TabMoveError::NotTabbed(source.to_string()));
            };
            data.normalize_tab_ids();
            let index = data.index_of_tab(tab_id).ok_or(TabMoveError::NoSuchTab {
                window: source.to_string(),
                tab_id,
            })?;
            let tab = data.tabs[index].clone();
            let count = data.tabs.len();
            (def.clone(), index, tab, count)
        };
        self.tabbed_def_mut(target)?;

        // Lift the live tab.
        let (lifted, prev_active_source) = {
            let tabbed = self.live_tabbed_mut(source)?;
            let prev_active = tabbed.active_tab_id();
            let live_index = tabbed.index_of(tab_id).ok_or(TabMoveError::NoSuchTab {
                window: source.to_string(),
                tab_id,
            })?;
            (tabbed.tabs[live_index].clone(), prev_active)
        };
        let streams = tab_def.get_streams();

        // Target: allocate a tab, reconcile capacity.
        let prev_active_target = self.live_tabbed_mut(target)?.active_tab_id();
        let (target_tab_id, prev_target_buffer_size, target_buffer_size) = {
            let data = self.tabbed_def_mut(target)?;
            data.normalize_tab_ids();
            let new_id = data.alloc_tab_id();
            let at = insert_at.unwrap_or(data.tabs.len()).min(data.tabs.len());
            let mut moved = tab_def.clone();
            moved.id = new_id;
            data.tabs.insert(at, moved);
            let prev = data.buffer_size;
            data.buffer_size = reconcile_capacity(prev, lifted.content.lines.len());
            (new_id, prev, data.buffer_size)
        };

        // Source: drop the tab; an emptied window goes away entirely.
        let source_removed = source_count == 1;
        if source_removed {
            self.layout.windows.retain(|w| w.name() != source);
            self.remove_window(source);
        } else {
            self.tabbed_def_mut(source)?.tabs.remove(source_index);
            self.sync_tabbed_window_tabs_keep(source, prev_active_source);
        }

        // Materialise the tab in the target, hand it the lifted buffer.
        self.sync_tabbed_window_tabs_keep(target, Some(target_tab_id));
        {
            let tabbed = self.live_tabbed_mut(target)?;
            let live_index = tabbed.index_of(target_tab_id).ok_or_else(|| {
                TabMoveError::Layout(format!("tab {target_tab_id} was not created"))
            })?;
            let mut buffer = lifted.content;
            install_text_buffer(&mut buffer, &streams, &tab_def.name, target_buffer_size);
            tabbed.tabs[live_index].content = buffer;
        }
        self.finish_tab_move();
        self.record_tab_undo(UndoTabMove::Move {
            source_def,
            source_removed,
            tab_def,
            source_index,
            prev_active_source,
            target: target.to_string(),
            target_tab_id,
            prev_active_target,
            prev_target_buffer_size,
        });

        let capacity_changed = if target_buffer_size != prev_target_buffer_size {
            vec![(
                target.to_string(),
                prev_target_buffer_size,
                target_buffer_size,
            )]
        } else {
            Vec::new()
        };
        Ok(TabMoveReceipt {
            kind: TabMoveKind::Move,
            window: target.to_string(),
            tab_id: Some(target_tab_id),
            old_scroll_id: scroll_id_for_tab(source, tab_id),
            new_scroll_id: scroll_id_for_tab(target, target_tab_id),
            affected_windows: vec![source.to_string(), target.to_string()],
            capacity_changed,
            removed_window: source_removed.then(|| source.to_string()),
            created_window: None,
        })
    }

    /// Reorder `window`'s tabs to `tab_ids` (a permutation of the current
    /// ids). Buffers and the active tab follow their ids.
    pub fn reorder_tabs(
        &mut self,
        window: &str,
        tab_ids: &[TabId],
    ) -> Result<TabMoveReceipt, TabMoveError> {
        let prev_active = self.live_tabbed_mut(window)?.active_tab_id();
        let tab_ids_before = {
            let data = self.tabbed_def_mut(window)?;
            data.normalize_tab_ids();
            let before: Vec<TabId> = data.tabs.iter().map(|t| t.id).collect();
            let mut sorted_new = tab_ids.to_vec();
            sorted_new.sort_unstable();
            let mut sorted_old = before.clone();
            sorted_old.sort_unstable();
            if sorted_new != sorted_old {
                return Err(TabMoveError::Layout(
                    "reorder must list every current tab id exactly once".to_string(),
                ));
            }
            let mut by_id: std::collections::HashMap<TabId, TabbedTextTab> =
                data.tabs.drain(..).map(|t| (t.id, t)).collect();
            data.tabs = tab_ids.iter().filter_map(|id| by_id.remove(id)).collect();
            before
        };
        self.sync_tabbed_window_tabs_keep(window, prev_active);
        self.finish_tab_move();
        self.record_tab_undo(UndoTabMove::Reorder {
            window: window.to_string(),
            tab_ids_before,
            prev_active,
        });
        let active_scroll = prev_active
            .map(|id| scroll_id_for_tab(window, id))
            .unwrap_or_else(|| scroll_id_for_window(window));
        Ok(TabMoveReceipt {
            kind: TabMoveKind::Reorder,
            window: window.to_string(),
            tab_id: prev_active,
            old_scroll_id: active_scroll.clone(),
            new_scroll_id: active_scroll,
            affected_windows: vec![window.to_string()],
            capacity_changed: Vec::new(),
            removed_window: None,
            created_window: None,
        })
    }

    // ---- undo --------------------------------------------------------------

    /// Restore the state before the last tab move. Refuses — and forgets the
    /// record — if the layout defs changed since, or if the windows the
    /// record names are no longer what it expects.
    pub fn undo_tab_move(&mut self) -> Result<UndoOutcome, TabMoveError> {
        let record = self
            .last_tab_move
            .take()
            .ok_or(TabMoveError::NothingToUndo)?;
        if self.layout_hash() != record.layout_hash {
            return Err(TabMoveError::LayoutChanged);
        }
        let global_ts = self.config.ui.timestamp_position;
        let gui = record.gui;

        let receipt = match record.op {
            UndoTabMove::TearOut {
                target,
                tab_def,
                index,
                prev_active,
                new_window,
            } => {
                let lifted = self.live_text(&new_window)?.clone();
                let streams = tab_def.get_streams();
                let buffer_size = {
                    let data = self.tabbed_def_mut(&target)?;
                    data.normalize_tab_ids();
                    if data.index_of_tab(tab_def.id).is_some() {
                        return Err(TabMoveError::Layout(format!(
                            "tab {} is already back in '{target}'",
                            tab_def.id
                        )));
                    }
                    let at = index.min(data.tabs.len());
                    data.tabs.insert(at, tab_def.clone());
                    if data.next_tab_id <= tab_def.id {
                        data.next_tab_id = tab_def.id + 1;
                    }
                    data.buffer_size
                };
                self.layout.windows.retain(|w| w.name() != new_window);
                self.remove_window(&new_window);
                self.sync_tabbed_window_tabs_keep(&target, prev_active);
                {
                    let tabbed = self.live_tabbed_mut(&target)?;
                    let live_index = tabbed.index_of(tab_def.id).ok_or_else(|| {
                        TabMoveError::Layout(format!("tab {} was not restored", tab_def.id))
                    })?;
                    let mut buffer = lifted;
                    install_text_buffer(&mut buffer, &streams, &tab_def.name, buffer_size);
                    tabbed.tabs[live_index].content = buffer;
                }
                TabMoveReceipt {
                    kind: TabMoveKind::Join,
                    window: target.clone(),
                    tab_id: Some(tab_def.id),
                    old_scroll_id: scroll_id_for_window(&new_window),
                    new_scroll_id: scroll_id_for_tab(&target, tab_def.id),
                    affected_windows: vec![target],
                    capacity_changed: Vec::new(),
                    removed_window: Some(new_window),
                    created_window: None,
                }
            }

            UndoTabMove::ConvertedInPlace { window, tabbed_def } => {
                let lifted = self.live_text(&window)?.clone();
                let WindowDef::TabbedText { data, .. } = &tabbed_def else {
                    return Err(TabMoveError::Layout(
                        "saved definition is not a tabbed window".to_string(),
                    ));
                };
                let tab = data.tabs.first().ok_or_else(|| {
                    TabMoveError::Layout("saved tabbed window has no tabs".to_string())
                })?;
                let mut content =
                    TabbedTextContent::from_specs(data.tab_specs(global_ts), data.buffer_size);
                let mut buffer = lifted;
                install_text_buffer(&mut buffer, &tab.get_streams(), &tab.name, data.buffer_size);
                content.tabs[0].content = buffer;
                let tab_id = tab.id;
                self.retype_window_in_place(
                    &window,
                    tabbed_def.clone(),
                    WindowContent::TabbedText(content),
                )?;
                TabMoveReceipt {
                    kind: TabMoveKind::WrappedInPlace,
                    window: window.clone(),
                    tab_id: Some(tab_id),
                    old_scroll_id: scroll_id_for_window(&window),
                    new_scroll_id: scroll_id_for_tab(&window, tab_id),
                    affected_windows: vec![window],
                    capacity_changed: Vec::new(),
                    removed_window: None,
                    created_window: None,
                }
            }

            UndoTabMove::WrappedInPlace { window, text_def } => {
                let (lifted, tab_id) = {
                    let tabbed = self.live_tabbed_mut(&window)?;
                    let tab = tabbed.tabs.first().ok_or_else(|| {
                        TabMoveError::Layout("wrapped window has no tab".to_string())
                    })?;
                    (tab.content.clone(), tab.definition.id)
                };
                let WindowDef::Text { base, data } = &text_def else {
                    return Err(TabMoveError::Layout(
                        "saved definition is not a text window".to_string(),
                    ));
                };
                let title = base.title.clone().unwrap_or_else(|| window.clone());
                let mut buffer = lifted;
                install_text_buffer(&mut buffer, &data.streams, &title, data.buffer_size);
                self.retype_window_in_place(
                    &window,
                    text_def.clone(),
                    WindowContent::Text(buffer),
                )?;
                TabMoveReceipt {
                    kind: TabMoveKind::ConvertedInPlace,
                    window: window.clone(),
                    tab_id: None,
                    old_scroll_id: scroll_id_for_tab(&window, tab_id),
                    new_scroll_id: scroll_id_for_window(&window),
                    affected_windows: vec![window],
                    capacity_changed: Vec::new(),
                    removed_window: None,
                    created_window: None,
                }
            }

            UndoTabMove::Join {
                source_def,
                target,
                tab_id,
                prev_active,
                prev_buffer_size,
            } => {
                let source = source_def.name().to_string();
                if self.layout.windows.iter().any(|w| w.name() == source)
                    || self.ui_state.windows.contains_key(&source)
                {
                    return Err(TabMoveError::NameTaken(source));
                }
                // Lift the buffer, note what the survivors hold.
                let (lifted, survivors_max) = {
                    let tabbed = self.live_tabbed_mut(&target)?;
                    let live_index = tabbed.index_of(tab_id).ok_or(TabMoveError::NoSuchTab {
                        window: target.clone(),
                        tab_id,
                    })?;
                    let lifted = tabbed.tabs[live_index].content.clone();
                    let survivors_max = tabbed
                        .tabs
                        .iter()
                        .filter(|t| t.definition.id != tab_id)
                        .map(|t| t.content.lines.len())
                        .max()
                        .unwrap_or(0);
                    (lifted, survivors_max)
                };
                // Target: drop the tab, restore capacity to what survivors need.
                let (target_cap_before, target_cap_after) = {
                    let data = self.tabbed_def_mut(&target)?;
                    let at = data.index_of_tab(tab_id).ok_or(TabMoveError::NoSuchTab {
                        window: target.clone(),
                        tab_id,
                    })?;
                    data.tabs.remove(at);
                    let before = data.buffer_size;
                    data.buffer_size = reconcile_capacity(prev_buffer_size, survivors_max);
                    (before, data.buffer_size)
                };
                // Source: recreate from its saved definition, capacity
                // reconciled against what it now holds.
                let mut restored = source_def;
                let (streams, title, source_cap_before, source_cap_after) = {
                    let WindowDef::Text { base, data } = &mut restored else {
                        return Err(TabMoveError::Layout(
                            "saved definition is not a text window".to_string(),
                        ));
                    };
                    let before = data.buffer_size;
                    data.buffer_size = reconcile_capacity(before, lifted.lines.len());
                    (
                        data.streams.clone(),
                        base.title.clone().unwrap_or_else(|| source.clone()),
                        before,
                        data.buffer_size,
                    )
                };
                self.layout.windows.push(restored.clone());
                self.add_new_window(&restored, 0, 0);
                {
                    let window = self.ui_state.windows.get_mut(&source).ok_or_else(|| {
                        TabMoveError::Layout(format!("'{source}' was not recreated"))
                    })?;
                    let mut buffer = lifted;
                    install_text_buffer(&mut buffer, &streams, &title, source_cap_after);
                    window.content = WindowContent::Text(buffer);
                }
                self.sync_tabbed_window_tabs_keep(&target, prev_active);

                let mut capacity_changed = Vec::new();
                if target_cap_after != target_cap_before {
                    capacity_changed.push((target.clone(), target_cap_before, target_cap_after));
                }
                if source_cap_after != source_cap_before {
                    capacity_changed.push((source.clone(), source_cap_before, source_cap_after));
                }
                TabMoveReceipt {
                    kind: TabMoveKind::TearOut,
                    window: source.clone(),
                    tab_id: None,
                    old_scroll_id: scroll_id_for_tab(&target, tab_id),
                    new_scroll_id: scroll_id_for_window(&source),
                    affected_windows: vec![target, source.clone()],
                    capacity_changed,
                    removed_window: None,
                    created_window: Some(source),
                }
            }

            UndoTabMove::Move {
                source_def,
                source_removed,
                tab_def,
                source_index,
                prev_active_source,
                target,
                target_tab_id,
                prev_active_target,
                prev_target_buffer_size,
            } => {
                let source = source_def.name().to_string();
                if source_removed
                    && (self.layout.windows.iter().any(|w| w.name() == source)
                        || self.ui_state.windows.contains_key(&source))
                {
                    return Err(TabMoveError::NameTaken(source));
                }
                // Lift from the target, note what the survivors hold.
                let (lifted, survivors_max) = {
                    let tabbed = self.live_tabbed_mut(&target)?;
                    let live_index =
                        tabbed
                            .index_of(target_tab_id)
                            .ok_or(TabMoveError::NoSuchTab {
                                window: target.clone(),
                                tab_id: target_tab_id,
                            })?;
                    let lifted = tabbed.tabs[live_index].content.clone();
                    let survivors_max = tabbed
                        .tabs
                        .iter()
                        .filter(|t| t.definition.id != target_tab_id)
                        .map(|t| t.content.lines.len())
                        .max()
                        .unwrap_or(0);
                    (lifted, survivors_max)
                };
                let (target_cap_before, target_cap_after) = {
                    let data = self.tabbed_def_mut(&target)?;
                    let at = data
                        .index_of_tab(target_tab_id)
                        .ok_or(TabMoveError::NoSuchTab {
                            window: target.clone(),
                            tab_id: target_tab_id,
                        })?;
                    data.tabs.remove(at);
                    let before = data.buffer_size;
                    data.buffer_size = reconcile_capacity(prev_target_buffer_size, survivors_max);
                    (before, data.buffer_size)
                };
                self.sync_tabbed_window_tabs_keep(&target, prev_active_target);

                // Source: recreate wholesale, or slot the tab back in.
                let streams = tab_def.get_streams();
                let (source_cap_before, source_cap_after) = if source_removed {
                    let mut restored = source_def;
                    let (before, after) = {
                        let WindowDef::TabbedText { data, .. } = &mut restored else {
                            return Err(TabMoveError::Layout(
                                "saved definition is not a tabbed window".to_string(),
                            ));
                        };
                        let before = data.buffer_size;
                        data.buffer_size = reconcile_capacity(before, lifted.lines.len());
                        (before, data.buffer_size)
                    };
                    self.layout.windows.push(restored.clone());
                    self.add_new_window(&restored, 0, 0);
                    (before, after)
                } else {
                    let data = self.tabbed_def_mut(&source)?;
                    if data.index_of_tab(tab_def.id).is_some() {
                        return Err(TabMoveError::Layout(format!(
                            "tab {} is already back in '{source}'",
                            tab_def.id
                        )));
                    }
                    let at = source_index.min(data.tabs.len());
                    data.tabs.insert(at, tab_def.clone());
                    let before = data.buffer_size;
                    data.buffer_size = reconcile_capacity(before, lifted.lines.len());
                    (before, data.buffer_size)
                };
                self.sync_tabbed_window_tabs_keep(&source, prev_active_source.or(Some(tab_def.id)));
                {
                    let tabbed = self.live_tabbed_mut(&source)?;
                    let live_index = tabbed.index_of(tab_def.id).ok_or_else(|| {
                        TabMoveError::Layout(format!("tab {} was not restored", tab_def.id))
                    })?;
                    let mut buffer = lifted;
                    install_text_buffer(&mut buffer, &streams, &tab_def.name, source_cap_after);
                    tabbed.tabs[live_index].content = buffer;
                }

                let mut capacity_changed = Vec::new();
                if target_cap_after != target_cap_before {
                    capacity_changed.push((target.clone(), target_cap_before, target_cap_after));
                }
                if source_cap_after != source_cap_before {
                    capacity_changed.push((source.clone(), source_cap_before, source_cap_after));
                }
                TabMoveReceipt {
                    kind: TabMoveKind::Move,
                    window: source.clone(),
                    tab_id: Some(tab_def.id),
                    old_scroll_id: scroll_id_for_tab(&target, target_tab_id),
                    new_scroll_id: scroll_id_for_tab(&source, tab_def.id),
                    affected_windows: vec![target, source.clone()],
                    capacity_changed,
                    removed_window: None,
                    created_window: source_removed.then_some(source),
                }
            }

            UndoTabMove::Reorder {
                window,
                tab_ids_before,
                prev_active,
            } => {
                let receipt = self.reorder_tabs(&window, &tab_ids_before)?;
                // Restore the selection the record remembers, not the one
                // the reorder just kept.
                if let Some(id) = prev_active {
                    let tabbed = self.live_tabbed_mut(&window)?;
                    tabbed.select_tab_id(id);
                }
                receipt
            }
        };

        // Undo is single-level: the record is consumed, never re-armed.
        self.last_tab_move = None;
        self.finish_tab_move();
        Ok(UndoOutcome { receipt, gui })
    }
}

#[cfg(test)]
mod tests;
