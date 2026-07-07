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

## Windows and Zones

The GUI arranges windows in five zones: header, footer, left sidebar,
center, and right sidebar. Toggle zones from the top toolbar.

- **Move a window**: drag its title bar (free placement in the center), or
  **Alt+drag** the window body to move it between zones.
- **Resize**: drag any window edge or corner.
- **Add/hide windows**: the **Windows** menu in the toolbar — add from
  categorized templates, toggle visibility, or reassign a window's zone.
- **Right-click** a window body for its context menu; title bars can be
  hidden per-window.
- Windows can be **detached** into separate OS windows (restored across
  sessions), or locked together into tab groups that move as a unit.

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

- Copying is plain text: select with the mouse, `Ctrl+C`.
- `Ctrl+F` opens in-window search with match highlighting.
- Terminal-only commands (`.setpalette`, `.resetpalette`, `.transparent`)
  don't apply; themes handle appearance instead.
