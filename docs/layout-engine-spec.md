# Layout Engine Specification (for the VellumFE Rust port)

Reference implementation: the `js/generation/` modules in this repo.
This document pins down the algorithms, constants, and semantics so the port
is a translation job. Bit-exact parity with JS is **not** a goal (iteration
order differs); the invariants and statistical targets in §9 are the goal.

## 1. Purpose and constraints

Generate a 2D grid layout for one *location* of the Lich mapdb, live, on
area entry:

- Deterministic: same mapdb bytes in, same layout out (given a fixed room
  iteration order — define it as ascending room id).
- Fast: JS reference runs 10ms (109 rooms) to 1.7s (3,227 rooms); Rust
  target <300ms worst case, background thread.
- Cache by content hash of the location's rooms; never persist raw layouts
  as shared artifacts. Human curation is a sparse uid-keyed override diff
  applied AFTER generation (§8).

## 2. Inputs (per room, from mapdb)

| field | use |
|---|---|
| `id` | Lich room id (may be renumbered between builds) |
| `uid[]` | game uids — the stable identity; use `uid[0]` |
| `location` | selection key |
| `title[]` | `"[Prefix, Room]"` — prefix names buildings/areas |
| `wayto{destId: cmd}` | connectivity; cmd is a movement string or `;e` stringproc |
| `dirto{destId: dir}` | hand-curated direction overrides |
| `paths` | `"Obvious exits: …"` = indoor, `"Obvious paths: …"` = outdoor |
| `climate`,`terrain` | `"none"`/`"none"` = weatherless (indoor fallback signal) |
| `image`, `image_coords` | hand-drawn overlay anchor: `[x1,y1,x2,y2]` px rect |

## 3. Direction analysis (connection-analyzer.js)

Cardinal set: `north south east west northeast northwest southeast southwest
up down` (note: `out` is NOT in the set).

`direction(room, targetId, lookup)`:
1. `dirto[target]` if present: `cross-group` → none (and edge excluded from
   positioning); `none`/`skip` → fall through; a cardinal → return it.
2. `wayto[target]`: stringproc (`;e` prefix) → only usable via dirto, else
   none. Exact cardinal → return. Otherwise scan with **word-boundary**
   regexes, longest name first: `northeast northwest southeast southwest
   north south east west down up` (so "go northeast gate" → northeast,
   "go upper hallway" → nothing).
3. Reverse inference: if target's `dirto[room]` is a cardinal → its opposite;
   if it is none/skip/cross-group → stop. Else if target's `wayto[room]` is
   an **exact** cardinal → its opposite. Extracted (word-boundary) hints are
   never reversed.

Direction offsets: N(0,-1) S(0,1) E(1,0) W(-1,0) NE(1,-1) NW(-1,-1)
SE(1,1) SW(-1,1), plus placement conveniences up(0,-1) down(0,1) out(1,0).
"Compass" = the 8 true directions; up/down/out are excluded from validation
and optimization.

## 4. Component build + BFS placement (room-positioner.js)

Repeat until all rooms placed:
- Start room: unplaced room with the most directional edges into the
  selection (ties: first encountered in ascending-id order).
- BFS over directional edges. Queue holds room ids; positions are re-read at
  processing time (rips move rooms).
- Collision → **grid rip**: shift a half-plane one cell so the occupant
  slides off the target cell and the stated direction stays true:
  - dx>0: all cells with x ≥ targetX get x+1
  - dx<0: all cells with x ≤ targetX get x−1
  - dx==0, dy>0: y ≥ targetY get y+1; dy<0: y ≤ targetY get y−1
  The parent is never inside the shifted half-plane. Rips cannot create sign
  violations (vertical edges shift together under column rips and vice versa).

## 5. Per-component optimization (room-positioner.js)

Skip if <3 rooms (still compact). Build compass adjacency both directions
(edge stored as expected sign of `other − this`). Hill climb, ≤12 passes:
- Candidates per room: for each neighbor, the ideal adjacent cell
  (`neighbor − sign`) ± 1 ring; plus current position ± 1 ring.
- Accept a free candidate iff **every** compass edge of the room keeps
  correct signs AND total Chebyshev edge length strictly decreases.
  (Strict decrease ⇒ termination.)

Then **compaction**: rank-map the distinct x values to 0..n−1, same for y.
Relative order is preserved, so all edge signs survive (proof: sign of
coordinate difference is invariant under strictly monotone maps; a
difference can never collapse to 0 because equal ranks require equal
coordinates).

Validation: every placed compass edge must sign-match its direction;
mismatches are reported as `violations` (they are genuine data conflicts,
kept visible, never "fixed" silently).

## 6. Interior classification (interior-classifier.js)

Per component:
1. `paths` majority vote: indoor rooms (`/obvious exits/i`) vs outdoor
   (`/obvious paths/i`); strict majority decides (99.5% coverage).
