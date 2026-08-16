# Window System Redesign: Template-First → Declaration-First

## Context

VellumFE was built template-first: ~55 hardcoded window templates (a 1500-line `match` in `src/config/templates.rs`) define what windows can exist, what they bind to, and where they sit. But Wrayth is a declaration-first protocol — the game itself declares windows (`<streamWindow>`, `<dialogData>`, resident `<openDialog>` panels), and lich scripts inject their own dialogs using the same XML (UberBar). The UberBar work proved the dynamic path: a general anchor-grid renderer translates dialogData positioning into native widgets with zero templates.

The goal of this effort: **make the dynamic path the foundation and retire templates**, so new game/script windows "just work" — starting with the GUI, porting to TUI later.

**Owner decisions (fixed):**
- Game-declared windows (streams, dialogs, resident panels, lich-script dialogs) → fully dynamic, no templates.
- Hotbars, spacers, command_input → small honest **local widget catalog** (the game never declares these).
- GameState-fed widgets (compass, hands, vitals, indicators, injury doll, room, targets, players) → classification decided in design below.
- **Transparent migration**: existing layout.toml keeps working; users notice only new capability.
- Deliverable this round: **design doc + phased plan**. Implementation in later sessions.

## Current State (verified 2026-08-05, three exploration passes)

### The good news: runtime is already template-free
- layout.toml windows are **self-contained** `WindowDef`s (serde-tagged enum, 32 variants, `src/config/window_def.rs:17`); templates are only a creation-time seed, never re-consulted on load. `parse_tolerant` (`src/config/layout.rs:329`) round-trips unknown widget types.
- **Identity = binding**, not name: `WindowBase.binding: Option<WindowBinding>` (`src/config/widgets.rs:84,360`) = `Dialog|Stream|Container(id)`. `has_window_bound_to` (`config/layout.rs:809`) is the dedup check.
- **Data delivery is dynamic id-match fan-out** in `core/messages.rs` (e.g. progress `:1232`) — templates supply the id string, the matcher delivers.
- **Discovery pipeline exists**: `WindowDiscovery` (Stream|DialogPanel|DialogPopup, `ui_state.rs:134`) → `register_window_discovery` (`state.rs:4451`) → persistent **hidden** bound `WindowDef`; adopt logic tags existing subscribers instead of duplicating. Hidden-until-shown is universal.
- **UberBar machinery**: always-ingest `dialog_store` (`ui_state.rs:111`), anchor-grid resolver `positioned_controls` (`ui_state.rs:711-920`, compass align + sibling anchors + implicit flow), GUI exact-rect renderer (`gui/app/widgets.rs:2565`), TUI banded fallback.
- Three `WindowDef` variants (Container, DialogPanel, WebUi) already live **without any template** — born via `WindowDef::blank`.

### What templates still do (the retirement surface)
- `get_window_template` (:130, ~55 arms), `list_window_templates` (:1687 parallel list), `stream_id_to_template` (:1857), `dialog_id_to_template` (:1812), `id_has_widget_template` (:1844), `template_game_type` (:1799) — all in `src/config/templates.rs`.
- Consumers (14 files): Add/Hide/Edit menus (TUI `state.rs:6190-6763`, GUI `menus.rs:1262`); unified Windows catalog (`enumerate_known_windows` `state.rs:6324` lists the whole template catalog as phantom rows); GUI creation editors; dedup guard (`state.rs:3955`); `remove_window_if_default`; discovered panels borrow `text_custom`'s base; name-keyed color fallback (`tui/sync.rs:647`); `widget_min_size` (independent of templates but part of the rigid surface).

### Known gaps in the dynamic path
- Dedicated-widget streams need the hardcoded `stream_id_to_template` translation (inv/reserve/room/Spells; else text_custom).
- `<openDialog location/width/height>` hints are **dropped**; ephemeral placement is hardcoded (containers 40x15 centered, panels 26x20 right edge).
- GUI has **no placement logic** for new windows (pixel `main_window_rects` keyed by TabKey; falls to egui defaults). ← main placement gap.
- Only `InjuriesPanel` skin renders art; other skins paint nothing.
- DialogPopup (bank/shop) never becomes a layout window (U5, deferred).
- Parser doesn't know `exposeWindow`; standalone `<skin>`/`<exposeContainer>` silently dropped.

### GUI seam (phase-1 territory)
- All in `VellumGuiApp` (`frontend/gui/app.rs:247`, 8132 lines). Tabs derive from `ui_state.windows` (`collect_available_tabs :893`); renderer dispatch = ~30-arm match over `WindowContent` (`gui/app/widgets.rs:5119`). Two layout stores: core cell-space + GUI pixel rects with canonical_canvas rescale. GUI window editor already edits live windows (template-decoupled); **creation** is template-coupled.
- Web/phone frontend is insulated behind `RemoteDelta`/`GameState` — never sees the window model. Keep `GameState` + `StyledLine` stable.

