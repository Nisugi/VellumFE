# Tabbed Text: Tab Tear-Out and Window Join (GUI first)

## Context

A `tabbedtext` window holds several stream-fed tabs behind one strip. Users
want the interaction every tabbed UI has: **grab a tab and drag it out** to
become its own window, and **drag a standalone window onto a strip** to join
it as a tab. Today both conversions are only reachable by hand-editing tabs
in the Tab Editor, and neither preserves scrollback.

Two things the code makes clear that shape the whole design:

1. **There are two unrelated "tab" systems in the GUI.** Shell tabs
   (`TabKey`, zones, `group_tabs`, `detach_tab`) make every *window* a tab of
   the shell. `tabbedtext` tabs are a strip *inside one window*
   (`render_tabbed_text_tab_strip`). This spec is about the second. The shell
   machinery is reused for placement and drag plumbing, never as the target.
2. **A tab and a standalone text window are the same data.** `TabbedTextTab`
   (`src/config/widgets.rs:486`) and `TextWidgetData` (`:383`) both reduce to
   `{streams, show_timestamps, timestamp_position, buffer_size}`, and both
   carry a `TextContent` buffer at runtime (`src/data/widget.rs:33`, `:552`).
   Tear-out and join are **moves**, not conversions — *provided the buffer
   moves too* (see Current State: the existing sync path drops it).

**Owner decisions (fixed, 2026-09-13):**
- GUI first. The core transactions are frontend-agnostic; a TUI gesture is a
  later session.
- A torn-out tab becomes a **zone-placed window in the shell** — not a
  detached OS viewport.
- **Plain drag on the tab** tears out (no modifier). Alt+drag stays the
  whole-window move gesture; the two never start on the same surface.
- The torn-out window **keeps the source window's size**.
- Join is armed by **dwelling** over a target strip during an Alt+drag.
- **Scrollback survives** both directions. Non-negotiable.
- Phase 3 polish (drag ghost, insertion caret, strip highlight) is in scope,
  not a stretch goal.

**Review resolutions (2026-09-13, fifth pass):** every buffer install copies
the resolved stream list into `TextContent.streams`; shell-key
reconciliation runs after every transaction and undo for every affected
window, driven by main-subscription (the actual key rule), not widget type;
undo of a join reconciles the recreated source's capacity too; join and its
undo carry an explicit GUI placement snapshot/restore built on
`forget_tab_state`.

## Current State (verified 2026-09-13)

### Data and persistence
- `WindowDef::Text { base, data: TextWidgetData }` /
  `WindowDef::TabbedText { base, data: TabbedTextWidgetData }`
  (`src/config/window_def.rs:19,27`). `TabbedTextTab` has `stream: Option`
  (legacy) and `streams: Vec`, resolved by `get_streams()` (`widgets.rs:
  488-493`). `TextWidgetData` adds `wordwrap` and `compact`.
- Runtime: `WindowState { widget_type, content, .. }` (`data/window.rs:11`);
  `TabbedTextContent { tabs: Vec<TabState>, active_tab_index }`;
  `TabState { definition: TabDefinition, content: TextContent, has_unread }`
  (`data/widget.rs:552-563`). `TextContent` has its own `streams: Vec`
  (`:45`).
- **Two stream lists, two readers.** The subscriber map
  (`core/messages/routing.rs:313-322`) reads `content.streams` for
  `WindowContent::Text` but `tab.definition.streams` for `TabbedText`. A
  tab's `TextContent.streams` is not what routes it and may be empty
  (`TabbedTextContent::new` builds it from the name, not the streams). Any
  `TabbedText → Text` move that installs the buffer without copying
  `definition.streams` into `content.streams` silently unsubscribes it.
- `UiState.widget_type_index` (`data/ui_state.rs:19`), rebuilt by
  `rebuild_widget_index` (`:1209`); `UiState`'s own insert/remove helpers
  call it (`:1195,1202`).
- Persistence recipe (Tab Editor, `editors/tabs.rs:100`): mutate
  `layout.windows` → `sync_tabbed_window_tabs` (`windows.rs:890`) →
  `schedule_layout_autosave`.
- **Scrollback hazard:** `update_tabs` (`widget.rs:677`) drains into a
  by-name map and drops removed tabs; `add_new_window` builds fresh content.
- **No tab identity; names not unique** (`apply_tab_editor` checks empty
  only).
