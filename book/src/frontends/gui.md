# Desktop GUI

> Build the layout you play in — drag windows anywhere, reskin them with real artwork, and
> change any setting from a right-click without leaving the game.

## What it's for

This is where you *design* your interface. Everything else — the terminal, the phone, the
browser — plays the layout; the GUI is where you make it. You drag a window to the corner
you want it in, right-click it to change its font, and watch the change land while combat
text is still scrolling past.

It is also the only frontend with pixels. The compass is a drawn rose, the injury doll is a
body diagram that colors by wound severity, and a skin can replace your window borders,
backgrounds, and icons with real artwork. A character grid cannot do those things, so the
GUI does them for everyone.

<figure class="shot" data-shot="gui/gui-hunting-layout-annotated">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A GUI hunting layout with the toolbar's <b>Windows</b>, <b>Settings</b>, <b>Zones</b>, and <b>Editors</b> hubs across the top, vitals in the header, and a skinned frame on the story window.</figcaption>
</figure>

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Run `vellum-fe` with **no arguments** (or double-click it) to open the
   [**VellumFE Launcher**](../getting-started/launcher.md), then click **Launch** on a saved
   connection. A saved connection opens in the GUI by default.
2. To start the GUI directly from a shell instead, run
   `vellum-fe --frontend gui --port 8000 --character YourName`. Direct login works here too:
   `vellum-fe --frontend gui --direct --account YOURACCOUNT --game prime --character YourName`.
3. Once the window is up, click **Windows** in the top toolbar and tick the windows you want.
   The catalog stays open while you work, so you can add several in one visit.

**A saved connection defaults to GUI, but the `--frontend` command-line flag defaults to
`tui`.** The same character launched from the Launcher and from a typed command lands in two
different interfaces. See [First Launch](../getting-started/first-launch.md) for the full
flag list.

<figure class="shot" data-shot="gui/gui-toolbar-hubs-open">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Windows</b> hub open as a stay-open catalog, showing show/hide checkboxes, per-row zone dropdowns, and <b>➕ Custom window…</b>.</figcaption>
</figure>

→ **Expected result:** a native window opens with your layout, and the top toolbar shows the
**Windows**, **Settings**, **Zones**, and **Editors** hubs.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

The terminal cannot run the GUI, but it shares its layouts and its settings files. Build an
arrangement in the GUI, save it with `.savelayout hunting`, then in the terminal run
`.loadlayout hunting` — both frontends read the same `~/.vellum-fe/layouts/` pool.

What does not cross over is artwork. Skin frames, window background images, and the drawn
doll, compass, and hand icons load without complaint on the terminal side and have nothing to
draw, because a character cell has no pixels. The [TUI](./tui.md) shows the same underlying
information with glyphs and color.

The TUI also **keeps its own full-screen window editor** (`.editwindow`), which the GUI
retired in favor of the right-click menu described below.

