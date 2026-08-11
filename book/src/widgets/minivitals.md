# Mini Vitals

> All four vitals in one strip, the width of a header — for when four separate bar windows are
> three windows too many.

## What it's for

Four [progress bars](./progress-bars.md) tell you everything you need, and cost you four windows
to place, four borders to align, and four things to re-drag every time you rearrange a layout.
Mini Vitals is the same information as one window: health, mana, stamina and spirit drawn side by
side across a single row, sized and moved as one thing.

It is also the widget that knows your real numbers. Ordinary bars and the phone's chrome work in
percentages; this one carries `142/193` because the game sends the current value and the maximum
together. Pick it when you want a compact strip and exact figures, and pick separate bars when
you want each vital in a different corner of the screen.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar and expand **Progress Bars**. The row reads
   **minivitals** in lowercase — this widget ships with no title, so the catalog falls back to
   its key. Tick it.
   (Typed equivalent: `.addwindow minivitals minivitals 0 0 80 3`.)
2. Right-click the window and open the **Vitals** section. It holds **Layout** (**One row** or
   **Stacked**), **Bar height**, **Bar text**, **Depleted color**, and a **Bars shown (in display
   order)** list with a checkbox and ▲ ▼ buttons per bar.
3. Set **Bar text** to taste. The choices show you their own output: **Health: 191/193**,
   **Health: 99%**, **191/193**, **99%**, or **No text**.

<figure class="shot" data-shot="widgets/minivitals-gui-vitals-section">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A Mini Vitals window right-clicked with the <b>Vitals</b> section open, showing <b>Layout</b>, <b>Bar height</b>, <b>Bar text</b>, <b>Depleted color</b>, and the <b>Bars shown (in display order)</b> list with its ▲ ▼ reorder buttons.</figcaption>
</figure>

→ **Expected result:** one window carrying four filled bars that redraw the moment the game
sends a vitals update, each reading its current value over its maximum.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow minivitals minivitals 0 0 80 3` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow minivitals`
> alone prints usage and adds nothing.

`.editwindow minivitals` opens the form. The widget fields are **Numbers Only**, **Current
Only**, a **[ Edit Bars & Colors ]** button, and **Depleted**. `Tab` moves between fields,
`Ctrl+S` saves, `Esc` cancels.

Press Enter on **[ Edit Bars & Colors ]** for the bar list. It offers five bars — health, mana,
stamina, spirit and concentration — with a toggle column and a color column each, and **caps you
at four enabled at once**.

