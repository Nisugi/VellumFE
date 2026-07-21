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
their own floor plan). Click a room to walk there — natively via the
built-in [travel engine](travel.md), no Lich needed (switchable to
`;go2` pass-through in **Settings → Travel**).

Right-click the window for a zoom control and **Open Map Explorer**.

## Ghost Rooms (Unmapped Interiors)

Map maintainers deliberately leave most shop interiors out of the map
database — they change too often to keep current. In everyday play the
map simply holds your last mapped room while you're inside one: what you
see on screen is always mapped truth.

Turn on **Cartography mode** (Settings → Map) and walking into an
unmapped room sketches a **ghost room** instead: a dashed, dimmed square
hanging off the street you entered from, labeled with the command that
took you inside ("go shop"). Moving between unmapped rooms grows the
sketch, and hovering a ghost shows the room's title. Ghosts render on
the mini map, the Map Explorer, and the phone map alike.

Ghosts are drawn dashed on purpose: solid squares are mapped truth,
dashed squares are what your client saw this session. The sketch lasts
until you close VellumFE — it is never saved, so it can never go stale.

## Player Shops

Player-shop warrens (hundreds of near-identical rooms) get their own map
per town — the location picker lists them as **"Mist Harbor (Player
Shops)"** and so on, right beside their town, so the town map stays
readable. Walking into the shops switches the map over automatically,
exactly like entering any other location; `.go2` routes through them as
always.

## Your Map Learns As You Play

Type `sense` (ranger) or `forage sense` anywhere and VellumFE captures
the response for the room you're standing in: climate and terrain,
wildlife signs, the creature circling overhead, visible structures, and
the full forageables list. Select that room in the Map Explorer and it
all shows as ordinary room info.

Like Lich's in-memory map edits, these observations are **session-only**:
they're never written to disk and vanish when you close VellumFE. The
map database itself is never modified — permanence comes only through
submitting curated data (the submission workflow is under construction).

## Map Explorer

A separate native OS window (like detached tabs) for browsing the whole
map, not just where you're standing:

- **Location picker** with filtering — jump to any mapped location
- **Follow mode** — track your character as you move
- **Outdoor / interiors sheets**, drag-pan, scroll-zoom
- **Room inspection** — click a room for its details in collapsible
  sections: Description, Environment (climate/terrain, plus wildlife and
  structures you've sensed this session), Forageables (from `forage
  sense`), Tags, and Exits
- **Walk-to** — double-click a room to travel there ([native travel](travel.md))
- **Override editor** — an edit mode where you drag room groups (Alt-drag
  a single room) to tidy a layout; edits are saved as per-room override
  diffs that survive mapdb updates, and edges can be restyled (e.g.
  dashed) without recomputing the layout. Your edits layer on top of any
  community-curated overrides that came with the downloaded map data —
  **Reset overrides** clears only your own layer.

## Where the Map Data Comes From

The map needs a Lich-format map database. Sources, in priority order:

1. **Explicit file** — `mapdb_path` points at a specific `map-*.json`
2. **Downloaded release** — the Download button in **Settings → Map**
   pulls the latest `mapdb.json` release asset from a GitHub repository
   (`mapdb_repo`, default `Nisugi/mapdb`). This is how mobile gets a map
   without Lich. The newest version plus one rollback are kept under
   `~/.vellum-fe/mapdb/`. If the release also carries an `overrides.json`
   asset (community layout curation), it's downloaded alongside and
   applied automatically underneath your own edits.
3. **Lich install** — `lich_dir` names the folder containing `data/`; the
   newest `data/<GAME>/map-<timestamp>.json` for the connected game is
   used (per-game: GSIV, GST, GSPlat, DR, ...).

Nothing downloads automatically — the Download button is an explicit
action. Downloaded releases carry GemStone data, so DragonRealms sessions
use the Lich folder.

From any frontend — including the phone apps, which have no Settings >
Map panel — `.mapdb download` fetches the latest release, `.mapdb` shows
status, `.mapdb remove` deletes downloads, and `.mapdb repo <owner/repo>`
changes the source.

## On the Phone

The mobile web client has its own map: once map data is downloaded
(`.mapdb download`) and your room resolves, a map button appears in the
top bar. It opens a full-screen view of the same generated layout the
desktop mini map shows — auto-following your character, switching to the
building's floor plan indoors, ghost rooms included when Cartography
mode is on. Drag to pan (which
pauses following until you tap **Follow**), pinch to zoom, and tap a
room to walk there with [native travel](travel.md).

Tap the **location name** in the title bar to browse any other mapped
location — filter the list, pick one, and tap a room there to travel
across the world; **Return** brings the view back to where you're
standing. While a trip is running the map shows its progress
("→ 8966 · 12/47 rooms · ETA 1:04") and a **Stop** button.

## Configuration

```toml
[map]
lich_dir = "C:/Lich5"            # Lich install (folder containing data/)
# mapdb_path = "C:/maps/map.json"  # explicit file; overrides everything
mapdb_repo = "Nisugi/mapdb"      # GitHub repo for the Download button
mapping_mode = false             # Cartography mode: sketch unmapped rooms
```

All of these are editable in **Settings → Map** in the GUI, which also
shows the downloaded version and offers **Download map data** /
delete-downloaded actions.

## Notes

- Layouts are generated in the background the first time you enter a
  location and cached on disk — instant thereafter.
- The map draws in your theme's colors (light/dark aware).
- On connections that never report a room id (some direct-connect
  setups), the map falls back to matching the room's title, description,
  and exits against the database — and only trusts an unambiguous match,
  holding in place otherwise.
