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
> **Verified against current source on 2026-08-10**, after the window context-menu
> overhaul (`8cd3b951` — the GUI Window Editor was deleted and replaced by a sectioned
> right-click menu; `69acf18e` — detached windows got the same menu) and the
> delete/hide parity fix. Citations are `file:line` under repo root; prefer full paths so
> a re-grep lands. `—` = n/a for that frontend.
>
> **⚠️ Corrections vs the (lost) prior matrix — do not regress to the old claims:**
> - **TUI `Ctrl+C` COPIES, it does NOT quit.** `quit=ctrl+c` is on `GlobalKeybinds`, consumed
>   by the **GUI only**. TUI quits via `.quit`/`.exit`. (`command_line.rs:81`, `keybinds.rs:27-43`)
> - **`.deletewindow` truly deletes in both frontends (since 2026-08-10).** It used to redirect
>   to `hide_window` in the TUI; it now calls the same `delete_and_stash_window` core path the
>   GUI menu uses. Hiding is `.hidewindow`/`.hidewin`, which also works in both.
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
| ~~**Delete vs Hide** asymmetry~~ — **RESOLVED 2026-08-10 by fixing the product** | TUI, GUI | **[fixed]** | Nisugi's call: parity, not documentation. `.deletewindow` now truly deletes in BOTH frontends (was a `hide_window` redirect in the TUI), and `.hidewindow`/`.hidewin` already worked in both. Delete stashes for restore and is refused for the only main-feed window. Document ONE behavior. `src/core/app_core/state/window_lifecycle.rs:374-405`, `src/frontend/tui/menu_actions.rs:248` |
| Can't add/move/resize panels | Mobile | **[by-design]** | Fixed chrome; snapshot+deltas stream. `app.js:808-840` |
| No per-window move/resize by coordinate/command | TUI, GUI | **[doc-only]** | Mouse-drag is the intended gesture |
| No standalone `.showwindow`/`.unhide` | TUI | **[doc-only]** | Restore via `.menu` Windows submenu or `.loadlayout`. `menu_actions.rs:238` |
| No GUI button to SAVE a layout | GUI | **[doc-only]** | Save is `.savelayout <name>` typed; catalog hint even says so. `menus.rs:1224` |
| Skins / per-window graphical appearance / doll-compass-hand art | TUI, Mobile | **[by-design]** | GUI-only forever (terminal/phone-chrome can't render this authoring) |
| Game keybinds editing | Mobile | **[by-design]** | Host-owned `keybinds.toml`, pushed read-only. `app.js:838-839` |
| Macro `hidden_when`/set conditions | Mobile | **[repair?]** | Phone macro editor: label/command/color/tap-mode only. `app.js:4386-4422` |
| **Mobile highlight editor redirects/squelch** | Mobile | **[doc-bug]** | `web.md:164` + `app.js:3410-3412` comment say phone CAN'T; **code DOES** (`app.js:3509,3660,3523-3526`). Fix the doc. |
| Copy: TUI `Ctrl+C` copies (not quit) / plain-text only (all) | all | **[doc-only]** | Plain-text-only copy is deliberate ([[styled-clipboard-copy-rejected]]). |
| **Targets click fires `target`, but the phone opens a verb menu** | Mobile | **[repair?]** | Desktop targets rows send `target #id` directly; `tapCreature` sends `link_tap` for Targets AND Players rows. Unify, or document as-is? `boards.rs:1039` vs `app.js:5388` |
| **No items surface on mobile** | Mobile | **[by-design?]** | Room objects reach the phone via interact mode's **Objects** category and the drawer's **Room** section only; no dedicated Items section. `app.js:2241-2243,5365-5413` |
| **No CONTAINER surface on mobile** (inventory HAS one) | Mobile | **[by-design?]** | `inv` is a first-class **stream filter chip** labelled **Inventory**, deliberately kept out of `HIDDEN_STREAMS` with a source comment ("so a phone-only player can read carried items", `app.js:25-33`), plus a default touch-wheel slice `open:inv` (`app.js:1619`) that switches the CHIP, not a drawer section (`app.js:1707-1710`). **Containers have no phone surface at all.** |
| **Inventory `buffer_size` is authored but never read** | TUI, GUI | **[repair?]** | Both editors expose it and the preset ships 0, but `init_windows` hard-codes 10000 (`state/windows.rs:243-247`) and `add_new_window` hard-codes 0 (`:610-614`). Content is replaced wholesale each update, so the control does nothing either way. |
| **Container position memory is TUI-only** | GUI | **[repair?]** | TUI drag-release writes `widget_state.toml` `[containers]` (`frontend/tui/input.rs:1267-1288`); the GUI never persists ephemeral container geometry, so every container window reopens centered at 40x15. |
| **Bare `.hidecontainers` closes dialog panels too** | TUI, GUI | **[repair?]** | `close_all_ephemeral_windows` iterates the whole `ephemeral_windows` set while reporting the count as "container window(s)" (`window_lifecycle.rs:1127-1142`). Misleading message, not a wrong action. |
| **Inventory/Reserve click hit-test assumes a border** | TUI | **[repair?]** | `room_window_ops.rs:236` hardcodes `border_offset = 1` ("Inventory windows always have borders"); a borderless window mis-registers row clicks by one. |
| ~~**`wordwrap` authored in the TUI but never applied**~~ — **FIXED 2026-08-11 (`4f959d5d`)** | TUI | **[fixed]** | `InventoryWindow::new` hardcoded wrap off and `sync_inventory_windows` never revisited it, so one layout rendered two ways. The sync path now reads the flag and forces a refill on change. |
| ~~**`.go2 targets` help strings contradict the code**~~ — **FIXED 2026-08-11** | TUI, GUI | **[fixed]** | `command_help.rs:116` and `editors/settings.rs:1186` both said `targets` lists saved targets; it lists reachable tagged destinations, and `saved` is the saved list. |
| **Effect times don't tick on desktop** | TUI, GUI | **[repair?]** | `expires_at` exists (`src/data/widget.rs:406-411`) but is read ONLY by `conditions.rs:158,334` and `hotbar.rs:106` — **never by a renderer**. Both desktop frontends draw the frozen server `time` string; only the web client ticks at 1 Hz (`app.js:647-657,760-763`). Conditions are therefore accurate while the window is stale. |
| **`setEffects` doesn't re-render an open status drawer** | Mobile | **[repair?]** | `app.js:647-657` ends at `renderEffects()`; every other setter guards with `renderStatusDrawer()`. Rows added/dropped while the drawer is open don't appear until something else re-renders. Existing rows keep ticking. |
| ~~**`missingspells` absent from `VALID_TYPES`**~~ — **FIXED 2026-08-11 (`8ca79f68`)** | TUI | **[fixed]** | Now at `src/data/window.rs:134`. It was only ever missing from the "Valid types:" error list — the picker is gated by `addable_by_category`/`CATALOG` (`presets.rs:188`, ungated) and always offered it. **`spells.md` claimed the opposite twice; corrected in Wave 7.** |
| **Perception settings are TUI-only; the GUI silently ignores them** | GUI | **[repair?]** | `sort_direction`, `use_short_spell_names`, `text_replacements` applied at `frontend/tui/sync.rs:2338-2391`. `gui/app/widgets/panels.rs:339-377` paints `entry.raw_text` in core's hard-coded descending order (`buffers.rs:227`) with no abbreviation, replacement, or re-sort. **One layout renders two ways.** |
| **Perception `stream` + `buffer_size` authored but never read** | TUI, GUI | **[repair?]** | Preset ships `stream="percWindow"`, `buffer_size=100` (`presets.rs:1599-1600`); the flush path reads neither (`buffers.rs:187-255`). Both editors deliberately omit them. Same shape as the Inventory `buffer_size` row. |
| **MiniVitals GUI settings are GLOBAL, not per-window** | GUI | **[repair?]** | `window_config.rs:351-353` reads one `ui_settings.vitals` for every MiniVitals window; `SetVitals` writes it back globally (`:668-673`). **Two MiniVitals windows cannot differ in the GUI.** The TUI stores per window in `layout.toml`, and the two frontends share no fields at all. |
| **MiniVitals: GUI draws 8 bar kinds, TUI accepts 4** | TUI | **[repair?]** | `VitalKind` (`gui/persistence.rs:166-178`) adds Mind, Encumbrance, NextLevel, Blood. `tui/minivitals.rs:190-218` accepts only `health\|mana\|concentration\|stamina\|spirit` and **silently drops** the rest via `filter_map`. A GUI-authored layout renders in the TUI with bars missing, not an error. |
| **Betrayer has no GUI widget section** | GUI | **[repair?]** | `widget_section_label` returns `None` for `W::Betrayer` (`window_config.rs:140-161`), so `show_items` and `bar_color` are TUI-only authoring. The GUI honors `show_items` from the layout but hardcodes the bar to `#cd4d4d` (`panels.rs:316-322`), never reading `bar_color`. |
| **Encumbrance color bands differ and only the TUI can author them** | GUI | **[repair?]** | TUI: four bands 0–20/21–50/51–80/81–100 with user colors (`tui/encumbrance.rs:146-153`). GUI: three fixed bands 0–33/34–66/67+ hardcoded, reading none of the four color fields (`panels.rs:282-286`). Each frontend also offers one toggle the other lacks (`show_bar` GUI-only, band colors TUI-only). |
| **Custom quickbars have no in-app editor** | TUI, GUI, Mobile | **[repair?]** | `[[quickbars.custom]]` is TOML-only (`config/widgets.rs:940-973`, applied `state.rs:1061-1165`); no `.quickbars` command, and `quickbars` sits in `EXEMPT_PREFIXES` (`config/registry.rs:141`). The one live violation of "every feature ships its editor". |
| **Quickbar is `Tab`-unreachable by default** | TUI | **[repair?]** | Arrow/Enter navigation needs focus (`input_handlers.rs:148-177`), but `"quickbar"` ships in `default_focus_exclude` (`src/config.rs:392`). Mouse clicking works regardless; the keyboard path is dead until the user edits `[ui.focus] exclude`. |
| **Six parsed GS4 experience fields are never rendered** | TUI, GUI | **[repair?]** | `field_exp`, `max_field_exp`, `until_next`, `fashlonae`, `lumnis`, `rpa` parsed off the `mindState` bar (`parser/handlers.rs:320-335`), stored (`core/state.rs:730-746`), read by no renderer. Only total and ascension exp have toggles. |
| **`dr_experience` is never cleared** | TUI, GUI | **[repair?]** | `DRExperienceState::clear` (`core/state.rs:653-656`) has no caller, so field order and values accumulate across character swaps for the process lifetime. |
| **`layout_template.toml` documents four nonexistent DR experience keys** | — | **[doc-bug]** | `defaults/globals/templates/layout_template.toml:342-345` lists `stream`, `buffer_size`, `show_rates`, `compact_mode`. `ExperienceWidgetData` has exactly one field, `align` (`config/widgets.rs:1119-1123`). Silently ignored if copied. |
| **`layout_template.toml` documents nonexistent quickbar selection keys** | — | **[doc-bug]** | `:780-781` advertises `selection_fg` / `selection_bg` as window keys. `WindowBase` has no such fields; the TUI feeds selection colors from `theme.text_selected` / `theme.background_selected` (`tui/sync.rs:983-986`). |
| **Stale source comment — Targets category** | — | **[doc-bug]** | `editors/settings.rs:12-14` says the Targets category moved to the Window Editor (`editors/windows.rs`), which is deleted. It lives in **Settings** (`settings.rs:41,1192-1200`); the window menu only links to it. Comment-only fix. |

---

## Launching & sessions (pre-session — added 2026-08-10)

> The matrix used to cover only IN-session gestures, so the first pages written
> against it (introduction, installation, launcher) had no rows to transcribe.
> These were verified directly in source during Wave 1.
>
> **Two different things are called "the Launcher."** The **Launcher** is the
> graphical connection list (`launcher.toml`, double-click / `--launcher`). The
> **SSH Launcher** is a separate in-session panel (`ssh-launcher.toml`,
> `.launcher` / `.launch`) that cold-starts a headless Lich on another machine.
> Never conflate them on a page.

| Task | Terminal (TUI) | Desktop GUI | Mobile / Web |
|------|----------------|-------------|--------------|
| **Open the Launcher** | ❌ n/a — the Launcher is an egui window; `run_launcher()` is eframe-only. `src/main.rs:424` | ✅ Run with **no arguments** (double-click) or `--launcher`. Window **"VellumFE Launcher"**, heading **VellumFE**, subheading **"Choose a connection to launch"**. `src/frontend/gui/launcher.rs:787-788,813` | ❌ the apps use their own login screen |
| **Add a connection** | ❌ | ✅ **➕ New connection** → form headed **New connection** → **Save**. `launcher.rs:387,415` | ❌ |
| **Launch / Edit / Delete** | ⚠️ `--launch-profile "<name>"` runs a saved connection by name | ✅ Per-row **Launch**, **Edit**, **Delete**; Delete confirms in a **"Delete profile?"** window. `launcher.rs:754` | ❌ |
| **Save a password** | ❌ | ✅ **Save password** checkbox → OS credential store via the `keyring` crate, service id `vellum-fe`, keyed by the LOWERCASED account. `launcher.rs:472`, `src/config/profiles.rs:23,293-294` | ❌ |
| **Set a connection's frontend** | ❌ | ✅ **Advanced** ▸ **Frontend** ▸ **GUI** / **Terminal**. **Default is GUI.** `src/config/profiles.rs:79-83` | ❌ |
| **Cold-start a remote Lich** | ⚠️ `.launch`/`.launcher` dispatch a UiAction; the panel itself is GUI-only | ✅ `.launcher` opens the **SSH Launcher** panel; `.launch <character>` runs the flow. `src/frontend/gui/app/editors/launcher.rs` | ❌ |

**The default-frontend trap (state this on any page that launches anything):**
the CLI's `--frontend` defaults to **`tui`** (`src/main.rs:40`), while a saved
connection's frontend defaults to **GUI** (`profiles.rs:79-83`). The same
character started two ways lands in two different interfaces.

**`--frontend` has THREE values, not two:** `tui`, `gui`, and **`headless`** —
core plus web server only, no local UI, with a browser at `/play` as the
interface (also what the Android shell drives). `src/main.rs:126-132`

**`--profile` is not `--character`.** `--character` is the *login* name (Lich
proxy selection + direct login); `--profile` picks the *config directory* under
`profiles/<name>/` and silently falls back to `--character` when omitted — which
is why each new character starts with a blank layout. `src/main.rs:440,482-499`,
`src/config/paths.rs:83-86`

**Direct-login password resolution:** `--password` → `connection.password` →
a hidden terminal prompt `"Password for account {account}: "`. The prompt is
**desktop-feature only**; headless/Android builds must supply it up front.
`src/network.rs:271-284`

**Game-name spellings differ between CLI and config.** CLI (clap value enum) is
hyphenated — `dr-platinum`, `dr-fallen`, `dr-test`; config/profile strings are
not — `drplatinum`, `drfallen`, `drtest`. **An unrecognized `game` in config
silently falls back to GemStone IV Prime** rather than erroring
(`src/network.rs:240`). `src/main.rs:135-146`

**Subcommands run and exit instead of connecting:** `validate-layout`,
`migrate-layout`, and `import-highlights` (Wrayth/StormFront XML → TOML — the
third is undocumented elsewhere). `src/main.rs:165-206,347`

**The default layout ships six windows:** `main`, `command_input`, `thoughts`,
`speech`, `room`, `society`. Everything else is user-added.
`defaults/globals/layouts/layout.toml`

**Connection list summary format:** `<character> @ <game>` for direct,
`<character> via Lich @ <host>:<port>` for Lich. The account is deliberately
NOT shown (source comment says the list is on screen and in screenshots
constantly). `profiles.rs:187-197`

**`.quit` vs `.exit`:** `.quit` disconnects but keeps the window open — run it
again, or use `.exit`, to close. Governed by `ui.keep_open_on_quit`.
`src/config/registry.rs:394-395`

## Core layout gestures

| Task | Terminal (TUI) | Desktop GUI | Mobile / Web |
|------|----------------|-------------|--------------|
| **Add / show a window** | ✅ `.addwindow` takes **1 or ≥6 arguments — never 2–5**. No args → **picker**; full form `.addwindow <name> <type> <x> <y> <w> [h]` (h defaults 10; `x`→Col, `y`→Row; unparseable numbers fall back silently to x/y 0, w 40). **`.addwindow players` alone prints usage and adds NOTHING.** `name` is a free label, not a catalog key — the window is built from `<type>`, not a preset. Types = `WidgetType::VALID_TYPES` (`src/data/window.rs:116-148`); the in-app usage string is stale and lists `hands`, which is not a type (`hand` is). `src/core/app_core/commands.rs:2295-2317`, `state/window_lifecycle.rs:1175-1183,1318` | ✅ **Top-toolbar "Windows" button → stay-open inline catalog:** checkbox per window (tick=show/untick=hide), per-row **zone** submenu, **➕ Custom window…**, **↩ Restore deleted…**. Also `.addwindow` typed. **NO right-click add.** `app.rs:7060-7068`, `known_windows.rs:74-299` | ❌ **Not available** — fixed chrome; surfaces render from server snapshot+deltas. `app.js:808-840` |
| **Move a window** | ✅ Left-drag the **top row (title bar)**. Locked = grab but no move. `src/frontend/tui/input.rs:72-105,1675-1769` | ✅ **Drag anywhere** (body/title, no modifier). Snap-docking active; **hold Shift to suspend**. Also right-click **Arrange ▸ Move Window** (works locked). `zones.rs:2013-2024,1561`, `menus.rs:1603` | ⚠️ Only floating elements move: **wheel puck** (drag >8px), **macro/compass** (long-press 450ms), **interact bar** (drag >8px). No game windows. `app.js:1887,4302,5003,2418` |
| **Move a window to ANOTHER ZONE** | ❌ the TUI has no shell zones | ✅ Three ways: **Alt+drag** the window (a tinted overlay highlights the zone under the pointer; normal movement is suppressed while Alt is held), right-click ▸ **Arrange ▸ Move to ▸** *zone*, or the per-row **zone dropdown** in the Windows catalog. `src/frontend/gui/app/zones.rs:988-1000,1033-1045,1759`, `menus.rs:1770-1783`, `known_windows.rs:311-330` | ❌ (fixed chrome) |
| **Resize a window** | ✅ Drag **right col / bottom row / bottom-right corner**. Locked = no resize. `.resize` refits layout (not per-window). `input.rs:81-105` | ✅ egui native **edge/corner** drag; snaps unless Shift held. `zones.rs:1986,2047` | ❌ Not available — fixed-size chrome. `app.js` (no resize handles) |
| **Hide a window** | ✅ `.hidewindow`/`.hidewin [name]` (no name → picker). Can't hide the sole main-feed window. `src/core/app_core/commands.rs:2584`, `src/core/app_core/state/window_lifecycle.rs:100-115`, `src/frontend/tui/menu_actions.rs:248` | ✅ Right-click ▸ **Hide**, or untick in the Windows catalog (same layer). `src/frontend/gui/app/menus.rs:1707`, `known_windows.rs:315` | ❌ (fixed chrome) |
| **Delete a window (true removal)** | ✅ `.deletewindow`/`.delwindow <name>` — truly removes it from the layout and stashes it for restore. Refused for the only main-feed window. `src/core/app_core/state/window_lifecycle.rs:374-405` | ✅ Right-click ▸ **Window** ▸ **Advanced** ▸ **Delete Window…**, then a two-step confirm ("Really delete this window from the layout?" ▸ **Delete** / **Cancel**). Same core path as the dot-command. `src/frontend/gui/app/window_config.rs:843-869` | ❌ |
| **Restore a hidden/deleted window** | ⚠️ Hidden → `.menu` **Windows** submenu (`ShowWindow`). Deleted-stash → menu only. No `.showwindow`. `src/frontend/tui/menu_actions.rs:238` | ✅ Windows catalog: re-tick to unhide; **↩ Restore deleted…** for stashed. `known_windows.rs:168`, `src/core/app_core/state/window_lifecycle.rs:483` | ❌ |
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
| **Context menu (window chrome)** | ✅ **Right-click title bar** → Close Window / Edit Window… / Open Menu. The TUI keeps its own **window editor**. `src/frontend/tui/input.rs:987-1051` | ✅ **Right-click window** (not a link) → sectioned menu: flat **Hide** / **Detach** / **Send to Back** (`src/frontend/gui/app/menus.rs:1707-1729`), then collapsible **Window** (`:1741`), the widget's own section, **Arrange** (`:1767`), **Appearance** (`:1823`), **Group** (`:1831`), each with an **Advanced** fold. **The GUI Window Editor is gone — this menu is the only home for per-window settings** (`menus.rs:2202`); settings apply live, with no Save. | ⚠️ Long-press floating elements → arrange/edit sheet |
| **Open the main menu** | ✅ `.menu` (**no default keybind**). `commands.rs:2645` | ✅ Toolbar hubs (**Windows / Settings / Zones / Editors**, stay-open) + `.menu` popup tree. `app.rs:7060-7098` | ⚠️ Fixed chrome; touch wheel + drawers |
| **Open settings** | ✅ `.settings`. `commands.rs:2466` | ✅ Toolbar **Settings** hub: **All Settings…** + per-section jump; or `.settings`. `app.rs:7076-7089` | ⚠️ Settings sheet exposes registry over the wire |

## Per-window settings — where each control lives (GUI)

> Added 2026-08-10. Every widget page's §2/§4 draws from here, so transcribe
> rather than paraphrase. The GUI Window Editor no longer exists; all of this is
> in the window's **right-click menu**, applied live (no Save button). Text
> fields commit on **Enter** or when focus leaves.
> Source: `src/frontend/gui/app/window_config.rs`, `src/frontend/gui/app/menus.rs`.

| Setting | Where in the menu | Cite |
|---|---|---|
| Title, title-bar text, streams (+ seen-stream **+** picker), buffer lines, speak-new-lines (TTS), **Lock in place** | **Window** | `window_config.rs:1010-1090` |
| Compact, timestamps (+ at-line-start), **Delete Window…** (two-step confirm) | **Window ▸ Advanced** | `window_config.rs:820-869` |
| Timer/Bar feed id, label, color, display mode, stay-visible-at-rest | *widget section* (named for the widget) | `window_config.rs:900-1000` |
| Effects category · Room sections · Targets/Experience/Encumbrance toggles · Vitals bars | *widget section* | `window_config.rs:1000-1110` |
| Font, text size, word wrap, content alignment | **Appearance ▸ Text** | `menus.rs:2056+` |
| Show title bar, title alignment | **Appearance ▸ Title bar** | `menus.rs:2056+` |
| Border on/off + style, accent color, skin frame, background | **Appearance ▸ Frame** | `menus.rs:2056+` |
| Title-bar height, border sides, corner radius, frame scale | **Appearance ▸ Advanced** | `menus.rs:2330+` |
| Move Window, Move to *zone*, Release Anchors | **Arrange** | `menus.rs:1767+` |
| Fixed size | **Arrange ▸ Advanced** | `menus.rs:1767+` |

**Dedicated editors launched from the widget section** (these open a panel; the
rest of the menu is inline): **Edit tabs…** (tabbed windows, `window_config.rs:1119`),
**Edit hotbars…** (`:1127`), **Edit indicators…** (`:1135`), **Global target
settings…** → Settings ▸ Targets (`:1143`), **Edit dashboard…** (`menus.rs:1997`),
**Hand icons…** (`menus.rs:2005`), **Calibrate doll…** (`menus.rs:1992`), **Open
Map Explorer** (`menus.rs:1989`).

**Detached windows** get the same menu, with **Reattach** replacing Detach and no
Arrange/Group/Send-to-Back (meaningless in an OS window); drag the body to move
one. `src/frontend/gui/app/detached.rs`, `menus.rs:1707-1729`.

## Mobile connection modes (added 2026-08-11)

> **Why this block exists:** the "two modes: sidecar vs standalone" line further
> down describes the **browser** client. Wave 1 read it as covering the native
> apps too and published a wrong claim ("the apps are not remote controls for a
> desktop session" — they are). A row about one frontend's shape is NOT a row
> about another frontend. Re-verify per surface.
>
> The phone's login screen is the **web client's login overlay rendered in a
> WebView**, not native UI. Native Swift/Kotlin contributes one extra screen: a
> saved **Characters** picker. Four capabilities, two surfaces.

| Capability | Where the user finds it | Cite |
|---|---|---|
| **Direct play.net login** | Tab **`play.net`** (default). Placeholder-only fields: `play.net account`, `password`, `character`, a Game `<select>`, checkbox **Remember this login**. Submit is **Connect**. | `src/frontend/web/assets/index.html:137-161,206` |
| **Attach to a running Lich** | Tab **`Lich`**: `host`, `port`, `label (optional)`, checkbox **Remember this connection** (checked by default). | `index.html:139-178` |
| **Cold-start Lich over SSH** | **Not a fourth tab** — a `custom launch command (optional)` textarea *inside* the Lich tab, plus a **SSH launcher** sheet under the gear ⚙. Connecting probes the port, SSHes in if Lich is down, then attaches. | `index.html:169-173`, `src/frontend/web/assets/app.js:3359,5886-5973`, `src/launcher/ssh.rs` |
| **Remote-connect to a running VellumFE session** | The in-page **`Remote`** tab is **deliberately hidden on phones** — the native picker replaces it. Phones reach it via the person icon → **Characters**. | `index.html:141`, `app.js:1193` (`modeRemoteBtn.hidden = !inShell \|\| nativePicker`) |

**So: a desktop browser shows three tabs; a phone shows two tabs plus a
Characters picker.** The user-facing reason (say this, don't document the flag):
the app swaps the in-page Remote tab for a native picker so it can scan pairing
QR codes and seal saved servers into the Keychain/Keystore.

**The Characters picker — quote per platform, do NOT blend:**

| | iOS | Android |
|---|---|---|
| Title | **Characters** (`ios/app/Sources/RemotePickerView.swift:78`) | **Characters** (`android/app/src/main/java/dev/vellumfe/RemotePickerView.kt:60`) |
| Add buttons | **Scan QR to add** (`:51`), **Add manually** (`:58`) | **⧉  Scan QR to add** (`:70`), **＋  Add manually** (`:71`) |
| Back | accessibility label **Back to login** (`:71`) | **‹  Back to login** (`:79`) |
| Empty state | "No saved characters yet — scan a QR or add one manually." (`:25`) | "No saved characters yet. Scan a pairing QR or add one manually." (`:101`) |
| Delete | swipe + `EditButton` (`:36-40,79`) | **✕** → dialog **"Delete {name}?"** / "Removes the saved server (and its pairing token)." (`:156-157`) |

Manual-add form is **Add character** on both, with fields **Label (required,
e.g. Rysk)**, **Host (e.g. 192.168.1.21)**, **Port (e.g. 8042)**, **Pairing
token (optional)**, and Cancel / Save. Rows show `host:port` plus a live/offline
dot from a `/health` probe.

**Pairing:** QR payload `vellum://remote?host=…&port=…&token=…&name=…`,
generated PC-side by `.webinfo`. A second deep link `vellum://lich?host=…&port=…`
prefills the Lich tab and never auto-connects.

**Credential storage** (players ask): iOS Keychain, service
`dev.vellumfe.remote-server`, accessibility after-first-unlock-this-device-only;
Android Keystore key alias `vellum-master` sealing `remote.bin` in app-private
files. Never SharedPreferences, never plaintext. play.net passwords are sealed
by the Rust core with a device master key.

## Widget feed bindings and traps (added 2026-08-11)

> Verified while writing the Wave 4 widget pages. Several of these contradict
> what the old pages said.

**Feed-id vocabulary (hover text is verbatim in source, `window_config.rs:891-900`):**
- **Progress `Bar id`**: `health`, `mana`, `stamina`, `spirit`, `encumlevel`,
  `mindState`, or a custom id pushed by Lich. The catalog also ships
  `concentration` (DR only) and `stance`, whose feed id is `pbarStance`.
- **Countdown `Timer id`**: `roundtime`, `casttime`, `stuntime`, a custom
  `[event_patterns]` `event_type`, or an id a script pushes via
  `<vellumTimer id='...' value='epoch'/>` (`value` = absolute epoch second the
  timer ends; `0` clears).

**⚠️ The name-fallback asymmetry — a real trap.** A **countdown** window matches
on `countdown_id` **OR the window's name** (`element.rs:38`), so
`.addwindow roundtime countdown …` works with no id set. A **progress** window
matches on `progress_id` **only** (`element.rs:919`) — naming a window `health`
does nothing. Never write the two as if they behave the same.

**A blank custom bar or timer renders as NOTHING** until it has a feed id, which
is why creating one from **➕ Custom window…** auto-opens its menu
(`src/frontend/gui/app.rs:1408-1413`). Feed ids are **case-sensitive**.

**`compact` is bounty-specific, not a blank-line stripper.** Gated on
`content.compact && current_stream == "bounty"` (`messages/flush_line.rs:468-469`);
inert on every other stream. Toggling it re-renders the existing bounty
immediately via `refresh_bounty_window`. The old page called it "Remove blank
lines" — wrong.

**`buffer_size` default is 10,000** (`src/config.rs:485-487`), and on a tabbed
window it is **per tab**.

**Word wrap is shared for text windows, GUI-only for tabbed ones.** `Text`,
`Inventory`, and `Reserve` defs carry `wordwrap` and it reaches `layout.toml`
and the TUI; `tabbedtext` has no such field, so the setting is a GUI-local
per-tab preference (`menus.rs:801-818`).

**Room `show_name` is TUI-only and is NOT the border title.** In the TUI it
prepends the room name as a bold content line; the border title is the window's
own `title` (default `"Room"`). **The GUI never reads `show_name`** — it always
draws the room name as the first content line (`gui/app/widgets/boards.rs:604-637`).

**Hand identity is by name substring, with a silent fallback:** `left` →
LEFTHAND, `right` → RIGHTHAND, **anything else (including a typo) → SPELLHAND**
(`menus.rs:1004-1017`). Hand icon size follows window height, floored by the
configured size (`widgets/panels.rs:55-63`). Empty reads `Empty` for the physical
hands and `None` for the spell slot.

**The compass window is FREE-FORM, never square-locked.** The rose paints as a
centered square of `min(width, height)`, so growing one axis pads evenly while
the art holds size and growing both scales it. **Compass windows floor at 48pt
width; every other widget floors at 120pt.** `widgets/map_compass.rs:206-219`,
`window_manager.rs:506-512`.

**No blink/flash/pulse/threshold color exists on ANY bar or timer** — not
vitals, not standalone `progress`, not `countdown`. Alerting is a hotbar state,
an indicator, a sound, or rumble.

**Tab-switching actions ship UNBOUND:** `next_tab`, `prev_tab`, and
`next_unread_tab` exist in the **Tabs** keybind category with no default key
(`src/config/keybinds.rs:423-425`).

**The Tab Editor is Save-buffered** while the rest of the window menu is
live-apply. Its two stream lists differ: **Known streams** is the full persisted
registry; the per-row **+** is seen-this-session only. Unticking a Known stream
removes it from **every** tab (`editors/tabs.rs`).

**TUI authoring gaps (renders fine, cannot author):** countdown **Stay visible
at rest** has no TUI field though `sync.rs:747` honors it.

### Entity list widgets — targets, players, items (added 2026-08-11)

**One component feeds two of the three.** `<component id='room objs'>` is parsed
once (`src/core/messages/component.rs`), and **boldness is the splitter**: `<a>`
links inside `<b>…</b>` become **creatures** (`component.rs:229-244`), links
outside bold become **room objects** (`component.rs:333-356`). Players come from
a separate room-players component into `GameState.room_players`
(`src/core/state.rs:563-579`).

**The targets list has TWO independent gates; conflating them is the trap.**
1. **Hostile gate** — requires a `<crtrStatus>` snapshot with `hostile="1"`.
   **`flags: None` (no snapshot seen yet) is excluded by design**, not a bug
   (`boards.rs:988-993`, `targets.rs:122-134`; test `targets.rs:550-579`).
2. **`is_valid_target`** — dead/gone, `animated*` (except "animated slush"),
   appendages, and `excluded_nouns`. **It deliberately does NOT check
   hostility** (`src/core/state.rs:426-454`).

**⚠️ Appendages are NOT severed body parts.** They are limbs summoned from the
ground that attack you — a sorcerer's **Grasp of the Dead (709)** and similar.
They are targetable but **cannot be damaged**, so filtering them keeps the list
actionable. Regex on the **noun**:
`^(?:arm|appendage|claw|limb|pincer|tentacle)s?$|^(?:palpus|palpi)$`, overridden
when the **name** matches `(?:amaranthine|ghostly|grizzled|ancient) kraken
tentacle` — those four are real, killable creatures (`state.rs:406-426`).
*(The old source comment said "severed arms"; corrected 2026-08-11 per Nisugi.)*

**The appendage footer counts HOSTILE appendages only.** The hostile check
`continue`s *before* `body_part_count += 1` in both frontends
(`boards.rs:991-997`, `targets.rs:127-141`), so an appendage with no hostile
snapshot is never counted. The footer means "how many hostile appendages were
hidden", not "how many are in the room".

**Status abbreviation fallback is by CHARS, not bytes.** `config.status_abbrev`
lookup is lowercased; on miss, `s.chars().take(3)` — and a status of ≤3 chars
passes through whole (`targets.rs:178-195`, `players.rs:86-95`). Test: `"awake"`
→ `[awa]` (`players.rs:405-427`). Defaults ship 12 entries.

**Multi-status only comes from the structured feed.** `<crtrStatus>` yields
`[stu,prn]`; the legacy text parse contributes at most one (`state.rs:390-401`).
Dead leads the tag list for players: `Regyy [ded] [prn]` (`players.rs:74-81`).
**Dead creatures are filtered from targets entirely** — there is no `[ded]` row
there; that styling is players-only.

| Task | Terminal (TUI) | Desktop GUI | Mobile / Web |
|---|---|---|---|
| **Click a TARGET row** | ✅ Sends **`target #<id>` directly** — no verb menu. `targets.rs:393-399` → `_direct_` link `room_window_ops.rs:554-565` | ✅ Sends **`target #<id>` directly**. `boards.rs:1035-1041` | ⚠️ **Opens the server verb menu instead** — `tapCreature` → `link_tap`. `app.js:5388,4951-4966` |
| **Click a PLAYER row** | ✅ `LinkData` → **server verb menu**. `players.rs:209-212` | ✅ Same. `boards.rs:1079-1088` | ✅ Same. `app.js:5408-5409` |
| **Click an ITEM row** | ✅ `LinkData` → verb menu. `room_window_ops.rs:464-510` | ✅ verb menu via `request_menu` → `_menu #id`. `panels.rs:394-418` | ❌ **No items surface.** Reachable via interact mode's **Objects** + drawer **Room** section. `app.js:2241-2243` |
| **Right-click a row** | ❌ **No per-row right-click** — it's the window menu | ❌ Same | ❌ |
| **Drag a row** | ⚠️ Items are drop targets (`input.rs:1121-1128`) | ✅ **Items only** — `Sense::click_and_drag()`. Targets/players are `Sense::click()`. `panels.rs:394-418` vs `boards.rs:1033,1076` | ❌ |
| **Count in title `[04]`** | ✅ All three. `targets.rs:305`, `players.rs:155`, `items.rs:95` | ❌ **No count suffix** — no `{:02}` under `src/frontend/gui/` | ❌ |
| **Empty state** | (blank) | ⚠️ **Items only**: `"No objects here."` `panels.rs:381-384` | — |

**Per-window settings (targets only) — GUI right-click ▸ widget section, live-apply:**

| Control | GUI label | TUI label | Cite |
|---|---|---|---|
| Appendage footer | **Show filtered appendage count** | **Show Appendages** | `window_config.rs:1009`, `render.rs:1272-1273` |
| Status side | **Status position** ▸ Global default / Before the name / After the name | **Status Pos:** ▸ Left / Right | `window_config.rs:1020-1045`, `render.rs:1287-1295` |
| Jump to globals | **Global target settings…** → Settings ▸ Targets | ❌ none; use `.settings` | `window_config.rs:1141-1152` |

`players` and `items` expose **only** `entity_id` in the TUI editor and **no
widget section at all** in the GUI menu (`construction.rs:283-288`).

**Global `[target_list]` lives in Settings ▸ Targets** (`settings.rs:41,1192-1206`):
`status_position` (`"end"`), `truncation_mode` (`"noun"`), `excluded_nouns`
(`["arm","coal"]`), `boss_color` (`#ff5555`), `challenging_color` (`#ffaa55`),
**`dead_color` (`#888888`, the players window's corpse styling)**, plus a bespoke
**Status abbreviations** editor. **`truncation_mode = "noun"` only fires when a
status is present AND the line overflows** — not an always-on shortener
(`targets.rs:200-223`).

### Active effects and spells (added 2026-08-11)

**Effects arrive on `<dialogData>`, not a text stream.** Four literal wire ids
gated at `src/parser/dialogs.rs:409`: `Active Spells` (**with a space**),
`Buffs`, `Debuffs`, `Cooldowns`. The category key strips the space →
`ActiveSpells`. A `<progressBar>` missing any of `id`/`value`/`text`/`time` is
silently dropped (`dialogs.rs:431-452`). Every ActiveEffects window subscribes to
all four **lowercase** streams regardless of its own category
(`routing.rs:373-378`) — three distinct spellings of one concept.

**⚠️ Desktop effect times DO NOT tick.** See the watch-list row. Renderers draw
the frozen server `time` string; **conditions compute from `expires_at` and are
accurate while the window is stale.** Only the web client ticks (1 Hz, anchored
to client `Date.now()` with **no clock-offset correction**, unlike RT/CT).

**No expiry sweep exists.** Effects are removed only by a server `clear='t'`
(`element.rs:1712-1737`). An effect at zero keeps rendering. **Effects never
sort** — first-seen arrival order at all three layers; server
`anchor_left`/`anchor_top` are discarded. **Changing Category clears effects and
bumps `generation`** (`window_config.rs:599-618`).

**The bar tracks `value` (server percentage), not remaining time**
(`boards.rs:803-810`).

**Time format diverges:** TUI `[MM:SS]` / `[HH:MM]` / `[??:??]` **in brackets**
(`active_effects.rs:23-41`); **GUI paints the raw `03:06:54` unbracketed**
(`boards.rs:834`).

**TUI Category is free text, not a dropdown**, and blank silently falls back to
`ActiveSpells` (`window_editor/sync.rs:402-410`). The GUI is a 4-item combo
(`window_config.rs:975-992`).

**`.addwindow` type strings are unforgiving:** `active_effects` **only**
(`activeeffects` rejected); `missingspells` **only** (`missing_spells` rejected)
— `src/data/window.rs:91,94`. `from_str` silently falls back to **Text** for
unknown strings (`:69-71`), so a typo yields a wrong-widget window, not an error.

**Spell colors key on the effect id parsed as u32 (the spell number), resolved
once at parse time** and baked onto the effect (`element.rs:1639-1647`).
Renderers never call `get_spell_color_style`. **`SpellColorRange` is a
misnomer** — an explicit ID list, first match wins; **`bg_color` is parsed but
dropped by `style()`**. Applies to ActiveEffects only — **Missing Spells is
hard-coded amber `#d78700`** (`missing_spells.rs:58`); Spells uses server
styling. **`.spellcolors` is desktop-only** (`headless/runtime.rs:1168`).

**The Spells window is a login-time snapshot**, wholesale-replaced — not a live
"known spells" roster. **Clicking a spell requests a server context menu**
(`input.rs:1221-1233`); it does not prepare or cast. **`.spellwatch` has no
`clear`** — use `rem all`. **`add all` snapshots ActiveSpells + Buffs only**;
Debuffs/Cooldowns deliberately excluded (`missing_spells.rs:8-15`). A single
unparseable number rejects the whole command.

### Indicators and dashboards (added 2026-08-11)

**An indicator window's NAME is its status id** — there is no picker anywhere.
**A dashboard auto-adds an unknown `set_status` id as a new cell; an indicator
cannot**, because the window *is* the id. Exception: the dashboard won't
auto-add an id a combined indicator template already claims in a condition.

**Two Save-buffered editors sit inside the live-apply menu:** the **Indicator
Icons** editor (`editors/indicators.rs:203`, **Save all** at `:210`) and the
**Dashboard** editor. Everything around them applies live.

**`DashboardLayout` has FOUR variants** — Horizontal, Vertical, Grid, **Flow**.
Unrecognized layout strings fall back to horizontal silently. **The TUI grid
truncates past `rows × cols`** (a fifth status in `grid:2x2` is dropped); the GUI
does not. **`hide_inactive` defaults true.** **`stack` collapses entries into one
cell** and they count once for the height cap. Height is capped from the row
count for every layout **except Flow**, which is uncapped because wrapping
depends on width.

**An inactive indicator draws NOTHING — it does not dim.** Inactive art is
opt-in in both frontends, so `inactive_color` has nothing to color unless an
inactive icon is set.

**Mobile indicators live in the STATUS ROW, not the status drawer**
(`#indicators` inside `#status-row`, `index.html:80-89`). The badge set is
exactly nine (`INDICATOR_BADGES`, `app.js:597-607`): DEAD, STUN, BLEED, WEB,
HIDDEN, INVIS, KNEEL, SIT, PRONE. **`poisoned`, `diseased`, `joined`, and
`standing` have none, and custom `set_status` ids never reach the phone.**

**TUI indicator editor has five fields and no conditions** (Id, Title, Icon,
Active color, Inactive color). Multi-condition switching is GUI authoring;
**both frontends display** whatever was authored.

### Inventory, Reserve, and containers (added 2026-08-11)

**Inventory/Reserve are SNAPSHOT windows, not logs.** `flush_inventory_buffer` clears
`content.lines` and refills on every change (`core/messages/buffers.rs:26-37`). An
unchanged snapshot is skipped, not redrawn (`buffers.rs:15`). **`buffer_size` is
authored in BOTH editors but no constructor reads it** — `init_windows` hard-codes
10000 (`state/windows.rs:243-247`), `add_new_window` hard-codes 0 (`:610-614`), preset
ships 0 (`presets.rs:296`). The control is inert. **[repair?]**

**Inventory has NO empty-state placeholder** — unlike Items (`"No objects here."`) and
Container (`"Empty."`).

**⚠️ CONTAINERS ARE OPT-IN. `LOOK IN` does NOT create a window.** A sighting only sets
`newly_registered_container` (`messages/element.rs:1776-1778`); the window is created
only if the title is already in `ui_state.shown_container_titles`
(`window_lifecycle.rs:904-910`). Ticking the Windows-list row inserts it and creates
(`:791-793`); unticking removes both. Session-only, wiped on relog. Test:
`state/tests.rs:1377-1416` ("Sighted while not opted in → no window").

**Ephemeral containers ALWAYS open 40x15 centered** (`window_lifecycle.rs:546-551`) —
game placement hints never reach them.

**⚠️ Container position memory is TUI-ONLY.** Written on TUI drag-release to
`widget_state.toml` `[containers]` (`frontend/tui/input.rs:1267-1288`). The GUI never
writes it. **[repair?]**

**`.hidecontainers` semantics:** bare = `close_all_ephemeral_windows`, which closes
**every ephemeral window including dialog panels** while still reporting "container
window(s)" (`window_lifecycle.rs:1127-1142`). With an arg = a **lowercased,
space-to-underscore, substring** match that can close several at once (`:1145-1172`).
**[repair?]** on the misleading message.

**A layout-declared `widget_type = "container"` IS persistent** (`window_def.rs:490`;
`ContainerWidgetData.container_title`). **Nothing dedupes it against the ephemeral
window** — a different name gives you two windows on one bag. `find_container` matches
exact, then substring, then article-stripped (`core/game_objects/mod.rs:470-492`).

**Neither Inventory nor Container gets a GUI widget section** —
`widget_section_label` returns `None` for both (`window_config.rs:140-161`). The
`menus.rs:967-977` type list gates **Appearance ▸ Text ▸ Word wrap + content
alignment** only. TUI editor: Inventory/Reserve expose Streams, Buffer size, Wordwrap
(no Timestamps); **Container exposes NOTHING** (`construction.rs:289-291`).

**TUI container title carries a live count `[06]` / `(empty)`**
(`container_window.rs:179-183`); the GUI has no count and prints **`Empty.`** or
**`No contents cached for "X".`** (`gui/app/widgets/panels.rs:1050,1074`).

**Inventory/Reserve `wordwrap` — FIXED 2026-08-11 (`4f959d5d`).** Was authored in both
editors and honored only by the GUI; `InventoryWindow::new` hardcoded wrap off and the
sync path never revisited it. The TUI now reads the flag and forces a refill when it
changes (wrapping is applied at `finish_line`, so a stale flag would otherwise persist
until the game re-sent the list). Document ONE behavior across frontends.

### Game-type gating — which windows exist for which game (added 2026-08-11)

Gating lives in ONE table: `CATALOG` in `src/config/presets.rs:132-201`, as
`(key, Option<GameType>)`. `None` = both games.

**GS4-only:** `reserve` (`:185`), `gs4_experience` (`:197`), `minivitals` (`:199`),
`betrayer` (`:200`). **DR-only:** `concentration` (`:137`), `perception` (`:195`),
`experience` (`:196`).

**⚠️ The trap: `GameType::from_game_string` NEVER returns `None`** — it returns `DR`
only when the game string starts with `dr` (case-insensitive) and **defaults to GS4 for
everything else, including an unset game** (`src/config.rs:220-227`; the comment says
this is deliberate so GS4 templates appear when connecting via Lich without `--game`).
So "GS4-gated" means **hidden from DR characters**, not "requires you to set the game".

**Gated:** the GUI **Windows** catalog (`state/menus.rs:797-798` → `creatable_for_game`)
and the bare-`.addwindow` picker (`state/menus.rs:573` → `addable_by_category`). **NOT
gated:** the six-argument `.addwindow` form — `commands.rs:2295-2317` calls `add_window`
with no game check. A DR player can force a `reserve` window; it renders empty.

### Map and travel (added 2026-08-11)

**The map is GUI + mobile only.** The TUI renders one literal string,
`"Map is available in the GUI frontend (--frontend gui)"`
(`src/frontend/tui/frontend_impl.rs:462-467`). There is **no**
`src/frontend/tui/map_compass.rs` — that file exists only at
`src/frontend/gui/app/widgets/map_compass.rs`, shared by the GUI map and GUI compass.

**⚠️ The mini map is CLICK-ONLY — it does not pan or zoom by gesture.**
`map_view.rs:341` is `Sense::click()`; no `drag_delta`, no `smooth_scroll_delta`. Zoom is
the **Custom map zoom** checkbox (`menus.rs:2014-2020`, default 16.0 px/cell, clamped
2.0–96.0 at paint). Drag-pan and scroll/pinch-zoom exist only in the **Map Explorer**
(`map_explorer.rs:686,714,818`) and on the phone.

| Task | Terminal (TUI) | Desktop GUI | Mobile / Web |
|---|---|---|---|
| **See a map** | ❌ one-line hint. `frontend_impl.rs:462-467` | ✅ catalog or `.addwindow map map 0 0 30 12`. `presets.rs:190,1054` | ✅ 🗺 **Map** button, hidden until `map_state.available`. `app.js:7411` |
| **Zoom** | ❌ | ⚠️ **Not a gesture** — right-click ▸ **Custom map zoom** + slider | ✅ pinch |
| **Pan** | ❌ | ❌ on the mini map (drag moves the window); ✅ in the Explorer | ✅ drag; turns Follow off. `app.js:7398` |
| **Click a room to travel** | ❌ | ✅ single click → `.go2 <id>`, or `;go2 <id>` when `native_map_clicks=false`. `map_compass.rs:177-186` | ✅ tap; **the overlay closes itself**. `app.js:7837-7839` |
| **Cancel a trip** | ✅ `.go2 stop`, or **Esc in Normal mode only**. `input_handlers.rs:135-144` | ✅ `.go2 stop` or Esc | ✅ **■ Stop** only — **no Esc**. `app.js:7452` |
| **Walk to a selected room** | ❌ | ✅ **two ways**: **Walk here** (`map_explorer.rs:363`) and double-click (`:882-885`) | ✅ tap |

**Travel progress banner text differs — do not blend.** Desktop GUI paints ASCII
`-> {dest} | {done}/{total} rooms | ETA {eta}` (`map_compass.rs:149-155`); the web canvas
paints `→ {dest} · {done}/{total} rooms · ETA {eta}` (`app.js:7730`).

**⚠️ `.go2 targets` is NOT the saved list.** `targets` prints a nearest-first directory
of reachable mapdb-**tagged** destinations (`commands.rs:1354+`); the saved list is
**`.go2 saved`** (`commands.rs:1338`, whose comment reads "formerly what `.go2 targets`
showed"). **Two stale help strings FIXED 2026-08-11**: `app_core/command_help.rs:116`
and `gui/app/editors/settings.rs:1186`.

**Undocumented `.go2` forms** (`travel/target.rs:42-113`): `goback` (alias of `back`),
`guild` / `guild shop` (needs profession from `INFO`), `locker` / `public locker`, and
`.go2 reload`.

**Resolution order, first match wins:** `back` → room id → `u`-uid → saved name →
guild/locker specials → mapdb tag → free text over titles then descriptions. Saving a
name that parses as a room id or equals `back` is **refused** (`commands.rs:1308-1311`).

**⚠️ "No silver handling" is OBSOLETE — delete it wherever it appears.** Paid and gated
travel are implemented and settings-gated, all **off by default**: `use_seeking`,
`use_portmasters`, `get_silvers`, `get_return_trip_silvers`, `use_urchins`,
`use_day_pass`, `buy_day_pass`, `day_pass_sack` (`settings.rs:897-970`).
`native_map_clicks` is the only one defaulting **on**.

**Ghost rooms: capture ALWAYS runs; `mapping_mode` gates RENDERING only**
(`settings.rs:1017-1022`, explicit). Session-only, never written to disk.

**Map data source priority: explicit `mapdb_path` → downloaded release → `lich_dir`**
(`map_service.rs:37-60`). Nothing downloads automatically. `.mapdb` / `.mapdb download` /
`.mapdb remove` / `.mapdb repo <owner/repo>` work in **every** frontend including the
phone. Default repo `Nisugi/mapdb`.

**Four distinct mini-map empty states** (`map_compass.rs:25-55`), each meaning something
different: no database, loading spinner, `"Map unavailable: {err}"`, and
`"Waiting for a mapped room..."` / `"Generating map..."`.

**Automation lease:** one automation drives at a time —
`[go2] {owner} is driving - .stop to cancel it first.` (`travel_ticks.rs:363-368`).

### The two Experience widgets — one name, two games (added 2026-08-11)

**`experience` and `gs4_experience` are DIFFERENT WIDGETS** with different renderers, different
feeds, and opposite game gates (`presets.rs:196-197`). **Both carry the catalog title
"Experience"** and both sit in category **Character**, so a player only ever sees one. Never
write one page as covering both.

- **DR `experience`** ← `<component id='exp XXX'>` / `<compDef>`; the `exp ` prefix is stripped
  and the remainder is the field name (`core/messages/component.rs:139-156`). `compDef`
  establishes ORDER at login. State: `GameState.dr_experience` (`core/state.rs:602-657`).
- **GS4 `gs4_experience`** ← the `expr` DIALOG: `Label id='yourLvl'`, `progressBar
  id='mindState'`, `id='nextLvlPB'` (`element.rs:952-993`). State: `GameState.gs4_experience`.

**⚠️ `gs4_experience` AUTO-CREATES ITSELF.** It carries `WindowBinding::Dialog("expr")`
(`config/layout.rs:442`) and `dialog_seed_alias` maps `"expr" → "gs4_experience"`
(`core/view_resolver.rs:43`), so a first `openDialog id='expr'` queues a pending addition
(`element.rs:1160-1165`) that `process_pending_window_additions` builds and binds
(`window_lifecycle.rs:344-370`). Re-sends never duplicate. Tests: `state/tests.rs:563-617`.
**DR `experience` has no binding and never auto-appears.**

**⚠️ Bar labels DIVERGE.** GUI prefixes `"Mind: {}"` / `"Next: {}"` (`panels.rs:224,236`); the
**TUI paints the raw server text with no prefix** (`tui/gs4_experience.rs:355-382`).

**⚠️ The DR list SCROLLS in the GUI and CLIPS in the TUI.** GUI wraps in an `egui::ScrollArea`
(`panels.rs:260-269`); the TUI renders a plain `Paragraph` with no scroll offset
(`tui/experience.rs:114-124`), so lines past the window height are unreachable.

**Empty states:** GUI **"No experience data yet."** for both; TUI **"(No experience data)"**.

**GUI widget section: GS4 has one ("Experience"); DR has NONE** (`window_config.rs:140-161`).
TUI editor mirrors it — `gs4_experience` exposes seven fields, `experience` exposes nothing
(`window_editor/construction.rs:302-315`).

**`align` is authored NOWHERE and read ONCE**, inside `or_insert_with` at widget construction
(`tui/sync.rs:2427-2431,2482-2486`). A hand-edit needs a fresh window. The GUI DR renderer
ignores it entirely.

**GS4 height is CAPPED at 7 rows** (floor 3) — one row per toggleable field plus borders.

### Vitals, encumbrance, and Betrayer (added 2026-08-11)

**MiniVitals is NOT fed by a `minivitals` dialog.** `MiniVitalsState` updates from **any**
`ProgressBar` element whose id is `health|mana|concentration|stamina|spirit`
(`element.rs:904-947`) — the same arm feeding `progress` windows and `GameState.vitals`
percentages. **One feed, three consumers**; running a strip and separate bars costs nothing.

**MiniVitals carries absolute values; `GameState.vitals` carries percentages only**, and the
phone is fed the percentages (`app.js:494-507`), so **`142/193` can never appear on the phone**.

**MiniVitals ships `title: None`** (`presets.rs:1680`, deliberate — "like Wrayth Stats"), and
`enumerate_known_windows` falls back to the window NAME when title is None
(`state/menus.rs:811-815`), so **the GUI catalog row reads the lowercase key `minivitals`** while
Encumbrance and Betrayer show proper titles. Rows pinned at 3 (`min_rows == max_rows == 3`).
**`concentration` is a TUI-only fifth bar**, capped at 4 enabled (`MAX_ENABLED`).

**One `encumlevel` reading feeds THREE surfaces at once**: any `progress` window bound to it,
the `encum` widget, and (GUI only) the **Encumbrance** bar inside MiniVitals — all off the same
`ProgressBar` arm (`element.rs:959-961`). The blurb is a separate `encumblurb` Label.

**⚠️ The `encum` type string is NOT `encumbrance`.** Type = `encum` (`data/window.rs:103`), but
the window **title is "Encumbrance"** and the GUI catalog labels by TITLE — so the row a player
clicks reads **Encumbrance** while the string they must type is `encum`. `from_str` falls back to
**Text** for unknowns, giving a silent blank window.

**GUI encumbrance re-labels the bar and ignores `align`.** It always prints
`"Encumbrance: {text}"` (`panels.rs:287-291`); the TUI centers the bare level name and honors
`align` for the blurb.

**⚠️ Betrayer is the ONLY self-resizing widget.** `adjust_content_driven_windows`
(`state/persistence.rs:263-309`) recomputes `base.rows` from the item count, clamps to 3–12,
writes it into both the layout and `ui_state`, and schedules an autosave. **A user-dragged height
does not stick.** `show_items = false` pins it at 3.

**Betrayer's `!` active marker colors in the TUI, not in the GUI widget.** TUI splits the leading
`!` into `active_color` (`tui/betrayer.rs:242-258`); the GUI widget does a plain `ui.label(item)`
(`panels.rs:332-334`). The *text-window* path colors it via `config.ui.betrayer_active_color`.
Three code paths, two behaviors.

**Betrayer feed shape:** `<dialogData id='BetrayerPanel'>`; `lblBPs` parsed by
`strip_prefix("Blood Points: ")` → u32 out of 100 (`core/state.rs:917-930`); items from
`lblitem1..lblitem20`, **stopping at the first gap** (`element.rs:1486-1496`).

**Catalog categories:** `minivitals` → **Progress Bars**, `encum` → **Character**, `betrayer` →
**Dialogs**, `quickbar` → **Hotbars**.

### Quickbars (added 2026-08-11)

**A quickbar is game-fed AND user-authorable — do not write it as one or the other.** Bars arrive
as `<openDialog id="quick…">` / `<dialogData id="quick…">` carrying `<link cmd=>`, `<menuLink
exist= noun=>`, `<label>`, `<sep>` children (`parser/dialogs.rs:480-485,877-952`). **Only ids
that are exactly `quick` or start with `quick-` are quickbars** (`dialogs.rs:868-870`); anything
else on that tag is a dialog panel. Users add their own via `[[quickbars.custom]]`, with the
**same id rule enforced silently** — an invalid id is skipped with a log warning only
(`state.rs:1065-1090`). A custom `quick` **replaces** the game's main bar.

**Bars, their order, and the active id persist per character** in
`profiles/<Character>/session_cache.toml`, which is what makes a mid-session Lich attach show
buttons. A cold cache seeds three canned bars (`state.rs:1497-1528`).

**The switcher diverges, and one frontend hides it.** TUI: a literal `>>` at the left, **always
rendered**, opening a popup of `_qlink change <id>` items (`tui/quickbar.rs:268,292-305`). GUI:
an `egui::ComboBox` shown **only when `ids.len() > 1`** (`links_bars.rs:222-243`).

**Only `Link` and `MenuLink` entries are selectable**; `Label` and `Separator` get no selectable
index (`tui/quickbar.rs:321-332`). `Link` sends `cmd`; `MenuLink` fires `request_menu(exist,
noun)` — the two-step server verb menu. **GUI empty state:** `"No quickbars configured."`

### Perception (added 2026-08-11)

**Perception is DR-only, and its preset title is `"Perceptions"` — plural** (`presets.rs:1589`).

**The flush is prompt-driven, not push-driven.** `clearStream percWindow` empties the buffer
(`element.rs:266-279`); the buffer survives `pushStream`/`popStream` and flushes on the next
prompt (`element.rs:375-378` → `buffers.rs:187-255`). Entries arrive concatenated and are split
by duration-suffix detection (`buffers.rs:266-297`).

**Core sorts descending by weight unconditionally** (`buffers.rs:227`). Weights
(`buffers.rs:318-327`): Percentage `3000+pct`, OngoingMagic `2000`, Indefinite/Cyclic `1500`,
Other `500`, Roisaen `= the count`, **Fading `0`**.

**Rows run through the highlight engine on stream id `"perception"`** (`tui/perception.rs:166`) —
highlight rules, `color_entire_line` and bold all apply. **A text replacement that empties a row
drops the row** (`sync.rs:2366-2369`).

### Widget-section coverage (added 2026-08-11)

`widget_section_label` returns `None` for **Perception, Quickbar, MissingSpells, Betrayer,
Inventory, Container, Experience (DR)** (`gui/app/window_config.rs:140-161`) — none has a GUI
widget section. In the TUI editor, `Quickbar` and `MissingSpells` push **zero** fields
(`construction.rs:206-207`); `Perception` pushes exactly three (`:296-301`).

### Mobile coverage for the Wave 7 widgets (added 2026-08-11)

- **Status drawer ▸ Experience** — part of `CHAR_SECTIONS` (`app.js:5527-5546`), **fed from
  `gs4_experience` ONLY** (`core/remote.rs:1096-1112`): the level text, `Mind: {text} ({n}%)`,
  `Next level: {text}`. **`dr_experience` never reaches the phone.** The drawer ignores the
  desktop window's field toggles.
- **Status drawer ▸ Encumbrance** — pre-formatted `"{text} ({value}%)"` plus the blurb
  (`core/remote.rs:1113-1119`).
- **No phone surface at all** for quickbar, perception, missing-spells, Betrayer, or MiniVitals.
  `percWindow` is in `HIDDEN_STREAMS` (`app.js:31`) so it gets no chip. Honest redirects:
  quickbar → the **macro tray**; perception and missing-spells → the **status drawer**'s effect
  sections, which tick live where desktop does not.
- **`experience` in `HIDDEN_STREAMS` is inert** — no code path produces a text stream by that
  name; neither Experience widget is stream-fed. The entry is defensive.

### The Windows catalog — what the reader actually sees (added 2026-08-11)

**⚠️ EVERY catalog category is COLLAPSED by default, and Status nests a second fold.**
`CollapsingHeader::new(category.display_name()).default_open(false)`
(`gui/app/editors/known_windows.rs:236-237`). Inside **Status**, the thirteen `indicator` rows
sit in their own nested **Indicators** fold, also `default_open(false)` (`:246-258`), so the
Dashboard row isn't buried. **Any page that says "tick X in the catalog" without "expand the
group first" is under-specified** — ten shipped pages were corrected in Wave 8.

**Rows sort ALPHABETICALLY BY TITLE, not in catalog order** (`known_windows.rs:99`). `CATALOG`
order (`presets.rs:132-201`) is fixture order. Category buckets follow `WidgetCategory::ALL`.

**⚠️ SEVEN valid types are absent from `CATALOG`, and two are reachable under another key.**
Absent as catalog keys: `command_input`, `performance`, `webui`, `dialogpanel`, `container`,
`injury_doll`, `active_effects`. But `injury_doll` is reachable as catalog key **`injuries`**
(`presets.rs:191`; `try_from_str` accepts both — `data/window.rs:83`), and `active_effects` as
five preset keys (`buffs`/`debuffs`/`cooldowns`/`active_spells`/`active_effects_custom`).
**Truly typed-only: `command_input`, `performance`, `webui`, `dialogpanel`, `container`,
`spacer`.**

**⚠️ But `command_input` and `spacer` ARE injected into the HIDE/EDIT pickers** as special
cases (`presets.rs:2110-2134`, into `WidgetCategory::Other`). "Not in the catalog" is an
ADD-path fact only.

**`spacer` and the six `*_custom` seeds are excluded from the GUI catalog**
(`state/menus.rs:800`). The custom seeds resurface under **➕ Custom window…** with different
labels — **Text · Tabbed text · Progress bar · Countdown · Entity list · Active effects**
(`core/local_catalog.rs:92-101`). So `entity_custom` is never a row reading "Custom"; it is the
**Entity list** item.

**Type aliases `VALID_TYPES` does not list** (`data/window.rs:83,88,108,110`): `injuries` =
`injury_doll`, `commandinput` = `command_input`, `lichui` = `webui`, `dialog_panel` =
`dialogpanel`. The "type strings are unforgiving" note is true for `active_effects` and
`missingspells` but **over-general** — these four accept two spellings each.

**Title-vs-key divergences** (all `presets.rs`): `main`→**Story**, `hotkeybar`→**Actions**,
`bounty`→**Bounties**, `society`→**Society Tasks**, `roundtime`→**RT**, `casttime`→**Cast**,
`stuntime`→**Stun**, `injuries`→**Injuries**, `perception`→**Perceptions**,
`encum`→**Encumbrance**, `command_input`→**Command Input**, `performance`→**Performance
Stats**. Plus `minivitals` with `title: None` falling back to the bare key.

### Hotbars and utility widgets (added 2026-08-11)

**⚠️ Hotbar hotkeys are BAR-scoped, not window-scoped.** `merge_hotbar_hotkeys` walks every bar
in the loaded config with **no check that any window displays it**
(`core/app_core/keybinds.rs:50-99`), so a bar with no window on screen still owns its keys.
Inserted into the **runtime map only** — never into `keybinds.toml`, invisible to the keybind
editor. Existing bindings win; conflicts name the owner as `bar:button`. **The old
`hotkeybar.md` claimed hotkeys register "while a window shows the bar" — false, corrected in
Wave 8.**

**Hotkeybar `bar` and `orientation` are authorable in NEITHER window editor.** The GUI
**Hotbar** section holds exactly one item, **Edit hotbars…** (`window_config.rs:1125-1132`);
the TUI pushes zero fields (`construction.rs:208`). `create_window_content` overwrites `bar`
with the window NAME (`window_lifecycle.rs:1293-1297`), so the preset default `"default"` is
unreachable for `.addwindow`-created windows. **[repair?]**

**⚠️ `.addwindow webui` is the WRONG route.** `.webui <script/page>` (`commands.rs:2287-2293`)
builds a window named `webui:<page>` with the binding set and sizes it from the page's own
hint (`window_lifecycle.rs:1425-1500`). A hand-added `webui` window has `page=""` and **no
editor exposes the field**. Same for `dialogpanel`: panels are DISCOVERED from
`DialogPanelOpen` (`element.rs:1249-1267`) and built as **ephemeral** windows placed from the
dialog's hints (`window_lifecycle.rs:600-643`).

**⚠️ Hiding `command_input` MEANS DIFFERENT THINGS per frontend.** The GUI paints a **fixed
bottom fallback panel** when no input window renders (`gui/app.rs:2602-2624`). The TUI persists
the hidden flag but **keeps the input on screen**: *"Command input hidden in the layout (GUI
shows its fallback bar); the TUI keeps it visible."* (`window_lifecycle.rs:119-140`, gated on
`force_show_command_input`).

**Min sizes** (`core/app_core/layout.rs:451-459`): `spacer` **(1,1)** — the smallest in the
product, so a thin alignment spacer survives `distribute_1d`; `dialogpanel` (14,4);
`hotkeybar`/`quickbar` (20,1). Everything else floors at (5,3).

**TUI-authored-only widgets:** `command_input` pushes **six** fields (`construction.rs:176-183`)
and `performance` pushes **EditMetrics** (`:292-294`), while **neither has a GUI widget
section**. `Spacer`, `WebUi`, and `DialogPanel` push zero in both. **[repair?]**

### The injury doll (added 2026-08-11)

**⚠️ The severity scale is 0-6 and the scar names are off by three.** `severity_level_from_key`
(`config/skins.rs:289-300`): `healthy`→0, `injury1..3`→1-3, **`scar1..3`→4-6**. So **Injury >=
4** means "any scar" and **Injury >= 1** means "anything wrong". Three vocabularies for one
scale: TUI editor labels (**Wound1**/**Scar1**), TOML color keys (`injury1_color`/`scar1_color`),
skin art keys (`injury1`/`scar1`). Counting scars from 1 in a condition is the most common way
to write a rule that never fires.

**⚠️ AUTHORING IS SPLIT, AND EACH FRONTEND OWNS HALF.** Severity colors are **TUI-only**
authoring (`construction.rs:249-257` pushes seven fields; the GUI has no `InjuryDoll` arm in
`render_widget_config_controls`), while art, grayscale, and calibration are **GUI-only**. Each
frontend *reads* the other's half — a TUI-authored palette paints the GUI vector doll. Document
as a division of labor, not a gap.

**Both `injuries` and `injury_doll` work as type strings** (`data/window.rs:83`,
`presets.rs:1090`). Catalog key `injuries`, **ungated — both games**, category **Character**.

**⚠️ Variants are FULL REPLACE, including suppression.** `doll_set` builds the view entirely
from the variant's own base/parts/anchors/dots with **no merge** (`gui/skin.rs:433-454`); a
default-set `hidden_when` does **not** apply while a variant is active
(`doll_rules.rs:264-268`). First match in declaration order wins. Nesting variants is a parse
error.

**⚠️ A pool doll is NOT a skin doll.** Applying `doll_image` sets the base then **clears**
`doll_parts`, `doll_anchors`, `doll_hidden_when` (`gui/skin.rs:1052-1055`); variant loading is
gated on `doll_override.is_none()` (`:1140`). **Pool dolls carry no variants and no
`hidden_when`** — pinned by `doll_rules.rs:272-279`. Calibration writes a `<stem>.toml` sidecar
beside the image.

**The calibrator's Doll set combo discards unsaved edits on switch**
(`doll_calibration.rs:240-263`), and is **skin targets only** — pool dolls get no combo
(`:140-149,214`). Two doors to it: the window's **Injury doll ▸ Calibrate doll…**
(`menus.rs:1992`) and **Settings ▸ Appearance ▸ Calibrate injury doll…**
(`editors/settings.rs:976`).

**Overlay art suppresses the generated dot entirely** — never a dot stamped on hand-drawn art
(`widgets/injury.rs:149-169`). "No overlay" is a deliberate reveal for inverted skins. Artless
parts keep dots. **nsys draws FIRST as an underlay** (`:133-140`). **GUI tooltip only on
wounds** (`:193-196`) — an unwounded doll has no hover text.

**Grayscale is GENERATED, never authored** — luminance recolor cached as `"<path>#gray"`
(`gui/skin.rs:1800-1809`). **There is no `base_gray` TOML key.**

**Other-player popup (`injuries-<playerid>`) is DESKTOP-ONLY and always default set/palette** —
variants and hidden parts read SELF state, so your prone flag must never reshape someone else's
body (`widgets/injury.rs:304-307`). No remote transport exists.

**Mobile: DOLL ART DOES REACH THE PHONE.** `/doll.json` and
`/doll/image?kind=base|overlay&part=&level=` (`web/server.rs:290-291,676`, token-gated) serve
the real skin PNGs; `skinDollSvg()` (`app.js:5312-5372`) embeds them as SVG. Vector fallback
only when the host has no doll base. Surface is the **status drawer** (`#status-doll`, then an
**Injuries** section) — **not** the status row, which carries indicators. **The host pushes
ANSWERS**: `RemoteDelta::Doll { variant, hidden }` — the phone never evaluates a condition.
Nothing there is tappable. **[repair?]** Per-widget palette overrides never reach the phone —
`LEVEL_COLORS` (`app.js:5185-5188`) is a hardcoded default subset.

**[doc-bug] FIXED 2026-08-11:** `tui/injury_doll.rs:19-24` claimed `ns`/`nk`/`bk` at rows
1/3/5; the code (`:209-213`) is **`nk`=neck row 1, `bk`=back row 3, `ns`=nerves row 5**. The
old `injury-doll.md` copied the wrong comment. Both corrected.

**Note on "has a section" vs "has config rows":** `widget_section_label` NAMES a section for
InjuryDoll, Compass, Hand, Map, and Dashboard, but `render_widget_config_controls` has no arm
for some of them — those sections contain only appearance controls.

## Conditions — the shared vocabulary (added 2026-08-11)

> One condition language drives **hotbar button states**, **indicator icon
> states**, and **hand icons**. Teach it once; it transfers.
> `src/config/conditions.rs`

Leaf kinds as the GUI builder labels them, in order: **Effect active · Effect
inactive · Effect time remaining · Roundtime active · Casttime active ·
Indicator · Vital · Injury · Spell affordable · Hand empty · Hand holds · Spell
prepared · Time of day** (`editors/hotbars.rs:65-79`). Groups nest one level as
**all of** / **any of**; deeper trees are file-authored, render as *(nested
group - edit in hotbars.toml)*, and still evaluate.

- **Vital**: vital · cmp · number · unit **%** or **abs**. New leaf defaults to
  `stamina < 25 %`.
- **Injury**: area (head, neck, chest, abdomen, back, leftArm, rightArm,
  leftHand, rightHand, leftLeg, rightLeg, leftEye, rightEye, nsys) · cmp ·
  level, where **1-3 = wounds, 4-6 = scars, 0 = healthy**.
- **Indicator** ids: `standing`, `kneeling`, `sitting`, `prone`, `stunned`,
  `bleeding`, `hidden`, `invisible`, `webbed`, `joined`, `dead`.

**Ordering rule — state this on every page that touches states: the FIRST
matching state wins.** A broad condition above a narrow one swallows it.
`src/config/hotbars.rs:65-67`

**Authoring parity (a real per-frontend gap):**

| | Desktop GUI | Terminal (TUI) | Mobile |
|---|---|---|---|
| Hotbar bars/buttons/hotkeys/countdowns | ✅ | ✅ | ❌ no hotbars at all — the phone has **macros** (`macros.toml`, macro tray/rail), a separate system with no conditions |
| Hotbar **condition states** | ✅ | ❌ **cannot author** — shows `"{N} state(s) defined - edit in the GUI editor or hotbars.toml"` and round-trips them untouched (`src/frontend/tui/hotbar_editor.rs:1-8,789-793`) | ❌ |
| Indicator **conditions** | ✅ (`editors/indicators.rs:351`) | ❌ id/title/icon/colors only (`src/frontend/tui/indicator_template_editor.rs:19-26`) | ❌ |
| Highlight rules incl. `set_status`, redirects, squelch | ✅ | ✅ | ✅ — **compile-enforced parity** across all three editors (`src/config/highlights.rs:151-234`) |

**Binding by name, not by picker:** a `hotkeybar` window displays the bar whose
name matches the **window's** name; an `indicator` window's name **is** its
status id. There is no bar-picker in the window menu — only **Edit hotbars…**.
`state/window_lifecycle.rs:1241-1242,1293-1297`

**Scope merge:** a character bar with the same name **replaces the global bar
wholesale** (not a per-button merge). Editor badges: `[G]` global, `[C]`
character. `src/config/hotbars.rs:275-294`

**No blink/flash/pulse animation exists for vitals bars in any frontend.** The
vitals window offers Layout, Bar height, Bar text, Depleted color, and per-bar
visibility — no threshold color, no animation. Low-health alerting is a hotbar
state's color, an indicator lighting, a sound, or controller rumble. Verified by
a whole-`src/` grep; the only `pulse` hits are controller rumble.
`window_config.rs:1159-1290`, `web/assets/app.js:494-507`

**`set_status` / `status_duration` / `clear_status`:** a highlight rule can
light any indicator **and** dashboard cell by id (case-insensitive), riding the
same machinery as the server's own status icons. A **dashboard auto-adds an
unknown id**; an indicator only flips one it already has. Empty duration = stays
lit until a `clear_status` rule matches. `src/core/app_core/state.rs:894-975`

**`.testline <text>`** injects verbatim text through the live highlight/squelch
pipeline — the intended way to prove a rule without waiting for a mob.
`src/core/app_core/commands.rs:2354-2364`

## Frontend-exclusive capabilities

**GUI-only:** stay-open toolbar hubs (Windows/Settings/Zones/Editors); per-window graphical **Appearance** (skin frame, background, accent, corner radius, doll/compass/hand art) via right-click; skins (`.setskin`/`.makeskin`); `.controller`, `.snapdebug`, shell zones (`.header`/`.footer`/`.leftbar`/`.rightbar`), WebUI bridge; **`Ctrl+C`=quit** (GlobalKeybinds, GUI-only). `command_help.rs:178-182`

**TUI-only:** `.setpalette`/`.resetpalette` (terminal 256-color); `.deletewindow` hide-redirect; Ctrl+C copies.

**Mobile-only:** touch radial wheel from **puck** (long-press 300ms, not open-anywhere — `app.js:1826-1838`); **macro rail/tray** (tap fires, long-press 450ms edits; create via **＋ New button…** → editor with tap-modes send/type/type-then-send — `app.js:4363-4422`); **the BROWSER client has two modes**: sidecar (second screen, `session_control:false`) vs headless/standalone (login screen) — `app.js:800-802,933`. ⚠️ **This describes the browser only. The native apps are a separate story — see "Mobile connection modes" above.** Do not generalize this row to iOS/Android.

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
- **Valid widget types:** `WidgetType::VALID_TYPES`, `src/data/window.rs:116-148`
