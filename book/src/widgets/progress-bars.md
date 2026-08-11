# Progress Bars

> Health, mana, stamina and spirit as bars you read in a glance — so you never type `health`
> with a troll swinging at you.

## What it's for

Mid-fight you need one number: am I still safe? Typing `health` costs you a line of scrollback
and a beat of attention, and the answer is already three exchanges old by the time you read it.
A progress bar keeps that answer on screen permanently, filling and draining as the game sends
updates, so checking your health becomes a glance instead of a command.

Each bar is its own window, which is the point — you place health where your eye already goes,
put spirit somewhere quieter, and skip the ones you don't care about. Bars are not limited to
vitals either: the same widget draws stance, encumbrance level, mind state, and any bar a Lich
script invents, because a bar is really just "a number out of a maximum, drawn as a fill."

If you'd rather have all four vitals stacked in one compact window, that is a different widget
— see [Mini Vitals](./minivitals.md).

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar. The catalog stays open while you work.
2. Tick **health**, **mana**, **stamina**, and **spirit**. Each one appears as its own window,
   already bound to the right feed and colored.
3. Set each row's **zone** control to **Header** to line them up across the top, or leave them
   floating and drag them where you want.
4. Right-click any bar to open its menu. The widget section holds **Bar id**, **Label**,
   **Bar color**, **Show value/max**, and **Show value only**. Changes apply live — there is no
   Save button, and text fields commit when you press Enter or click away.

For a bar the catalog doesn't list — a Lich-pushed feed, or `encumlevel` — use
**➕ Custom window… ▸ Progress bar**. VellumFE drops you straight into the new window's menu,
because **a bar with no Bar id draws nothing at all**. Type the feed id there and it comes alive.

<figure class="shot" data-shot="widgets/progress-gui-bar-menu-open">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A health bar right-clicked, with the widget section open showing <b>Bar id</b>, <b>Label</b>, <b>Bar color</b>, and the <b>Show value/max</b> and <b>Show value only</b> checkboxes.</figcaption>
</figure>

→ **Expected result:** four bars sit across the top of your layout, each filling to its current
value and redrawing the moment the game sends a vitals update.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

1. Add each bar with its full placement, one command per bar:

   ```text
   .addwindow health progress 0 0 20 3
   .addwindow mana progress 0 3 20 3
   ```

   **`.addwindow` takes one argument or six or more — never two through five.** Typing
   `.addwindow health` alone prints usage and adds nothing at all. With no arguments it opens
   a picker instead, which is the easier route when you don't want to count columns.
2. Type `.editwindow health` to open the full-screen window form. The progress fields are
   **Progress ID:**, **Text Color**, **Numbers Only**, **Bar Color**, and **Current Only**.
3. Move between fields with `Tab`, save with `Ctrl+S`, back out with `Esc`. The footer shows
   `[Ctrl+S: Save] [Esc: Cancel]` the whole time.

Note that **`.addwindow` builds the window from the `<type>` you give it, not from a catalog
preset** — so `.addwindow health progress …` creates a *blank* progress window named `health`,
and you still set **Progress ID:** to `health` in the editor. The name is a label, not a binding.

