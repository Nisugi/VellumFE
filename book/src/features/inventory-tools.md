# Inventory Tools (`.foreach`, `.sorter`)

VellumFE ships native versions of two of the most-used Lich inventory
scripts, so they work **without Lich** (direct connections included). If
you *are* running through Lich, the classic `;foreach` and `;sorter`
still work as always — these are the `.`-prefixed native equivalents,
and you can use whichever you prefer.

Both rely on the same item database Lich uses (`gameobj-data.xml`); see
[the data pack](#the-data-pack-data) below for how VellumFE keeps it
fresh.

## `.foreach` — run commands over matching items

`.foreach` finds items in your containers and runs a series of commands
for each one.

```
.foreach [OPTIONS] [ATTR=]VALUE in <CONTAINER>[,<CONTAINER>...][; command; command; ...]
```

Some examples:

```
.foreach gem in backpack; get item; sell item
.foreach box in locker; move to backpack
.foreach unique sorted scroll in cloak; read item
.foreach first 5 sellable=gemshop in pouch; get item; appraise item
.foreach name=*quartz orb in inv-sack; get item; put item in locker
.foreach in bandolier
```

The last form — no commands — is a **dry run**: it lists the matching
items with their types and ids, so you can check a filter before acting
on it. Running a dry run first is a good habit.

### Filters (what to match)

| Attribute | Matches | Example |
|-----------|---------|---------|
| `type` (default) | item category from `gameobj-data.xml` | `.foreach gem in bag` |
| `sellable` | shop the item sells at | `.foreach sellable=furrier in sack` |
| `noun` | the item's noun | `.foreach noun=box in locker` |
| `name` / `fullname` | the full item name | `.foreach name=*ruby* in pouch` |
| `quick` | shorthand for `name=*VALUE*` | `.foreach q=elan*pack in cloak` |

Shorthands `t`, `s`, `n`, `m`, `f`, `q` work too. Values support `*`
wildcards, comma-separated lists (match *any*), and `/regex/` for a
regular-expression match against the name. `type=none` matches items
with no known type.

### Options (which matches, in what order)

| Option | Effect |
|--------|--------|
| `unique` | skip repeats of the same item name |
| `first N` | act on only the first N matches |
| `after N` | skip the first N matches |
| `sorted` | order alphabetically (by last word, articles ignored) |
| `reversed` | reverse the order |

### Targets (where to look)

A target is either a **container name** or a **pseudo-target keyword**. A
comma-separated list searches several. Suffix any target with `?` to
**skip it if it isn't found** instead of stopping with an error:

```
.foreach box in inv-sack, disk?; move to locker
.foreach gem in worn; get item; put item in my gem pouch
.foreach herb in floor; get item
```

**Container names** — a container you have looked in this session,
addressed by title (`backpack`, `red sack`, `my bandolier`).

> **Containers must have been opened and looked in.** Like `;foreach`,
> this is blind to closed containers — if a container isn't matching,
> `look in` it once so VellumFE has seen its contents.

**Pseudo-targets:**

| Keyword | Matches |
|---------|---------|
| `inv` / `inventory` | everything you carry — worn items, both hands, and items at your feet |
| `worn` | worn items only |
| `feet` | items at your feet ("Placed alongside you") |
| `floor` / `ground` / `room` | loot on the ground in the room |

Not yet supported (planned): `desc` (room-description scenery) and
`locker` auto-open.

### Marked / registered filters

Filter by whether items are marked or registered:

```
.foreach marked gem in my gem pouch; get item; sell item
.foreach unregistered in backpack
```

Options: `marked`, `unmarked`, `registered`, `unregistered`. This status
isn't in the passive game feed — VellumFE fetches it with an `INVENTORY
FULL` scan (its output is hidden). The first time you use one of these on
items whose status isn't yet known, `.foreach` runs the scan and asks you
to **re-run the command in a moment**; the second run has the data.

### Commands

Each command runs once per matched item. Two words are substituted:

- `item` → `#<exist id>` (the item's unique id — works on direct
  connections, since these are ordinary game commands)
- `container` → `#<id>` of the container the item was found in

So `put item in container` becomes `put #12345 in #77`. Because ids are
unambiguous, five identical gems are each handled individually with no
noun-collision guesswork.