- **Def→runtime entry points that bypass load:** Tab Editor
  (`editors/tabs.rs:40,50`), `Layout::add_window` (`config/layout.rs:911`),
  `add_new_window`/`init_windows` (`windows.rs:158,564`).
- **Wholesale replacement paths** (none call autosave; all end in
  `init_windows`, which rebuilds every buffer): `layout.rs:338`,
  `layout.rs:1026`, `persistence.rs:684`.
- Active tab only clamped across `update_tabs`.
- `add_line` (`widget.rs:752`) pops **at most one** per push; `0` = unlimited.
- GUI reading position: egui state keyed by `scroll_id`
  (`gui/app/widgets/text.rs`): temp `text_row_heights` (:1470),
  `text_scroll_area_id` (:1485), `text_scroll_view_h` (:1523),
  `text_scroll_pending` (:1586), `text_scroll_follow` (:1587); persisted
  `text_split_frac` (:2471-2517); `{id}~live` pane (:2426, :2611); history
  pane `ScrollArea` id derives from its parent `Ui` (:2449).
- TUI widget cache: per-name typed maps; `remove_widget_from_all_caches`
  (`widget_manager.rs:161`).
- GUI-side new-window policy: `create_custom_window`
  (`editors/custom_windows.rs:247`) — `Layout::add_window(CUSTOM_TEXT_SEED)`
  + `add_new_window`. Reuse.
- `remove_window` (`windows.rs:930`) rebuilds routing.
- `tests/template_characterization.rs:166` serialises seeds into the golden.

### GUI shell keys and placement (why moves re-key windows)
- `tab_key_for_window` (`gui/app/tabs.rs:59-69`): for `Text` **and**
  `TabbedText` alike, the key is the singleton `TabKey::TextMain` iff
  `is_main_stream_window` (`:135-151`) — name is `main`, **or** the content
  subscribes to `main`: `content.streams` for `Text`, any tab's definition
  for `TabbedText` — else `TabKey::TextByName { id: name }`. **Widget type
  does not enter the key.** Main-subscription does.
- `collect_available_tabs` (`:8-30`) resolves singleton collisions: canonical
  names claim first; a later window whose key is taken falls back to
  `TabKey::WindowByName { id: name }`.
- All placement state is keyed by `TabKey`: `main_window_rects`,
  `tab_zones`, `window_anchors` (+ sibling refs from other windows),
  `last_center_window_rects`, `sidebar_gap_above`, `no_title_tabs`,
  `tab_settings`, `hidden_tabs`, `detached_tabs`, group membership
  (`tab_groups`); `pending_zones` is keyed by window name.
- `forget_tab_state(key, name)` (`tabs.rs:405-424`) is the existing
  complete cleanup for a deleted window: dissolves its group, prunes
  sibling anchor refs to it, removes it from all eleven maps, sets
  `layout_dirty`.

### Layout ownership
- One window universe per character (`layout.toml`), shared for identity
  and routing; each frontend owns its arrangement (TUI: `base.row/col`;
  GUI: `layout_v1.json` + embedded `window_defs` copy). A GUI tear-out
  also changes the TUI's tabbed window — a persisted character-level edit,
  hence undo.

### Drag infrastructure
- Alt+drag: `GuiZoneDragState` (`zones.rs:461`), overlay `:1087`, drop
  `apply_zone_drop` `:755`. `press_became_drag` / `PRESS_DRAG_THRESHOLD =
  6.0` (`:120,136`). `member_rects` precedent (`tabs.rs:579`).
  `set_tab_zone` (`:687`). Dwell idiom: `controller_tuning.aim_dwell_ms`.
  Context menu: `GuiWindowMenuCommand`.
- Strip (`widgets/links_bars.rs:78`): early-returns when `tabs.len() < 2`;
  `Sense::click()`; scroll ids keyed by index (`widgets.rs:225`).

## Design

### 0. Stable tab identity

`TabId: u32`, unique per window, never reused; `0` = unassigned, never
valid. `TabbedTextTab.id` and `TabbedTextWidgetData.next_tab_id`, both
`#[serde(default)]`, `skip_serializing_if = "TabId::is_unassigned"` → seeds
serialise unchanged, **template golden byte-identical**.

