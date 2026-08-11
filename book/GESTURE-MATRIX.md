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
