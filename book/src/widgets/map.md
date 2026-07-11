# Map

A live map of where you are — rooms as squares, exits as lines, your
current room highlighted. VellumFE generates the layout itself from the
Lich map database, so the map works anywhere the mapdb does, including
mobile where there is no Lich install.

**GUI only.** The TUI shows a placeholder; run with `--frontend gui` (or
use the mobile apps).

## Mini Map Widget

```toml
[[windows]]
name = "map"
widget_type = "map"
row = 0
col = 0
rows = 12
cols = 40
zoom = 16          # pixels per grid cell (optional)
```

Or in-app: `.addwindow map`.

The mini map follows your character: it recenters as you move and swaps
sheets automatically when you go inside a building (interiors render as
their own floor plan). Click a room to walk there via `;go2` — this needs
a Lich connection with the go2 script available.

Right-click the window for a zoom control and **Open Map Explorer**.

## Ghost Rooms (Unmapped Interiors)

Map maintainers deliberately leave most shop interiors out of the map
database — they change too often to keep current. When you walk into an
unmapped room, the mini map sketches a **ghost room** instead of losing
you: a dashed, dimmed square hanging off the street you entered from,
labeled with the command that took you inside ("go shop"). Moving between
unmapped rooms grows the sketch, and hovering a ghost shows the room's
title.

Ghosts are drawn dashed on purpose: solid squares are mapped truth,
dashed squares are what your client saw this session. The sketch lasts
until you close VellumFE — it is never saved, so it can never go stale.

## Map Explorer

A separate native OS window (like detached tabs) for browsing the whole
map, not just where you're standing:

- **Location picker** with filtering — jump to any mapped location
- **Follow mode** — track your character as you move
- **Outdoor / interiors sheets**, drag-pan, scroll-zoom
- **Room inspection** — click a room to see its details
- **Walk-to** — double-click a room to travel there (`;go2`)
- **Override editor** — an edit mode where you drag room groups (Alt-drag
  a single room) to tidy a layout; edits are saved as per-room override
  diffs that survive mapdb updates, and edges can be restyled (e.g.
  dashed) without recomputing the layout

## Where the Map Data Comes From

The map needs a Lich-format map database. Sources, in priority order:

1. **Explicit file** — `mapdb_path` points at a specific `map-*.json`
2. **Downloaded release** — the Download button in **Settings → Map**
   pulls the latest `mapdb.json` release asset from a GitHub repository
   (`mapdb_repo`, default `Nisugi/mapdb`). This is how mobile gets a map
   without Lich. The newest version plus one rollback are kept under
   `~/.vellum-fe/mapdb/`.
3. **Lich install** — `lich_dir` names the folder containing `data/`; the
   newest `data/<GAME>/map-<timestamp>.json` for the connected game is
   used (per-game: GSIV, GST, GSPlat, DR, ...).

Nothing downloads automatically — the Download button is an explicit
action. Downloaded releases carry GemStone data, so DragonRealms sessions
use the Lich folder.

## Configuration

```toml
[map]
lich_dir = "C:/Lich5"            # Lich install (folder containing data/)
# mapdb_path = "C:/maps/map.json"  # explicit file; overrides everything
mapdb_repo = "Nisugi/mapdb"      # GitHub repo for the Download button
```

All three are editable in **Settings → Map** in the GUI, which also shows
the downloaded version and offers **Download map data** / delete-downloaded
actions.

## Notes

- Layouts are generated in the background the first time you enter a
  location and cached on disk — instant thereafter.
- The map draws in your theme's colors (light/dark aware).