`alloc_tab_id()` returns `next_tab_id` then increments (debug-asserted
`≥ 1`). `normalize_tab_ids()` is **reserve then repair**: (1) `next_tab_id =
max(next_tab_id, 1, max(nonzero ids) + 1)`; (2) walk tabs in order,
allocating for any id that is `0` or already seen. Called at every
def→runtime boundary: layout parse, `Layout::add_window`,
`add_new_window`/`init_windows`, top of `sync_tabbed_window_tabs`,
`apply_tab_editor` (new rows allocate, never `default()`).

`update_tabs` keys by `TabId`, gains `keep_active: Option<TabId>` and
reselects it; if gone, the **neighbour rule** (tab now at the removed index,
else last). Reselection clears `has_unread`. Scroll ids:
`"{window}::tab#{id}"`.

### 1. Core transactions (`src/core/app_core/state/tab_moves.rs`)

Single atomic methods on `AppCore`. Each returns:

```rust
pub struct TabMoveReceipt {
    pub kind: TabMoveKind,               // TearOut | ConvertedInPlace | Join | WrappedInPlace | Reorder
    pub window: String,                  // window now holding the moved content
    pub tab_id: Option<TabId>,
    pub old_scroll_id: String,
    pub new_scroll_id: String,
    pub affected_windows: Vec<String>,   // every window whose def or content changed
    pub capacity_changed: Vec<(String, usize, usize)>, // (window, from, to)
    pub removed_window: Option<String>,  // join: the source that no longer exists
    pub created_window: Option<String>,  // tear-out: the custom-* that now exists
}
```

**Buffer install — the one helper every branch uses.**
`install_text_buffer(content: &mut TextContent, streams: &[String], title,
max_lines)`: sets `content.streams = streams.to_vec()` (resolved via
`get_streams()`, so legacy `stream:` works), `title`, `max_lines`; leaves
`lines`, `scroll_offset`, `generation` untouched. **No branch installs a
buffer without it** — including `ConvertedInPlace`, where the buffer stays
in the same window but its reader switches from `definition.streams` to
`content.streams`. Test: a tab loaded from config (whose
`content.streams` starts empty) converts in place and still routes.

**Runtime retype — shared by both in-place conversions and their
inverses.** `retype_window_in_place(name, new_def, new_content)`: replace
the def; set `WindowState.widget_type` and `content`;
`ui_state.rebuild_widget_index()`; routing rebuild; autosave;
`needs_render`. Frontends: TUI `remove_widget_from_all_caches(name)`; GUI
runs §5 reconciliation (type does not re-key, but main-subscription might).

**`tear_out_tab(window, tab_id)`**
1. Resolve id → index. Snapshot for undo (§2) incl. `prev_active`.
2. Lift `TabState { content, has_unread }`; `streams = definition.
   get_streams()`.
3. Remove the tab from the def.
   - **Last tab → `ConvertedInPlace`**: `install_text_buffer(&mut buf,
     streams, name, buffer_size)`; `retype_window_in_place(window,
     Text { base: same, data: from tab }, Text(buf))`.
   - Otherwise → `TearOut`: `Layout::add_window(CUSTOM_TEXT_SEED)`; set
     `TextWidgetData { streams, buffer_size: source buffer_size, show_
     timestamps, timestamp_position, wordwrap: true, compact: false }`;
     title = tab name; copy source `base` appearance. `add_new_window`;
     `install_text_buffer` into it. `created_window = Some(name)`.
4. `sync_tabbed_window_tabs(source, keep_active = prev_active)`.
5. Routing rebuild, autosave, render. `affected_windows = [source,
   destination]`.

**`join_window_as_tab(source, target, insert_at)`**
1. Source `Text`, target `TabbedText`, distinct. Snapshot incl. target
   `prev_active`, `prev_buffer_size`, and the **full `source_def`**.
2. Lift the source buffer; `streams = source data.streams`.
3. `id = target.alloc_tab_id()`; insert the tab at `insert_at`.
4. Capacity rule (§3).
5. Remove the source: `layout.windows` + `remove_window`.
   `removed_window = Some(source)` — the GUI acts on this (§5).
6. `sync_tabbed_window_tabs(target, keep_active = Some(id))`;
   `install_text_buffer` into the new tab (`max_lines` = target
   `buffer_size`).
7. Routing rebuild, autosave, render. `affected_windows = [target]`.

**`wrap_text_as_tabbed(window)`** — `Text` only. `install_text_buffer`
(same streams), then `retype_window_in_place` with `TabbedText { base:
same, data: { tabs: [tab from data, id: 1], next_tab_id: 2, buffer_size:
same, .. } }` and the buffer in the single active tab.

