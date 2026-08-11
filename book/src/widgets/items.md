# Items

> What's actually on the ground, pulled out of the room prose and held still — so the box that
> just dropped doesn't scroll away while you're still fighting.

## What it's for

Loot arrives at the worst moment. The room description names it once, in the middle of a
sentence that also names the creatures, and then combat messages push the whole thing off the
top of your screen.

The items window keeps the room's non-creature objects in a standing list: dropped loot,
boxes, furniture, scenery. Click one and the game's verb menu opens on it, so you can look, get,
or open without typing a noun you'd have to scroll back to read. In the GUI you can also drag an
item onto a container or your hand.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Entities**, and tick **Items**. Set the row's
   **zone** — the **Right Bar** keeps it next to your other room lists.
   (Typed equivalent: `.addwindow items items 92 25 28 10`.)
2. There are no per-window options for this widget; its border, colors, and title are the
   standard window settings under **Appearance**.

<figure class="shot" data-shot="widgets/items-gui-ground-list">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>An Items window listing four objects on the ground, with the cursor over one row showing the pointing-hand cursor.</figcaption>
</figure>

→ **Expected result:** a list of everything on the ground. Clicking one opens that object's verb
menu; an empty room reads **No objects here.**
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow items items 92 25 28 10` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow items` alone
> prints usage and adds nothing.

`.editwindow items` opens the form; the only widget-specific field is the entity id. `Tab`
moves between fields, `Ctrl+S` saves, `Esc` cancels.

→ **Expected result:** a bordered list titled `Items [04]`, the count tracking what's on the
floor. Clicking a row opens that object's verb menu.
{{#endtab}}
{{#tab name="Mobile"}}

There is no Items panel on the phone. The **status drawer** carries **Targets**, **Players**, and
**Room** — ground objects are not one of its sections.

You can still reach them two ways. The drawer's **Room** section renders the description with its
scenery and objects still clickable, so tapping an object name there opens its verb menu. And
**interact mode** — the `◎` button, or a controller button bound to `interact` — cycles the room
by category; its **Objects** category walks exactly this list without you touching the screen.

→ **Expected result:** interact mode's **Objects** category steps through the ground objects one
at a time, and activating one raises its verb sheet.
{{#endtab}}
{{#endtabs}}

## Common setups

### A loot shelf under your hunting column

Add the items window, send it to the **Right Bar**, and give it about ten rows below
[Targets](./targets.md) and [Players](./players.md). All three read the same room feed and
refresh together when you move or something drops.

**You'll see:** one column that answers "what's attacking me / who's watching / what's on the
floor" without a single scroll.

### Bagging loot without typing

With the items window and a container window both open in the GUI, drag an item row onto the
container.

**You'll see:** the item moves into that container, and it leaves the items list on the next
room update — the same result as typing `put <item> in <container>`, with no noun to spell.

## Tips & gotchas

> ⚠️ **Right-clicking an item row opens the *window* menu, not a menu for that item.** Left-click
> the name to ask the server for the object's verbs.

> ⚠️ **The TUI puts a live count in the title (`Items [04]`); the GUI does not.** The GUI shows
> **No objects here.** in an empty room instead.

**Creatures and items come from the same feed, split by how the game bolds them.** The room's
object line marks creatures in bold and everything else plain. Bold entries become
[Targets](./targets.md); plain ones land here. That's why a creature never appears in this
window and a chest never appears in that one.

**Dragging is a GUI-only gesture, and items are the only one of the three room lists that has
it.** Target and player rows respond to clicks only.

**The list is in the game's order, not sorted.** Rows appear exactly as the room reports them,
so a new drop shows up wherever the game puts it rather than at the bottom.

**The window follows the room, not your inventory.** Picking something up removes it here on the
next room update. What you're carrying belongs to the [Inventory](./inventory.md) window.

## See also

- [Targets](./targets.md) — the bolded, creature half of the same room feed
- [Players](./players.md) — the room roster, with corpse styling
- [Inventory](./inventory.md) — what you're carrying, rather than what's on the floor
- [Containers](./containers.md) — a window per bag, and the drop target for dragging loot
- [Room Window](./room-window.md) — the prose, with its **Objects** section toggle

<details>
<summary>Config reference (TOML)</summary>

### Per-window (`layout.toml`)

`widget_type = "items"`. This widget has exactly one field of its own.

| Field | Type | Default | What it does |
|---|---|---|---|
| `entity_id` | string | `"items"` | Entity feed id for the window |

Everything else is a standard window field. The shipped template is 10 rows by 40 columns, with
a floor of 4 rows by 20 columns.

There is no `[items]` config section — this window has no global settings. The
`[target_list]` settings that shape [Targets](./targets.md) and [Players](./players.md) do not
apply to it.

```toml
[[windows]]
name = "loot"
widget_type = "items"
title = "Ground"
row = 25
col = 92
rows = 10
cols = 28
show_border = true
```

</details>
