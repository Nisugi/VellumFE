# Inventory

> Everything you're carrying and wearing, standing still in its own window — so you can see
> what's in your hands and on your back without typing `inv` and watching it scroll away.

## What it's for

`INVENTORY` answers a question you ask constantly and it answers it into the main window, where
the next combat message buries it. Ten seconds later you're typing it again.

The inventory window holds that answer permanently. It lists your worn and carried items, and it
rewrites itself the moment the list changes — you pick something up, stow a gem, draw a weapon,
and the window is already correct. Click any item and the game's verb menu opens on it, so you
can look, wear, or stow without spelling the noun.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, open the **Character** category, and tick **Inventory**.
   (Typed equivalent: `.addwindow inventory inventory 92 3 40 20`.)
2. Use the row's **zone** control to place it — the **Right Bar** keeps it out of the text.
3. There is no widget section in this window's right-click menu. Its look lives under
   **Appearance**, and **Appearance ▸ Text ▸ Word wrap** is the one setting that changes how the
   list reads.

<figure class="shot" data-shot="widgets/inventory-gui-carried-list">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>An Inventory window in the right bar listing worn and carried items, with the pointer over one row showing the pointing-hand cursor.</figcaption>
</figure>

→ **Expected result:** a standing list of what you're carrying. Clicking an item opens that
item's verb menu; the list rewrites itself whenever your inventory changes.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow inventory inventory 92 3 40 20` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead, and choose **inventory**.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow inventory`
> alone prints usage and adds nothing.

`.editwindow inventory` opens the form. Its widget fields are **Streams**, **Buffer size**, and
**Wordwrap**. `Tab` moves between fields, `Ctrl+S` saves, `Esc` cancels. Turning **Wordwrap**
off clips long item names at the border instead of flowing them onto a second row; the change
takes effect as soon as you save.

> **Buffer size has no effect on this window.** The field is there and it saves, but nothing
> reads it — the window replaces its whole contents on every update, so there is no scrollback
> to size.

→ **Expected result:** a bordered list of your worn and carried items. Clicking a row opens that
item's verb menu.
{{#endtab}}
{{#tab name="Mobile"}}

You don't place windows on the phone, but inventory is a first-class surface there. It's a
**stream filter chip** labelled **Inventory** in the chip bar above the text pane — tap it and
the pane switches from the story to your carried items. The touch wheel ships an **Inventory**
slice that jumps straight to the same chip.

> ⚠️ **Inventory is a chip, not a section of the status drawer.** The drawer carries **Targets**,
> **Players**, and **Room**; your carried items are not one of its sections. Tapping the wheel's
> **Inventory** slice switches the text pane rather than opening the drawer.

Items in the pane stay tappable, so tapping one raises its verb sheet exactly as on desktop.

→ **Expected result:** tapping the **Inventory** chip fills the text pane with your worn and
carried items, and tapping an item raises that item's verb sheet.
{{#endtab}}
{{#endtabs}}

## Common setups

### A carry column you can bag loot into

Add the inventory window, send it to the **Right Bar**, and give it about twenty rows. Open an
[Items](./items.md) window above it and a [Container](./containers.md) window for your pack
below it. Now hold **Ctrl** and drag a row from the items window onto the container window.

**You'll see:** the item lands in the bag, disappears from the ground list on the next room
update, and appears inside the container window — the result of `put <item> in <pack>` with no
noun typed. Dropping onto the **inventory window's body** instead sends `wear`, so a cloak
dragged there goes on rather than into a bag.

### Watching a stow actually land

Keep the inventory window open and stow something from your hand.

**You'll see:** the list rewrites in place the instant the game reports the change — no scroll,
no re-typing `inv`, and the item you stowed is no longer listed loose.

## Tips & gotchas

> ⚠️ **This window is a snapshot, not a log — it has no scrollback.** Every update clears it and
> refills it from the game's current list. `PageUp` will not show you what you were carrying a
> minute ago, because that text was never kept. The **Buffer size** control the editors offer for
> this widget is inert; leave it alone.

> ⚠️ **Dropping an item onto the inventory window's empty body means `wear`, not "put in
> inventory".** Release over a *row* and the drop targets that specific item instead. This is the
> one drop target whose meaning isn't "into the thing under the cursor."

> ⚠️ **In the GUI, `Ctrl+C` quits. In the TUI it copies your selection.** Quit the TUI with
> `.quit` or `.exit`.

**Dragging needs a modifier, and it's `Ctrl` by default.** Plain dragging selects text in both
desktop frontends. Hold the drag modifier first — it's `ui.drag_modifier_key` in `config.toml`
and accepts `ctrl`, `alt`, or `shift`.

**The window can be empty-looking rather than empty.** Unlike [Items](./items.md), which prints
**No objects here.**, and [Containers](./containers.md), which prints **Empty.**, inventory
renders nothing at all until the game has sent the list. Type `inv` once after login if a fresh
window looks blank.

**An unchanged list is not redrawn.** The client compares each incoming snapshot against the last
one and skips identical updates, so a window that doesn't flicker on every `inv` is working
correctly.

**You can have more than one.** Every inventory window receives the same feed, so a second one in
another zone is a legitimate way to see a long list without a tall window.

**[Reserve](./reserve.md) is the same widget on a different feed.** That window behaves
identically — snapshot in, snapshot replaced.

**`.foreach` and `.sorter` read your inventory without a window open.** The item registry is fed
straight from the feed whether or not you've placed this widget, so batch commands work either
way. See [Inventory Tools](../features/inventory-tools.md).

## See also

- [Containers](./containers.md) — a window per bag, and the drop target for stowing
- [Items](./items.md) — what's on the ground rather than on you
- [Reserve](./reserve.md) — the same snapshot widget on the `reserve` feed (GemStone IV)
- [Inventory Tools (.foreach, .sorter)](../features/inventory-tools.md) — batch commands over
  what you're carrying
- [Hands](./hands.md) — what's in each hand, on its own

<details>
<summary>Config reference (TOML)</summary>

### Per-window (`layout.toml`)

`widget_type = "inventory"`. The `.addwindow` type string is `inventory`.

| Field | Type | Default | What it does |
|---|---|---|---|
| `streams` | list | `["inv"]` | Feed the window reads. `inv` is what the game sends |
| `buffer_size` | number | `0` | **Inert for this widget.** Saved and ignored — content is replaced wholesale each update |
| `wordwrap` | bool | `true` | Wrap long item names instead of clipping them |
| `show_timestamps` | bool | `false` | Present on the type but not offered in either editor for inventory windows |

The shipped template is 20 rows by 40 columns with a floor of 4 rows. There is no `[inventory]`
section in `config.toml` — this window has no global settings.

```toml
[[windows]]
name = "inventory"
widget_type = "inventory"
title = "Inventory"
row = 3
col = 92
rows = 20
cols = 40
show_border = true
streams = ["inv"]
wordwrap = true
```

</details>
