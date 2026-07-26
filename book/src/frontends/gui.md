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
center, and right sidebar. Toggle zones from the top toolbar.

- **Move a window**: drag its title bar (free placement in the center), or
  **Alt+drag** the window body to move it between zones.
- **Resize**: drag any window edge or corner.
- **Add/hide windows**: the **Windows** menu in the toolbar — add from
  categorized templates, toggle visibility, or reassign a window's zone.
- **Right-click** a window body for its context menu — including **Edit
  Window…**, which opens the window editor; title bars can be hidden
  per-window.
- Windows can be **detached** into separate OS windows (restored across
  sessions), or locked together into tab groups that move as a unit. The
  context menu reorders group members (**Move Up / Move Down**) and can
  ungroup one member or the whole group.

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
- While [interact mode](#interact-mode) or a context menu is open, the
  **d-pad navigates, South (A/cross) confirms, East (B/circle)
  cancels** — that part is fixed.

Everything else is yours to bind with **`.controller`**: each button
maps to a keybind action or macro, with a "press a button" capture in
the editor (`look` on South and interact mode on Start ship as
defaults). Bindings live in the `[controller]` table of the global
keybinds.toml and apply to all characters.

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
