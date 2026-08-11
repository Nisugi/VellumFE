# Dashboard

> Every status you care about in one compact grid — instead of a row of separate one-cell
> windows you have to place, size, and align by hand.

## What it's for

Once you're tracking more than two or three statuses, individual [indicator](./indicators.md)
windows start costing more than they're worth. Each is its own window to position, and eight of
them means eight things to nudge back into line every time you rework your layout.

A dashboard is one window holding all of them. You list the statuses you want, pick horizontal,
vertical, or a grid, and it lays them out and keeps its own height. Adding a tenth status is a
line in an editor, not a new window.

It also does something no indicator can: **it grows a cell on its own.** Point a highlight rule
at an id the dashboard has never heard of, and the first time that rule fires the cell appears.
That makes it the right home for statuses you're still experimenting with.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar and tick **dashboard** in the catalog. Set the row's
   **zone** to place it. (Typed equivalent: `.addwindow dashboard dashboard 0 0 10 3`.)
2. Right-click the window and choose **Edit dashboard…** in its widget section. A new dashboard
   ships empty, so this is where it gets its contents.
3. Set **Layout** — **Horizontal**, **Vertical**, **Flow (wrap)**, **Grid 2x2**, or
   **Grid 3x3** — plus **Spacing** and the **Hide inactive** checkbox.
4. Under **Statuses**, type an id or pick one from the **Known…** drop-down, then click **Add**.
   Use **⬆** and **⬇** to order them and **✕** to remove one.
5. Click **Save**.

<figure class="shot" data-shot="widgets/dashboard-editor">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Dashboard</b> editor: <b>Layout</b>, <b>Spacing</b>, and <b>Hide inactive</b> at the top, the ordered <b>Statuses</b> list with its <b>stack</b> fields below, and the <b>Save</b> button showing "unsaved changes".</figcaption>
</figure>

→ **Expected result:** one window showing your chosen statuses in the layout you picked, each
cell lighting as its status turns on.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow status dashboard 0 0 15 3` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow status`
> alone prints usage and adds nothing.

The terminal renders all four layouts — horizontal, vertical, grid, and flow — honors
**Spacing** and **Hide inactive**, and aligns cells with the window's content alignment. Type
`.indicators` to change the glyphs and colors the cells use.

> ⚠️ **The terminal has no dashboard editor.** **Edit dashboard…** is a GUI panel, so choosing
> which statuses a dashboard lists and in what layout is desktop authoring. A dashboard built in
> the GUI renders fully in your terminal, and a dashboard still auto-adds a new cell here when
> an unknown `set_status` id fires.

