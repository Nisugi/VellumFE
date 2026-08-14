# Bestiary — the creature codex

> Look up what you're about to fight — its AS, its DS, what it wards with, what
> it drops, and which rooms it spawns in — without leaving the client.

## What it's for

You've walked into a new hunting ground and something with a scorpion tail is
looking at you. The bestiary answers the questions you'd otherwise alt-tab to a
wiki for: how hard does it hit, what should your DS be, is it undead, does it
skin, and where else does it live. It ships with the client, so it works on a
direct connection with no Lich and no browser.

It has two faces, and both read the same bundled codex:

- **`.bestiary`** — the command. Prints a styled, clickable page to the
  `bestiary` stream.
- **Bestiary Browser** (the `bestiaryview` widget) — a GUI window that browses
  that same codex with a search box and filters.

> **The codex is shipped data, not your session.** It does not watch what you
> kill and it does not learn. Every VellumFE install carries the identical file,
> baked from lich-5's creature templates (the stats) joined against Saga's spawn
> tables (the room ranges). Neither face is a live game feed.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

You have two surfaces, and it's worth having both.

1. For the browser: click **Windows** in the top toolbar, expand **Other**, and tick
   **Bestiary Browser**. (Or type `.addwindow bestiaryview`.)
2. For the command's output: add a text window subscribed to the `bestiary`
   stream — expand **Text Windows** and tick **Bestiary**, or type `.addwindow bestiary`.

<figure class="shot" data-shot="features/bestiary-gui-browser-and-stream">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Bestiary Browser</b> window beside a <b>Bestiary</b> text window, the browser showing its filtered results table.</figcaption>
</figure>

→ **Expected result:** the Bestiary Browser opens on its browse page with the
full creature list; `.bestiary here` now prints into the Bestiary text window
instead of your main window.

