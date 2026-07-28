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

Targets are containers you have looked in this session, addressed by
title (`backpack`, `red sack`, `locker sack`). A comma-separated list
searches several. Suffix a container with `?` to **skip it if it isn't
found** instead of stopping with an error:

```
.foreach box in inv-sack, disk?; move to locker
```

> **Containers must have been opened and looked in.** Like `;foreach`,
> this is blind to closed containers — if a container isn't matching,
> `look in` it once so VellumFE has seen its contents.

Not yet supported (planned): the `inv`, `hands`, `floor`, and `locker`
pseudo-targets, and the `marked`/`registered` filters.

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