**`reorder_tabs(window, tab_ids)`** — reorder `def.tabs`;
`sync(keep_active = prev_active)`; scroll ids unchanged.

**Context menu.** `ConvertToTextWindow` only when `tabs.len() == 1`
(= `tear_out_tab(window, only_id)`); `ConvertToTabbedWindow` on `Text`
(= `wrap_text_as_tabbed`). Both undoable.

### 2. Undo — snapshot restore, dedicated inverse per operation

```rust
pub enum UndoTabMove {
    TearOut { target, tab_def: TabbedTextTab, index, prev_active, new_window },
    ConvertedInPlace { window, tabbed_def: WindowDef },
    WrappedInPlace   { window, text_def: WindowDef },
    Join    { source_def: WindowDef, target, tab_id, prev_active, prev_buffer_size },
    Reorder { window, tab_ids_before: Vec<TabId>, prev_active },
}
pub struct UndoRecord { op: UndoTabMove, layout_hash: u64, gui: serde_json::Value }
```

**Gate.** `layout_hash` = hash of every serialised `WindowDef` in
`layout.windows` (ids and counters included), taken after the move. Undo
applies only if the current hash matches — every def mutation through any
path invalidates without knowing undo exists; GUI arrangement is not in
`layout.windows` and leaves it valid. **Explicit invalidation**
(`invalidate_tab_undo()`) from `init_windows` — the common tail of all
three wholesale replacement paths — and from profile/character switch,
because a reload reproduces identical defs over fresh buffers.

Inverses (each passes `keep_active = prev_active`; each returns a receipt
so §4/§5 run):
- **`TearOut`**: lift from `new_window` (`streams` from its def); re-insert
  `tab_def` at `index`; remove `new_window` (`removed_window`); sync;
  `install_text_buffer` into the tab.
- **`ConvertedInPlace`** / **`WrappedInPlace`**: lift; `install_text_
  buffer`; `retype_window_in_place` with the saved def.
- **`Join`**: lift from the tab; re-add `source_def` verbatim (name must be
  free); `add_new_window` (`created_window`); **capacity on undo** for both
  windows (§3); `install_text_buffer`; remove the tab; sync. GUI restores
  the placement snapshot (§5).
- **`Reorder`**: reorder to `tab_ids_before`; sync.

Validation: hash matches; named windows exist with expected kinds;
referenced ids exist; `Join` source name free. Otherwise *"Can't undo — the
layout changed"* and the record is cleared. GUI: ~6 s toast with **Undo**.
TUI: `.undotabmove` (later).

### 3. Buffer capacity rule

Invariant: **no tab or window holds more lines than its `max_lines`**
(unlimited excepted), and `max_lines` always equals the persisted
`buffer_size`. `add_line` pops at most one per push, so transactions
maintain the invariant; pruning is never changed.

Reconciliation helper `reconcile_capacity(def_buffer_size, len) ->
usize`: `0` if unlimited, else `max(def_buffer_size, len)`.

- **Tear-out**: destination `buffer_size` := source `buffer_size`.
- **Join**: target `buffer_size` := `reconcile_capacity(target, lifted
  len)`; every tab's `max_lines` follows via the sync.
- **Undo of a join** reconciles **both** windows:
  - target: `reconcile_capacity(prev_buffer_size, max len over surviving
    target tabs)`;
  - recreated source: `reconcile_capacity(source_def.buffer_size, lifted
    len)` — a 1,000-line source that grew to 1,200 inside a 5,000 target
    comes back at 1,200, persisted in its def. Unlimited stays unlimited.
- Every change is reported in `capacity_changed`; the toast names any
  window whose capacity differs from what the user last set.

### 4. GUI reading-position migration (lazy, composed)

`pending_scroll_migrations: HashMap<new, original_owner>` via
`queue_scroll_migration(old, new)`: `old == new` → no-op; if `old` is
pending (`old → orig`), collapse to `new → orig`; if that gives `new ==
orig`, remove (inverse pair cancelled); else insert `new → old`. Applied at
entry to `render_text_content_auto_split`, where the parent `Ui` and
derived ids exist: temp keys for base and `~live`, persisted split
fraction, `scroll_area::State` from the area id under
`text_scroll_area_id(orig)` to the ids computed in situ;
`text_scroll_area_id(new)` written fresh; old entries removed. Tests (a)–(d)
as before: two parent `Ui`s; move→undo before render; A→B→C; reorder.

