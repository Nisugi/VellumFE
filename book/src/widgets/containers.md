# Containers

> A live window per bag — your pack, your bandolier, your gem pouch — each one showing what's
> actually inside it right now, and each one a target you can drop loot onto.

## What it's for

`LOOK IN MY PACK` tells you what's in the pack once, then scrolls away, and the next time you
want to know you type it again. Meanwhile the answer changes every time you stow a gem.

A container window pins one bag open. It lists the contents, refreshes when the game re-sends
them, and its items are clickable for the verb menu. Better, the whole window is a drop target:
drag a gem from the ground onto your pouch and it goes in, no `put` and no noun.

The trade-off is deliberate — **these windows are session-only.** They come and go with your
login and never touch your saved layout, which is what makes it reasonable to have one per bag
without cluttering the layout you actually keep.

## Set it up

Container windows are the one widget you don't place from the catalog by type. You place them
**per bag**, and a bag only becomes placeable once the client has seen inside it.

**Look in the bag once first** (`look in my pack`). That registers it — it does not open a
window. The bag then appears as its own row under **Containers** in the Windows list, and ticking
that row is what opens the window.

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Look in the container once: `look in my pack`.
2. Click **Windows** in the top toolbar and open the **Containers** category. Your pack is a row
   there, labelled with its in-game name and marked **(session)**.
3. Tick the row. Untick it to close the window again.

<figure class="shot" data-shot="widgets/containers-gui-windows-catalog-rows">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Windows</b> catalog with the <b>Containers</b> category expanded, showing two bags as rows marked <b>(session)</b>, one ticked.</figcaption>
</figure>

→ **Expected result:** a window titled with the bag's name opens in the middle of the screen at
40 by 15, listing its contents. An untracked bag reads **No contents cached for "…"**; a tracked
but empty one reads **Empty.**
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

1. Look in the container once: `look in my pack`.
2. Type `.menu`, choose **Windows ▸**, then **Show/Hide windows ▸**. Your pack is a row under
   the `── Containers ──` header, shown as `[ ] a leather pack (session)`.
3. Press Enter on the row to tick it. Enter again unticks and closes it.

`.editwindow` offers this widget **no fields of its own** — a container window has only the
standard geometry and border settings.

> **`.addwindow <name> container <x> <y> <w> <h>` builds a container window with no bag bound
> to it**, so it renders nothing useful. The Windows list is the gesture that binds a window to
> an actual bag. (And `.addwindow` takes 1 argument or 6 or more — never 2 through 5.)