→ **Expected result:** a single-row window with four colored fills side by side, each carrying
its text centered over the fill.
{{#endtab}}
{{#tab name="Mobile"}}

The phone uses fixed chrome, so you don't place this window. Your vitals live in the **status
row** above the command input as a built-in cluster of four bars labelled **HP**, **MP**, **ST**
and **SP**. Turn the whole cluster off under the gear ⚙ with the **Vitals bars** toggle — that
is the extent of the control.

> ⚠️ **The phone shows percentages, never your real numbers.** The status row reads `HP 74%`
> where the desktop widget reads `142/193`. The phone is fed the percentage vitals, not the
> value-and-maximum pair this widget draws, so no setting anywhere makes `142/193` appear there.

Bar order, bar text and colors are desktop authoring. They are stored per character and waiting
for you next time you play there.

→ **Expected result:** four labelled percentage bars track your vitals in the phone's status row,
moving on the same updates the desktop widget reads.
{{#endtab}}
{{#endtabs}}

## Common setups

### A Wrayth-style stats strip across the top of the screen

This is what the widget was shaped for: one row, no title, no wasted height.

1. Add **minivitals** from the catalog and set its row's **zone** to **Header**.
2. Right-click it ▸ **Vitals** ▸ set **Layout** to **One row** and **Bar text** to
   **191/193** — position and color already tell you which bar is which, so the word is spent
   space.
3. Right-click ▸ **Appearance ▸ Frame** and turn the border off, so the strip reads as part of
   the chrome rather than a box sitting on it.
4. Drag the bottom edge until the bars are the height you want, or set **Bar height** directly.

**You'll see:** a single unbroken row across the top of your layout reading four pairs of
numbers in red, blue, orange and gray, each fill draining as you take the hit that caused it.

## Tips & gotchas

> ⚠️ **In the GUI these settings are shared by every Mini Vitals window; in the terminal they
> belong to the window.** The GUI stores Layout, Bar height, Bar text, Depleted color and the bar
> list once, in your per-character GUI layout — change them on one window and every Mini Vitals
> window changes with it. The terminal stores its own set of fields per window in `layout.toml`.
> **The two frontends do not share these settings at all**, so a window you tuned in the GUI
> arrives in the terminal with terminal defaults, and the reverse.

> ⚠️ **The GUI version draws four bars the terminal version cannot.** The GUI's bar list offers
> **Mind**, **Encumbrance**, **Next Level** and **Blood** alongside the four vitals, so one
> window can carry your health and your encumbrance together. The terminal accepts only health,
> mana, stamina, spirit and concentration, and **silently ignores anything else in the list** —
> a layout using the GUI's extra bars renders in the terminal with those bars missing rather than
> with an error.

> ⚠️ **Four bars maximum in the terminal, no cap in the GUI.** The terminal's
> **[ Edit Bars & Colors ]** editor refuses a fifth tick. The GUI lets you enable all eight, and
> they divide the width between them.

**This widget and your ordinary bars read the same feed, so both are live at once.** The game
sends one `health` update and it lands in every place that wants it: each `progress` window bound
to `health`, this widget, and the phone's status row. Running a Mini Vitals strip and a separate
health bar is not a conflict and costs you nothing.

**The window ships with no title on purpose**, which is why the catalog row reads `minivitals` in
lowercase where its neighbours read proper names. Give it a title under **Window** if you want
one, or leave the title bar off entirely.

**`Current Only` beats `Numbers Only` in the terminal.** With both ticked you get the bare
current value. The order is: current-only, then numbers-only, then the game's own text.

**Nothing here changes color at a threshold.** No low-health red, no flash, no pulse — each bar
draws one solid color at every value. Alerting on low health is a hotbar button's condition
state, an indicator, a sound, or controller rumble; see
[Make your health bar shout when you're hurt](../how-to/vitals-flash.md).

**Depleted color is the unfilled part of each bar.** Leave it empty and the unfilled portion
follows the window background, which is usually what you want. Set it to pick out how much of
each bar is gone at a glance.

**This widget is GemStone IV only.** Both pickers hide it from DragonRealms characters — it is
absent from the GUI **Windows** catalog and from the bare-`.addwindow` picker. The full
six-argument form does not check, so it will build the window on a DR character. **If the game is
not set at all, VellumFE assumes GemStone IV and the row appears**, which is why it shows up when
you connect through Lich without naming a game.

## See also

- [Progress Bars](./progress-bars.md) — one bar per window, when you want them in different places
- [Experience (GemStone IV)](./gs4-experience.md) — where mind state and next-level get their full detail
- [Encumbrance](./encumbrance.md) — the dedicated widget for the encumbrance bar
- [Betrayer](./betrayer.md) — where the **Blood** bar's number comes from
- [Dashboard](./dashboard.md) — statuses in a grid, the other "several things, one window" widget
- [Injury Display](./injury-doll.md) — where you're hurt, where a health bar only says how much
- [Make your health bar shout when you're hurt](../how-to/vitals-flash.md) — alerting, since bars never change color
- [Creating Layouts](../customization/layouts.md) — placing and saving these windows

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog and the editors above. Hand-editing is for troubleshooting, not the normal
path.

```toml
[[windows]]
name = "minivitals"
widget_type = "minivitals"
row = 0
col = 0
rows = 3
cols = 80
show_border = true
numbers_only = false
current_only = false
bar_order = ["health", "mana", "stamina", "spirit"]
```

`widget_type = "minivitals"`. The type string is exactly `minivitals`; an unrecognized type does
not error, it quietly creates a **text** window instead.

### Widget fields (`MiniVitalsWidgetData`) — read by the terminal

| Field | Type | Default | What it does |
|---|---|---|---|
| `numbers_only` | boolean | `false` | Draw `current/max` instead of the game's text |
| `current_only` | boolean | `false` | Draw only the current value. Takes precedence over `numbers_only`. |
| `bar_order` | array of string | `["health","mana","stamina","spirit"]` | Which bars, in which order. Names outside `health`, `mana`, `concentration`, `stamina`, `spirit` are dropped. |
| `health_color` | string | `#6e0202` | Fill color for the health bar |
| `mana_color` | string | `#08086d` | Fill color for the mana bar |
| `stamina_color` | string | `#bd7b00` | Fill color for the stamina bar |
| `spirit_color` | string | `#6e727c` | Fill color for the spirit bar |
| `concentration_color` | string | *(none)* | Fill color for the concentration bar |
| `depleted_color` | string | *(none)* | Unfilled portion of each bar. Empty follows the window background. |

**The desktop GUI reads none of these.** Its equivalent settings live in the per-character GUI
layout file as one shared vitals config — orientation (`horizontal` / `vertical`), bar height
(default `18.0`, clamped 8–60), text format
(`label_value_max` / `label_percent` / `value_max` / `percent` / `none`), depleted color, and
the bar list (default health, mana, stamina, spirit; the full vocabulary adds `mind`,
`encumbrance`, `next_level`, `blood`).

### Catalog preset

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| `minivitals` | Progress Bars | *(none — the row shows the key)* | 3 rows x 80 cols (fixed at 3 rows; floor 40 cols) | GemStone IV only |

Rows are pinned at 3 by `min_rows` and `max_rows` — one content row plus two borders. The game
gate hides the row from DragonRealms characters in both pickers; the six-argument `.addwindow`
form is not gated. **An unset game counts as GemStone IV.**

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Layout / orientation | Right-click ▸ **Vitals** ▸ **Layout** | not offered (always one row) |
| Bar height | Right-click ▸ **Vitals** ▸ **Bar height** | window height |
| Bar text | Right-click ▸ **Vitals** ▸ **Bar text** | `.editwindow` ▸ **Numbers Only** / **Current Only** |
| Which bars, and their order | Right-click ▸ **Vitals** ▸ **Bars shown** | `.editwindow` ▸ **[ Edit Bars & Colors ]** |
| Per-bar color | not offered | `.editwindow` ▸ **[ Edit Bars & Colors ]** ▸ color column |
| Depleted color | Right-click ▸ **Vitals** ▸ **Depleted color** | `.editwindow` ▸ **Depleted** |

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`, `title`,
`locked` — apply as they do to any window.

</details>