→ **Expected result:** a compact row or grid of glyphs, each in its active color while its
status is on.
{{#endtab}}
{{#tab name="Mobile"}}

There is no dashboard on the phone. Its fixed chrome carries a shorter, built-in equivalent:
small text badges in the **status row**, the strip above the command input that also holds your
hands and vitals.

Nine statuses get a badge — `DEAD`, `STUN`, `BLEED`, `WEB`, `HIDDEN`, `INVIS`, `KNEEL`, `SIT`,
and `PRONE`. **Poisoned, diseased, joined, and standing have none, and custom `set_status` ids
never reach the phone's badges**, so a dashboard's most useful trick doesn't carry over. Build
your dashboard on desktop; the phone shows you the common statuses regardless.

→ **Expected result:** badges appear and disappear in the status row as statuses turn on and
off, with nothing to configure.
{{#endtab}}
{{#endtabs}}

## Common setups

### One compact affliction strip

Add a dashboard, open **Edit dashboard…**, set **Layout** to **Horizontal** and **Spacing** to
`1`, and add `BLEEDING`, `POISONED`, `DISEASED`, and `STUNNED`. Leave **Hide inactive** ticked.
Click **Save**, then right-click ▸ **Appearance** ▸ **Frame** and turn the border off.

**You'll see:** empty space while you're healthy, and icons appearing one at a time as things
go wrong — with the strip staying one row tall the whole time.

### A posture and presence block beside your compass

Add a second dashboard, set **Layout** to **Grid 2x2**, and add `STANDING`, `KNEELING`,
`SITTING`, and `PRONE`. Untick **Hide inactive** so all four cells always show and only the
current one is lit. Click **Save**.

**You'll see:** a two-by-two block where exactly one cell is bright — your posture readable at
a glance, in a window that stays exactly two rows tall because the grid tells it so.

### One square that shows three afflictions at once

In **Edit dashboard…**, give `BLEEDING`, `POISONED`, and `DISEASED` the same **stack** name, for
example `afflict`. Click **Save**.

The three collapse into a single cell and their active icons paint over each other. It works
because the artwork is authored to sit in different parts of the square — the way Wrayth did it
— so blood, poison, and disease occupy the corners rather than covering one another.

**You'll see:** one square instead of three, lighting up in layers as afflictions land, and a
vertical dashboard that got shorter because three cells became one.

## Tips & gotchas

> ⚠️ **A dashboard auto-adds an unknown status; an indicator does not.** The first time a
> `set_status` rule fires an id your dashboard has never listed, a cell appears for it. An
> [indicator](./indicators.md) window can only flip an id it already has, because the window's
> name *is* the id. This is the practical reason to reach for a dashboard when you're inventing
> your own statuses.
>
> One exception: the dashboard won't auto-add an id that a combined indicator template already
> claims in one of its conditions. That id is spoken for, so it stays where it was authored.

> ⚠️ **Two different editors, and it's easy to open the wrong one.** **Edit dashboard…** (in
> the dashboard window's own right-click menu) chooses *which* statuses it lists, the layout,
> and the spacing. **Edit indicators…** — the **Editors** hub ▸ **Indicators**, or
> `.indicators` — chooses what each status *looks like*, for every dashboard and indicator at
> once. The dashboard editor has an **Edit indicators…** button that jumps straight there.

**The dashboard editor is Save-buffered.** Unlike the right-click menu around it, which applies
live, this panel shows "unsaved changes" and does nothing until you click **Save**. Closing it
first discards your edits.

**Your dashboard's height is decided by its rows, not by dragging.** A **Horizontal** dashboard
is one row tall. A **Vertical** one is as tall as its cell count. A **Grid** is as tall as
cell count divided by columns, rounded up. Those all cap the window's height so the frame hugs
the grid instead of leaving a slab of empty space underneath — which is why a dashboard often
won't stretch as far as you drag it.

**Flow is the exception, and that's what it's for.** **Flow (wrap)** fills the width and wraps,
so its row count depends on how wide the window is and can't be known ahead of time. It's the
one layout left uncapped: it grows as its contents need. Reach for Flow when you have many
statuses and want them to reflow as you resize; reach for Grid when you want a shape that holds
still.

**Stacked rows count as one cell for height, too.** Three statuses sharing a **stack** name
occupy one square, so a vertical dashboard of six statuses stacked in pairs is three rows tall,
not six.

**Hide inactive is on by default, and usually should stay on.** A shipped dashboard starts with
it ticked so the grid isn't a wall of dim icons. Untick it for the posture case above, where
seeing all the options makes the lit one meaningful.

> ⚠️ **In the terminal, a grid drops anything past its cell count.** `grid:2x2` renders the
> first four statuses and silently ignores a fifth. Count your rows against your grid, or use
> **Flow (wrap)**, which never truncates. The GUI doesn't have this limit.

**Anything the layout string doesn't recognize becomes horizontal.** A typo like `verticle`
won't error — it lays out horizontally. Use the editor's drop-down and you can't hit this.

**Ids are matched without regard to case.** `BLEEDING` and `bleeding` are one status. Icons and
colors come from the shared templates, so a cell and an indicator window with the same id always
look alike.

## See also

- [Indicators](./indicators.md) — the single-status form, and why its name is its id
- [Make your health bar shout when you're hurt](../how-to/vitals-flash.md) — `set_status`
  driving these cells from game text
- [Highlight Patterns](../customization/highlights.md) — `set_status`, `status_duration`, and
  `clear_status`
- [Injury Display](./injury-doll.md) — wounds by body part, in more detail than a status cell
- [Mini Vitals](./minivitals.md) — the numbers a dashboard's statuses sit beside
- [Skins (GUI Graphics)](../customization/skins.md) — where cell art comes from, including
  stack-friendly sets
- [Build a hunting layout](../how-to/hunting-layout.md) — where a status dashboard sits in a
  combat screen

<details>
<summary>Config reference (TOML)</summary>

`widget_type = "dashboard"`. Set these through **Edit dashboard…** in the GUI.

| Field | Type | Default | What it does |
|---|---|---|---|
| `dashboard_layout` | string | `"horizontal"` | `"horizontal"`, `"vertical"`, `"flow"`, or `"grid:RxC"`. Unrecognized values fall back to horizontal. |
| `dashboard_spacing` | integer | `1` | Gap between cells. The editor allows 0–8. |
| `dashboard_hide_inactive` | bool | `true` | Hide cells whose status is off |
| `dashboard_indicators` | array of tables | empty | The statuses to show, in display order |

Each `[[windows.dashboard_indicators]]` entry:

| Field | Type | Default | What it does |
|---|---|---|---|
| `id` | string | required | Status id, matched case-insensitively |
| `icon` | string | from template | Per-entry glyph. Kept for older terminal layouts; the shared templates are the normal source. |
| `colors` | array of strings | from template | Colors indexed by value — `[off, on]`, or more for multi-level statuses |
| `stack` | string | empty | Layer group. Entries sharing a name render into one cell with their icons painted over each other. Empty means its own cell. |

**Height.** Rows come from the layout and the cell count, where stacked entries count once:
vertical is one row per cell, `grid:RxC` is cells divided by `C` rounded up, horizontal is one
row, and flow is uncapped because its wrapping depends on the window's width.

```toml
[[windows]]
name = "status"
widget_type = "dashboard"
title = "Status"
row = 0
col = 0
rows = 1
cols = 15
dashboard_layout = "horizontal"
dashboard_spacing = 1
dashboard_hide_inactive = true
show_border = false

  [[windows.dashboard_indicators]]
  id = "STUNNED"

  [[windows.dashboard_indicators]]
  id = "BLEEDING"
  stack = "afflict"

  [[windows.dashboard_indicators]]
  id = "POISONED"
  stack = "afflict"
```

</details>
