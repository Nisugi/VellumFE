# Encumbrance

> Know you're overloaded before the roundtime tells you — the level, the color, and the game's
> own explanation of it, parked on screen.

## What it's for

Encumbrance is the stat you notice by its consequences. You loot a suit of plate, your swings get
slower, and you spend a command and a screenful of text working out why. Meanwhile the game has
been publishing your encumbrance level the whole time.

This window parks that reading on screen: a bar that fills as you load up, colored by how bad it
has got, with the level name written across it and the game's own description underneath. It
changes the moment you pick something up, so "should I sell this before the next room?" becomes a
glance instead of a check.

It is one of the few GemStone-flavored windows that works in **both** games — DragonRealms
characters get it too.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Character**, and tick **Encumbrance**. It
   arrives 4 rows by 25 columns.
   (Typed equivalent: `.addwindow encum encum 0 0 25 4` — note the type string is **`encum`**,
   not the word on the row.)
2. Right-click the window and open the **Encumbrance** section. It carries two checkboxes:
   **Level bar** and **Help text**. Untick **Help text** for a bar on its own; untick **Level
   bar** for the description alone.

<figure class="shot" data-shot="widgets/encumbrance-gui-section-toggles">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>An Encumbrance window right-clicked with the <b>Encumbrance</b> section open, showing the <b>Level bar</b> and <b>Help text</b> checkboxes above the filled bar and its blurb.</figcaption>
</figure>

→ **Expected result:** a bar reading **Encumbrance: Light** that lengthens and changes color as
you load up, with the game's description of that level on the line below.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow encum encum 0 0 25 4` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow encum` alone
> prints usage and adds nothing.

> ⚠️ **The type string is `encum`, not `encumbrance`.** An unrecognized type does not error —
> it quietly builds a **text** window instead, which then sits there empty. If your encumbrance
> window is a blank box, check the spelling first.

`.editwindow encum` opens the form. The widget fields are **Show Label** and four color fields —
**Light**, **Moderate**, **Heavy** and **Critical** — one per band of the bar. `Tab` moves
between fields, `Ctrl+S` saves, `Esc` cancels.

