# Map

> Know where you are and where that road goes — and click a room across town to walk there,
> without keeping a wiki tab open beside the game.

## What it's for

GemStone is big, and the part of it you can see is one room of prose. Everything else lives in
your head or in a browser tab: which way the bank is, whether this alley connects back to the
square, how far you drifted while hunting.

The map draws the town around you from the map database — rooms as squares, exits as lines,
your room ringed — and recenters as you walk. Go inside a building and it swaps to that
building's floor plan on its own. Click any room you can see and you walk there, using the
client's own pathing. No Lich required, which is why it works on the phone too.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Get map data first — the map cannot draw without it. Open **Settings** in the top toolbar,
   go to the **Map** section, and click **Download map data**.
   (Typed equivalent: `.mapdb download`.)
2. Click **Windows** in the top toolbar, expand **Navigation**, and tick **Map**. Use the row's **zone**
   control to place it. (Typed equivalent: `.addwindow map map 0 0 30 12`.)
3. Right-click the map for its two controls: **Custom map zoom** — tick it to reveal a
   pixels-per-cell slider, untick to return to the default 16 — and **Open Map Explorer**.

<figure class="shot" data-shot="widgets/map-gui-minimap-and-zoom">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A Map window showing the current room ringed with exit ticks, its right-click menu open on <b>Custom map zoom</b> and <b>Open Map Explorer</b>.</figcaption>
</figure>

→ **Expected result:** the town draws around you with your room ringed, and it recenters with a
short glide each time you move. Clicking another room starts a trip to it.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**The terminal cannot draw the map.** A map window in the TUI renders one line of text:
`Map is available in the GUI frontend (--frontend gui)`. This is deliberate — the map is a
painted canvas of squares and lines, and a character grid has nowhere to put it.

What you still get in the terminal is everything the map is *for*, minus the picture:

- `.go2 <destination>` walks you anywhere, and it is the same engine the map clicks use.
- `.go2 targets` lists tagged destinations reachable from here, nearest first, each as a
  ready-to-run command.
- `.room` prints how your current room resolved against the map database.
- `.mapdb` manages the data itself — the map database is shared, so downloading it in the
  terminal sets you up for the GUI as well.

