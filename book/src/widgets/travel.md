# Travel (.go2)

> Type where you want to be and put your hands back on the keyboard — `.go2 bank` walks you
> there, waiting out roundtime and standing you up on the way.

## What it's for

Crossing town is a dozen movement commands you already know, typed in the right order, with a
`stand` in the middle because something knocked you down. It is not hard. It is tedious, and you
do it forty times a session.

`.go2` does the walking. Name a destination — a room id, a tag like `bank`, a name you saved, or
plain text to search for — and the client paths there over the map database and walks it, waiting
out roundtime, standing you up, and pausing while you are stunned or webbed. Press **Esc** and it
stops.

This is VellumFE's own engine, not a wrapper. It runs without Lich, which is why it works on the
phone. If you *are* on Lich, you can hand the hard cases back to `;go2`.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Get map data first — travel paths over the same database the map draws. Open **Settings** ▸
   **Map** and click **Download map data**. (Typed equivalent: `.mapdb download`.)
2. Type a destination in the command input:

   ```text
   .go2 bank          the nearest room tagged "bank"
   .go2 8966          a map database room id
   .go2 u7150105      a game uid
   .go2 town square   text search over room titles, with a pick list if several match
   ```

3. Open **Settings** ▸ **Travel** to decide what the engine may spend and use. **Native Map
   Clicks** is on by default; the paid-travel switches — **Use Portmasters**, **Use Urchin
   Guides**, **Use Day Passes**, **Get Silvers** — are all off until you turn them on.

<figure class="shot" data-shot="widgets/travel-settings-travel-section">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>Settings with the <b>Travel</b> section open, showing <b>Native Map Clicks</b> on and the paid-travel switches off.</figcaption>
</figure>

→ **Expected result:** a line like `[go2] -> Town Square Central (8966): 12 rooms, ETA 0:47`,
then your character walks it, and `[go2] arrived at room 8966 - travel time 0:44` on arrival.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Travel works fully in the terminal — the map picture is the only GUI-only part.

1. `.mapdb download` fetches the map database. The database is shared, so this only happens once.
2. `.go2 <destination>` starts a trip. `.go2` with no argument prints the full usage line.
3. `.settings` opens the settings editor, where the **Travel** section holds the same switches
   the GUI shows.

Because the terminal cannot draw the map, `.go2 targets` is your map: it prints tagged
destinations reachable from here, nearest first, each as a runnable command with its ETA.

> ⚠️ **Esc cancels a trip only in Normal mode.** If a popup, menu, or editor has the keyboard,
> Esc belongs to that instead. `.go2 stop` always works.