## Design (target architecture)

### Thesis
Templates conflate three orthogonal concerns: **identity** (which game feed), **presentation** (which renderer), **default geometry/config**. Identity already migrated out (`WindowBinding` + discovery). The redesign extracts the other two into a small set of new concepts — everything else is deletion:

| New concept | Replaces | Home |
|---|---|---|
| `ViewKind` (names a renderer) | widget_type-as-identity | `src/data/` |
| `resolve_view()` + one `DEDICATED_VIEWS` table | `stream_id_to_template`, `dialog_id_to_template`, `id_has_widget_template` | `src/core/` |
| `LocalCatalog` (~40 `CatalogEntry { key, title, game, seed: fn()->WindowDef }`) | the 55-arm template match, `template_game_type` | `src/config/` (replaces templates.rs) |
| `PlacementHint` + placement engine | hardcoded ephemeral rects, dropped openDialog hints | `data/` + `core/` policy + `frontend/gui/window_manager.rs` (new) |
| `SkinRegistry` (GUI) | InjuriesPanel special case | `frontend/gui/` |
| Discovery-memory store (`window_registry.toml`) | phantom template rows in the Windows list | `core/` + `config/` persistence |

### Window taxonomy — the principled rule (simplified per owner, 2026-08-05)
**Two buckets. Classify by who authors the *structure*, not the transport:**
- **Game-declared** — the XML declares content *and* structure (dialogs with positioned controls, titled streams). Identity = binding; renderer chosen by resolver. Includes lich-script dialogs (UberBar).
- **Local catalog** — everything we invent, whether or not it reads GameState: compass, hands, vitals bars, indicators, injury doll, room, targets, players, countdowns, dashboard (GameState-fed) plus hotbars, spacers, command_input, performance (pure UI). Targets/players are *derived* from game data, never sent as windows; room is technically a game stream but deliberately routed through GameState so its pieces (name/desc/exits/players) stay toggleable.

**Claiming rule**: when the game *does* declare a binding that a catalog view presents better (streamWindow `room`, dialog `minivitals`/`IconBAR`, `expr`, `encum`…), the resolver's `DEDICATED_VIEWS` table claims that binding for the catalog view — the binding is game-declared, the presentation is ours.

**Vitals/indicators nuance**: Wrayth *does* declare `minivitals`/`IconBAR` structurally. The bound window for those dialogs is game-declared; the resolver's dedicated-view table maps them to the polished MiniVitals/indicator views **by default**, but presentation becomes a per-window `view` override field — a user can flip any dialog to the raw dynamic rendering (UberBar-style skinning for free) without regressing defaults. Decomposed widgets (one health bar) stay catalog data views — which is also how "add a health bar before the game declares vitals this session" keeps working.

### Presentation resolver
`resolve_view(binding, override) -> ViewKind`, three layers, first hit wins:
1. Per-window `view` override on `WindowBase` (serde-optional → zero layout.toml impact).
2. `DEDICATED_VIEWS` const table (inv→Inventory, reserve, room, Spells, expr→GS4Experience, minivitals, encum, Buffs/Debuffs/Cooldowns/ActiveSpells…). ONE table — the "must agree" invariant currently split across three functions becomes true by construction.
3. Kind fallback: `Dialog(_)` → generic dialog renderer, `Stream(_)` → text, `Container(_)` → container.

Content-shape inference as a view selector was considered and rejected — the generic dialog renderer already handles a dialog-of-progressBars correctly; inference survives only as *sizing* input.

### Placement
Start honoring `<openDialog location/width/height>` (parser currently drops them). `PlacementHint { zone, size_px }` on `WindowDiscovery`, persisted on the hidden `WindowDef`. Sizing precedence: persisted user geometry → game hint → intrinsic size (dialog bounding box from `positioned_controls`; streams 40×10 cells). Placement runs at first *show*. GUI gets a real placement engine (`frontend/gui/window_manager.rs`, first extraction from the 8132-line app.rs): zone → band of canonical_canvas, first-fit free-rect, cascade fallback. TUI mirrors the same policy over the cell grid later.

### Anchored geometry — snap permanence (added 2026-08-05, Workstream P-A)
Provenance: Niffy's snap-permanence prototype/spec (2026-08-05), validated against code. The confirmed gap: GUI snaps commit as raw `[x,y,w,h]` pixels with zero relationship metadata (`snap.rs:548-553`, `dock.rs:38-48`), so a sidebar-splitter drag moves the pane edge without moving windows the user docked to it (they only get display-clamped via `compute_center_display_rects`). Fix: on release, a snap **promotes to a persisted anchor**, and geometry becomes a per-frame solver output.