{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**The Bestiary Browser is a GUI-only widget.** You can place a `bestiaryview`
window in the TUI, but it will not browse anything — it renders a one-line note
in its place:

```
Bestiary browser is a GUI widget. In the TUI, use .bestiary (add a
'bestiary' text window for a dedicated page view).
```

The command is the terminal path, and it is not a lesser one — the tables and
entries are fully clickable there too.

1. Type `.addwindow bestiary` — or `.addwindow` for the picker, then choose
   **Bestiary**.
2. Type `.bestiary undead` to confirm it's wired up.

→ **Expected result:** a **Bestiary** window appears, and the undead table
renders inside it rather than scrolling past in main.

{{#endtab}}
{{#tab name="Mobile"}}

The phone has **no bestiary surface** — no browser panel, and no `bestiary`
chip in its stream list. Nothing on the phone renders the codex as its own page.

The command itself still runs: type `.bestiary arctic manticore` into the
command input and it dispatches like any other dot-command. Because no mobile
window subscribes to the `bestiary` stream, the output takes the fallback route
and lands in your **main** text view, in reading order with everything else.

→ **Expected result:** the creature's boxed entry prints into your main text
view on the phone, readable and scrollable, but sharing that view with game
output.

{{#endtab}}
{{#endtabs}}

## Using `.bestiary`

Run it bare for the menu. `.bestiary` and `.bestiary help` print the same eight
lines:

```
Bestiary - creature lookup (bundled codex)
  .bestiary <name>        look up one creature
  .bestiary here          creatures spawning around this room
  .bestiary level <n|a-b> list by level or range
  .bestiary area <name>   list by area/map
  .bestiary family <name> list by family
  .bestiary undead        list undead
  .bestiary search <text> search names
```

| Command | What you get |
|---------|--------------|
| `.bestiary <name>` | One creature's full entry |
| `.bestiary here` | Everything whose spawn ranges cover your current room's uid |
| `.bestiary level 29` | Every level-29 creature |
| `.bestiary level 25-33` | Everything in that level band |
| `.bestiary area Frozen Battlefield` | Everything spawning in that area or map |
| `.bestiary family Chimeric` | Everything in that family |
| `.bestiary undead` | Every undead creature |
| `.bestiary search manti` | Name substring search |

### Naming a creature

`.bestiary <name>` tries three things in order, so you rarely need the full
name. It takes an exact name match first, then a **unique noun** — `.bestiary
manticore` finds the arctic manticore when nothing else ends in that word —
then a **unique substring**.

When none of those lands on exactly one creature, it does not give up: it falls
back to a search table of everything that matched, headed `Matches for '<your
query>'`. You get a list to click rather than an error. Only when that table is
empty do you see:

```
[bestiary] no creature matches 'gorbat'.
```

### Everything is clickable

The tables and entries carry more than text. Each cell holds the dot-command it
represents, and clicking re-enters the same dispatch as if you'd typed it. In a
results table, the **level** column links to `.bestiary level <n>`, the **name**
links to that creature's entry, and the **family** column links to
`.bestiary family <x>`. Inside an entry, the level, the family and each location
name are links too.

Two links leave the codex entirely:

- **`[go2]`**, on each location line, runs `.go2 u<uid>` against the first room
  in that spawn range — native pathing straight to the hunting ground.
- **Wiki:**, at the foot of the entry, opens that creature's gswiki page in your
  browser.

## What's in an entry

An entry is a fixed-width box, 65 columns wide, in this order. Sections with no
data are omitted rather than printed empty, so a thin creature gets a short box.

| Section | Contents |
|---------|----------|
| Header | Name in monsterbold caps, with its level as a link |
| Description | The creature's look text, wrapped |
| Identity line | Family (link), Type, an **Undead** flag when it applies, HP |
| **Locations** | Each resolved map name (link), its room count, and a `[go2]` link; then any authored area names not already covered by a spawn |
| **Offense** | Physical attacks with AS, bolts with AS, wardings with CS, offensive spells, maneuvers, and special abilities with their notes |
| **Defense** | ASG, Melee DS, Ranged DS, Bolt DS, UDF; then TD per magic circle; then immunities, defensive spells, abilities and special defenses |
| **Treasure** | Which of coins, gems, boxes and magic items it drops, its skin, and any other note |
| **Notes** | Free-text notes from the template |
| Wiki | The gswiki URL, as a browser link |

Stats print as an exact number or as a range where the source knows one — a
Ranged DS reads `108-114` when the template recorded it that way.

## Common setups

### Recipe: size up the room you just walked into

You've arrived somewhere unfamiliar and want to know what hunts here before
something finds you.

```
.bestiary here
```

**You'll see** a level-separated table of every creature whose spawn ranges
cover this room's uid, sorted by level, under a `Spawns around this room`
heading. Click the name of the one that worries you, and the same window turns
into its full entry: its AS on each attack, its DS against melee, ranged and
bolt, and whether it skins.

If the client doesn't know where you are yet, you get this instead of a table:

```
[bestiary] current room uid unknown.
```

Walk one room and try again — the uid arrives with the room.

### Recipe: find something to hunt at your level

You're 27 and looking for the next step up.

```
.bestiary level 28-32
```

**You'll see** one table of every creature in that band, split by a separator
row each time the level changes, so the level-28s group visibly apart from the
level-29s. Click a family name in the third column to pivot to everything in
that family. Click a promising name, then click **[go2]** on its location line
and you are walking there.

### Recipe: browse by filter in the GUI

Open the **Bestiary Browser** and use its toolbar rather than typing commands.
Type into the search box, put numbers in the **Lvl** min and max fields, pick a
**Family** from the dropdown, and toggle **Undead** on.

**You'll see** the count line under the toolbar update as you type, and the
four-column table below it (Lvl, Name, Family, HP) narrow in step. Click a name
and the window switches to its entry page: the name in gold caps, a row of
framed stat chips (HP, ASG, Melee DS, Ranged DS, Bolt DS, UDF, then one chip per
attack showing its AS or CS), a TD line, the description, then Locations,
Offense, Defense, Treasure and Notes. Click **◂ Back** to return to the browse
page with your filters still set.

## How the two faces differ

Both read the same codex, so the facts never disagree. How you move through
them does.

| | `.bestiary` (stream) | Bestiary Browser (GUI widget) |
|---|---|---|
| Where it runs | GUI, TUI, and mobile | GUI only |
| Navigation | Click a link, or type the next command | Click a row; **◂ Back** returns |
| Filtering | One command at a time (`level`, `family`, `undead`) | Search, level range, family and undead **combine live** |
| Room-aware | `.bestiary here` matches your current room uid | No equivalent — the browser has no "here" |
| Extra columns | Level, Name, Family | Level, Name, Family, **HP** |
| Travel | `[go2]` link on each location | **go2** button on each location |
| TD display | One line per circle group | Collapses to `all circles <n>` when every circle matches |

The browser's filters stack, which the command cannot do — there is no
`.bestiary family Chimeric undead`. The command has `here`, which the browser
does not.

> The Bestiary Browser has no page of its own in this manual — it is documented
> here, because it is the same feature.

## Tips & gotchas

> ⚠️ **A subscribed Bestiary window clears on every step; main does not.**
> Navigation is app-style — each lookup replaces the page rather than appending
> to it. If a window is subscribed to the `bestiary` stream, it is wiped before
> each new page is written, so you cannot scroll back to the previous creature.
> **The main-window fallback behaves the opposite way**: with no bestiary window
> in your layout, output goes to main and keeps its scrollback, so every lookup
> stacks up in your history. Choose deliberately — a dedicated window gives you
> a clean page, main gives you a trail.

**Word wrap is off in the bestiary window on purpose.** The entry boxes and
tables are drawn at a fixed 65 columns. The preset window is 69 columns wide to
fit them with the border. Make it narrower and it scrolls sideways rather than
reflowing, which keeps the boxes square.

**`.bestiary here` needs a known room uid.** It keys off the same room identity
the map uses. Straight after connecting, before the first room arrives, it
reports `current room uid unknown.`

**An empty filter prints a line, not an error.** `.bestiary family Wyrm` with no
matches gives you `No matching creatures.` Check your spelling against a family
you saw in a real entry, or click one out of a table.

**If the codex itself won't load**, every path says so plainly — the command
prints `[bestiary] bundled codex failed to load.` and the GUI widget shows
`Bundled bestiary failed to load.` in place of its toolbar. That indicates a
damaged install rather than anything you configured.

**The Bestiary Browser is GS4-only.** It does not appear in the Windows catalog
on other games. The `.bestiary` command and the `bestiary` text window are not
gated this way, though the codex only describes GemStone IV creatures.

## See also

- [Text Windows](../widgets/text-windows.md) — how stream subscription works,
  and how to point a window at the `bestiary` stream
- [Widgets overview](../widgets/README.md) — the full widget catalog
- [Inventory Tools](./inventory-tools.md) — the other place clickable client
  output drives commands for you

<details>
<summary>Config reference (TOML)</summary>

Both windows are placed from the catalog; these are the preset shapes they land
with, and every field is editable afterwards through the window editor.

**The `bestiary` text window** (widget type `text`):

| Field | Type | Default | What it does |
|-------|------|---------|--------------|
| `name` | string | `bestiary` | Window identity |
| `title` | string | `Bestiary` | Title bar text |
| `rows` | integer | `24` | Height |
| `cols` | integer | `69` | Width — fits the 65-column boxes plus border |
| `streams` | list | `["bestiary"]` | Subscribes to the bestiary stream |
| `buffer_size` | integer | `10000` | Lines retained |
| `wordwrap` | bool | `false` | Off, so boxes and tables stay aligned |
| `show_timestamps` | bool | `false` | Timestamps hidden |
| `show_border` | bool | `true` | Draw the window border |

**The `bestiaryview` window** (widget type `bestiaryview`):

| Field | Type | Default | What it does |
|-------|------|---------|--------------|
| `name` | string | `bestiaryview` | Window identity |
| `title` | string | `Bestiary Browser` | Title bar text |
| `rows` | integer | `24` | Height |
| `cols` | integer | `60` | Width |
| `min_rows` | integer | `8` | Smallest usable height |
| `min_cols` | integer | `30` | Smallest usable width |

The widget carries no data fields of its own — it reads the bundled codex
directly, and its search text, level range, family choice and undead toggle are
per-window view state that is not written to your layout.

**The codex file itself is not configurable.** `defaults/bestiary.json` is
compiled into the binary and loaded once on first use. It is regenerated only by
a maintainer subcommand, `extract-bestiary`, which joins the lich-5 creature
templates against Saga's spawn tables and bakes the result. Players never run it,
and there is no user-supplied override path.

</details>