→ **Expected result:** `.go2 targets` prints a nearest-first list like
`.go2 bank  -> room 8966 (0:47)`, and running one of those lines walks you there.
{{#endtab}}
{{#tab name="Mobile"}}

Travel is fully available on the phone, and it is the reason the phone can path at all — the
engine is native, so no Lich is involved anywhere.

Type `.go2 <destination>` into the command input exactly as on the desktop. Every form works:
tags, room ids, saved names, and text search.

The more natural phone gesture is the map. Tap 🗺 **Map** in the top bar and tap a room — the map
closes and the trip starts. Tap the **location name** to browse another town and tap a room there
to travel across the world.

While a trip runs, a progress banner rides on the map and a **■ Stop** button appears in the
title bar; tapping it cancels the trip. There is no Esc key on the phone, so **■ Stop** and
`.go2 stop` are the two ways to cancel.

**Settings ▸ Travel is available on the phone**, with the same switches as the desktop.

→ **Expected result:** tapping a room on the map starts a trip, the banner counts rooms down as
you walk, and **■ Stop** cancels it.
{{#endtab}}
{{#endtabs}}

## Common setups

### Save the places you actually go

Stand somewhere you return to constantly and save it:

```text
.go2 save home
.go2 save shop 8966
```

The first saves the room you are standing in; the second saves an explicit id. Afterwards
`.go2 home` walks you there from anywhere, and `.go2 saved` lists everything you have saved.
`.go2 back` returns to wherever your last trip started.

**You'll see:** `[go2] saved 'home' -> room 8966 (travel there with .go2 home)`, and from then on
`.go2 home` is a one-line trip from any town.

### Find the nearest shop without knowing its name

Run `.go2 targets` anywhere. It prints tagged destinations reachable from where you stand,
nearest first, with an ETA on each.

**You'll see:** a list like `.go2 furrier  -> room 1247 (0:22)` — copy the line, run it, and you
are walking. This is the fastest way to orient in a town you don't know.

## Tips & gotchas

> ⚠️ **`.go2 targets` and `.go2 saved` are different lists.** `targets` is the map's directory of
> *tagged* destinations near you (banks, shops, guilds). `saved` is *your* named rooms from
> `.go2 save`.

> ⚠️ **Paid travel is opt-in, and off by default.** Ferries, portmasters, urchin guides, and
> Chronomage day passes are all supported, but each has its own switch in Settings ▸ Travel. With
> them off, routes that need them are excluded from pathing rather than attempted — so a trip may
> report no route where Lich's `;go2` would have paid its way through.

**Destinations are tried in a fixed order**, and the first match wins: `back`, then a room id,
then a `u`-prefixed uid, then your saved names, then a map tag, then a text search of titles and
descriptions. That is why saving a target named for a tag shadows the tag — the client refuses
names that would shadow a room id or `back` for the same reason.

**There are more destination words than the usage line shows.** `.go2 guild` and `.go2 guild
shop` route to your profession's guild, which needs your profession known — run `INFO` once if it
reports the profession is unknown. `.go2 locker` and `.go2 public locker` find the nearest locker
you can use. `.go2 goback` is an alias for `.go2 back`.

**Ambiguous text gives you a pick list, not a guess.** `.go2 town square` with several matches
prints the candidates with their ids so you can rerun with an exact one.

**Failures name their cause.** `[go2] your current room hasn't resolved against the mapdb yet` is
the common one — run `.room` to see how your room matched. Others are direct:
`room {id} is not in the mapdb`, `you're already here...`, and
`map database not loaded - configure it in Settings > Map`.

**A trip that goes wrong re-paths instead of stranding you.** Ending up somewhere unexpected —
fleeing, being teleported, walking by hand mid-trip — re-paths from where you actually are. A
move that keeps failing gets that edge disabled for the session and the route recomputed. Dying
aborts the trip.

**One automation drives at a time.** Starting a trip while something else is running (a `.foreach`
batch, for instance) reports `[go2] {that} is driving - .stop to cancel it first.` `.stop` cancels
whatever is running; `.go2 stop` cancels a trip specifically.

**Map clicks can be handed to Lich instead.** Turn **Native Map Clicks** off and clicking a room
sends `;go2 <id>` to Lich rather than walking natively. Separately, **Lich ;go2 Fallback** hands
off to Lich only when native travel hits an edge it cannot cross. Both need a Lich connection — a
direct connection has no Lich to hand off to.

**Some regions scramble movement**, so their map edges are meaningless to walk. These are declared
as mazes and the engine uses a per-maze strategy instead. The shipped example is the Mist Harbor
Ranger Guild jungle approach, where each character has a personal route revealed by an NPC — the
walker asks automatically, captures the answer, and reuses it on every later trip.

## See also

- [Map](./map.md) — the picture behind click-to-travel, and where map data comes from
- [Compass](./compass.md) — one room's exits, for the step you are taking by hand
- [Room Window](./room-window.md) — the room prose and its exits as text
- [Travel & Day Passes](../features/travel-engine.md) — how routes are planned, what paid edges
  cost, and how day passes are bought and spent

<details>
<summary>Config reference (TOML)</summary>

### Global (`config.toml`)

`[go2]` — every field below is editable in **Settings ▸ Travel** in the GUI, `.settings` in the
terminal, and the settings sheet on the phone. GUI labels are given so you can find each switch.

| Field | Type | Default | GUI label | What it does |
|---|---|---|---|---|
| `saved` | table | empty | — | Saved targets, name to room id. Written by `.go2 save`; listed by `.go2 saved`. |
| `native_map_clicks` | bool | `true` | **Native Map Clicks** | Map clicks travel natively. Off sends `;go2 <id>` to Lich. |
| `lich_fallback` | bool | `false` | **Lich ;go2 Fallback** | When native travel can't cross an edge, hand off to Lich's `;go2`. Lich connections only. |
| `use_seeking` | bool | `false` | **Use Voln Seeking** | Route through Voln Symbol of Seeking edges. Only takes effect for a Voln Master. |
| `use_portmasters` | bool | `false` | **Use Portmasters** | Route through portmaster ship travel. Costs silver. |
| `get_silvers` | bool | `false` | **Get Silvers** | Let travel withdraw from the bank to fund paid travel when short. |
| `get_return_trip_silvers` | bool | `false` | **Get Return Trip Silvers** | Also withdraw enough to fund the return trip. |
| `use_urchins` | bool | `false` | **Use Urchin Guides** | Route through urchin guides. Needs active access; off while mounted. |
| `use_day_pass` | bool | `false` | **Use Day Passes** | Route through Chronomage day-pass edges, using a pass in your day-pass sack. |
| `buy_day_pass` | string | empty | **Buy Day Pass** | When to buy a pass if none is held: `on`/`yes`, `off`/`no`, or a town-pair list like `"sol,wl imt,wl"`. Needs **Get Silvers**. |
| `day_pass_sack` | string | empty | **Day Pass Sack** | Container holding your day passes, as a noun or name fragment. |
| `weaponsack` | string | empty | **Weapon Sack** | Container the hands-stow uses for a weapon your ready sheath doesn't cover. |
| `lootsack` | string | empty | **Loot Sack** | Fallback container for anything not routed by ready, sheath, or weapon sack. |
| `pathcodes` | table | empty | — | Personal maze routes, captured automatically from the maze NPC. Never hand-edited. |

```toml
[go2]
native_map_clicks = true
use_portmasters = false
get_silvers = false
use_day_pass = false
day_pass_sack = ""
```

### Command surface

| Command | What it does |
|---|---|
| `.go2` | Print the usage line |
| `.go2 <id \| uid \| tag \| saved name \| text>` | Travel there |
| `.go2 back` / `.go2 goback` | Return to where the last trip started |
| `.go2 guild` / `.go2 guild shop` | Your profession's guild. Needs profession known — run `INFO`. |
| `.go2 locker` / `.go2 public locker` | The nearest locker you can use |
| `.go2 stop` | Cancel the active trip. **Esc** does the same on the desktop. |
| `.go2 status` | Rooms done, rooms total, and ETA for the active trip |
| `.go2 save <name> [id]` | Save a target. Without an id, saves the current room. |
| `.go2 saved` | List your saved targets |
| `.go2 targets` | Reachable tagged destinations, nearest first, with ETAs |
| `.go2 reload` | Force a fresh load of the map database |
| `.stop` | Cancel whatever automation is running, travel included |
| `.portal [n]` | Walk the room's non-compass exit (`go door`, `climb stair`). Several offer a pick menu. |
| `.room` | How the current room resolved against the map database |

### Files

- `~/.vellum-fe/travel_overrides.toml` — teach the engine an edge it cannot cross. Copy
  `travel_overrides.toml` from the defaults; the file documents its own format. Overrides beat
  whatever the map database says about that edge.
- `~/.vellum-fe/mazes.toml` — add or replace maze definitions by name, on top of the shipped set.

</details>