→ **Expected result:** the bars render as colored fills with their label drawn over them, and
`Ctrl+S` in the editor writes the change straight into your layout.
{{#endtab}}
{{#tab name="Mobile"}}

The phone uses fixed chrome, so you don't place or size vitals bars there. What you get instead
is a built-in cluster of four bars reading **HP**, **MP**, **ST** and **SP**, each with its
percentage beside the label, rendered in the phone's own UI rather than as windows you arrange.

You can turn the whole cluster off: open the gear ⚙ and untick **Vitals bars** in the chrome
toggles. That is the extent of the control — there is no bar id, no color field, and no way to
add a fifth bar for a custom feed on the phone. The full vitals surface, including the room and
your injuries, lives in the **status drawer** (swipe in from the right).

Author your bar colors, labels, and any custom feeds on the desktop. They are stored per
character, so they are waiting for you next time you play there.

→ **Expected result:** four labelled percentage bars track your vitals in the phone's chrome,
updating on the same feed the desktop bars read.
{{#endtab}}
{{#endtabs}}

## Common setups

### A compact vitals strip that shows numbers, not words

You know which bar is which by position and color, so the word "Health" is wasted space — the
numbers are what you actually want.

1. Add **health**, **mana**, **stamina** and **spirit** from the **Windows** catalog and set
   each row's zone to **Header**.
2. Right-click each bar and tick **Show value/max** in the widget section.
3. Drag each bar narrower until the four sit side by side across the header.

→ Your header reads `142/193` `88/110` `95/95` `10/10` in dark red, dark blue, orange and gray —
four numbers, one row, no labels.

### A stance bar next to your roundtime

Stance is a bar like any other, and it belongs beside the timer that tells you when you can act.

1. In the **Windows** catalog, tick **stance**. It arrives bound to the feed id `pbarStance`,
   titled **Stance**, in navy.
2. Set its zone to match your [roundtime countdown](./countdowns.md) so the two sit together.
3. Right-click it, clear the **Label** field, and the bar draws its percentage instead.

→ Dropping to guarded shows the stance bar visibly shorter, right beside your RT bar, so
"can I act, and how exposed am I?" is one glance at one corner.

## Tips & gotchas

> ⚠️ **A progress window with an empty Bar id renders as nothing.** It is not broken and it is
> not invisible-because-of-color — a bar with no feed has no value to draw. This is why creating
> one from **➕ Custom window…** drops you into its menu immediately. If a bar you made is a blank
> rectangle, that empty field is almost always why.

> ⚠️ **Bar ids are case-sensitive.** `mindState` works; `mindstate` matches nothing and draws
> nothing. The same goes for `encumlevel` and `pbarStance` — copy the capitalization exactly.

> ⚠️ **Unlike countdowns, a progress window's *name* is not a fallback binding.** Naming a window
> `health` does not feed it; only the **Bar id** field does. A countdown window will fall back to
> its name, which is why `.addwindow roundtime countdown …` appears to work with no id set and
> the same trick fails completely for bars.

**Nothing on a bar changes color at a threshold.** There is no low-health red, no flash, no
pulse — a bar draws in one solid color at every value, by design. Alerting on low health is a
job for a hotbar button's condition state, an indicator, a sound, or controller rumble; see
[Make your health bar shout when you're hurt](../how-to/vitals-flash.md), which builds exactly
that.

**`Show value only` wins over `Show value/max`.** With both ticked you get the bare current
number. The precedence is: value-only, then value/max, then your **Label**, then a percentage
when the label is empty.

**Feeds that report a percentage have no maximum.** Stance arrives as 0–100 with no max, so
**Show value/max** on a stance bar reads `100/0`. Leave the numbers off for percent-style feeds
and let the fill do the talking.

**Encumbrance has a dedicated widget too.** `encumlevel` works as a Bar id if a plain bar is what
you want, but the **encum** window shows the level *name* with its own color bands — see
[Encumbrance](./encumbrance.md).

**Colors are per bar, and the defaults are deliberately dark.** Health is `#6e0202`, mana
`#08086d`, stamina `#bd7b00`, spirit `#6e727c`, stance `#000080` — dark enough that the text
drawn over the fill stays readable. If you brighten a bar, check the label is still legible
against it.

## See also

- [Mini Vitals](./minivitals.md) — all four vitals in one window, with bar height, bar text,
  depleted color, and per-bar reordering
- [Countdowns](./countdowns.md) — roundtime and casttime, the other half of a combat corner
- [Encumbrance](./encumbrance.md) — the dedicated encumbrance widget
- [Experience](./experience.md) — the GS4 window that consumes `mindState` with its full detail
- [Make your health bar shout when you're hurt](../how-to/vitals-flash.md) — condition-driven
  low-health alerting, since bars themselves never change color
- [Creating Layouts](../customization/layouts.md) — placing, zoning, and saving windows

<details>
<summary>Config reference (TOML)</summary>

Written by the editors above. Hand-editing is for troubleshooting, not the normal path.

```toml
[[windows]]
name = "health"          # a label — NOT the feed binding
widget_type = "progress"
id = "health"            # the feed binding
row = 0
col = 0
rows = 3
cols = 20
color = "#6e0202"
```

**Widget fields** (`ProgressWidgetData`):

| Field | Type | Default | What it does |
|---|---|---|---|
| `id` | string | *(none)* | Progress feed identifier — the game's `progressBar` id. **Case-sensitive.** With no id the window draws nothing. |
| `label` | string | *(feed text)* | Overrides the label drawn over the fill. Empty label draws a percentage instead. |
| `color` | string | per-id default | Fill color, `#rrggbb` or a color name. One solid color at every value. |
| `numbers_only` | bool | `false` | Draw `current/max` instead of the label. |
| `current_only` | bool | `false` | Draw only the current value — no label, no max. Takes precedence over `numbers_only`. |

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`,
`background_color`, `text_color`, `locked` — apply as they do to any window.

**Known feed ids.** The client does not keep a closed list; it draws whatever `progressBar` ids
arrive. The ones the GUI names are `health`, `mana`, `stamina`, `spirit`, `encumlevel`, and
`mindState`, plus any custom id a Lich script pushes. The catalog additionally ships
`concentration` (DragonRealms only) and `stance`, whose feed id is `pbarStance`.

**Preset defaults:** health `#6e0202`, mana `#08086d`, stamina `#bd7b00`, spirit `#6e727c`,
concentration `#00a0a0`, stance `#000080`. Each preset is 20 columns wide and 3 rows tall.

</details>