### 5. GUI shell-key reconciliation and placement (every transaction and undo)

**Why.** For text windows the shell key is `TextMain` iff the window
subscribes to `main`, else `TextByName{name}` (Current State). Tearing the
main tab out of a non-canonically-named tabbed window changes the
*survivor's* key to `TextByName`, and the new custom window claims
`TextMain`; joining main-stream content flips the target the other way;
undo flips them back. Type changes never re-key; main-subscription always
does. Placement state is keyed by `TabKey`, so an unreconciled re-key
orphans a window's rect, zone, anchors, group, and settings.

**When.** The GUI consumes every `TabMoveReceipt` (transactions and undos
alike) in one handler, in this order:

1. **Capture** (before the core call): `GuiPlacementSnapshot` for the
   window the gesture acts on — the join source (about to vanish) or, for
   a drag-initiated last-tab conversion, the converted window (about to be
   re-placed). Contents: rect, zone, `last_center` rect,
   `sidebar_gap_above`, `no_title`, `tab_settings`, `hidden`, `detached`
   viewport state, **and every cross-window reference expressed by window
   name, never by key**: own anchors as `(edge, target_window_name)`,
   the sibling anchors of *other* windows that reference this one as
   `(referrer_window_name, edge)`, and group membership as `(leader_window
   _name, member_window_names)`. Keys are re-resolved at restore time
   (step 5) because any of those windows may have changed key in between —
   `TextMain` in particular may name a different window by then. Stored
   in `UndoRecord.gui`. Also capture the **pre-move key map** `old_keys:
   HashMap<window_name, TabKey>` from last frame's `available_tabs`.
2. **Core call.**
3. **Forget** `removed_window` if present: resolve its old key and call
   `forget_tab_state(key, name)` — the existing complete cleanup (group
   dissolve, sibling-ref prune, eleven maps, `layout_dirty`).
