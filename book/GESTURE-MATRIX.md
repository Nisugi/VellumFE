# Frontend Gesture Matrix (source-verified)

> **Purpose:** the single source of truth for "how does the user do X" across TUI / GUI / Mobile.
> Every tabbed **Set it up** block in the book draws its facts from here. Do not write a
> per-frontend instruction that contradicts this table.

> ## ⚠️ Rules for using this matrix (read before writing ANY page)
>
> 1. **This matrix outranks prose and outranks intuition.** If a page and this matrix
>    disagree, the *page* is wrong. If your instinct and this matrix disagree, your
>    *instinct* is wrong. A plausible-sounding gesture that doesn't exist ("surely you
>    right-click to add a widget") is the #1 documented failure mode. Transcribe the cell;
>    do not paraphrase from memory.
> 2. **Re-verify at write-time.** This file is the index, not the last word — it drifts when
>    the product changes. Before writing a page, re-grep the cited source and confirm.
> 3. **Staleness markers.** When behavior changes under an in-flight fix, mark the cell
>    **🕒 STALE — re-verify (reason)**. Never write a page from a 🕒 cell.
>
> **Verified against current source on 2026-08-09** (post GUI menu-overhaul: "inline
> stay-open Windows catalog + Settings section menu", commit df654a58) by three read-only
> Explore passes. Citations are `file:line` under repo root. `—` = n/a for that frontend.
>
> **⚠️ Corrections vs the (lost) prior matrix — do not regress to the old claims:**
> - **TUI `Ctrl+C` COPIES, it does NOT quit.** `quit=ctrl+c` is on `GlobalKeybinds`, consumed
>   by the **GUI only**. TUI quits via `.quit`/`.exit`. (`command_line.rs:81`, `keybinds.rs:27-43`)
> - **TUI `.deletewindow` only HIDES** — it redirects to `hide_window` (`state.rs:4549-4552`).
>   True stash-delete (`delete_and_stash_window`) is used by GUI/menu Close flows, not the dot-command.
> - **GUI "add window" = the stay-open Windows catalog** (toolbar button), NOT right-click. There
>   is no right-click "Add Widget" and no right-click "Delete Window".

## Legend
- **✅ full** — first-class supported gesture
- **⚠️ partial / different** — works but with a caveat that MUST be stated in the tab
- **❌ n/a** — the frontend genuinely cannot do this (state it honestly; often "use the GUI")

---

## Parity gaps — watch list

> Triage on the axis that matters: **document as-is, or fix then document?**
> - **[by-design]** — permanent correct difference (terminals can't show images). Document honestly.
> - **[repair]** — a wart to fix in product, *then* document. Don't over-document; keep minimal until repaired.
> - **[repair?]** — my guess at a repair candidate; **flag for Nisugi's call**, not committed.
> - **[doc-only]** — no product change; just explain well.
> - **[doc-bug]** — the existing book/comments are WRONG vs current code; fix the doc.

| Gap | Frontend(s) | Triage | Note |
|-----|-------------|--------|------|
| **Delete vs Hide** — GUI has a true-delete (Window Editor red button) separate from Hide; TUI `.deletewindow` just hides | TUI, GUI | **[repair?]** | Nisugi previously mused this should collapse to Hide-only. TUI already effectively hides. Flag for decision; keep docs minimal until settled. `windows.rs:1623`, `state.rs:4549` |
| Can't add/move/resize panels | Mobile | **[by-design]** | Fixed chrome; snapshot+deltas stream. `app.js:808-840` |
| No per-window move/resize by coordinate/command | TUI, GUI | **[doc-only]** | Mouse-drag is the intended gesture |
| No standalone `.showwindow`/`.unhide` | TUI | **[doc-only]** | Restore via `.menu` Windows submenu or `.loadlayout`. `menu_actions.rs:238` |
| No GUI button to SAVE a layout | GUI | **[doc-only]** | Save is `.savelayout <name>` typed; catalog hint even says so. `menus.rs:1224` |
| Skins / per-window graphical appearance / doll-compass-hand art | TUI, Mobile | **[by-design]** | GUI-only forever (terminal/phone-chrome can't render this authoring) |
| Game keybinds editing | Mobile | **[by-design]** | Host-owned `keybinds.toml`, pushed read-only. `app.js:838-839` |
| Macro `hidden_when`/set conditions | Mobile | **[repair?]** | Phone macro editor: label/command/color/tap-mode only. `app.js:4386-4422` |
| **Mobile highlight editor redirects/squelch** | Mobile | **[doc-bug]** | `web.md:164` + `app.js:3410-3412` comment say phone CAN'T; **code DOES** (`app.js:3509,3660,3523-3526`). Fix the doc. |
| Copy: TUI `Ctrl+C` copies (not quit) / plain-text only (all) | all | **[doc-only]** | Plain-text-only copy is deliberate ([[styled-clipboard-copy-rejected]]). |

---

## Core layout gestures

| Task | Terminal (TUI) | Desktop GUI | Mobile / Web |
|------|----------------|-------------|--------------|
| **Add / show a window** | ✅ `.addwindow` (no args → **picker**; full form `.addwindow <name> <type> <x> <y> <w> [h]`, h default 10). Types = `WidgetType::VALID_TYPES` (`window.rs:112-144`; the usage-string subset is stale). `commands.rs:2185-2208` | ✅ **Top-toolbar "Windows" button → stay-open inline catalog:** checkbox per window (tick=show/untick=hide), per-row **zone** submenu, **➕ Custom window…**, **↩ Restore deleted…**. Also `.addwindow` typed. **NO right-click add.** `app.rs:7060-7068`, `known_windows.rs:74-299` | ❌ **Not available** — fixed chrome; surfaces render from server snapshot+deltas. `app.js:808-840` |
| **Move a window** | ✅ Left-drag the **top row (title bar)**. Locked = grab but no move. `input.rs:72-105,1675-1769` | ✅ **Drag anywhere** (body/title, no modifier). Snap-docking active; **hold Shift to suspend**. Also right-click **Arrange ▸ Move Window** (works locked). `zones.rs:2013-2024,1561`, `menus.rs:1603` | ⚠️ Only floating elements move: **wheel puck** (drag >8px), **macro/compass** (long-press 450ms), **interact bar** (drag >8px). No game windows. `app.js:1887,4302,5003,2418` |
| **Resize a window** | ✅ Drag **right col / bottom row / bottom-right corner**. Locked = no resize. `.resize` refits layout (not per-window). `input.rs:81-105` | ✅ egui native **edge/corner** drag; snaps unless Shift held. `zones.rs:1986,2047` | ❌ Not available — fixed-size chrome. `app.js` (no resize handles) |
| **Hide a window** | ✅ `.hidewindow`/`.hidewin [name]` (no name → picker). Can't hide the sole main-feed window. `commands.rs:2477`, `state.rs:4274-4306` | ✅ Right-click ▸ **Hide**, or untick in the Windows catalog (same layer). `menus.rs:1554`, `known_windows.rs:315` | ❌ (fixed chrome) |
| **Delete a window (true removal)** | ⚠️ **`.deletewindow`/`.delwindow` only HIDES** (redirects to `hide_window`). `state.rs:4549-4552` | ✅ **Window Editor → red "Delete Window"** ("…entirely (not just hide)"). Not on right-click. `windows.rs:1623-1634` | ❌ |
| **Restore a hidden/deleted window** | ⚠️ Hidden → `.menu` **Windows** submenu (`ShowWindow`). Deleted-stash → menu only. No `.showwindow`. `menu_actions.rs:238` | ✅ Windows catalog: re-tick to unhide; **↩ Restore deleted…** for stashed. `known_windows.rs:168`, `state.rs:4630` | ❌ |
| **Save a layout** | ✅ `.savelayout [name]` (default "default"). `commands.rs:2105` | ✅ `.savelayout <name>` typed — **no GUI save button**. `menus.rs:1224` | ❌ (layout host-owned) |
| **Load a layout** | ✅ `.loadlayout [name] [--keep-skin]`, `.layouts` to list. `commands.rs:2110-2136` | ✅ Same commands, or `.menu` **Layouts** submenu (`action:layout:load:<name>`). `menus.rs:1212-1235` | ❌ |

## Text & interaction gestures

| Task | Terminal (TUI) | Desktop GUI | Mobile / Web |
|------|----------------|-------------|--------------|
| **Scroll a window** | ✅ `PageUp`/`PageDown` (20-line page), `Alt+PageUp/Down` (line), mouse wheel. Home/End actions exist, no default key. `keybinds.rs:2600-2615` | ✅ Mouse wheel / scrollbar (native egui) | ✅ Touch-scroll the text pane |
| **Switch focused window** | ✅ `Tab` (`switch_current_window`; smart-Tab dot-completes first), `Shift+Tab` reverse. `keybinds.rs:2596`, `input_handlers.rs:359` | ✅ Click a window | ⚠️ n/a — fixed chrome; drawers/wheel instead |
| **Copy text** | ⚠️ **Drag-select → auto-copies on release** (`selection_auto_copy`). **`Ctrl+C` COPIES (command-input), does NOT quit.** Quit = `.quit`/`.exit`. `input.rs:1300-1337`, `command_line.rs:81` | ⚠️ Drag-select (double=word, triple=line) then **Ctrl+C**. **Plain text only.** `widgets.rs:5806,6629` | ⚠️ Native browser selection; plain text |
| **Interact with a game object (click a link)** | ✅ Left-click: URL→browser; `<d>`→sends cmd; coord→cmdlist; plain `<a>`→**requests server context menu**. `input.rs:1156-1235` | ✅ **Left-click** a link → direct command, or **server verb menu** at click pos (menu-type links). `app.rs:6602,6616` | ✅ **Tap** a link: direct/coord→immediate; plain noun→**bottom-sheet context menu**. `app.js:4002-4024` |
| **Drag object → target** | ✅ **Ctrl**+drag a link → `_drag #id <target>` (hand/inv/container/link). `input.rs:1064-1155` | ✅ Modifier+drag link onto another → `_drag #id #id`. `widgets.rs:5830`, `app.rs:6596` | ⚠️ Targeting via status-drawer taps + interact bar |
| **Context menu (window chrome)** | ✅ **Right-click title bar** → Close Window / Edit Window… / Open Menu. `input.rs:987-1051` | ✅ **Right-click window** (not a link) → management menu (Edit/Hide/Detach/Appearance/Arrange). `zones.rs:2181` | ⚠️ Long-press floating elements → arrange/edit sheet |
| **Open the main menu** | ✅ `.menu` (**no default keybind**). `commands.rs:2645` | ✅ Toolbar hubs (**Windows / Settings / Zones / Editors**, stay-open) + `.menu` popup tree. `app.rs:7060-7098` | ⚠️ Fixed chrome; touch wheel + drawers |
| **Open settings** | ✅ `.settings`. `commands.rs:2466` | ✅ Toolbar **Settings** hub: **All Settings…** + per-section jump; or `.settings`. `app.rs:7076-7089` | ⚠️ Settings sheet exposes registry over the wire |

## Frontend-exclusive capabilities

**GUI-only:** stay-open toolbar hubs (Windows/Settings/Zones/Editors); per-window graphical **Appearance** (skin frame, background, accent, corner radius, doll/compass/hand art) via right-click; skins (`.setskin`/`.makeskin`); true-delete (Window Editor); `.controller`, `.snapdebug`, shell zones (`.header`/`.footer`/`.leftbar`/`.rightbar`), WebUI bridge; **`Ctrl+C`=quit** (GlobalKeybinds, GUI-only). `command_help.rs:178-182`

**TUI-only:** `.setpalette`/`.resetpalette` (terminal 256-color); `.deletewindow` hide-redirect; Ctrl+C copies.

**Mobile-only:** touch radial wheel from **puck** (long-press 300ms, not open-anywhere — `app.js:1826-1838`); **macro rail/tray** (tap fires, long-press 450ms edits; create via **＋ New button…** → editor with tap-modes send/type/type-then-send — `app.js:4363-4422`); **two modes**: sidecar (second screen, `session_control:false`) vs headless/standalone (login screen). `app.js:800-802,933`

**Mobile CANNOT author:** layout/panels, resize, game keybinds, macro conditions. **CAN author:** macros, touch-wheel slices, highlights **incl. redirects & squelch** (despite stale doc), colors, controller binds, full settings registry.

## Mobile UI surface names (use these EXACT names — don't invent)
- **Status drawer** (right, `#drawer-right`): sections **Targets**, **Players** (who's-here list; tap row → noun menu), **Room** (name+desc), injury doll, hands, effects. `renderStatusDrawer` `app.js:5311-5390`
- **Macro tray** (left drawer, `#drawer-left`). `app.js:4877`
- **Stream filter chips** (`#chips`) — these are STREAM filters, NOT related to players. Don't call anything a "Players chip". `app.js:283`
- **Touch-wheel "Players" slice** → opens right drawer, scrolls to Players. `app.js:1563,1651`

## Canonical registries (cite these, don't re-derive)
- **Dot-command dispatcher:** `commands.rs` between `=== COMMAND ARMS BEGIN ===` (`:1820`) and `END` (`:2664`)
- **Help table (tripwire-tested to match dispatcher):** `command_help.rs`
- **UiAction protocol:** `data/ui_action.rs`
- **Valid widget types:** `WidgetType::VALID_TYPES`, `window.rs:112-144`
