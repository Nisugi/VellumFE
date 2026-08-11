# Hotbars

> Buttons you designed yourself — that dim during roundtime, turn green when you're hidden,
> count down a cooldown, and answer to a hotkey.

## What it's for

You have a handful of commands you fire constantly, and a few you fire only at particular
moments. Typing them is fine until the moment matters, and then you're typing `hide` at a
character who is already hidden, or `incant 909` with six mana.

A hotbar is a row of buttons you author. Each sends a command on click or on a key. What makes
it more than a row of shortcuts is that a button can **watch your state and restyle itself** —
recolor, relabel, dim, swap its icon, swap the command it sends, or paint a countdown across
its face. The button stops being a thing you aim at and becomes a thing you read.

**One line on the difference:** a [quickbar](./quickbar.md) shows the buttons the game and your
scripts push at you; a hotbar shows the buttons *you* built, and only a hotbar reacts to your
state. The full comparison table lives on the [quickbar page](./quickbar.md).

## Set it up

Two pieces have to exist: a **bar** (the buttons, in `hotbars.toml`) and a **window** to show it
in. They are joined by name, and that is the single rule to remember.

> ⚠️ **A hotkeybar window displays the bar with the same name as the window.** There is no
> bar-picker anywhere in the interface. A window named `combat` shows the bar named `combat`;
> rename either and the pairing breaks. `.addwindow combat hotkeybar 0 0 40 3` builds a window
> already bound to a bar called `combat`.

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Type `.addwindow combat hotkeybar 0 0 40 3` in the command input. The name you choose is the
   bar it will look for.
   (The **Windows** toolbar catalog also carries an **Actions** row under **Hotbars**, but it
   creates a window named `hotkeybar` bound to a bar of that name — use `.addwindow` when you
   want to pick the name.)
2. Open the **Editors** hub in the toolbar and click **Hotbars**, or type `.hotbars`. Add a bar
   named `combat`. Leave **Global (all characters)** ticked for a bar every character gets.
3. Click **Add button** and fill in **Label** and **Command**. Add a **Hotkey** by typing it or
   clicking **Capture** and pressing the key.
4. Under **States**, click **Add state** to make the button react to your situation. The kind
   combo carries thirteen leaves — **Effect active**, **Roundtime active**, **Indicator**,
   **Vital**, **Injury**, **Hand holds** among them.
5. Watch the **Preview:** row above the button list. It renders the bar against your real game
   state using the same code the window uses. Click **Save bar**.

Right-click the window ▸ the **Hotbar** section ▸ **Edit hotbars…** reopens the same editor.

<figure class="shot" data-shot="widgets/hotkeybar-gui-editor-states">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The hotbar editor: the bar list with <b>[G]</b> and <b>[C]</b> scope badges, the button form, and a button's <b>States</b> list showing <i>(first matching state styles the button)</i> above two ordered state cards.</figcaption>
</figure>

→ **Expected result:** a bar of buttons on screen. Clicking one sends its command; step into
roundtime and any button carrying a roundtime state dims and counts down.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow combat hotkeybar 0 0 40 3` — name, type, then column, row, width, height. The
name is the bar the window will display.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow hotkeybar`
> alone prints usage and adds nothing. Run `.addwindow` with no arguments for a picker;
> the **Actions** row is under **Hotbars**.

Type `.hotbars` to open the editor. It builds bars and buttons and covers **label**,
**command**, **hotkey** (validated against your existing binds), **tooltip**, **category**, and
**countdown source** — the whole of an ordinary bar.

> ⚠️ **Condition states cannot be authored here — but they display here perfectly.** A button
> carrying states shows the line `2 state(s) defined - edit in the GUI editor or hotbars.toml`
> in place of a state form, and **saving from the terminal round-trips them untouched**, so a
> terminal edit never damages GUI-authored conditions. Your terminal then *renders* every
> state — the recolor, the replacement label, the dimming, the countdown — off the same
> evaluation the desktop runs. **This is an authoring gap, not a display gap.**

`.editwindow combat` opens the window's form for borders and placement; this widget contributes
no fields of its own to it.

