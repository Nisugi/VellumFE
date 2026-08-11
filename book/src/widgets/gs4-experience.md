# Experience (GemStone IV)

> Your level, your mind state, and how close you are to the next one — parked on screen so
> you stop typing `exp` between every kill.

## What it's for

Mid-hunt you want two numbers: how saturated your mind is, and how much further you have to
go. Asking for them costs a command and scrolls away the combat you were reading.

This window keeps both standing. The mind bar fills as your head fills, the experience bar
creeps toward your next level, and your level sits above them. It updates itself when the
game sends new numbers — you never ask.

**This is the GemStone IV window.** DragonRealms tracks experience per skill and has an
entirely different widget for it: see [Experience (DragonRealms)](./experience.md). The two
share a name in the catalog and nothing else.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Character**, and tick **Experience**. It
   arrives 5 rows by 30 columns, titled **Experience**, with its text centered.
   (Typed equivalent: `.addwindow gs4_experience gs4_experience 0 0 30 5`.)
2. Right-click the window and open its **Experience** section to choose what it shows.
   **Level**, **Mind state**, and **Experience bar** are on; **Total exp** and **Ascension
   exp** are off. **Mind bar color** and **Exp bar color** take a `#rrggbb` value or a color
   name, and commit on **Enter** or when you click away.

<figure class="shot" data-shot="widgets/gs4-experience-window-section">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The window's right-click menu with the <b>Experience</b> section open, showing the five field checkboxes above the two bar-color fields.</figcaption>
</figure>

→ **Expected result:** a compact window reading your level, a **Mind:** bar, and a **Next:**
bar. Ticking **Total exp** adds an `Exp: 1,234,567` line beneath them.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow gs4_experience gs4_experience 0 0 30 5` — name, type, then column, row,
width, height. Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow
> gs4_experience` alone prints usage and adds nothing.

`.editwindow gs4_experience` opens the form. It offers seven fields: **Show Level**, **Show
Exp Bar**, **Show Mind Bar**, **Show Total Exp**, **Show Ascension Exp**, and the **Mind**
and **Exp** color boxes. `Tab` moves between fields, `Ctrl+S` saves, `Esc` cancels.