If you want the picture, run the same character with `--frontend gui`.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow map` alone
> prints usage and adds nothing.

→ **Expected result:** `.go2 bank` walks you to the nearest bank and prints its progress, and
`.room` confirms the map database recognizes where you are standing.
{{#endtab}}
{{#tab name="Mobile"}}

The phone has a full map, and it is the same generated layout the desktop draws.

1. Download map data once with `.mapdb download` typed into the command input.
2. When your room resolves against the database, a 🗺 **Map** button appears in the top bar.
   Tap it for a full-screen map.

Inside the map: **drag to pan**, **pinch to zoom**, and **tap a room to walk there** — the map
closes itself as the trip starts. Panning turns following off; the button in the title bar reads
**Following** or **Follow**, and tapping it re-locks the view to your character.

Tap the **location name** in the title bar to browse anywhere else on the map. Filter the list,
pick a location, and tap a room there to travel across the world. While browsing, that same
button reads **Return** and brings you back to where you are standing.

While a trip runs, a progress banner rides on the map and a **■ Stop** button appears in the
title bar.

→ **Expected result:** the map button appears in the top bar, opens a pannable full-screen map
following your character, and tapping a room closes the map and starts walking you there.
{{#endtab}}
{{#endtabs}}

## Common setups

### A corner mini map you never think about again

Download the data, add the map window, and send it to a zone where it can stay — a corner of the
**Right Bar** works. Leave zoom at its default. Then forget it: it follows you, swaps to a floor
plan when you step inside a building, and swaps back when you leave.

**You'll see:** a small live map that always shows the block you are standing on, with your room
ringed and its exits ticked around the square.

### Reading a whole town without leaving your chair

Right-click the map and choose **Open Map Explorer**. It opens as its own OS window, so you can
park it on a second monitor. Turn **Follow** off, drag to pan and scroll to zoom, then click a
room to select it and read its details in the collapsible **Description**, **Environment**,
**Forageables**, **Tags**, and **Exits** sections. **Walk here** sends you to the selected room;
double-clicking a room does the same in one gesture.

**You'll see:** the full town laid out, with any room's title, tags, and exits one click away —
and a trip starting the moment you press **Walk here**.

## Tips & gotchas

> ⚠️ **The mini map does not pan or zoom by gesture.** Dragging it moves the *window*, and the
> scroll wheel does nothing to it. Zoom is the **Custom map zoom** checkbox in its right-click
> menu. Drag-pan and scroll-zoom live in the **Map Explorer**, and pinch-zoom on the phone.

> ⚠️ **The map is GUI and mobile only. The terminal prints a one-line hint instead of drawing.**
> The travel commands behind it work identically in all three.

**An empty map is telling you which thing is missing.** Each message means something different:
*"Download map data in Settings > Map (or point at your Lich folder)"* means no database;
*"Waiting for a mapped room…"* means the database loaded but your room hasn't matched yet;
*"Generating map…"* means the layout is being computed and will appear shortly. Layouts are
generated once per location and cached on disk, so only the first visit waits.

**Nothing downloads on its own.** **Download map data** and `.mapdb download` are explicit
actions. `.mapdb` on its own reports the loaded database, its room count, and which release you
have.

**Downloaded releases carry GemStone data.** DragonRealms sessions get their map from a Lich
install instead — set **Lich folder** under Settings ▸ Map.

**Unmapped shop interiors are normal, not broken.** Map maintainers leave most shop interiors
out on purpose, because they change constantly. Walk into one and the map holds the street
outside: what is on screen stays mapped truth.

**Cartography mode sketches those interiors instead.** Turn on **Cartography Mode** in Settings ▸
Map and unmapped rooms draw as dashed, dimmed **ghost rooms** hanging off the room you entered
from. Dashed means "what your client saw this session"; solid means "in the database". Ghosts are
never saved — they vanish when you close VellumFE, so they can never go stale. Ghost *capture*
runs whether or not the mode is on; the setting controls only whether you see them.

**Your map learns things it never writes down.** Run `forage sense` or a ranger's `sense` and the
response is captured for the room you are in, showing up in the Map Explorer's **Environment**
and **Forageables** sections. Like Lich's in-memory map edits, these are session-only — the map
database on disk is never modified.

**Player-shop warrens get their own map per town**, listed in the location picker as, for
example, **Mist Harbor (Player Shops)**. Walking in switches the map over the same as entering
any other location, which keeps the town map readable.

**Explorer edits survive map updates.** The Explorer's **Edit** toggle lets you drag a group of
rooms to tidy a layout (hold **Alt** for a single room). Edits save as per-room override diffs
layered on top of any community-curated overrides shipped with the data. **Reset overrides**
clears only your own layer.

**If the map is stuck on the wrong room, run `.room`.** It prints how your current room resolved,
including its id, location, and edge count. On connections that never report a room id, the map
falls back to matching title, description, and exits — and only trusts an unambiguous match,
holding in place otherwise.

## See also

- [Travel (.go2)](./travel.md) — the commands behind every map click
- [Compass](./compass.md) — the current room's exits, for the step you are about to take
- [Room Window](./room-window.md) — the room's own prose and exits as text
- [Travel & Day Passes](../features/travel-engine.md) — how the pathing engine routes, and what
  it costs to cross paid edges

<details>
<summary>Config reference (TOML)</summary>

### Per-window (`layout.toml`)

`widget_type = "map"`. This widget has exactly one field of its own.

| Field | Type | Default | What it does |
|---|---|---|---|
| `zoom` | float | `16.0` | Pixels per grid cell. Clamped to 2.0–96.0 at paint time. |

Everything else is a standard window field. The shipped template is 12 rows by 30 columns, with a
floor of 5 rows by 10 columns.

```toml
[[windows]]
name = "map"
widget_type = "map"
title = "Map"
row = 0
col = 0
rows = 12
cols = 30
show_border = true
zoom = 16
```

### Global (`config.toml`)

`[map]` — all four are editable in **Settings ▸ Map**, which also shows the downloaded version
and offers **Download map data** and a delete action.

| Field | Type | Default | What it does |
|---|---|---|---|
| `mapdb_path` | string | unset | Explicit map database JSON file. Outranks everything. |
| `mapdb_repo` | string | `"Nisugi/mapdb"` | GitHub `owner/repo` whose releases carry `mapdb.json`. Empty disables downloads. |
| `lich_dir` | string | unset | Lich install folder (the one containing `data/`). The newest `data/<GAME>/map-<timestamp>.json` for the connected game is used. |
| `mapping_mode` | bool | `false` | Cartography Mode — render unmapped rooms as ghost sketches. |

**Source priority: explicit file, then downloaded release, then Lich folder.** The newest
download plus one rollback are kept under `~/.vellum-fe/mapdb/`. If a release also carries an
`overrides.json` asset, it is applied underneath your own edits.

**Dot-commands, available in every frontend including the phone:** `.mapdb` (status),
`.mapdb download`, `.mapdb remove`, `.mapdb repo <owner/repo>`, `.go2 reload` (force a fresh
load), and `.room` (how the current room resolved).

```toml
[map]
lich_dir = "C:/Lich5"
mapdb_repo = "Nisugi/mapdb"
mapping_mode = false
```

</details>
