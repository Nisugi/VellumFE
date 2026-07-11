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
  sessions), or locked together into tab groups that move as a unit.

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
- **Delete Window** — actually removes the window from the layout
  (unlike hiding, or the `.deletewindow` command, which only hides).

## Custom Windows

**Windows menu → Add → Custom Window…** opens an authoring panel for
custom text windows fed by any Lich stream id. Name the window, type
comma-separated stream ids — or click one from the **streams seen this
session** list — and it starts collecting that output. The panel also
edits or deletes existing custom windows. (The TUI can do the same from
its window editor's Streams field; `Ctrl+P` there opens the same
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
login. Requires a Lich proxy connection (not `--direct`).

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

## Graphics

The GUI draws real graphics where the terminal uses characters:

- **Compass rose** — a vector rose with lit direction markers.
- **Injury paperdoll** — a vector body diagram colored by wound/scar
  severity.
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