→ **Expected result:** a bordered window titled `Experience` with your level on the first
row and two text-labelled bars beneath it. **The TUI paints the bar text bare** — the mind
state and the experience string, with no `Mind:` or `Next:` prefix.
{{#endtab}}
{{#tab name="Mobile"}}

The phone uses fixed chrome, so you don't place this window — but the data is there. Open
the **status drawer** (the right drawer) and scroll to its **Experience** section. It carries
three lines pushed from your desktop session: your level, `Mind: <state> (<n>%)`, and
`Next level: <text>`.

That section is **built from the GemStone IV feed only** — it shows nothing for a
DragonRealms character. It also ignores your window's field checkboxes: the three lines
appear whenever the game has sent them, whatever you ticked on the desktop.

→ **Expected result:** the **status drawer**'s **Experience** section lists your level, mind
state with its percentage, and your progress toward the next level.
{{#endtab}}
{{#endtabs}}

## Common setups

### A one-line experience strip above your vitals

The window is at its best flattened into a status strip rather than kept as a box.

1. Add **Experience** from the catalog's **Character** group.
2. Right-click it, open the **Experience** section, and untick **Level**. Your level rarely
   changes; the two bars are what you watch.
3. Under **Appearance ▸ Title bar**, turn the title bar off, and under **Appearance ▸
   Frame** keep only the left and right border sides.
4. Drag it directly above your vitals window, or send it to the same zone.

**You'll see:** two labelled bars, no chrome, sitting flush on your vitals stack — mind
saturation and next-level progress readable without a glance away from combat.

## Tips & gotchas

> ⚠️ **This window is GemStone IV only, and the two ways of adding it disagree about that.**
> Both pickers hide it from DragonRealms characters — it is absent from the GUI **Windows**
> catalog and from the bare-`.addwindow` picker. The full six-argument `.addwindow` form does
> not check, so it builds the window on a DR character, where it stays permanently empty.
> **If the game is not set at all, VellumFE assumes GemStone IV and the row appears** — which
> is why it shows up when you connect through Lich without naming a game.

> ⚠️ **The two frontends label the bars differently.** The GUI writes **Mind:** and **Next:**
> in front of the game's text; the TUI paints the game's text alone. Same numbers, different
> wording — don't be thrown reading a screenshot from the other frontend.

**The window can appear on its own.** GemStone IV sends this data as a dialog, and VellumFE
claims that dialog for this widget. If you have no experience window when the game first
sends it, one is created and bound for you. It never duplicates: a second send finds the
existing window and leaves it alone. Untick it in the catalog and it stays hidden even when
the game re-sends.

**Empty until the game speaks.** Before the first update both frontends print **No experience
data yet.** in dim text. Nothing is broken — the game sends this on login and on change,
not on demand.

**Two rows only appear when the numbers arrive.** **Total exp** and **Ascension exp** stay
blank until something has sent those figures, even with their boxes ticked. Ticking them on
a session that never receives them changes nothing visible.

**The height is capped, deliberately.** The window floors at 3 rows and ceilings at 7 — one
row per field plus borders. Ticking all five fields needs all 7; dragging taller does
nothing, because there is nothing more to draw.

**Leave `Exp bar color` empty on purpose if you're capped.** An unset exp color draws the
filled portion in your theme background instead of a fill color, which reads correctly for a
character with nowhere further to go. Setting a color overrides that.

**A blank color field is not an error.** Both color boxes accept `#rrggbb` or a color name;
an unparseable value is ignored and the previous color stands.

## See also

- [Experience (DragonRealms)](./experience.md) — the other game's experience widget, sharing
  the name and nothing else
- [Mini Vitals](./minivitals.md) — health, mana, stamina and spirit in the same compact style
- [Progress Bars](./progress-bars.md) — standalone bars, including a `mindState` bar of your own
- [Encumbrance](./encumbrance.md) — the other single-purpose bar-plus-label window

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog, `.addwindow`, and the window menu. Hand-editing is for
troubleshooting, not the normal path.

```toml
[[windows]]
name = "gs4_experience"
widget_type = "gs4_experience"
title = "Experience"
row = 0
col = 0
rows = 5
cols = 30
min_rows = 3
max_rows = 7
min_cols = 20
show_border = true
align = "center"
show_level = true
show_mind_bar = true
show_exp_bar = true
show_total_exp = false
show_ascension_exp = false
```

### Widget fields

`widget_type = "gs4_experience"`. The type string is exactly `gs4_experience`; an
unrecognized type does not error, it quietly creates a **text** window instead.

| Field | Type | Default | What it does |
|---|---|---|---|
| `align` | string | `"center"` | `left`, `center` (or `centre`), or `right`. Anything else falls back to left. **Read only when the window is first built** — change it and reload the layout. Neither editor offers it. |
| `show_level` | boolean | `true` | The level line |
| `show_mind_bar` | boolean | `true` | The mind-state bar |
| `show_exp_bar` | boolean | `true` | The next-level bar |
| `show_total_exp` | boolean | `false` | An `Exp: 1,234,567` line, drawn only once that number arrives |
| `show_ascension_exp` | boolean | `false` | An `Ascension: 1,234,567` line, same condition |
| `mind_bar_color` | string | unset (cyan) | Mind bar fill. `#rrggbb` or a color name. Omitted from the file when unset. |
| `exp_bar_color` | string | unset | Exp bar fill. **Unset means the theme background** — the correct look at max level. Omitted from the file when unset. |

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`,
`border_sides`, `show_title`, `title`, `locked` — apply as they do to any window. The TUI
honors `show_border`, `show_title`, and `border_sides` on this widget specifically.

### Catalog preset

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| **Experience** | Character | Experience | 5 rows x 30 cols (floor 3, ceiling 7 rows; floor 20 cols) | GemStone IV only |

The gate hides the row from DragonRealms characters in both the GUI catalog and the
`.addwindow` picker. **An unset game counts as GemStone IV**, so the row appears by default.
The six-argument `.addwindow` form is not gated and builds the window regardless.

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Level, Mind state, Experience bar, Total exp, Ascension exp | Right-click ▸ **Experience** | `.editwindow gs4_experience` |
| Mind bar color, Exp bar color | Right-click ▸ **Experience** | `.editwindow gs4_experience` (**Mind** / **Exp**) |
| Title, title bar, lock | Right-click ▸ **Window** / **Appearance ▸ Title bar** | `.editwindow gs4_experience` |
| Border, accent, background | Right-click ▸ **Appearance ▸ Frame** | `.editwindow gs4_experience` |

There is no `[gs4_experience]` config section — this window has no global settings.
**Word wrap and content alignment are not offered** under **Appearance ▸ Text** for this
widget; the layout is fixed rows, not flowing text.

### Data the client stores but never shows

The feed carries more than the window draws: exact field experience and its cap, experience
remaining until next level, and the Fash'lonae, Lumnis and RPA bonus flags. VellumFE parses
and keeps all of them, and **no frontend renders any of them today**. Only total absorbed
experience and ascension experience have display toggles.

</details>
