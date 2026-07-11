# Travel (.go2)

Native map travel — VellumFE walks you across the world using its own
pathfinding over the map database. No Lich session needed, which means it
works everywhere the map does, including the mobile apps.

```text
.go2 bank              nearest room tagged "bank"
.go2 8966              a mapdb room id
.go2 u7150105          a game uid
.go2 town square       text search over room titles (a pick list if several match)
.go2 home              a saved target
.go2 back              where your last trip started
.go2 stop              cancel the active trip
.go2 status            progress and ETA
.go2 save home         save the current room as "home" (.go2 save home 8966 for an explicit id)
.go2 targets           list saved targets
```

Starting a trip prints the room count and ETA; while traveling, the map
widget shows a progress banner and arrival reports the actual travel
time. Clicking a room on the mini map or in the Map Explorer travels
there too (see below).

## What it does on the way

The walker behaves like Lich's go2: it waits out roundtime between
moves, stands up first when needed (skipping that for swim/pedal
movement), pauses while you're stunned or webbed, and aborts if you die.
A move that keeps failing gets that edge disabled for the session and
the route recomputed — same for ending up somewhere unexpected (fleeing,
being teleported, walking by hand mid-trip).

## What it can't do (yet)

Lich's go2 has a decade of special cases; native v1 deliberately walks
the common world and leaves the exotic paths out:

- **No silver handling** — routes that require paying (ferries,
  portmasters, Chronomage day passes) are excluded from pathing rather
  than attempted. The same goes for urchin guides and the Confluence.
- **Scripted edges** are supported where VellumFE understands the
  script — about a quarter of them, covering doors, levers, pauses,
  spell-gated and sitting-gated passages. With plain movement included,
  roughly 93% of all map edges are walkable. A route that *requires* an
  unsupported edge fails with "no route" instead of walking you into a
  dead end.
- **One room at a time** — no typeahead pipelining, so long trips are a
  bit slower than Lich's go2.

If a specific edge you care about isn't walkable, it can be hand-taught:
copy `travel_overrides.toml` from the defaults into `~/.vellum-fe/` and
add the edge (the file documents the format). Overrides beat everything
the mapdb says about that edge.

## Settings

**Settings → Travel** in the GUI:

- **Map clicks travel natively** (default on) — turn off to have map
  clicks send `;go2 <id>` to Lich instead, if you prefer its go2 for
  silvers/day-pass trips.

```toml
[go2]
native_map_clicks = true
# saved targets are written here by .go2 save
```

## Debugging

`.room` shows how your current room resolved against the map database —
the first thing to check when `.go2` says your room is unknown.
