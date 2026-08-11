# Betrayer

> Your blood pool and the items feeding it, on screen instead of behind a command — and the
> window grows and shrinks itself as that list changes.

## What it's for

The game publishes a Betrayer panel: a blood-point total out of 100, and a list of the items
contributing to the pool, some of them flagged as active. It arrives as its own feed, which means
it can sit permanently in your layout rather than being something you ask for.

This window draws that panel. A dark red bar carries the point total, the contributing items are
listed beneath it, and the whole thing refreshes when the game sends an update. It is a small,
single-purpose window for a small, single-purpose reading.

> **What the numbers mean in play is a GemStone IV mechanic, not a client feature.** VellumFE
> reads what the game sends and draws it — the point total, the item names, and which items carry
> the game's active marker. It does not interpret them.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Dialogs**, and tick **Betrayer**. It arrives
   4 rows by 30 columns.
   (Typed equivalent: `.addwindow betrayer betrayer 0 0 30 4`.)
2. There is no widget section in this window's right-click menu — the GUI offers no Betrayer
   settings at all. Everything else on the menu applies as usual: **Appearance ▸ Frame** for the
   border, **Arrange** for placement, **Window ▸ Lock in place** to pin it.

<figure class="shot" data-shot="widgets/betrayer-gui-blood-pool-window">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A Betrayer window showing the dark red blood-point bar above its list of contributing items, with the <b>Dialogs</b> group of the <b>Windows</b> catalog open beside it.</figcaption>
</figure>

→ **Expected result:** a window headed **Betrayer** with a red bar reading **Blood Points: 100**
and the contributing items listed under it.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow betrayer betrayer 0 0 30 4` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow betrayer`
> alone prints usage and adds nothing.

`.editwindow betrayer` opens the form. The widget fields are **Show Items** and **Bar Color**.
`Tab` moves between fields, `Ctrl+S` saves, `Esc` cancels. Untick **Show Items** and the window
shrinks to the bar alone.

> ⚠️ **These two fields are terminal-only authoring.** The desktop GUI has no Betrayer section
> in its window menu, so **Show Items** and **Bar Color** can only be set here. The GUI still
> honors **Show Items** through the layout file, but **draws the bar in its own fixed red
> regardless of what you set** — your **Bar Color** applies to the terminal alone.