**Core architecture — anchors are a display-time resolve layer; the solver never writes the store.** The merged lossless rescale (canonical_canvas + pure proportional `rescale_rect`, `dock.rs:319-412`) requires that only user gestures and proportional maps write `main_window_rects`. Anchors join the existing pattern of per-frame pure passes (`compute_center_display_rects` dock.rs:475, `squeezed_sidebar_widths` zones.rs:76):

```
store (free rects, canvas-proportional, unclamped)   ← only gestures + proportional rescale write
  ▼ per frame
solve_anchors(store, anchors, pane_rect, roles)      ← NEW: anchored edges override free edges
  ▼
compute_center_display_rects(...)                     ← existing displacement/elastic pass
  ▼
clamp_main_window_rect                                ← existing last-resort net (unchanged)
```

**Commit-on-detach invariant**: any operation that removes/invalidates an anchor first commits the window's current *resolved* rect into the free store (prevents teleport to a stale free rect). Empty anchor map ⇒ byte-for-byte today's behavior ⇒ zero migration.

**Data model** (GUI-side only, in dock snapshot, NOT core): `EdgeRef = Pane(Side) | PaneCenter | Sibling{key: TabKey, side: Side}`; `Anchor { ref, offset }` (promotion writes offset 0); per-axis `AxisAnchoring = Free | Lo(Anchor) | Hi(Anchor) | Both{lo,hi} | Center(Anchor)` (center-vs-edge unrepresentable-illegal); `window_anchors: HashMap<TabKey, WindowAnchors>` beside `main_window_rects`. Resolution: single-edge anchors take the edge from the ref and the extent from the store (still proportional — anchored windows keep breathing on OS resize); `Both` makes size a solver output with compress-then-push min-size (hi yields; Pane outranks Sibling). `SizeRole { Proportional (default), Fixed }` per window: rescale exempts w/h for Fixed — fixes HUD widgets (compass/hands) shrinking on OS resize.

**Promotion** (at the existing release point, snap.rs:548-553): `Bound`→Pane, `Sibling`→Sibling, `Center`→PaneCenter; **Grid never promotes**. WYSIWYG per axis from `AxisGesture`: Translate + engaged edge guide anchors that edge and clears the opposite; MinEdge/MaxEdge resize anchors that edge and keeps the other (how users build `Both`); no engaged guide at release clears the anchors the gesture invalidated — drag-away-past-radius IS the removal gesture; Shift already suspends snapping ⇒ free placement for free. Cycle refusal at promotion (walk transitive sibling refs; snap happens visually, anchor not persisted). Mechanical prereq: `SnapGuide`/`AxisCandidate` carry `target_key: Option<TabKey>` + `candidate_side` (snap.rs:68-78, 123-135).

**Solver**: per-frame pure function in the existing feed slot (zones.rs:1191-1232, 1345-1357) — per-frame resolve dissolves "when to re-solve" (splitter drags, zone show/hide, squeeze, OS resize all already flow into the per-frame pane rect). Sibling chains: per-zone Kahn toposort, single pass (DAG by construction). Persisted cycles degrade all members to Free for the frame (log once). Dangling sibling refs degrade to free-edge but the anchor is kept (hide/show re-attaches); permanent deletion prunes dependents' anchors after commit-on-detach; zone reassignment clears own + dependent anchors with commit. Store-level "capture" is out of scope — `compute_center_display_rects` already provides it display-only.

**Persistence**: extend `MainWindowRectSnapshot` (dock.rs:39-48) with serde-optional `anchors` + `size_role` (old layouts = all Free; old builds ignore unknown fields). `merge_layout_geometry` (`.resize <name>`) does NOT import anchors. No automatic backfill — explicit one-shot `.anchorinfer` (epsilon ~1.5px against live pane rects, reports what it anchored, undoable by drag-away).

**Owner decisions (2026-08-05)**: (1) sibling anchors yes, as separable P-A2 (cuttable after living with P-A1); (2) backfill = explicit command, never automatic; (3) anchor UI v1 = context menu only ("Release anchors"), badges/tethers deferred to P-A3; (4) `SizeRole::Fixed` lands in P-A2, gated on the canonical_canvas rescale live-test (only P-A item touching that path).

**Prototype scorecard for the record**: slices 1 (lossless capture/return — achieved today via the unclamped store + display-only clamps) and 2 (committed-vs-displayed sidebar widths — shipped 04e429b) already exist in Vellum; slices 3–6 (dock survives, elastic/locked roles, pane-vs-OS anchor families, splitter-commit re-solve) are folded into P-A.