→ **Expected result:** a bordered window centered at 40 by 15, titled with the bag's name and a
live count — `a leather pack [06]`, or `a leather pack (empty)` when there's nothing in it.
{{#endtab}}
{{#tab name="Mobile"}}

There is no container surface on the phone. The **status drawer**'s sections are **Targets**,
**Players**, and **Room**; the chip bar carries streams. Neither exposes the contents of a bag.

What the phone does give you is the **Inventory** stream chip, which lists what you're carrying
including the bags themselves, and every item name in the text pane stays tappable — so tapping
a bag raises its verb sheet, and picking **look in** there prints the contents into the pane you
are already reading. Configure real container windows on desktop.

→ **Expected result:** tapping a container in the text pane raises its verb sheet, and looking in
it prints the contents into the story pane — no standing per-bag panel.
{{#endtab}}
{{#endtabs}}

## Common setups

### A bagging strip for hunting

Look in your gem pouch and your pack. Tick both in the Windows list, then drag them into a column
beside your [Items](./items.md) window. Now hold **Ctrl** and drag a gem row from the items
window onto the pouch window's body.

**You'll see:** the gem lands in the pouch and shows up inside the pouch window, while the items
window drops it on the next room update — the same result as `put gem in my pouch`, with nothing
typed. Dropping onto a *row* inside a container puts the item into that nested container instead.

### Clearing the screen after a looting run

You've ticked four bags and the screen is busy. Type `.hidecontainers`.

**You'll see:** every one of them closes at once and the client reports how many it closed. Your
opt-ins are cleared with them, so those bags stay closed until you tick them again — they will
not reappear the next time you look inside one.

### Keeping one bag in your saved layout

If you always want your pack window in the same place, declare it in `layout.toml` with
`widget_type = "container"` and `container_title = "leather pack"`. That window is persistent —
saved, restored, and placed wherever you put it.

**You'll see:** the pack window comes back at your chosen position on every login, without
ticking anything.

> ⚠️ **Give it the same name the session window would get** — the bag's title, lowercased with
> spaces as underscores (`leather_pack`). Name it anything else and ticking the bag in the Windows
> list opens a *second* window on the same bag. Nothing merges them for you.

## Tips & gotchas

> ⚠️ **Container windows are session-only and are never saved.** Close the client and every one
> of them is gone — they are not in your layout, and `.savelayout` does not capture them. This is
> by design; the persistent alternative is the layout-declared window in the recipe above.

> ⚠️ **Looking in a bag does not open its window.** It only makes the bag appear as a row in the
> Windows list. Ticking the row is the step that opens anything. Once ticked, though, the opt-in
> sticks for the session — look in that bag again after closing its window by *any* route other
> than unticking, and it re-opens on its own.

> ⚠️ **`.hidecontainers <name>` matches on a substring and can close several windows at once.**
> `.hidecontainers pack` closes every open container whose name contains "pack". Matching is
> case-insensitive and spaces are treated as underscores, so `.hidecontainers my pack` finds
> `my_pack` correctly. Bare `.hidecontainers` closes them all.

> ⚠️ **Bare `.hidecontainers` also closes open game dialog panels**, even though it reports the
> count as "container window(s)". If you have a combat or befriend panel open, it goes too.

> ⚠️ **Only the TUI remembers where you dragged a container window.** Move one in the terminal
> and its position is written to `widget_state.toml`, so it reopens there next session. **The GUI
> does not persist session container geometry at all** — every container window opens centered
> at 40 by 15 in the GUI, every time.

**Contents come from the client's item registry, not from a text buffer.** The window shows what
the client last learned about that bag, which is why an unlooked-in bag reads **No contents cached
for "…"** rather than being empty. Look in it once and it fills.

**Bag names are matched loosely.** A window bound to `pack` finds "a battered leather pack" —
matching tries the exact title, then a substring, then the title with articles stripped. Short
distinctive words work well; over-specific ones can miss.

**Only the TUI shows a count in the title.** `a leather pack [06]`, or `(empty)` when it's bare.
The GUI keeps whatever title the window has and prints **Empty.** in the body instead.

**Word wrap is the one appearance control that matters here.** It's under **Appearance ▸ Text**
in the GUI. Long item names clip rather than wrap when it's off, which is usually what you want in
a narrow bag window.

**Right-clicking an item row opens the *window* menu, not a menu for that item.** Left-click the
item name to ask the server for its verbs.

**`.foreach` needs the bag seen open, exactly like this window does.** If a batch command reports
it can't find your container, look in it once and re-run. See
[Inventory Tools](../features/inventory-tools.md).

## See also

- [Inventory](./inventory.md) — everything you're carrying, including the bags themselves
- [Items](./items.md) — what's on the ground, and the usual drag source for stowing
- [Reserve](./reserve.md) — the GemStone IV reserved-items snapshot
- [Inventory Tools (.foreach, .sorter)](../features/inventory-tools.md) — batch commands across
  bags
- [Hands](./hands.md) — the other drop target pair

<details>
<summary>Config reference (TOML)</summary>

### Per-window (`layout.toml`)

`widget_type = "container"`. Declaring one this way makes it **persistent**, unlike the
session-only windows the Windows list creates.

| Field | Type | Default | What it does |
|---|---|---|---|
| `container_title` | string | `""` (empty) | The bag this window shows. Matched case-insensitively: exact title, then substring, then articles stripped. Empty = the window renders no contents |

Everything else is a standard window field. Session-created container windows are always 40 by
15, centered.

```toml
[[windows]]
name = "leather_pack"
widget_type = "container"
container_title = "leather pack"
title = "Pack"
row = 20
col = 92
rows = 15
cols = 40
show_border = true
```

### Position memory (`widget_state.toml`, TUI only)

Dragging a session container window in the TUI records its geometry under `[containers]`, keyed
by the window name. Written by the terminal frontend only.

```toml
[containers.leather_pack]
x = 40
y = 8
width = 44
height = 18
```

There is no `[containers]` section in `config.toml` — this widget has no global settings.

</details>