→ **Expected result:** a bordered window titled `Betrayer` with a dark red fill carrying the
point total, and the item list beneath it.
{{#endtab}}
{{#tab name="Mobile"}}

There is no Betrayer surface on the phone — not a drawer section, not a stream chip, not a wheel
slice. The blood pool is a desktop window only.

The phone's **status drawer** (swipe in from the right) is where its character readings live —
**Targets**, **Players**, **Room**, your injuries, hands, effects, and the character-sheet
sections for experience, encumbrance, bounty and society. Betrayer is not among them. To check
the pool from your phone, ask the game in the command input and read the reply in the story pane
like any other output.

→ **Expected result:** the **status drawer** opens with its usual sections and no Betrayer among
them; the reading is available only by asking the game directly.
{{#endtab}}
{{#endtabs}}

## Common setups

### A blood pool that stays out of the way until it matters

The item list is what makes this window tall, and most of the time you want the number, not the
list.

1. Add **Betrayer** from the catalog's **Dialogs** group and place it wherever you keep small
   readouts.
2. In the terminal, `.editwindow betrayer` and untick **Show Items**. The window drops to its
   3-row floor: border, bar, border.
3. Right-click it in the GUI ▸ **Appearance ▸ Frame** and turn the border off, so what is left is
   a bare red strip.

**You'll see:** a one-line red bar reading your point total, taking almost no space — and if you
tick **Show Items** back on, the window grows on its own to fit the list again.

## Tips & gotchas

> ⚠️ **This window resizes itself, and it is the only widget that does.** VellumFE recalculates
> its height from the item count on every update — one row for the bar, one per item, plus
> borders — clamped between 3 and 12 rows, and saves the new height into your layout. **Dragging
> it to a height you like will not stick**, because the next update recomputes it. Control the
> height with **Show Items** instead: off pins it at 3 rows.

> ⚠️ **The active marker shows in the terminal and not in the desktop GUI.** The game prefixes an
> active item with `!`. The terminal draws that `!` in its own alert color; the GUI prints the
> line plainly, `!` and all. The mark is visible in both — it is the coloring that differs.

**An empty window means the game hasn't sent the panel yet.** The terminal says so out loud with
`(No blood pool data)`. It fills in when the panel next arrives, and it empties again when the
game clears it.

**The bar is the point total out of 100.** It fills proportionally and carries the game's own
text — `Blood Points: 100` — centered on the fill. When the game sends no text, the window writes
that line itself from the number.

**The desktop GUI can put this reading in a different window.** A [Mini Vitals](./minivitals.md)
window in the GUI offers **Blood** as one of its selectable bars, so the point total can ride
alongside health and mana in one strip. That gets you the number without the item list — and
without this window at all.

**This widget is GemStone IV only.** Both pickers hide it from DragonRealms characters — it is
absent from the GUI **Windows** catalog and from the bare-`.addwindow` picker. The full
six-argument form does not check, so it will build the window on a DR character. You get an empty
box: DragonRealms never sends this feed. **If the game is not set at all, VellumFE assumes
GemStone IV and the row appears**, which is why it shows up when you connect through Lich without
naming a game.

## See also

- [Mini Vitals](./minivitals.md) — the **Blood** bar, for the total without the item list
- [Progress Bars](./progress-bars.md) — how feed-bound bars work generally
- [Active Effects](./active-effects.md) — the other dialog-fed windows, and how their feeds arrive
- [Inventory](./inventory.md) — the full list of what you're carrying
- [Creating Layouts](../customization/layouts.md) — placing and saving these windows

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog and `.editwindow`. Hand-editing is for troubleshooting, not the normal
path.

```toml
[[windows]]
name = "betrayer"
widget_type = "betrayer"
title = "Betrayer"
row = 0
col = 0
rows = 4
cols = 30
show_border = true
show_items = true
bar_color = "#8b0000"
```

`widget_type = "betrayer"`. The type string is exactly `betrayer`; an unrecognized type does not
error, it quietly creates a **text** window instead.

### Widget fields (`BetrayerWidgetData`)

| Field | Type | Default | What it does |
|---|---|---|---|
| `show_items` | boolean | `true` | List the contributing items under the bar. Also decides the window's height. |
| `bar_color` | string | `#8b0000` | Fill color of the point bar. **Terminal only** — the GUI draws `#cd4d4d` regardless. |

`rows` is not yours to set for long: it is recomputed from the item count on every update and
written back, clamped to the `min_rows` / `max_rows` below.

### Feed

The window is a view onto the game's `BetrayerPanel` dialog. The point total comes from its
`lblBPs` label — read as `Blood Points: N`, treated as a percentage of 100 — and the item list
from its `lblitemN` labels, kept in the order the game sends them with any leading `!` intact.
A clear from the game empties the window.

The point total is also selectable as the **Blood** bar inside a desktop-GUI
[Mini Vitals](./minivitals.md) window.

### Catalog preset

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| `betrayer` | Dialogs | Betrayer | 4 rows x 30 cols (floor 3 rows / 20 cols, ceiling 12 rows) | GemStone IV only |

The game gate hides the row from DragonRealms characters in both the GUI catalog and the
`.addwindow` picker. **An unset game counts as GemStone IV**, so the row appears by default. The
six-argument `.addwindow` form is not gated and will build the window regardless.

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Show items | not offered | `.editwindow betrayer` ▸ **Show Items** |
| Bar color | not offered (fixed) | `.editwindow betrayer` ▸ **Bar Color** |
| Border, placement, lock | Right-click ▸ **Appearance** / **Arrange** / **Window** | `.editwindow betrayer` |

There is no widget section in the GUI right-click menu for this type. The active-item color the
terminal uses for the `!` marker is a global setting (`[ui] betrayer_active_color`, default
`#ff4040`), not a per-window one. Standard window keys — `row`, `col`, `rows`, `cols`,
`show_border`, `border_style`, `title`, `locked` — apply as they do to any window.

</details>