### WindowDef / WindowContent: freeze, don't collapse
- `WindowDef` (32 variants) survives as the **frozen wire format** — the serde tag is the layout.toml compat surface. New game windows never add variants (dialogs serialize as `dialogpanel`, streams as `text` — already proven by Container/DialogPanel/WebUi). Add `WindowDef::view()` and `WindowDef::for_view(view, base)` (generalizes `blank`).
- `WindowContent` unchanged this round; collapsing the data-less marker variants is an optional final cleanup.
- Rejected: single-struct `WindowDef { base, view, config: toml::Value }` — forces serde migration of every layout + massive test churn for ~0 extra benefit over the frozen enum.
- User template stores (`window_templates.toml`, `indicator_templates.toml`) **survive** as a preset layer the resolver consults first.

### Authoring surface
The Windows list becomes a union of: persisted layout windows (incl. hidden discovered) + session ephemerals + local catalog entries not yet instantiated + **discovery memory** (per-character `window_registry.toml` of every binding ever seen — so "Bounty" is addable in a fresh layout before the game re-declares it; seeded at first run from the same static maps we then delete). Menus/editors enumerate this union through unchanged AppCore method signatures.

### Skins
GUI-side `SkinRegistry`: skin name → renderer with rect + controls + dialog state + art/frames pool. Built-ins: InjuriesPanel (existing doll), then opt-in bar-fill art for healthBar/manaBar/etc. Unknown skin paints nothing (current behavior) — so this never blocks the core redesign. Extension: `skins.toml` later.

**storm.skn findings (2026-08-05, examined `C:\Gemstone\SIMU\Wrayth\storm.skn`):** Wrayth's skin file is an OLE compound document (ActiveSkin COM persistence, 9.3 MB). Confirmed: **skin/image names on the wire are pure client-side lookups** — no pixels or colors ever cross the network; Wrayth resolves them against its .skn asset table, and bar colors live as skin *properties* ("Roundtime Color", `rtColor`, `ulColor`) — the direct analog of our theme/colors system, validating SkinRegistry's name→(art | themed color fill) design. The complete built-in name vocabulary recovered from the file: `HealthBar`, `HealthBar2`, `ManaBar`, `StaminaBar`, `SpiritBar`, `ConcentrationBar` (DR), `InjuriesPanel`, `Injury1-3`, demeanor faces (`friendlyFace`/`neutralFace`/`coldFace`/`crossFace`/`warmFace`/`removeFace`/`reservedFace` — these resolve `<menuImage name=…>` too), compass sprites (`CompassN`…`CompassOut`, glow variants), hand slots (`leftHand`/`rightHand`/`spellHand` + drop/highlight states), and `rtBar1-5`/`ctBar1-5` (5-frame RT/CT bar sequences). Two implementation notes: (1) **case-insensitive lookup required** — the wire sends `healthBar`, the skin table says `HealthBar`; (2) pixel data is in ActiveSkin's proprietary format and is Simutronics' art — we never extract or ship it; VellumFE supplies its own art via the existing skin/frames pool, keyed by the same names.