**Implicit `get`**: if the first command is `drop`, `place`, `sell`,
`appraise`, `trash`, `register`, `mark`, or `unmark`, a `get item` is run
first automatically — so `.foreach gem in bag; sell item` gets and sells
each gem.

A few special commands are handled by VellumFE rather than sent to the
game:

| Command | Effect |
|---------|--------|
| `waitrt` | wait until roundtime clears before continuing |
| `sleep <seconds>` | pause (e.g. `sleep 1.5`) |
| `echo <text>` | print a note to your main window |
| `move to <container>` | shortcut for dragging the item into that container |

Everything else is sent to the game verbatim, once per item — including
`;yourscript item` when you're behind Lich.

### Pacing and stopping

Commands are paced: one goes out at a time, only while roundtime is
clear, with a short gap between. A run announces each item as it starts.

Stop a run at any time with **`.stop`** (or Esc). Dying stops it
automatically. Only one automation task drives the game at once — if a
`.go2` trip is in progress, `.foreach` will tell you to `.stop` it first,
and vice-versa.

## `.sorter` — categorized container contents

`.sorter` reformats the output of *looking in a container* so items are
grouped by type and duplicates are counted, instead of one long run-on
sentence.

```
.sorter on      # enable
.sorter off     # disable
.sorter         # toggle
```

With it on, `look in my backpack` turns this:

```
In the backpack you see a blue sapphire, a quartz crystal, a blue
sapphire and a copper lockpick.
```

into this:

```
In the backpack:
  gem (3): quartz crystal, blue sapphire (2)
  other (1): copper lockpick
```

Item names stay clickable, categories come from the same
`gameobj-data.xml` as `.foreach`, and the setting is remembered between
sessions (it also appears in **Settings → UI → Sort Container Looks**).

## Extended-feed commands (`.invsync`, `.find`, `.viewitem`, `.drag`)

These four commands work from a **structured snapshot** of everything you
carry, rather than by reading room prose. That buys you things `.foreach`
cannot do: search every container at once by name, pull up an item's full
detail by id, and move an item with the client *confirming* the move
landed.

### First: you need the extended feed

All four need the extended feed. In the client's own words, that means
**"the WRAYTH banner, sent in direct mode or by Lich."**

Without it the server does not answer these commands at all. There is no
error from the game — the request goes out and nothing comes back.

> ⚠️ **The symptom is silence, not an error.** If `.invsync` prints
> `[invsync] requesting inventory snapshot...` and nothing ever follows,
> you do not have the extended feed. Connect with `--direct`, or launch
> through Lich. Everything above — `.foreach`, `.sorter` — works either
> way; only these four depend on the banner.

### `.invsync` — take the snapshot

```
.invsync
```

`.invsync` sends the extended feed's `_inventory manager` request and
follows its continuation cursors until the whole snapshot has arrived.

You'll see:

```
[invsync] requesting inventory snapshot...
[invsync] snapshot complete: 213 items (room 5872).
```

Run it again and the count updates. Run it while one is already running
and it declines rather than overlapping:

```
[invsync] refresh already in progress.
```

**Refresh is manual, on purpose.** Nothing re-syncs behind your back — not
after you loot, not on a timer. When you have moved a lot of items, take a
fresh snapshot yourself.

The same snapshot is what the
[Containers window](../widgets/containers-window.md) draws as a
collapsible tree; its **⟳ Refresh** button runs this command for you.

### `.find` — where is that thing

```
.find <name fragment>
```

`.find` searches **the snapshot**, not the live game — it matches your
fragment against each item's name and its long description, and prints
where each match lives.

You'll see, for `.find sapphire`:

```
[find] 2 matches:
  a blue sapphire - in your leather backpack > blue velvet pouch  (#42817)
  a blue sapphire - at your feet  (#42901)
```

The path reads outermost-first, and a container that is shut is marked
inline — `in your leather backpack > iron strongbox (closed)` — so you
know to open it before reaching in.

With no snapshot taken yet:

```
[find] no inventory snapshot yet - run .invsync first.
```

And when the snapshot arrived only partly, every result set ends with:

```
  (snapshot INCOMPLETE - rerun .invsync)
```