→ **Expected result:** your GUI-built arrangement redraws on the character grid, with the
same windows in the same relative places and no skin artwork.
{{#endtab}}
{{#tab name="Mobile"}}

The phone does not run the desktop GUI and cannot author a layout — its chrome is fixed by
design and renders from a snapshot the host streams. To see a GUI session on your phone,
start the GUI with the web server on
(`vellum-fe --frontend gui --port 8000 --character YourName --web-port 8080`), run `.webinfo`
for the pairing URL and QR code, and open that URL in the phone browser or scan the QR from
the app's **Characters** picker (person icon ▸ **Characters** ▸ **Scan QR to add**).

The phone then shows the same character through its own surfaces — the right **status
drawer**'s **Targets**, **Players**, and **Room** sections — rather than your GUI windows.
You can still author macros, touch-wheel slices, highlights, colors, and controller binds
from the phone; window and skin work stays on the desktop.

→ **Expected result:** the phone shows the same game text and vitals as the GUI window, and a
command typed on either lands in the game once.
{{#endtab}}
{{#endtabs}}

## Working with windows

### Adding, hiding, deleting

**Adding a window is the Windows catalog, not a right-click.** Click **Windows** in the
toolbar and you get every window the client can have — the built-in catalog plus game
dialogs, streams, and containers — sorted by title and grouped into collapsed categories.
Each row is a **checkbox** (tick to show, untick to hide) and a **zone** dropdown. Setting the
zone on a hidden window decides where it appears when you show it.

Two buttons sit at the top of the catalog:

- **➕ Custom window…** creates a blank widget of the type you pick and drops you into the new
  window's right-click menu to configure it.
- **↩ Restore deleted…** brings back a window you deleted, with its position, streams, and
  widget type intact. It appears only when something is waiting to be restored.

| Goal | Gesture | Typed equivalent |
|---|---|---|
| Add / show a window | **Windows** ▸ tick the row | `.addwindow` (picker), or the full `.addwindow <name> <type> <x> <y> <w> [h]` |
| Hide it, keep it in the layout | Right-click ▸ **Hide**, or untick in the catalog | `.hidewindow [name]` |
| Remove it from the layout | Right-click ▸ **Window** ▸ **Advanced** ▸ **Delete Window…** | `.deletewindow <name>` |
| Bring a deleted one back | **Windows** ▸ **↩ Restore deleted…** | — |
| Save the arrangement | — (no button) | `.savelayout <name>` |
| Load one | `.menu` ▸ **Layouts** | `.loadlayout <name>` · `.layouts` |

> ⚠️ **`.deletewindow` truly deletes and `.hidewindow` hides — in *both* desktop frontends.**
> There is no asymmetry to remember. Delete stashes the window so **↩ Restore deleted…** can
> bring it back, and both commands refuse to remove your only main-feed window.

> ⚠️ **There is no button to save a layout.** `.savelayout <name>` is typed, in the GUI as
> much as the terminal. The catalog says so itself when no layouts exist yet.

### Moving, resizing, snapping

**Drag a window from anywhere — body or title bar, no modifier held.** Interactive content
still wins, so selecting text or clicking a link does not start a drag. Resize by dragging any
edge or corner.

> ⚠️ **The GUI drags from the whole window; the TUI drags from the title bar only.** And
> **`Ctrl`+drag is not window movement in either frontend** — it is the object-drag gesture
> that sends `_drag`, for dropping an item onto a hand, a container, or another item.

While you drag or resize, edges snap to the zone's bounds, to other windows in the same zone,
and optionally to a grid. An engaged snap draws a guide line at the matched coordinate.
**Hold Shift to suspend snapping** and place the window exactly where the pointer is — the
guides disappear for as long as you hold it.

A snapped drop can promote to a persistent **anchor**, which makes the window keep following
the edge it snapped to when the app window resizes. Right-click ▸ **Arrange** ▸ **Release
Anchors** forgets them; the window stays exactly where it is and stops following. Dragging
the window away from its snap does the same thing. The row is always visible, greyed when the
window has no anchors, so you can tell at a glance which case you are in.

Tune the whole system under **Settings** ▸ **GUI**: snap distance, which targets participate,
grid pitch, and grid sizing. `.snapdebug` writes a trace of the snap engine to
`~/.vellum-fe/vellum-fe.log` when a snap is not doing what you expect.

### Zones

The GUI arranges windows in five zones — **Header**, **Footer**, **Left Bar**, **Center**, and
**Right Bar**. Toggle them from the toolbar's **Zones** hub, which stays open so you can
adjust several and watch each take effect, or type `.header`, `.footer`, `.leftbar`,
`.rightbar` (each takes `on`, `off`, or `toggle`, defaulting to toggle).

Every zone is a free canvas: windows sit exactly where you drop them, may overlap, and
remember their spot. Moving a window to a *different* zone has three routes:

- **Alt+drag the window.** Hold `Alt` and drag, and a tinted overlay highlights the zone under
  your pointer; release to drop it there. Ordinary movement is suppressed while `Alt` is held,
  so the two gestures never fight.
- **Right-click ▸ Arrange ▸ Move to ▸** the zone you want.
- **The zone dropdown** on that window's row in the **Windows** catalog — the one that also
  works on a window that is currently hidden.

### Overlapping and detaching

Where windows can overlap — Center, Header, and Footer — the right-click menu offers **Send to
Back**, dropping the window behind anything it covers so a buried one becomes reachable.
Clicking a window still raises it. The packed sidebars do not offer it, because nothing
overlaps there.

**Detach** puts a window in its own OS window, restored across sessions. A detached window
gets the same right-click menu with **Reattach** in place of **Detach**, and without
**Arrange**, **Group**, or **Send to Back** — none of them mean anything inside an OS window.
Drag its body to move it.

## The right-click menu

**Right-click any window body and you get every per-window setting there is.** Changes apply
**live** — there is no Save button. Text fields commit when you press **Enter** or click away.
`.editwindow <name>` opens the same menu for a window by name.

> ⚠️ **The GUI's separate Window Editor no longer exists.** If a guide tells you to open an
> editor window and press Save, it is describing the old client. Right-click is the one home
> for these settings now. (The [TUI](./tui.md) still has its own full-screen editor form.)

Three quick actions sit at the top — **Hide**, **Detach** (or **Reattach**), and **Send to
Back** — then collapsible sections, each with its own **Advanced** fold so the menu opens
short.

### Window

Title, **Title bar text** (custom text for the bar, separate from the window's name),
**Streams** — the stream ids this window collects, with a **+** picker listing every stream
seen this session so you do not have to know an id in advance — **Buffer lines** (scrollback
size), **Speak new lines (TTS)**, and **Lock in place**. A locked window can still be grabbed
but will not move or resize; `.lockwindows` locks everything at once.

Under **Advanced**: **Compact** (condenses known content such as bounty text), **Timestamps**
with a **Timestamp at line start** toggle, and **Delete Window…**. Delete is double-gated —
the first click arms it, then the menu asks *"Really delete this window from the layout?"*
with **Delete** and **Cancel**.

### The widget section

Named for the widget itself, and present only when the widget has settings of its own:

- **Timer / Bar** — the feed id it tracks, its label, color, whether it shows `value/max` or
  a bare value, and **Stay visible at rest** for timers that should not vanish at zero.
- **Effects** — which category the window carries.
- **Room** — section toggles for **Description**, **Objects**, **Players**, and **Exits**.
- **Targets** — the filtered-appendage count and status position, plus **Global target
  settings…**, which jumps to **Settings** ▸ **Targets** where the colors and excluded nouns
  live for every targets window at once.
- **Experience** — **Level**, **Mind state**, **Experience bar**, **Total exp**, **Ascension
  exp**, and the two bar colors. **Encumbrance** — **Level bar** and **Help text**.
- **Vitals** — **Layout** (orientation), **Bar height**, **Bar text** format, **Depleted
  color**, and a checkbox per bar in display order.

Widgets with an editor of their own link to it from here: **Edit tabs…** (tab names, streams,
and order), **Edit hotbars…**, **Edit indicators…**, **Edit dashboard…**, **Hand icons…**
(status-driven icons for an empty hand, a held weapon, a prepared spell), **Calibrate doll…**,
and **Open Map Explorer**.

<figure class="shot" data-shot="gui/gui-window-menu-sections">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A story window's right-click menu with <b>Hide</b> / <b>Detach</b> / <b>Send to Back</b> on top and the <b>Window</b> section expanded over the <b>Streams</b> field and its seen-stream picker.</figcaption>
</figure>

### Arrange, Appearance, Group

**Arrange** holds **Move Window** (a keyboard-friendly move that works even on a locked
window), **Move to** ▸ each zone, and **Release Anchors**. Under Advanced: **Fixed size**,
which keeps a window's exact width and height when the app window resizes or zooms so only its
position adapts — what you want for a compass or a hands widget.

**Appearance** splits into **Text** (font, text size, word wrap, content alignment), **Title
bar** (show it, and its alignment), and **Frame** (border on/off and style, accent color, skin
frame, background). Under Advanced sit the knobs you set once: title-bar height, which border
sides draw, corner radius, and frame scale.

**Group** locks windows together so they move as a unit. It sets the group's orientation
(**Stacked** or **Side by side**), reorders members with ⬆ / ⬇, and ungroups one member or the
whole group.

## The toolbar hubs

Four stay-open menus sit across the top. None of them close when you click inside — only
clicking outside dismisses them — so you can make several changes and watch each land.

| Hub | What's in it |
|---|---|
| **Windows** | The full window catalog: show/hide checkbox and zone per row, **➕ Custom window…**, **↩ Restore deleted…** |
| **Settings** | **All Settings…**, plus one row per section that opens the Settings window scrolled to that section |
| **Zones** | Show/hide and an overlay toggle for each of the five zones |
| **Editors** | Themes · Colors · Highlights · Keybinds · Menu Keybinds · Controller · Touch Wheel · Hotbars · Indicators · **Streams & Custom Windows** · Sorter · Room Images · UI Packs · Asset Manager (Jinx) |

The distinction between the last two is worth knowing: **Settings** holds the knobs that live
in `config.toml`, while **Editors** holds the authored content that owns its own files —
your highlight rules, your keybinds, your hotbars.

## Graphics and skins

The GUI draws what the terminal spells:

- **Compass rose** — a vector rose with lit direction markers.
- **Injury doll** — a body diagram colored by wound and scar severity. With a skin that ships
  doll art, wounds render as dots at positions you set by clicking, through **Calibrate
  doll…** in the doll window's own menu section.
- **Status icons** — vector pictograms for stance, hidden, stunned, and the rest of the
  indicator set.

A **skin** replaces all of it at once — nine-slice window borders, background art, icon
sprites, a sprite compass and paperdoll. `.skins` lists what is installed, `.setskin <name>`
activates one, `.setskin none` turns skinning off, and `.makeskin <name>` scaffolds a starter
skin you can edit. Per-window, **Appearance** ▸ **Frame** picks which frame from the active
skin a given window wears. See [Skins (GUI Graphics)](../customization/skins.md).

## Common setups

### A hunting layout you can rebuild in five minutes

1. Click **Zones** and turn on **Header** and **Right Bar**.
2. Click **Windows**, tick **Vitals** and set its zone dropdown to **Header**; tick
   **Roundtime**, **Compass**, and **Targets** and set them to **Right Bar**.
3. Drag each one where you want it. Hold **Shift** while dropping the compass if the snap
   keeps pulling it flush against an edge you want it off of.
4. Right-click the compass ▸ **Arrange** ▸ **Advanced** ▸ tick **Fixed size**, so it keeps its
   proportions when you resize the app window.
5. Type `.savelayout hunting`.

**You'll see:** the arrangement redraw exactly as you left it after `.loadlayout hunting` —
including on the terminal, which reads the same layout pool.

### A story window that reads the way you read

Right-click your main story window. Under **Window**, raise **Buffer lines** so a long hunt
stays scrollable. Under **Advanced**, tick **Timestamps** and **Timestamp at line start** if
you want to reconstruct a fight afterward. Under **Appearance** ▸ **Text**, pick a font and
size that suit your monitor, and turn **word wrap** on. Under **Appearance** ▸ **Frame**, set
an accent color so the window you look at most is instantly identifiable.

**You'll see:** each change take effect while game text is still arriving — timestamps appear
on the *next* line, the font reflows what is already on screen, and nothing needs saving.

## Tips & gotchas

> ⚠️ **In the Desktop GUI, `Ctrl+C` quits.** In the [Terminal (TUI)](./tui.md) the same
> combination **copies your selection** and does not quit at all. This is the single biggest
> trap when you move between the two. To copy in the GUI, select with the mouse (double-click
> for a word, triple-click for a line) and *then* press `Ctrl+C`.

> ⚠️ **`.quit` disconnects but keeps the window open by default.** Run it again, or use
> `.exit`, to close. Change it with `ui.keep_open_on_quit` in Settings.

- **There is no right-click "Add Window".** Adding is the **Windows** catalog or
  `.addwindow`. Right-click configures a window that already exists.
- **Copy is plain text, deliberately.** Selections are anchored to the text itself, so they
  survive scrolling — drag past the window edge to auto-scroll and the copy picks up lines
  currently out of view — but colors and styling are not carried to the clipboard.
- **Hiding is the control for game dialogs.** A hidden window stays hidden even when the game
  re-sends it. There is no separate blocklist to maintain.
- **Even the story window can be hidden**, once another window or tab carries the `main`
  stream. Hiding the command input hands typing to the built-in bottom bar.
- **Terminal-only commands do nothing here.** `.setpalette`, `.resetpalette`, `.transparent`,
  and `.resize` belong to the TUI; themes and skins handle appearance in the GUI.
- **Layouts are shared; the live arrangement is not.** `.savelayout` / `.loadlayout` write to
  the pool both desktop frontends read, but the GUI keeps its own per-character live layout
  (positions, zoom, fonts) apart from the TUI's `layout.toml`.
- **`Ctrl+F`** opens in-window search with match highlighting. **`Ctrl+=` / `Ctrl+-` /
  `Ctrl+0`** zoom the whole interface.
- **The two "Launchers" are unrelated.** The [**Launcher**](../getting-started/launcher.md) is
  the graphical connection list. The **SSH Launcher** (`.launcher` / `.launch <character>`) is
  an in-session panel that cold-starts a headless Lich on another machine.

## See also

- [Frontends overview](./README.md) — all five, and how to choose
- [Terminal (TUI)](./tui.md) — the terminal counterpart, and its own window editor
- [Mobile Web](./web.md) — joining a GUI session from a phone or browser
- [The Launcher](../getting-started/launcher.md) — saved connections and per-connection frontend
- [First Launch](../getting-started/first-launch.md) — command-line flags and the default screen
- [Skins (GUI Graphics)](../customization/skins.md) · [Widgets](../widgets/README.md) · [Map](../widgets/map.md)
- [Configuration Files](../configuration/README.md) — the settings all frontends share

<details>
<summary>Config reference (TOML)</summary>

The GUI writes its own state; you never need to hand-edit these to use the client. They are
here for troubleshooting and for reading a layout someone shared with you.

**Command line**

| Flag | Type | Default | What it does |
|---|---|---|---|
| `--frontend` | `tui` \| `gui` \| `headless` | `tui` | Pass `gui` explicitly; a *saved connection* defaults to `gui` instead. |
| `--launcher` | flag | on when run with no arguments | Opens the graphical Launcher. GUI only. |
| `--launch-profile <NAME>` | string | — | Runs a saved connection by name from `launcher.toml`. |
| `--web-port <PORT>` | u16 | — | Enables the embedded web server so a phone or browser can join this GUI session. |
| `--data-dir <DIR>` | path | `~/.vellum-fe` | Config directory root. Also settable via `VELLUM_FE_DIR`. |

**Where GUI state lives**

| Path | Contains |
|---|---|
| `~/.vellum-fe/gui/<profile>/<character>/layout_v1.json` | The per-character live layout: window positions, zones, detached windows, tab groups, zoom, fonts. |
| `~/.vellum-fe/layouts/<name>.json` | Named layout checkpoints from `.savelayout`, shared with the TUI's pool. |
| `~/.vellum-fe/global/skins/` | Installed skins. The active skin is recorded in the GUI layout, not `config.toml`. |

**Session behavior** (`config.toml`)

| Field | Type | Default | What it does |
|---|---|---|---|
| `ui.keep_open_on_quit` | bool | `true` | After `.quit`, disconnect but keep the window open. `.quit` again or `.exit` closes it. |

**GUI-only dot-commands**

| Command | What it does |
|---|---|
| `.header` / `.footer` / `.leftbar` / `.rightbar` `[on\|off\|toggle]` | Show or hide a shell zone. Default is toggle. |
| `.skins` · `.setskin <name>` · `.makeskin <name>` | List, activate (`none` disables), or scaffold a skin. |
| `.controller` | Open the controller bindings editor. |
| `.webui [page\|off]` | Render Lich WebUI pages as native docked panels. Needs a Lich proxy connection. |
| `.snapdebug` | Toggle the snap-engine trace in `vellum-fe.log`. |
| `.editwindow [name]` | Open that window's right-click menu; with no name, the Windows catalog. |

The in-app `.help` footer states the same split: commands marked `(GUI)` need the desktop
GUI, and `.setpalette` / `.resetpalette` / `.resize` are TUI-only.

</details>