2. Tie/no data: literal `out` wayto whose destination lies in a **different**
   component → interior. (An `out` staying inside means the component
   contains its own outdoors — beach grottos — NOT a building.)
3. Else: all rooms weatherless (`climate=="none" && terrain=="none"`) →
   interior.
4. Propagation to fixed point: a component whose inter-component connections
   all lead to interiors is interior (back rooms behind a second door).

Entrances: every outdoor room with an edge into an interior component; these
get door markers. If the selection is entirely interiors, skip the split.

## 7. Cluster packing (cluster-packer.js)

Constants: `groupPadding=3`, `searchRadius=30`, `defaultScale=30 px/cell`,
scale clamp `[5,300]`, anchored-pair cap 20, grid-delta window `1..50`,
crossing penalty `1000`, bbox ("courtyard") penalty `4` per cell, candidate
exploration = first-fit radius + 2, committed connector length cap 30,
committed directional-segment length cap 8, bridged-contact pair cap 10,
segment/box window prefilter = searchRadius + 4.

Edges between components ("connectors") = any wayto whose endpoints are in
different packed components. Plus **bridged virtual edges**: for every
excluded (interior) component touching ≥2 packed components, add edges
between each pair of touching packed rooms.

Pass 1 — image anchors: primary image = dominant image of the **largest
anchored component** (raw room counts get fooled by collage overlays).
Scale = median over intra-component pairs of |pixelΔ|/|gridΔ| per axis.
Offset = round(mean(pixelCenter/scale − internal)); resolve collisions by
nearest-free spiral.

Pass 2 — connector BFS: pick the unplaced group with most edges to placed
(tie: min uid delta); propose landing its connector room beside the placed
neighbor (chosen by min uid delta); choose among free candidates within
first-fit+2 rings scored by
`Σ Chebyshev(connector) + 1000·(crossed committed segments) + 4·(cells
inside another placed group's bbox)`. Committed segments = placed inter-group
connectors (≤30 cells) + placed intra-group directional edges (≤8 cells).
If nothing touches the placed set, seed the largest remaining connected
group below the map, all seeds pinned to one shared left edge. Failures are
deferred, not fatal.

Pass 3 — strip: true orphans line up below everything.

Interiors sheet: wrapped shelf rows sorted by name; row width =
`max(20, ceil(sqrt(Σ (w+pad)·(h+pad))))`.

## 8. Presentation model + overrides

Render: rooms (squares), solid directional edges, **stub** any directional
edge stretched past `longEdgeCells=8` (short dashed arrow + partner room id
at both ends), dashed labeled connectors (skip past `connectorMaxCells=30`),
door markers on entrance rooms, group labels on the interiors sheet.

Override diff (all uid-keyed, applied after generation, sparse):
1. Position pins — group offset keyed by group anchor (lowest uid in the
   group, fallback lowest id; uid ≥ 7 digits so key spaces never collide);
   room pin stored **relative to its group's frame**.
2. Edge overrides keyed by uid pair — force direction / hide / demote to
   connector / choose stub side.
3. Classification overrides — force a component outdoor/interior (later:
   floor assignment).
4. Names and free annotations.

Orphaned overrides (anchor no longer resolves) are skipped silently; the
solver's placement shows. UI should visually distinguish overridden items
and allow reset-to-auto.

## 9. Validation targets (from the reference implementation)

Hard invariants, any zone:
- zero rooms sharing a cell (per sheet)
- every compass edge sign-correct OR reported in `violations`
- deterministic across runs

Statistical targets (repo mapdb, see `layout-tests/fixtures.json` for the
generated snapshot; small drift from iteration-order differences is fine,
large drift means a logic bug):

| zone | rooms | outdoor comps | interior comps (rooms) | violations | connector median len | JS time |
|---|---|---|---|---|---|---|
| Moonsedge | 109 | 4 | 21 (57) | 0 | 1 | ~10ms |
| the Atoll | 60 | 6 | 4 (8) | 0 | 1 | ~5ms |
| Mist Harbor | 1,691 | ~199 | ~692 (1,241) | ~67 | 1 | ~450ms |
| Icemule Trace | 1,354 | ~68 | ~462 (920) | ~9 | 2 | ~250ms |
| Wehnimer's Landing | 3,227 | ~165 | ~1,431 (2,563) | ~57 | 1 | ~1.7s |

Mist Harbor outdoor connector-connector crossings ≤ ~5 (reference: 3).

## 10. Explicitly out of scope / future

- up/down layering (currently up/down borrow N/S offsets in the plane)
- splitting large interiors (castles) onto their own sheets
- room-level edge-occupancy during BFS (short stretch crossings remain)
- multi-location generation (cross-location docks stay peripheral)
