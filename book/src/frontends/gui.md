# Desktop GUI

A native windowed client built on egui:

```bash
vellum-fe --frontend gui --port 8000 --character YourName
```

Direct connection (`--direct`) works in the GUI too.

## What's Shared with the TUI

Connection settings, highlights, keybinds, colors, themes, and all
dot-commands work identically — editors opened in the GUI write to the same
config files, so changes carry over if you switch frontends.

**Layout is not shared.** The GUI keeps its own per-character layout
(window positions, zoom, fonts) in `~/.vellum-fe/gui/`, separate from the
TUI's layout.toml. Window size, position, and zoom are restored between
sessions automatically.

`.savelayout <name>` / `.loadlayout <name>` / `.layouts` work here too,
on GUI-native layouts: save the current arrangement as a named
checkpoint (say, `combat` vs `town`), then swap with one command.
Loading applies instantly — windows, zones, tab groups, detached
windows, fonts, zoom — and later rearranging never rewrites a
checkpoint; only an explicit `.savelayout` does. TUI `.toml` layouts
are a separate format and can't be loaded here (`.resize` is also
TUI-only).

## Windows and Zones

The GUI arranges windows in five zones: header, footer, left sidebar,
center, and right sidebar. Toggle zones from the top toolbar. Every zone
is a free canvas: windows go exactly where you drag them, may overlap,
and remember their spot.

- **Move a window**: drag it anywhere — body or title bar (interactive
  content like text selection and links still wins over the drag). To
  move a window **between zones**, **Alt+drag** it; the drop point
  becomes its new position. Within its zone, just drag it.
- **Resize**: drag any window edge or corner.
- **Snapping**: while you drag or resize, edges snap to the zone's
  bounds, to other windows in the same zone, and (optionally) to a grid.
  Engaged snaps draw a guide line with the matched coordinate, and the
  grid shows as a faint overlay during the gesture. **Hold Shift** to
  place a window freely. Tune it under **Settings → GUI**: snap distance,
  targets (other windows / pane edges / pane center), grid pitch, and
  **Grid sizing** (moves also pull each edge onto the grid, so windows
  conform to it as you drag).
- **Sidebar width**: drag the strip on the sidebar/center boundary — it
  stays grabbable even with a window parked flush against it. Header and
  footer heights have their own edge handles.
- **Lock Window** (context menu) pins a window's position and size;
  `.lockwindows` locks everything at once.
- **The Windows manager**: the **Windows** button in the toolbar opens one
  window listing everything the client knows about — layout widgets, game
  dialogs, streams, and containers — grouped by category and collapsible.
  Each row has a **show/hide checkbox** and a **Zone** selector; **➕ Add
  window…** creates a new one (custom windows open the editor). Hidden
  windows stay hidden even when the game re-sends them (there is no dialog
  blocklist — hiding is the control).
- **Right-click** a window body for its context menu — including **Edit
  Window…**, which opens the window editor; title bars can be hidden
  per-window. Overlapping windows offer **Send to Back**, dropping the
  window behind any it covers so a buried one can be reached (clicking a
  window still raises it to the front).
- Windows can be **detached** into separate OS windows (restored across
  sessions), or locked together into tab groups that move as a unit. The
  context menu reorders group members (⬆ / ⬇) and can ungroup one member
  or the whole group.

Layouts saved by older versions arranged sidebar windows in a fixed
stack; they convert to freely placed windows automatically the first
time each sidebar is shown, keeping their on-screen positions.

## The Map

The GUI renders a live [map](../widgets/map.md) of your surroundings: a
mini map widget that follows your character (click a room to walk there),
and a **Map Explorer** native window for browsing any mapped location,
with a drag-to-tidy override editor. Map data comes from your Lich install
or a one-click download in **Settings → Map** — see the
[Map page](../widgets/map.md) for setup.

## The Window Editor

Right-click a window → **Edit Window…** (or `.editwindow`) to configure
it in place. Beyond title, streams, and buffer size, the editor exposes:

- **Text windows**: per-line **timestamps** (with an at-line-start
  toggle) and **compact** mode (drop blank lines).
- **Tabbed windows**: add, remove, rename, and reorder tabs; edit each
  tab's stream subscriptions; a **Quiet** toggle stops a tab from
  marking unread; per-tab timestamps.
- **Progress bars**: bar color (hex or palette picker) and display
  modes (`value/max` or bare `value` instead of a filled bar).
- **Countdowns**: a **fill color** override (defaults: roundtime red,
  casttime blue).