→ **Expected result:** a bordered window with a colored fill carrying the level name centered on
it, and the game's description on the row beneath.
{{#endtab}}
{{#tab name="Mobile"}}

The phone uses fixed chrome, so you don't place this window — but the reading does reach you.
Open the **status drawer** (swipe in from the right) and scroll to its **Encumbrance** section,
which prints the level and percentage together as `Light (18%)` with the game's description on
the line below.

It is text rather than a bar, and there is nothing to configure: no color bands, no toggles, no
way to move it out of the drawer. Author the colors and toggles on the desktop; they are stored
per character.

→ **Expected result:** the **status drawer**'s **Encumbrance** section reads your current level
and percentage, updating as you pick things up.
{{#endtab}}
{{#endtabs}}

## Common setups

### A carry-weight corner next to what you're carrying

Encumbrance is only actionable next to the thing causing it, so put it where you already look
when you're deciding what to drop.

1. Add **Encumbrance** from the catalog's **Character** group and **inventory** from the same
   group.
2. Send both to the same zone — the **Right Bar** works well — with encumbrance directly above
   the inventory list.
3. Right-click the encumbrance window ▸ **Encumbrance** ▸ untick **Help text**. Once you know
   what "Moderate" means, the sentence explaining it is spent space, and the window shrinks to
   the bar alone.

**You'll see:** a single colored bar above your carried list, shifting from green through orange
as you loot, with the exact list of what did it directly underneath.

## Tips & gotchas

> ⚠️ **The dedicated widget and a plain bar bound to `encumlevel` are both real, and they are
> not the same thing.** The game publishes one `encumlevel` reading and everything that wants it
> gets it, so you can run both at once. Choose by what you want on screen:
> - **This widget** — the level *name* on the fill, color bands that change as you load up, and
>   the game's description line. The most information, in a 4-row window.
> - **A [progress bar](./progress-bars.md) with Bar id `encumlevel`** — one flat-colored fill,
>   no bands, no description, in a window as small as you like. Right for a header strip where
>   everything else is a bar too.
> - **A bar inside [Mini Vitals](./minivitals.md)** — desktop GUI only, where **Encumbrance** is
>   one of the selectable bars, so it rides alongside health and mana in a single window.

> ⚠️ **The color bands differ between frontends, and only the terminal lets you set them.** The
> terminal uses four bands — 0–20, 21–50, 51–80, 81–100 — with **Light**, **Moderate**, **Heavy**
> and **Critical** colors you choose. The desktop GUI uses **three fixed bands** — 0–33, 34–66,
> 67 and up — in green, orange and red, and **reads none of the four color fields**. Setting them
> changes your terminal and leaves the GUI alone.

> ⚠️ **Each frontend offers one toggle the other doesn't.** The GUI has **Level bar** and **Help
> text**; the terminal has **Show Label** (the description) and no way to hide the bar. A layout
> with `show_bar = false` renders bar-less in the GUI and with a bar in the terminal.

**The two frontends word the bar differently.** The GUI always prefixes it — `Encumbrance: Light`
— while the terminal centers the bare level name on the fill. Content alignment is a terminal
setting; the GUI ignores it here.

**An empty window means the game hasn't sent a reading yet.** The terminal says so out loud with
`(No encumbrance data)`. It fills in on the next update.

**This widget works in both games.** Unlike its neighbours in the **Character** group, it carries
no game gate, so it appears in the catalog and the `.addwindow` picker for DragonRealms
characters as well as GemStone IV ones.

**The window is capped at 4 rows** and floors at 3, so growing it is a matter of width rather
than height. With **Help text** off, 3 rows is the whole window.

## See also

- [Progress Bars](./progress-bars.md) — the plain `encumlevel` bar, and every other feed
- [Mini Vitals](./minivitals.md) — encumbrance as one bar in a shared strip (desktop GUI)
- [Inventory](./inventory.md) — what you're carrying, which is what moved this number
- [Containers](./container-windows.md) — a window per bag, for finding the heavy thing
- [Experience (GemStone IV)](./gs4-experience.md) — the other resident dialog with its own widget
- [Creating Layouts](../customization/layouts.md) — placing and saving these windows

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog and the editors above. Hand-editing is for troubleshooting, not the normal
path.

```toml
[[windows]]
name = "encum"
widget_type = "encum"
title = "Encumbrance"
row = 0
col = 0
rows = 4
cols = 25
show_border = true
align = "left"
show_label = true
show_bar = true
```

`widget_type = "encum"`. **The type string is `encum`, not `encumbrance`** — the catalog row and
the window title read "Encumbrance", but the type is the short form. An unrecognized type does
not error, it quietly creates a **text** window instead.

### Widget fields (`EncumbranceWidgetData`)

| Field | Type | Default | What it does |
|---|---|---|---|
| `show_bar` | boolean | `true` | Draw the level bar. **GUI only** — the terminal always draws it. |
| `show_label` | boolean | `true` | Draw the game's description line under the bar |
| `align` | string | `"left"` | Alignment of the description line: `left`, `center`, `right`. **Terminal only.** |
| `color_light` | string | green | Fill for 0–20%. **Terminal only.** |
| `color_moderate` | string | yellow | Fill for 21–50%. **Terminal only.** |
| `color_heavy` | string | orange `#ffa500` | Fill for 51–80%. **Terminal only.** |
| `color_critical` | string | red | Fill for 81–100%. **Terminal only.** |

The desktop GUI's bands are fixed at 0–33 green `#55b86c`, 34–66 orange `#ff8800`, and 67+ red
`#cd4d4d`, and it prefixes the bar text with `Encumbrance: `.

### Feed

The window is a view onto the game's `encum` dialog: the `encumlevel` progress bar supplies the
percentage and the level name, and the `encumblurb` label supplies the description. The same
`encumlevel` reading is available as a **Bar id** for an ordinary `progress` window, and as the
**Encumbrance** bar inside a desktop-GUI Mini Vitals window. All three can be on screen at once.

### Catalog preset

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| `encum` | Character | Encumbrance | 4 rows x 25 cols (floor 3 rows / 15 cols, ceiling 4 rows) | **Both games — no gate** |

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Show the bar | Right-click ▸ **Encumbrance** ▸ **Level bar** | not offered |
| Show the description | Right-click ▸ **Encumbrance** ▸ **Help text** | `.editwindow encum` ▸ **Show Label** |
| Band colors | not offered (fixed) | `.editwindow encum` ▸ **Light** / **Moderate** / **Heavy** / **Critical** |
| Content alignment | Right-click ▸ **Appearance ▸ Text** | `.editwindow encum` |

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`, `title`,
`locked` — apply as they do to any window.

</details>