4. **Reconcile keys — globally, not per affected window.** Singleton
   allocation is a property of the whole window set: when window A
   releases `TextMain`, an *untouched* main-subscribed window B that had
   lost the earlier collision inherits it, moving `WindowByName{B} →
   TextMain` without appearing in any receipt. So: compute `new_keys` by
   re-running `collect_available_tabs`'s resolution over the **entire**
   current `ui_state` (the same function, so collisions resolve
   identically), and diff it against the captured `old_keys`. The changed
   set is `{ name | old_keys[name] != new_keys[name] }` over the union of
   both maps (created windows have no old key; removed windows no new
   key). Move every keyed entry — the eleven maps, group membership, and
   sibling refs pointing at the old key — for **every** changed name, as a
   set: first vacate every old key into a scratch map, then write each
   window's state under its new key, so a handoff (`X: TextMain →
   TextByName{X}`, `B: WindowByName{B} → TextMain`) never overwrites. The
   receipt's `affected_windows` is not an input to this step; it exists
   for the TUI cache and for tests.
5. **Restore** (undo of join, or undo of a drag-placed conversion): apply
   the snapshot under the restored window's *final* key from `new_keys`.
   Every cross-window reference in the snapshot is a **window name**;
   resolve each through `new_keys` at this moment — never through the
   key it had at capture, since `TextMain` (or any singleton) may now name
   a different window. Own anchors: restore `(edge, target_name)` only
   where `target_name` still exists and resolves. Sibling refs: for each
   `(referrer_name, edge)`, if the referrer still exists, re-point its
   anchor at this window's final key. Group: if `leader_name` still exists,
   re-join under the leader's *final* key; else stand alone in the saved
   zone. Then rect/zone/gap/`last_center`, `no_title`, `tab_settings`,
   `hidden`; if it was detached, re-open the viewport with the saved
   geometry. (Existence is guaranteed for defs by the hash gate; this step
   guards against key drift, which the gate does not see.)
6. **Place — a property of the gesture, not of window creation.** The
   drop position applies to *whichever window now holds the dragged
   content*, which is `receipt.window`: the created `custom-*` for
   `TearOut`, or the **same, converted window** for a drag-initiated
   `ConvertedInPlace` (the single-tab hover handle). In both cases write
   the drop rect/zone under that window's final key from `new_keys` —
   after step 4, so a `TextMain` handoff has already vacated the slot.
   For a `ConvertedInPlace` the pre-drag placement was captured in step 1
   and lands in `UndoRecord.gui`, so undo restores the original position
   (step 5). A **context-menu** conversion has no drop point: it skips
   this step and the window keeps its position (still captured for undo,
   which is then a no-op restore). The handler knows which case it is
   from a `GestureContext { drop: Option<DropPoint> }` the caller passes
   alongside the receipt; `wrap_text_as_tabbed` and `reorder_tabs` never
   carry a drop.
7. `queue_scroll_migration` from the receipt; `layout_dirty = true`.

The TUI ignores keys (it renders by name) and only needs step 3's
equivalent (`remove_widget_from_all_caches`).

### 6. GUI gesture — tear-out (plain drag on a tab)

Strip tabs `Sense::click_and_drag()`; `TabDragState { window, tab_id,
press_origin }`; armed past `PRESS_DRAG_THRESHOLD`; unarmed release
selects, armed release never does; ghost chip while armed; release in the
own strip → §8, elsewhere → `tear_out_tab` then §5 (place at pointer
grabbed top-center, size = source rect, zone under pointer, via the
free-placement helper factored from `apply_zone_drop`). Esc cancels.
Last-tab reachability: context menu (guaranteed) + Phase 3 hover handle.

### 7. GUI gesture — join (dwell during Alt+drag)

`tab_strip_rects` registry; strip renders as a drop band during any zone
drag even when `tabs.len() < 2`; `GuiZoneDragState.dwell` arms after
`JOIN_DWELL_MS` (350) over another `tabbedtext`'s strip when dragging a
`Text` window; armed release → capture (§5.1) → `join_window_as_tab` →
§5; unarmed release → today's zone drop. Non-`Text` → inert band.

### 8. Reorder within a strip (Phase 4)

Drop inside the own strip → `reorder_tabs`. Buffers and selection follow
ids; §4 and §5 see no changes.

## Phases

| Phase | Deliverable | Verified by |
|---|---|---|
| **1** | §0 identity; `install_text_buffer`; `retype_window_in_place`; the four transactions + `reorder_tabs`; §3 capacity incl. undo for both windows; `UndoRecord` (hash gate + `invalidate_tab_undo`); context-menu commands | headless core tests; `--no-default-features`; architecture gates; goldens byte-identical |
| **2** | §5 receipt handler (capture / forget / reconcile / restore / place); tear-out gesture; placement helper; §4 migrations | GUI unit tests on the reconciliation (headless, no rendering) + egui migration tests; live run |
| **3** | Ghost, `tab_strip_rects`, drop band, dwell, caret/highlight, undo toast (capacity notes), hover handle | dwell tests; live run |
| **4** | Drop-in-own-strip → `reorder_tabs` | core reorder test |

## Tests

Phase 1 (core, headless)
- Identity: legacy all-zero; mixed `[5,0,5,7] → [5,8,9,7]`, counter 10;
  stale counter raised before repair; in-session adds without reload;
  never-recur; duplicate names keep both buffers.
- Selection: `[A,B,C]` B active — tear A → B, tear B → C; `[A,B]` tear B →
  A; reorder keeps B.
- **Streams on conversion**: a tabbed window loaded from config whose tab
  `content.streams` is empty; `ConvertToTextWindow` → `content.streams`
  equals the tab's resolved streams (incl. legacy `stream:`), subscriber
  map routes them, a subsequent line arrives in the window. Same for
  `WrappedInPlace` and both undos.
- Conversions: def, `widget_type`, `get_window_by_type`, routing, buffer,
  autosave, receipt fields.
- Tear-out / join: scrollback identical; streams move exactly once;
  `Familiar` naming; appearance copied; join `insert_at`, duplicates,
  non-`Text` rejected.
- Capacity: 8k into 5k → 8000 everywhere, `capacity_changed = [(target,
  5000, 8000)]`; one `add_line` → 8000. Unlimited target stays 0.
- **Capacity on undo, both windows**: a source with `buffer_size 1000`
  holding 6,000 lines (its cap was raised earlier; or set `max_lines`
  directly in the test) joins a 5,000 target → target raised to 6,000.
  Then 200 lines arrive on the joined tab (6,000, one-in-one-out) and
  another target tab grows from 4,950 to 5,100. Undo → target
  `reconcile(5000, 5100) = 5100`; recreated source `reconcile(1000, 6000)
  = 6000`; both persisted; no buffer exceeds its `max_lines`;
  `capacity_changed` lists both windows. Unlimited source comes back
  unlimited.
- Undo: all five variants round-trip; hash gate refuses after rename in
  another window / window added / reorder / dot-command def change; accepts
  after a GUI-only move; reload (all three paths) refuses; source name
  taken refuses; autosave after every op.

Phase 2 (GUI, headless where possible)
- **Re-key matrix** (each direction, then undo, asserting every placement
  map lands under the final key and nothing is left under the old one):
  - tear the `main` tab out of tabbed window `story` → `story` moves
    `TextMain → TextByName{story}`, the new `custom-*` claims `TextMain`;
  - join a `main`-subscribed `Text` window into non-main tabbed `combat` →
    `combat` moves `TextByName → TextMain`;
  - tear a non-main tab out of a `TextMain` window → no re-key;
  - the canonical name `main` stays `TextMain` regardless of streams;
  - singleton collision: two windows would both claim `TextMain` after a
    move → the loser lands on `WindowByName` with its state intact.
  - **untouched third window inherits the singleton**: `story` (tabbed,
    holds `main`, key `TextMain`) and `mirror` (`Text`, subscribes to
    `main`, lost the collision → `WindowByName{mirror}`, has a rect and a
    zone). Tear the `main` tab out of `story` into a new `custom-*` that
    is *hidden* or otherwise sorted after `mirror` → `mirror` inherits
    `TextMain` although it appears in no receipt; assert its rect/zone
    moved to `TextMain` and nothing remains under `WindowByName{mirror}`.
    Undo → `mirror` returns to `WindowByName` with state intact. Mirror
    case: an untouched window *loses* the singleton when a join hands
    `main` to a canonically-earlier window.
- **Drag-placed last-tab conversion**: single-tab tabbed window at rect R1;
  drag its hover handle to a different zone → `ConvertedInPlace`, same
  window name, now at the drop rect in the new zone; undo → back at R1 in
  the original zone. Context-menu `ConvertToTextWindow` on the same
  window → stays at R1; undo → still R1.
- **Grouped source whose leader changes shell key during the join, then
  undo.** Setup: source `S` (`Text`, subscribes to `main`, sorts first)
  owns `TextMain`; leader `L` (`Text`, also subscribes to `main`, sorts
  after `S` but before `T`) lost the collision → `WindowByName{L}`; `S` is
  grouped under `L`, and `L` has a sibling anchor to `S`; `T` is a
  non-main tabbed window.
  Join `S` into `T`. The snapshot records `leader_name = "L"` and
  `(referrer "L", edge)` — names, not keys. With `S` gone, `main` content
  now lives in `T`, and global reconciliation (§5.4) gives `TextMain` to
  `L` (it sorts before `T`) — an untouched window re-keyed by the join.
  Assert after the join: `L`'s rect/zone/group leadership live under
  `TextMain`; nothing remains under `WindowByName{L}`; `forget_tab_state`
  cleared `S`.
  Undo. `S` is recreated and reclaims `TextMain`; reconciliation returns
  `L` to `WindowByName{L}`. Restore (§5.5) resolves `leader_name = "L"`
  through `new_keys` → `WindowByName{L}` and re-joins `S` under it;
  `L`'s sibling anchor is re-pointed at `S`'s final key `TextMain`.
  Assert: group membership and anchor are exactly as in setup, and a
  capture-time key would have been wrong at both points (it was
  `WindowByName{L}` at capture, `TextMain` mid-flight, `WindowByName{L}`
  at restore — only name-resolution survives all three).
- **Join placement round-trip**: source is grouped under a leader and has
  a sibling anchor from another window; join → `forget_tab_state` ran
  (group dissolved for the source, sibling ref pruned, all maps clear);
  undo → recreated under the final key with rect, zone, group membership,
  and the sibling ref restored; `layout_dirty` set both times. Variant:
  source was detached → viewport re-opened with saved geometry.
- Migration tests (a)–(d).

Goldens: both byte-identical.

## Out of scope

TUI drag gesture; drop onto a plain `Text` window via gesture; detached-
viewport tear-out; multi-level undo; optional TUI geometry.

## Decisions closed (owner, 2026-09-13)

- Torn-out windows **copy the source `base` appearance**.
- Capacity: **raise on join, never trim; on undo each window keeps what it
  holds** (never below its old cap). Confirmed as the lossless policy.

## Open questions

- Dwell time 350 ms — tune live (Phase 3).