- **Active effects**: category (spells/buffs/debuffs/cooldowns).
- **Vitals bars**: orientation, height, text format, and per-bar toggles
  are edited here, on the vitals window itself.
- **Targets windows**: also edit the global `target_list.*` display
  settings (status position, truncation, boss colors) in place.
- **Speech**: a **speak new lines (TTS)** checkbox reads everything routed
  to this window aloud (needs TTS on in Settings > Speech).
- **Delete Window** — actually removes the window from the layout
  (unlike hiding, or the `.deletewindow` command, which only hides).

## Streams & Custom Windows

**Windows menu → Streams & Custom Windows…** opens a panel that does two
things:

- **Stream routing** — every stream seen this session, with a route for
  each: a window, main, or discard. This is the GUI counterpart of the
  `.streams` editor and writes `[streams.routes]` in
  [config.toml](../configuration/config-toml.md#stream-routing).
- **Custom windows** — author text windows fed by any Lich stream id.
  Name the window, type comma-separated stream ids — or click one from
  the seen-streams list — and it starts collecting that output. The panel
  also edits or deletes existing custom windows. (The TUI can do the same
  from its window editor's Streams field; `Ctrl+P` there opens the same
  seen-streams picker.)

## Lich WebUI Panels

Lich 5.18+ scripts can register live UI pages (`;ui`). The GUI renders
those pages as **native docked panels** — real widgets, not an embedded
browser:

```
.webui            # connect to Lich's WebUI and pick from its pages
.webui <page>     # open a specific script's page as a panel
.webui off        # disconnect the bridge
```

Open panels are saved with your layout and reconnect automatically at
login. Requires a Lich proxy connection (not `--direct`). Works with
containerized Lich too — the bridge follows the host Lich advertises in
its handshake instead of assuming localhost.

## Appearance

Open `.settings` → GUI panel:

- **Zoom** (Ctrl+= / Ctrl+- / Ctrl+0, or the slider), **text size**,
  **density** (spacing scale — the default approximates Wrayth's compact
  look), title bar size, bar corner radius.
- **Fonts**: pick any installed system font app-wide, or per-window.
- **Vitals bars**: orientation, height, text format, and per-bar toggles
  (health/mana/stamina/spirit/mind/encumbrance/...), with automatic
  light/dark bar text for contrast.
- Per-window overrides: text size, accent (border) color, wrapping, fonts.

Every size is adjustable — the Wrayth-like defaults are just defaults.

## Interact Mode

Press **F6** (keybind action `interact_mode`) for pointer-free interaction
with the room — built for controller players (map your d-pad to the arrow
keys with Steam Input or similar) and anyone who'd rather not mouse:

- **↑/↓** cycle entities in the current category, **←/→** switch category
  (creatures → objects → players → exits).
- **Enter** opens the same server context menu a click would — arrow keys
  and Enter navigate it, Esc backs out to interact mode.
- Activating an **exit** walks that direction and leaves the mode.
- The focused entity is highlighted in the room window, named in a status
  overlay, and announced through TTS when speech is enabled.
- **Esc** exits the mode.

## Controllers

Plug in a gamepad and the GUI reads it natively (Xbox and PlayStation
pads verified on Windows; no mapper software needed — turn yours off to
avoid doubled inputs).

- **Left stick** walks the 8 compass directions — deflect toward
  northeast to head `ne`; one step per deflection.
- **D-pad** covers the rest of movement by default: up/down go `up` and
  `down`, right goes `out`, and left runs [`.portal`](../reference/commands.md)
  — walk the room's door/arch/gate, with a pick list when there are
  several.
- While a **context menu** is open, the d-pad navigates, South
  (A/cross) confirms, East (B/circle) cancels — that part is fixed.
- In **[interact mode](#interact-mode)** the **right stick** does the
  cycling — up/down switch categories, left/right step entities — and
  **South selects** (menu, or walk the exit). Everything else keeps
  its binds: the left stick still walks, the d-pad still runs its
  commands, and the other face buttons (plus the shift bank) fire
  their macros. Macros may use `<target_id>` / `<target_noun>`,
  filled from the focused entity at press time — bind
  `target #<target_id>\rincant 611\r` to West and cast at whatever
  the ring is on. Toggle the mode off from its Start bind.

Everything else is yours to bind with **`.controller`**: each button
maps to a keybind action or macro, with a "press a button" capture in
the editor (`look` on South and interact mode on Start ship as
defaults). Bindings live in the `[controller]` table of
[controller.toml](../configuration/controller-toml.md). A **Save to:**
switch at the top of the editor picks global (all characters) or the
active character's own override file; loading merges character over
global, so a class can keep only its own diffs.

Beyond plain bindings:

- **Shift layer** — hold the button bound to `controller_shift` (l2 by
  default) and every button switches to a second bank, edited on the
  editor's Shift tab. Defaults: shift+d-pad pages the story window,
  shift+South stands up.
- **Radial command wheels** — hold the button bound to
  `controller_wheel` (r2 by default), aim with the free stick (the one
  that isn't walking — right, unless you move `movement_stick`), release
  to fire. Slices can be **folders** (South opens, East backs up) and
  carry **colors** — the wedge tints from its aim floor out to the rim
  (dim at rest, bright while aimed), so the colored band is exactly
  the zone where the slice activates and wheels can be color-coded by
  function. Wedges don't
  have to be even: give a slice a fixed **span** in degrees, rotate the
  ring with **Start**, and set a per-slice **inner** floor that demands
  a deeper stick throw before that slice will aim. Multiple named
  wheels via `controller_wheel:<name>` bindings; build it all on the
  editor's Wheels tab. Its **Visual** view is a drag-and-drop designer:
  drag a divider to trade width between wedges (it snaps to the compass
  points — hold Shift to go free), drag a wedge's floor arc to set its
  inner, drag a wedge's body to reorder the ring, click a wedge to edit
  its fields, double-click a folder to step inside (and the Back wedge
  to step back out), plus lock / even-out / mirror / rotate tools.
  Inside a folder, **Add Back** turns the auto Back ghost into a real
  slice you can move, resize, and color (dwelling it still ascends);
  **Remove Back** restores the ghost. **Lock** a slice to freeze its
  width — its span field and dividers disable, and even-out leaves it
  alone. **Numeric** shows the same wheel as exact rows. One name is
  dynamic:
  **`controller_wheel:portals`**
  (r3 by default) fills its slices from the current room's noun exits —
  the same list [`.portal`](../reference/commands.md) resolves — so
  `go gate` / `climb ladder` are always one hold-and-flick away.
- **Binding legend** — Select toggles a compact overlay of your
  bindings (`controller_overlay` action). It's curated: check **HUD**
  on the rows you want shown. While shift is held the shift entries
  read strong — the legend always shows what the pad does right now.
- **Rumble** — the pad buzzes when roundtime ends, you're stunned, or
  you die; pattern per event on the editor's Rumble tab. Beyond the
  built-ins (off/short/long/double) you can define **custom patterns**
  there — strength, buzz length, buzz count, gap, with a **Test**
  button that plays the row on the pad — and any
  [highlight rule](../customization/highlights.md) can name a pattern
  to buzz when its text matches (rate-limited so a chatty pattern
  can't vibrate continuously).
- **Right stick** scrolls the story window with an analog speed curve;
  the same page-scroll actions work from any bound key or button.
- With several portals in a room, `.portal` (d-pad left) opens a
  pad-navigable picker menu.

## Speech (Text-to-Speech)

**Settings > Speech** holds the TTS controls: enable, rate, volume, a
voice picker, pronunciation substitutions, gag patterns, and a **Test**
button that speaks a sample line. Pick which windows are read aloud with
the per-window **speak new lines** checkbox in the window editor; the
[`.tts` commands](../reference/commands.md#text-to-speech) drive the same
settings from the input line.

## Graphics

The GUI draws real graphics where the terminal uses characters:

- **Compass rose** — a vector rose with lit direction markers.
- **Injury paperdoll** — a vector body diagram colored by wound/scar
  severity. With a skin that ships doll art, wounds render as generated
  dots (solid = wound, ring = scar, numeral = rank) at positions you set
  by clicking in Settings > Appearance > Skin > Calibrate injury doll.
- **Status icons** — vector pictograms for stance, hidden, stunned, and
  the rest of the dashboard/indicator set.

All of it can be reskinned with your own images — window background art,
nine-slice borders, icon sprites, a sprite compass and paperdoll. See
[Skins](../customization/skins.md).

## Differences from the TUI

- Copying is plain text: select with the mouse, `Ctrl+C`. Selections are
  anchored to the text itself, so they survive scrolling — drag past the
  window edge to auto-scroll, and copy picks up everything selected, even
  lines currently scrolled out of view.
- `Ctrl+F` opens in-window search with match highlighting.
- Up/Down in the input bar browse command history; whatever you were
  typing is stashed and restored when you come back down.
- Terminal-only commands (`.setpalette`, `.resetpalette`, `.transparent`)
  don't apply; themes handle appearance instead.