→ **Expected result:** a bordered strip of buttons. Pressing a button's hotkey sends its
command, and buttons with states recolor and dim as your situation changes.
{{#endtab}}
{{#tab name="Mobile"}}

**The phone has no hotbars at all** — not a window, not a drawer section, not a wheel slice. It
has **macros**, which cover the same reach-for-a-button need and are a genuinely different
system.

Open the **macro tray** (the left drawer) for your macro buttons. Tap one to fire it,
long-press to edit it, and use **＋ New button…** to create one with a label, a command, a
color, and a tap mode. Macros are stored separately from hotbars and are edited on the phone.

What a macro cannot do is react: it has no conditions, so it never dims during roundtime, never
recolors when you're hidden, and never counts down. Build your conditional bar on desktop and
keep the macro tray for the commands you fire while away from your PC.

→ **Expected result:** the **macro tray** opens from the left drawer with your macro buttons;
tapping one sends its command in a color that never changes.
{{#endtab}}
{{#endtabs}}

## Common setups

### A button that tells you your own state

The payoff of a state is that a button can stop offering an action you're already in.

1. Open `.hotbars`, add a button with **Label** `Hide` and **Command** `hide`.
2. Add a state, choose **Indicator** on its condition row, set the id to `hidden` and leave
   **active** ticked.
3. Under **Style while active:**, set **Label** to `Hidden` and the foreground to `#80ff80`.
4. Add a second state below it for **Roundtime active** with dim ticked. **Save bar**.

**You'll see:** a plain **Hide** button in the open, dimmed while roundtime runs, and a green
button reading **Hidden** the moment you're in the shadows — with hidden winning over roundtime,
because it sits above it.

### A cooldown you can see from across the room

Give a button a **Countdown overlay** of **Roundtime**, **Casttime**, or an **Effect** by
category and name. The remaining seconds paint on the button's face and vanish when it reaches
zero.

Pair it with a state on the same condition and set that state's dim: the button dims *and*
shows the number, so it reads as unavailable and tells you how long for.

**You'll see:** an **Attack** button that dims and reads `3` at the top of a roundtime, ticking
down to nothing as you become able to swing again.

## Tips & gotchas

> ⚠️ **The first matching state wins.** States are checked top to bottom and the first hit
> styles the button; the rest never run. A broad condition above a narrow one swallows it — put
> **Injury >= 3** above **Injury >= 1**, and `health < 25%` above `health < 50%`. Reorder with
> each card's `^` and `v`. When nothing matches, the button falls back to its **default style**,
> then to the window's and theme's colors.

> ⚠️ **Hotkeys belong to the BAR, not to the window.** Every hotkey in every loaded bar
> registers when your config loads, whether or not any window is showing that bar. A bar you
> stopped displaying still owns its keys. These live only in the running keybind map — they are
> never written to `keybinds.toml`, and the keybind editor cannot see them.

> ⚠️ **An existing keybind always beats a hotbar button.** On a clash the button loses silently
> at runtime; the editor warns you at the time and marks the row `(key conflict)`. Two buttons
> claiming one key behave the same way — the first bar in the file wins.

> ⚠️ **A per-character bar REPLACES a global bar of the same name, wholesale.** It is not a
> per-button merge: the character copy is the entire bar. The editor's `[G]` and `[C]` badges
> tell you which copies exist. Delete the `[C]` one to fall back to global.

**The window's name is the binding, and there is no picker to change it.** If a hotkeybar window
renders empty, the overwhelmingly likely cause is that no bar carries the window's name. This is
the same rule [indicator windows](./indicators.md) follow.

**Icons are GUI-only; the terminal always draws the label.** A button's **Face** setting (Text /
Icon / Icon + label), its icon art, grayscale, and border effects render in the desktop GUI
only. Give every button a label worth reading and one bar serves both frontends.

**Conditions are one vocabulary, learned once.** The same thirteen leaves drive hotbar states,
[indicator icons](./indicators.md), and [hand icons](./hands.md). **Indicator** ids are
`standing`, `kneeling`, `sitting`, `prone`, `stunned`, `bleeding`, `hidden`, `invisible`,
`webbed`, `joined`, and `dead`. **Injury** levels run 1-3 for wounds and 4-6 for scars, with 0
healthy. **Vital** takes a `%` or an `abs` unit — use `abs` for spell costs, because a spell
costs a number of mana and not a fraction of your pool.

**Editors build one level of nesting.** **all of** / **any of** groups nest once in the GUI
builder. Deeper trees written by hand still evaluate correctly and render in the editor as
*(nested group - edit in hotbars.toml)*.

**A state's command override is literal text.** Whatever you type in **Command while active:**
is sent as-is instead of the button's command while that state matches. Anything dynamic belongs
inside the command — a `;eq …` line that Lich intercepts and evaluates.

**Conditions that can't be answered fail closed.** **Spell affordable** returns false for spell
numbers it doesn't know and for formula-cost spells, and hand item-type tests return false when
the item classifier is unavailable. A button grays out rather than lying to you.

## See also

- [Wire a hotbar for combat](../how-to/combat-hotbar.md) — the full authoring walkthrough, step
  by step, in all three frontends
- [Quickbar](./quickbar.md) — the game's own command bars, and the comparison table
- [Indicators](./indicators.md) — the same condition vocabulary lighting status icons
- [Hands](./hands.md) — the same conditions on hand icons
- [Countdowns](./countdowns.md) — roundtime and casttime as their own windows
- [Active Effects](./active-effects.md) — the effect categories conditions read from
- [Keybind Actions](../customization/keybinds.md) — the key syntax hotkeys use
- [Skins (GUI Graphics)](../customization/skins.md) — where button icon art comes from

<details>
<summary>Config reference (TOML)</summary>

Two files are involved. The **window** lives in your layout; the **bar** lives in
`hotbars.toml`. Use `.hotbars` and the catalog rather than editing either by hand.

### The window

```toml
[[windows]]
name = "combat"
widget_type = "hotkeybar"
row = 38
col = 0
rows = 3
cols = 60
bar = "combat"
orientation = "horizontal"
```

| Field | Type | Default | What it does |
|---|---|---|---|
| `bar` | string | `"default"` | Bar name from `hotbars.toml`. **A window created by `.addwindow` or the catalog sets this to the window's own name**, and neither window editor exposes the field — treat the window name as the binding |
| `orientation` | string | `"horizontal"` | `horizontal` flows buttons on one row; `vertical` puts one per row. **Layout-file only** — no editor in either frontend authors it |

Preset: 3 rows x 60 cols, pinned at 3 rows, border on, title bar off, title **Actions**.
Catalog category **Hotbars**, ungated — offered to GemStone IV and DragonRealms alike. Minimum
size 20 cols x 1 row.

**Widget section (GUI right-click):** **Hotbar** — one entry, **Edit hotbars…**. **TUI
`.editwindow`:** no widget fields at all. Everything about the buttons is in `.hotbars`.

### The bar — `hotbars.toml`

Global: `~/.vellum-fe/global/hotbars.toml`. Per character:
`~/.vellum-fe/profiles/<Character>/hotbars.toml`. A character bar with the same name replaces
the global bar wholesale; character-only names append.

```toml
[[bars]]
name = "combat"
title = "Combat"
icon_size = 32

[[bars.buttons]]
id = "hide"
label = "Hide"
command = "hide"
hotkey = "alt+h"
tooltip = "Attempt to hide"
category = "Stealth"

[bars.buttons.countdown]
source = "effect"
category = "Cooldowns"
name = "Shadow Mastery"
name_match = "contains"

[[bars.buttons.states]]
[bars.buttons.states.when]
type = "indicator"
id = "hidden"
active = true
[bars.buttons.states.style]
label = "Hidden"
fg = "#80ff80"

[bars.buttons.default_style]
fg = "#d0d0d0"
```

**Bar fields**

| Field | Type | Default | What it does |
|---|---|---|---|
| `name` | string | — | The bar's id. **A window of this name displays this bar** |
| `title` | string | none | Display name in the editor |
| `icon_size` | integer | none | Icon face edge in pixels, GUI only. Unset matches text-button height |
| `buttons` | array | `[]` | The buttons, in display order |

**Button fields**

| Field | Type | Default | What it does |
|---|---|---|---|
| `id` | string | — | Stable id, unique within the bar; editor bookkeeping |
| `label` | string | — | Button text |
| `command` | string | — | Sent on click or hotkey |
| `hotkey` | string | none | `keybinds.toml` key syntax (`alt+h`, `f5`). Registers for every loaded bar; existing binds win |
| `tooltip` | string | none | GUI hover text |
| `category` | string | none | Editor grouping only; no runtime effect |
| `countdown` | table | none | Countdown overlay source |
| `states` | array | `[]` | Ordered condition rules; **first match wins** |
| `default_style` | table | none | Appearance when no state matches |
| `icon` | table | none | Base icon, GUI only |
| `icon_mode` | string | `"text"` | `text`, `icon`, or `icon_and_label`. GUI only |

**Countdown source** (`source` is required)

| `source` | Extra fields | Shows |
|---|---|---|
| `effect` | `category`, `name`, `name_match` | Seconds until that effect expires |
| `roundtime` | — | Seconds of roundtime left |
| `casttime` | — | Seconds of casttime left |

`category` is one of `Buffs`, `Debuffs`, `Cooldowns`, `ActiveSpells`. `name_match` is `exact`
(default) or `contains`; both are case-insensitive. An elapsed or absent source draws no
overlay.

**State fields**

| Field | Type | Default | What it does |
|---|---|---|---|
| `when` | table | — | The condition. `type` names the leaf or `all`/`any` |
| `style` | table | empty | Appearance while this state matches |
| `countdown` | table | none | Replaces the button's countdown while active |
| `command` | string | none | Sent instead of the button's command while active. **Literal text** |

**Style fields** — `label`, `fg`, `bg`, `dim` (bool), `icon`. Unset fields fall through to the
button's `default_style`, then to widget and theme colors.

**Condition leaves** — `effect_active`, `effect_inactive`, `effect_time`, `rt_active`,
`ct_active`, `indicator`, `vital`, `injury`, `spell_affordable`, `hand_empty`, `hand_holds`,
`spell_prepared`, and time of day, plus the `all` and `any` groups. **An unrecognized condition
type is a parse error** — unlike widget type strings, this one fails loudly.

### Commands

| Command | Purpose |
|---|---|
| `.hotbars` / `.hotbar` | Open the hotbar editor |
| `.reload hotbars` | Re-read `hotbars.toml` from disk |

</details>