**Community-skin ecosystem check** (`E:\Saved Files From Latest Reinstall Yay\Gemstone\SIMU\Wrayth\`, 7 .skn files): full-conversion skins (stealth.skn, 55 MB) contain **9/9 core asset names** — community skins re-skin the *same named slots*; partial skins (Brute, NishiFront21) override a subset (2/9) and fall back to defaults for the rest. This is exactly the proposed name-keyed SkinRegistry with layered fallback (user skin → built-in), validated by the existing ecosystem.

**Wrayth settings exports found** (same folder: `Nisugi3.xml`, `NewLayoutWrayth.xml`, `Mnstr.xml`, `YepCock.xml`): full `<settings client=…>` blobs — the same format `settingsInfo`/`<settings major=N>` streams at login — containing `<strings>` (highlights + colors), `<presets>`, `<palette>`, `<names>`, and the complete persisted window model: 61 `<w id vis frame="panel|float" location panel open height width x y detach autohide ts/>` entries (note the previously unseen `autohide` attribute). Uses: (a) static, readable reference for the Wrayth window model — better than reconstructing from stgupd log diffs; (b) **Phase 0 test fixtures** for the existing wrayth import in `src/migrate.rs`; (c) future "import Wrayth settings" feature source (highlights/presets/palette).

## Migration Plan (7 phases, strangler-fig)

**Core insight: nothing on disk needs migrating.** layout.toml windows are self-contained; templates are consulted only at creation/enumeration time. The migration is a creation-path swap plus one invisible on-load backfill. Governing invariant every phase: **binding is identity** (one feed id → one auto-created window, via `has_window_bound_to` + adopt logic).

### Phase 0 — Characterization suite (tests only)
Per the "write tests first" rule. Pin: (1) golden TOML snapshots of all ~55 templates × game types (the equivalence oracle); (2) truth tables for the three id-mapping functions + `template_game_type` incl. negatives (combat, UberBar); (3) menu/catalog content snapshots (`addable_window_templates`, `enumerate_known_windows` incl. phantom rows) — the "no dead menu" tripwires; (4) layout round-trip incl. unknown widget type; (5) discovery behaviors (adopt, tab-suppression, dedup, text_custom base borrow, DialogPopup skip); (6) `layout_equivalent_window_name` identity rules; (7) the deleted-then-reshown widget resurrection path (templates.rs:1832-1843 contract); (8) **P-A0 geometry pins** (shared with Workstream P-A): snap release writes the snapped rect as raw pixels and clears all snap state (pin, superseded by P-A1); splitter drag does NOT move a window flush to the old pane edge (the defect — pinned so P-A1's diff proves the fix); `compute_center_display_rects` displacement/elastic cases; squeezed-sidebar non-write-back (exists, zones.rs:1707); rescale round-trips (exist, dock.rs:779-964).

### Phase 1 — Resolver + seen-bindings registry (dark)
Introduce `ViewKind` + resolver as a **pure façade delegating to templates** — equivalence test vs Phase 0 snapshots makes every later swap provable. Add `window_registry.toml` (separate file, so old builds never see it) written from `register_window_discovery`; seed with well-known GS4/DR feeds from the soon-to-die static maps.

### Phase 2 — Binding backfill on load (the only "migration", invisible)
Load-time normalization beside existing precedents (layout.rs:407): derive bindings **data-first** (streams list, dialog_id) with the inverse of the old maps, name-fallback only for empty data; never backfill data views. Kill the last name-as-identity consumer: TUI progress color fallback re-keyed to `data.id` (tui/sync.rs:647). Tests: idempotence; discovery burst on backfilled default layout creates **zero** windows; round-trips load on the previous release.

### Phase 3 — Creation/enumeration paths → resolver (GUI seam)
Swap consumers while resolver output is still template-identical (menu snapshots must stay green **unchanged** — the transparency proof): phantom rows → catalog + registry; Add menus (TUI state.rs:6189-6318, GUI menus.rs:1243-1306); GUI creation editors off `text_custom`/`CUSTOM_SEEDS` consts; discovery base borrow → resolver-supplied base; route the two hardcoded ephemeral placements through one placement helper honoring openDialog hints (full free-rect packing = follow-up hook).

### Phase 4 — Declaration-first routing (the behavioral heart)
Game paths stop consulting templates: `DialogOpen`/`DialogPanelOpen` guards → `catalog.claims_dialog(id)` (one function ⇒ the must-agree contract is trivially satisfied); stream routing → `catalog.claims_stream(id)`; `layout_equivalent_window_name` → binding-first identity; `remove_window_if_default` → drop minimization for bound windows, keep for catalog widgets (decision: safer than resolver-equality games; benign file growth).

### Phase 5 — Long-tail consumers
TUI call sites re-pointed through the resolver façade (window_editor, menu_actions, sync.rs perf overlay, keybinds); `migrate.rs` wrayth import → resolver; user preset stores re-homed (never die).

### Phase 6 — Kill the catalog
Delete the 55-arm match, `list_window_templates*`, `template_game_type`, three id-maps, `get_addable_templates_by_category`. templates.rs shrinks to user-preset handling (`presets.rs`). Add an architecture-test assertion that nothing outside presets references the dead symbols. **Never die:** user preset stores + editors, `widget_min_size`, `parse_tolerant`/unknown_windows, migrate-layout subcommand, `WindowDef::blank`.

### Workstream P-A — snap-permanence anchors (parallel GUI track, added 2026-08-05)
Anchors are orthogonal to template retirement (redesign P0–P2 touch no GUI files). Two interlock points: shared Phase-0 characterization (item 8 above), and shared ownership of the new `frontend/gui/window_manager.rs`. **Module contract fixed now** so whichever workstream reaches it first creates it: *pure geometry policy — placement, anchor solve, displacement — free functions with no `&mut VellumGuiApp`* (`compute_center_display_rects`, `rescale_*`, `snap_rect` are already in that shape; moving them is pure relocation).

- **P-A0 — gate + characterization.** Live-test the canonical_canvas rescale (existing matrix; hard gate for P-A2's `SizeRole::Fixed`); land the geometry pins (Phase 0 item 8).
- **P-A1 — pane anchors end-to-end** (can run parallel to redesign P1/P2): data model + persistence; `SnapGuide`/`AxisCandidate` target metadata; Pane/PaneCenter promotion + removal rules; per-frame solver wired into the zone feed slot; commit-on-detach; context-menu "Release anchors"; `.snapdebug` anchor logging; extract solve + displacement into `window_manager.rs` (coordinate with redesign P3, which adds the placement engine to the same module — synergy hook: the placement helper can *emit* anchors for zone-hinted windows, e.g. `openDialog location='right'` seeds a `Pane(Max)` x-anchor).
- **P-A2 — siblings + Fixed size**: sibling anchors (toposort, cycle refusal + degrade, dangling/prune/re-attach rules); `SizeRole::Fixed` rescale exemption (post-gate); `.anchorinfer`.
- **P-A3 — optional polish**: generalized elastic role (TextMain is the de-facto elastic today, dock.rs:545-575); pin badges / tether visualization.

**P-A test list**: promotion WYSIWYG cases (Grid-never, Shift-clears, resize-preserves-other-edge, translate-clears-opposite); lo-anchored window tracks pane edge across splitter widths (vs the P-A0 pin); `Both` spans pane + compress-then-push (Pane beats Sibling on yield); `Center` recenters after pane resize; chain resolves order-independently; cycle degrade + promotion refusal; dangling fallback / hide-show re-attach / delete-prunes-without-teleport; persistence round-trip + legacy-fixture-all-free + unknown-field tolerance on old build; stored free rect still round-trips the rescale chain exactly while anchored; commit-on-detach; zone reassignment clears with commit; Fixed: 120-step resize chain leaves w/h bit-identical; manual smoke: dock→splitter-drag follows, zone toggles, OS resize, save/load round trip, sibling chain drag.

### Risk register (top items)
- **GS4/DR gating** — truth-table + per-game menu snapshots (P0) re-run in P3/P4.
- **Lich dialogs misclassified** — catalog claims exact ids only; default is the generic panel (safe: dialog_store always ingests); UberBar test pinned.
- **Menu regressions** — AppCore signatures never change; content snapshots gate every phase; registry pre-seeding prevents "empty until game speaks."
- **Branch conflicts** (feat/ssh-launcher, fix/audit-2026-08 in state.rs/app.rs) — P0–P2 touch no GUI files; land fix/audit-2026-08 before P3; P3 GUI diffs confined to menus.rs/editors.
- **Duplicate windows during coexistence** — binding-identity invariant + P2 backfill + zero-new-windows-on-discovery-burst test.
- **Anchors vs proportional-rescale purity (P-A)** — the solver must NEVER write resolved values into `main_window_rects` (would break the pure-scale composition proof and resurrect the 800a38d drift class). Mitigations: display-time-only solve, commit-on-detach invariant, rescale round-trip test run with anchors active. `SizeRole::Fixed` is the only P-A item editing the rescale function — gated behind its live-test.

### Out of scope (hooks left)
TUI full port (P5 shims make core work reusable); DialogPopup/bank U5 (registry schema carries `kind`, ready for a Popup row); web frontend (insulated; verify with architecture test); GUI free-rect packing (P3 helper is the hook); **TUI anchors** (P-A is structurally GUI-only — anchors live in GUI dock state, never in core or layout.toml; the `EdgeRef` concept ports if the TUI ever wants dock persistence, the representation wouldn't); **store-level capture** (a pane edge pushing a free window is already provided display-only by `compute_center_display_rects` and stays that way).

## Wire-data verification (11.4 GB of real Lich logs, 2368 files, 8 characters incl. GST)

Full tag-frequency inventory and distinct window-declaration extraction are saved in the scratchpad (`tag_freq.txt`, `window_decl.txt`). Findings that materially shape the design:

### Tags the parser does NOT handle today (verified zero matches in parser.rs)
| Tag | Count | What it is |
|---|---|---|
| `<exposeDialog id='bank'/>` | 4,265 | Game says "show this dialog NOW" — sent after `<dialogData id='bank'>` content. **This is the missing U5 bank verb.** |
| `<exposeStream id='charprofile'/>` | 12 | Same verb for streams (charprofile arrives as `location="force-center" resident="false"` — a popup-like stream) |
| `<deleteContainer id="..."/>` | 7,559 | Container removal (we handle container/clearContainer/inv but not delete) |
| `<streamBox id='instructions' .../>` | 12 | A dialog control that is itself a text stream region (bug-report dialog) — new `DialogState` control kind |
| `<dynaStream id='...'>text</dynaStream>` / `<clearDynaStream>` | 12 / 6 | The content feed for a `streamBox` |
| `<exposeContainer>` | 2,714 | Known-dropped today |

Note: `exposeWindow` does not exist on the wire — the real verbs are `exposeDialog`/`exposeStream`.

### Attributes we drop that the design needs
- **`<streamWindow>`** carries `location` (center/right/**force-center**), `resident`, `save`, `scroll` (auto/manual), `ifClosed`, `appearance` (e.g. "story"), `target` — parser extracts only id/subtitle/title. These feed `PlacementHint` + popup-vs-resident classification.
- **`<openDialog>`** location vocabulary is richer than assumed: `right`, `center`, `quickBar`, `statBar` (minivitals!), `detach` (+ `noResize`, `noDock`, `save='false'` on utility popups like bugDialogBox). `target` attr also present.
- **`<dialogData name='bugDialogBox'>`** — some dialogs use `name=` instead of `id=`. Parser must accept both.

### Observed dialog id inventory (validates resolver tables)
`combat` (2.6M — most frequent!), `minivitals`, `Buffs/Debuffs/Cooldowns/Active Spells`, `encum`, `expr`, `injuries`, `stance`, `mapViewMain`/`mapMaster`, `BetrayerPanel`, `bank`, `befriend`, `quick`/`quick-simu`/`quick-combat`, `familiar`, `tables`, `lumnis_day`/`lumnis_schedule`, `espMasterDialog`/`espMasterData` (the game's ESP thought-network panel — **game-sent, not Lich**, owner-corrected 2026-08-05), and **per-entity ids** like `injuries-10154507` ("Zoleta's Injuries" — other players' injury dolls with dynamic ids). Per-entity ids confirm the resolver rule: claim *exact* ids only; everything else defaults to the generic dialog view.

### The Rosetta stone: `<stgupd>` CLIENT blocks
Logs capture Wrayth's own layout sync to the server (`<!-- CLIENT --><stgupd>...`): every window is `<w id=... vis frame="panel|float" location panel="Left|Right" x y width height open detach ts/>`, organized into `<panels><group id='Left'>` lists of `<dialog id=.../><stream id=.../><builtin id='windows'/>`, plus `<font>`/`<columnFont>`/`<detach x y w h>`. This is independent confirmation of the target model: **Wrayth itself models a window as binding-id + kind + zone + frame + rect + open/vis** — exactly the `WindowBinding` + `ViewKind` + `PlacementHint` + visibility decomposition. (VellumFE need not emit stgupd; it's our own layout store's job.)

### Design deltas from wire data (folded into phases)
1. **Parser vocabulary phase-item** (goes with Phase 1, parser-side, dark): parse `exposeDialog`/`exposeStream` → new `ParsedElement::Expose { kind, id }`; `deleteContainer`; `streamBox` control + `dynaStream` feed; accept `dialogData name=`; extract full `streamWindow`/`openDialog` attribute sets into `PlacementHint`.
2. **Expose semantics** (Phase 3/4) — **owner-decided 2026-08-05, persistence refined 2026-08-05**: expose = show. The game's own setting already gates whether these are sent, so we honor them. Rules: (a) an exposed window appears in the Windows menu on first sight and **stays there** (via the discovery-memory registry); (b) the Show flag doubles as popup permission — checked = may pop up, unchecked = expose is blocked for that id; (c) default = shown/allowed when a window first arrives via expose; (d) honor `<closeDialog>` to dismiss (verified on the wire: `closeDialog id="bank"` ×3,911 on leaving the bank, plus `withdraw`/`deposit` sub-dialogs, `tables`, `dlgCustomize`, `familiar`); (e) **persistence follows the wire's own `save` attribute**: `save='t'`/resident exposed dialogs (bank sends `save='t' location='right'`) become persistent layout windows — hidden unless exposed, user geometry saved in layout.toml, so bank pops up where the user last put it and the game's re-sent hints never override saved geometry (`has_window_bound_to` no-ops re-declaration); only `save='false'`/`location='detach'` utility popups (bugDialogBox, alertDialogBox) stay transient and out of the layout, with per-id position memory via the existing `saved_dialog_positions` store. This closes U5/bank with wire evidence.
3. **Zone vocabulary**: `PlacementHint.zone` must cover quickBar/statBar/detach/force-center, mapping quickBar→existing quickbar system, statBar→minivitals slot, detach/force-center→floating popup.

### Field-guide addenda (2026-08-05, from attribute-level log samples)
Six findings from building the protocol guide; all fold into existing phases:
1. **Uniform verb model across all three binding kinds.** The wire has a symmetric lifecycle — declare (`openDialog`/`streamWindow`/`container`) / update (`dialogData`/stream text/`inv`) / expose (`exposeDialog`/`exposeStream`/`exposeContainer`) / close (`closeDialog`) / delete (`deleteContainer`). Design the discovery pipeline around one verb enum for Dialog|Stream|Container instead of per-kind special cases. `exposeContainer` (×2,714, currently silently dropped) gets the same expose-=-show semantics; `deleteContainer` (×7,559) removes the container window. (Phase 1 parser + Phase 4 routing.)
2. **Containers declare placement too**: `<container id='stow' title="My Longcoat" target='#…' location='right' save='' resident='true'/>` — PlacementHint extraction must cover the `container` tag, not just openDialog/streamWindow. (Phase 1.)
3. **Clamp game hints.** Even the game itself sends viewport-busting sizes — `espMasterDialog height='2100'` (the ESP panel is a tall scrolling surface) — and Lich scripts can send anything. Placement precedence gains a sanitization step: clamp hint sizes to the canvas/zone. (Phase 3 placement helper.)
4. **Percentage coordinates are real** — minivitals uses `left='25%' width='25%'`, but `DialogControlLayout` parses coords as `i32`/`u16`, so `%` values fail parse → controls silently lose position. Harmless today only because minivitals routes to the dedicated view; the moment the per-window `view` override ships, flipping minivitals to the generic dialog renderer would break. **Percentage-aware layout parsing (`Px(i32) | Pct(f32)`) must land before the view override.** (Phase 3, ordering constraint.)
5. **Vertical align needed for popup rendering.** bugDialogBox anchors buttons `align='se'/'sw' top='-5'` — bottom-relative. The anchor resolver currently ignores the vertical compass component (s/c treated as n). Required when popup-class dialogs start rendering. (Phase 3/4.)
6. **closeDialog for never-opened ids must no-op.** Leaving the bank sends `closeDialog` for `withdraw`/`deposit` (×3,911 each) even when those sub-dialogs never opened — the game closes defensively. Pin the close-unknown-id-is-a-no-op invariant with a Phase 0 test.
7. **`justify` decoding is wrong/incomplete** (found 2026-08-05 corpus census). Wire values: `2` ×10.5M (effect-duration labels in Buffs/Debuffs/Cooldowns/Active Spells, paired with `anchor_right`), `0` ×441K, `4` ×393K, `5` ×6, `6` never. Correct decoding: **low 2 bits = alignment (0=left, 1=center, 2=right), bit 4 = flag** — so 4/5/6 are flagged left/center/right, consistent with VellumFE's current 4/5/6 map, but the majority value `2` (right) currently falls through to default-left in `DialogLabel.justify` handling (ui_state.rs:655-658). **Visible TODAY** (owner screenshot 2026-08-05, Lich Experience+/ENHANCIVES panels vs Wrayth): script dialogs go through the generic renderer now — "Level 106" renders left instead of centered, bar text renders centered instead of right-justified. Same screenshot exposed a second renderer gap: labels anchored beside bars (the `--` markers) stack at the bottom instead of sitting left of their bar row. **Justify fix SHIPPED 2026-08-05 (this session)**: `DialogLabel::align()` bitfield decode + both GUI label paths (panel painter + popup path, which previously ignored justify entirely) + decode unit tests + verbatim Buffs wire fixture. **Anchor work DEFERRED — needs captured XML** (screenshots were another player's session; script not in our logs). Two named suspects, pinned for when the XML is captured: (a) the implicit-flow heuristic (`ui_state.rs` `positioned_controls` pass 2) overrides the vertical position of ANY control with `anchor_left`-but-no-`anchor_top` — a UberBar-shaped assumption applied universally, exactly matching the bottom-stacking symptom; (b) `parse_control_layout` (parser.rs) discards empty-string anchors (`anchor_right=''`, present on every Buffs duration label ×10.5M) — if Wrayth treats an empty target as "anchor to parent edge," we're dropping real layout data; current dropped-behavior is pinned by a characterization assert in the new parser test. Fix: `align = justify & 3`, test with wire values 0/2/4/5 + the Experience+/ENHANCIVES layouts as fixtures. Small and standalone — can ship ahead of the redesign. (Phase 1, alongside percentage coords.)
8. **Stream text has NO alignment markup — confirmed by exhaustive census.** The only text-alignment mechanism on the entire wire is `justify` on dialog `label`/`link` controls. Story/stream text carries only `style` (ids: roomName/roomDesc/"" — nothing else in 11.4 GB), `preset`, bold, and `output class="mono"`. Window-level text alignment (our `content_align`) is purely a client-side presentation choice with no wire counterpart — nothing to migrate, nothing to honor.

## Next steps after approval (this round)

Per owner decision, this round delivers the design doc, not code:
1. Commit this document into the repo as `docs/design/window-system-redesign.md` (or the repo's work-units convention) so it survives sessions and reviews.
2. Save a memory pointer with status + phase sequencing.
3. Implementation begins in a later session with **Phase 0 (characterization suite)** on a `test/window-system-characterization` branch — coordinate with landing `fix/audit-2026-08` and `feat/ssh-launcher` first (both touch `state.rs`).

## Verification

- **Phase 0 gate**: characterization suite green on main before any production change.
- **Equivalence proofs**: resolver-vs-template golden test (P1); menu snapshots unchanged (P3); mapping truth tables re-run against catalog (P4).
- **Live smoke per phase** (GUI): Add menu, Windows window, custom-window creation, mid-session discovery of a fresh stream, GS4 expr/stance/encum widgets, a lich UberBar dialog, a generic panel (befriend), skin rendering (injury doll).
- **Compat checks**: backfilled layout.toml loads on the previous release; `cargo test` full suite (~2200) each phase; `tests/architecture.rs` extended with the no-dead-symbol assertion in P6.
- **Wire-data check**: resolver `DEDICATED_VIEWS` and registry seeds validated against the log-derived id inventory (below).
