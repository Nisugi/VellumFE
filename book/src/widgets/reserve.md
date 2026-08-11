# Reserve

> A standing list of what the game is holding aside for you, so you can check it without
> spending a command and a screenful of text mid-hunt.

## What it's for

Some things the game sets aside for you rather than handing over — and the only way to see them
is to ask, which costs you a command and scrolls your screen at the exact moment you'd rather it
held still.

A reserve window parks that list on screen. The game sends the whole list at once, the window
shows it, and it sits there until the game sends a different one. Nothing scrolls, nothing
accumulates, and the names stay clickable — so you can act on an entry without typing a noun
you'd have to scroll back to read.

It is the [Inventory](./inventory.md) window's twin in every mechanical respect. The only
differences are which feed fills it and which game offers it.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Character**, and tick **reserve**. It arrives
   20 rows by 40 columns, titled **Reserve**.
   (Typed equivalent: `.addwindow reserve reserve 100 0 40 20`.)
2. There is no widget section in this window's right-click menu — reserve has no settings of its
   own. **Streams** and **Buffer lines** live under **Window**, and **Wrap text** under
   **Appearance ▸ Text**. Leave **Streams** on `reserve`; that is the feed the game sends.

<figure class="shot" data-shot="widgets/reserve-gui-character-catalog">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Windows</b> catalog with the <b>Character</b> group expanded, showing <b>reserve</b> beside <b>inventory</b>.</figcaption>
</figure>

→ **Expected result:** a window titled **Reserve** holding the current list. Clicking an entry
opens that item's verb menu.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow reserve reserve 100 0 40 20` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow reserve` alone
> prints usage and adds nothing.

`.editwindow reserve` opens the form. It offers three fields — streams, buffer size, and word
wrap — the same three an inventory window offers. `Tab` moves between fields, `Ctrl+S` saves,
`Esc` cancels.

→ **Expected result:** a bordered window titled `Reserve` listing the current entries. Clicking a
row opens that item's verb menu.
{{#endtab}}
{{#tab name="Mobile"}}

There is no reserve surface on the phone — not a drawer section, not a stream chip, not a wheel
slice. The list is a desktop window only.

What the phone does carry is the neighbouring feed: the **Inventory** stream chip, in the
**stream filter chips** row, prints what you're carrying as it arrives. The touch wheel's
**Inventory** slice is the fast route — it switches you straight to that chip. For anything the
reserve window would have told you, ask the game in the command input and read the reply in the
story pane like any other output.

→ **Expected result:** the touch wheel's **Inventory** slice switches the view to the
**Inventory** chip, showing your carried items. Reserve entries are not among them.
{{#endtab}}
{{#endtabs}}

## Common setups

### A character column that answers "what do I have?" in one glance

Reserve is a single-purpose window and it earns its space by sitting next to its relatives rather
than alone.

1. Add **inventory** and **reserve** from the catalog's **Character** group.
2. Send both to the same zone — the **Right Bar** works well — and stack reserve under inventory.
3. Give reserve the shorter of the two. It is usually the smaller list.

**You'll see:** one column reading carried items on top and held-aside items below, both
refreshing on their own when the game sends a new list, and every name in both clickable.

## Tips & gotchas

> ⚠️ **This window is GemStone IV only, and the two ways of adding it disagree about that.**
> Both pickers hide it from DragonRealms characters — it is absent from the GUI **Windows**
> catalog and from the bare-`.addwindow` picker. The full six-argument `.addwindow` form does not
> check, so it will build the window on a DR character. You get an empty box: DragonRealms never
> sends this feed. **If the game is not set at all, VellumFE assumes GemStone IV and the row
> appears** — which is why it shows up when you connect through Lich without naming a game.

> ⚠️ **There is no scrollback, and that is the design.** Each list the game sends replaces the
> previous one wholesale rather than being added under it. Scrolling up shows you nothing earlier,
> because nothing earlier is kept. If you want history, the story pane already has it.

> ⚠️ **Right-clicking a row opens the *window* menu, not a menu for that entry.** Left-click the
> name to ask the server for that item's verbs.

**The window only redraws when the list actually changes.** VellumFE compares each incoming list
against the one it is already showing and leaves the window alone when they match. A window that
looks frozen is usually correct — ask the game to re-send if you want to be sure.

**Buffer lines does nothing useful here.** The field is offered because this window shares its
plumbing with text windows, but the contents are cleared on every update, so there is never a
backlog for the buffer to cap. The shipped preset sets it to zero.

**Keep the border on.** Click detection on these rows assumes a bordered window, so turning the
border off can shift where your clicks land by a row.

**Leave the streams field alone unless you know why you're changing it.** It is editable in both
frontends, but `reserve` is the feed the game sends. Point it elsewhere and the window fills with
something that is not your reserve list.

**Nothing about this window appears in a layout you share with a DragonRealms player.** The
window definition still loads for them, but it stays empty.

## See also

- [Inventory](./inventory.md) — the same widget mechanics, fed by what you carry
- [Containers](./containers.md) — a window per bag, for what's inside them
- [Items](./items.md) — what's on the ground, rather than what's yours
- [Text Windows](./text-windows.md) — how stream-fed windows work generally
- [Creating Layouts](../customization/layouts.md) — placing and saving these windows

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog and `.addwindow`. Hand-editing is for troubleshooting, not the normal path.

```toml
[[windows]]
name = "reserve"
widget_type = "reserve"
row = 0
col = 100
rows = 20
cols = 40
show_border = true
title = "Reserve"
streams = ["reserve"]
buffer_size = 0
wordwrap = true
```

### Widget fields

`widget_type = "reserve"`. The type string is exactly `reserve`; an unrecognized type does not
error, it quietly creates a **text** window instead.

| Field | Type | Default | What it does |
|---|---|---|---|
| `streams` | array of string | `["reserve"]` | Feeds the window reads. The game sends `reserve`. |
| `buffer_size` | integer | `0` | Scrollback cap. Inert here — content is replaced, not accumulated. |
| `wordwrap` | boolean | `true` | Wraps long lines instead of clipping them at the border |
| `show_timestamps` | boolean | `false` | Present in the shared data shape; not offered in either editor for this widget. |

These are the same four fields an `inventory` window carries — the two share one data shape.

### Catalog preset

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| `reserve` | Character | Reserve | 20 rows x 40 cols (floor: 4 rows) | GemStone IV only |

The game gate hides the row from DragonRealms characters in both the GUI catalog and the
`.addwindow` picker. **An unset game counts as GemStone IV**, so the row appears by default. The
six-argument `.addwindow` form is not gated and will build the window regardless.

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Streams, Buffer lines | Right-click ▸ **Window** | `.editwindow reserve` |
| Wrap text | Right-click ▸ **Appearance ▸ Text** | `.editwindow reserve` |
| Speak new lines (TTS) | Right-click ▸ **Window** | not offered |

There is no widget section in the GUI right-click menu for this type and no `[reserve]` config
section — this window has no global settings. Standard window keys — `row`, `col`, `rows`,
`cols`, `show_border`, `border_style`, `title`, `locked` — apply as they do to any window.

</details>