> ⚠️ **Results are only as fresh as your last `.invsync`.** `.find` never
> touches the game. An item you sold two rooms ago still shows up until
> you re-sync. This is the opposite of `.foreach`, which reads containers
> you have looked in this session.

### `.viewitem` — full detail for one item

```
.viewitem <exist-id>
.inspect <exist-id>
```

`.viewitem` (and its alias `.inspect`) asks the extended feed for an
item's detail — the look, inspect, analyze and read text — in one request.

**It takes an exist id, not a noun.** `.viewitem dagger` will not work.
Ids come from the `(#42817)` column in `.find` output, and from clicking
any item in the [Containers window](../widgets/containers-window.md),
which fills in the id for you.

The answer is banner-separated by section and routed to the **`inspect`**
stream, never to your main window — an ANALYZE result alone can run pages
long. Subscribe a text window to the `inspect` stream to keep a log of
what you have looked at. In the GUI it also loads into the Containers
window's **Item** tab.

If nothing is subscribed to that stream, the client tells you where the
answer went instead of dropping it.

### `.drag` — moves that are confirmed, not assumed

```
.drag <exist> left|right|drop|wear|feet
.drag <exist> in|on|behind|underneath <dest-exist>
```

`.drag` is a **verified** move. "Verified" is literal here: the client
decides whether the move worked by watching your hands change in the
feed, and never by matching the game's prose. A move that produced no
confirming hand update inside 8 seconds is reported as a failure even if
the game printed something that looked like success.

Three outcomes, in these words:

| Outcome | What you'll see | Meaning |
|---------|-----------------|---------|
| Confirmed | `[drag] ... - confirmed.` | A hand event proved it |
| Sent | `[drag] ... - sent (container-direct; no hand event to confirm).` | Command went out; the item never passed through a hand, so nothing could confirm it |
| Failed | `[drag] ... FAILED: no confirming hand update within 8s` | The window closed with no proof |

It also refuses moves that cannot succeed, before sending anything — a
full hand, or a move already in progress. Only one `.drag` runs at a time.

> **"Sent" is not "failed."** A container-to-container move the server
> completes in one motion never touches a hand, so there is no event to
> watch. The command went out. Re-run `.invsync` and `.find` the item if
> you want to see where it ended up.

### Worked recipe: find a gem you know you own, look at it, pocket it

You remember buying a star sapphire and have no idea which bag it went in.

```
.invsync
.find star sapphire
```

You'll see:

```
[invsync] snapshot complete: 213 items (room 5872).
[find] 1 match:
  a star sapphire - in your leather backpack > blue velvet pouch  (#42817)
```

Now pull its detail and move it to your gem pouch (id `#9114`, from the
same snapshot):

```
.viewitem 42817
.drag 42817 right
.drag 42817 in 9114
```

**You'll see** the detail arrive on the `inspect` stream, then two `[drag]`
lines reporting **confirmed** — both moves proved against real hand events.
When the snapshot knows a container's noun phrase, `.drag` addresses it by
name rather than by id, which is what makes lockers work.

### Which one do I reach for?

| You want to | Use |
|-------------|-----|
| Run commands over every gem in a bag | `.foreach` (above) |
| Locate one item across every container at once | `.find`, after `.invsync` |
| Read an item's full look / analyze / read text | `.viewitem` |
| Move one item and know it landed | `.drag` |
| Browse everything you carry as a tree | the [Containers window](../widgets/containers-window.md) |

## The data pack (`.data`)

`.foreach` and `.sorter` classify items using `gameobj-data.xml`, the
same file Lich maintains through `;repo`. VellumFE resolves it from the
first available of three sources:

1. **Your Lich data folder**, if the Lich directory is configured
   (Settings → Map → Lich Directory) — the freshest copy, since your
   `;repo` keeps it current.
2. **A local copy** in `~/.vellum-fe/global/data/`.
3. **A bundled snapshot**, refreshed with each VellumFE release.

Check what's in use and how old it is:

```
.data status    # show the source and age of each data asset
.data reload     # re-resolve now (e.g. after ;repo download in Lich)
```

In the GUI, the same information and a **Reload** button live under
**Settings → Data**. Setting your Lich folder (**Settings → Map → Lich
Directory**) lets VellumFE use your `;repo`-maintained copy, which is
always the freshest.
