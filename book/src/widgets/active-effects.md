# Active Effects

> Watch your buffs, debuffs, cooldowns and active spells sitting on screen with their
> remaining time — so a lapsing 401 is something you see coming, not something you find out
> about the hard way.

## What it's for

Every buff you're running has a clock on it, and the game only tells you about that clock when
you ask. Between asks you're guessing. That's fine right up until Spirit Warding drops in the
middle of a swarm, or you burn a prep re-casting something that had four minutes left.

An active effects window puts the game's own effect dialog on your screen and keeps it there.
Each effect draws as one row: the name on the left, the time on the right, and a bar behind
both that fills to show how much of the effect's life is left. You get the same information the
`SPELL` and `SPELLUP` checks give you, without spending a command on it mid-fight.

The same data feeds more than the window. The condition vocabulary that drives hotbar buttons,
indicator icons, and hand icons reads these exact effect feeds — so a button that dims when a
buff lapses and this window are two views of one truth. Wire both and the window tells you
*what*, while the button tells you *act now*.

## Set it up

Effects arrive in four separate dialogs, and **one window shows exactly one of them**. There is
no combined view — three categories on screen means three windows.

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**1. Add the windows you want.**

Click **Windows** in the top toolbar and expand **Active Effects**. Four rows sit there:
**buffs**, **debuffs**, **cooldowns**, and **active_spells**. Tick each one you want. Every row
arrives 10 rows by 30 columns, titled after its category.

**Categories start collapsed** in the catalog, so expand the heading before deciding a row is
missing.

**2. Change what a window shows.**

Right-click the window. The widget section holds a **Category** combo listing **ActiveSpells**,
**Buffs**, **Debuffs**, and **Cooldowns**. Pick one and the window switches feeds immediately.
Settings apply live — there is no Save button.

<figure class="shot" data-shot="widgets/active-effects-gui-category-combo">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A Buffs window right-clicked, its widget section open on the <b>Category</b> combo showing <b>ActiveSpells</b>, <b>Buffs</b>, <b>Debuffs</b>, and <b>Cooldowns</b>.</figcaption>
</figure>

You can also build one from **➕ Custom window… ▸ Active effects**, which creates the window
with **no category set**. It renders empty until you pick one from that combo.

→ **Expected result:** cast a buff and a row appears in the Buffs window — the spell's name,
its duration on the right, and a filled bar behind the row.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**1. Place the window with its full geometry.**

```text
.addwindow buffs active_effects 0 0 30 8
```

**`.addwindow` takes one argument or six or more — never two through five.** `.addwindow buffs`
on its own prints the usage line and creates nothing. Run it bare for a picker instead.

> ⚠️ **The type string is `active_effects`, with the underscore.** `activeeffects` is not
> accepted, and an unrecognized type does not error — it silently creates a **text** window
> instead. A window that draws but stays stubbornly blank is usually this typo.

**2. Set the category.**

Type `.editwindow buffs` to open the window form and move to the **Category:** field. `Tab`
moves between fields, `Ctrl+S` saves, `Esc` cancels.

> ⚠️ **The TUI Category field is free text, not a dropdown, and it is spelled exactly.**
> The four accepted values are `Buffs`, `Debuffs`, `Cooldowns`, and `ActiveSpells` — one word,
> capital A and S, no space and no underscore. A typo matches no feed and the window stays
> empty; leaving it blank falls back to `ActiveSpells` rather than erroring. The GUI's combo
> makes this mistake impossible, which is the one real advantage it has here.

Each row draws the effect name, a right-aligned time in square brackets, and a background fill
across the row proportional to the effect's remaining fraction.

→ **Expected result:** after `Ctrl+S`, your running buffs list in the window as
`Spirit Warding I        [12:34]`, each row backfilled to show how much is left.
{{#endtab}}
{{#tab name="Mobile"}}

The phone's chrome is fixed, so you don't place effect windows — but effects are one of the
better-served surfaces there, and the phone does something the desktop does not.

Open the **status drawer** (the right drawer). Below the room, injury, and hands sections it
carries one section per category, headed **Active Spells**, **Buffs**, **Debuffs**, and
**Cooldowns**, each listing effect names with their remaining time. Only categories with
something in them appear.

For a denser read, tap the **✦** or **⚠** pill in the status row to open the **Effects** sheet.
That view adds the fill bar per effect and sorts by expiry, so the thing about to lapse is at
the top. The pill itself turns urgent as the soonest effect runs down — a warning tint under two
minutes, a critical tint under thirty seconds.

**On the phone these times genuinely tick.** The desktop redraws only when the game re-sends
the dialog; the phone converts each duration to a local deadline on arrival and counts it down
every second.

You can hide the pills under the gear ⚙ ▸ **Appearance** ▸ **Effect pills: hidden**. That
removes the chips and with them the only tap route to the Effects sheet — the drawer sections
stay put either way.

→ **Expected result:** the status drawer lists your buffs under a **Buffs** heading with times
that decrease each second, and the ✦ pill reddens as your shortest buff nears its end.
{{#endtab}}
{{#endtabs}}

## Common setups

### The three-window caster stack

One category per window means the layout does the sorting for you — you learn where to look
rather than reading labels.

1. From the **Windows** catalog, tick **active_spells**, **buffs**, and **cooldowns**.
2. Set all three rows' **zone** control to **Right bar**, so they stack in reading order.
3. Right-click **cooldowns** and shrink it — cooldown lists are short, and the space is better
   spent on the spell list above it.
4. Save with `.savelayout hunting` (typed — there is no GUI save button).

**You'll see:** a right-hand column where your running spells sit on top, buffs beneath them,
and cooldowns in a short block at the bottom. Nothing overlaps, and a spell dropping off the top
list is a change in a place your eye already knows.

### Colored spell families so you read shape, not text

Effect rows can carry per-spell colors, which turns the window into something you parse by
color block instead of by reading every line.

1. Type `.spellcolors` to open the **Colors** window on its **Spell Colors** tab.
2. Click **Add spell color range** and enter spell numbers into **Spell IDs**, comma-separated
   — `101, 102, 103, 107, 120` for the Minor Spirit circle.
3. Pick a bar color and a text color, then save.

VellumFE ships eleven of these already: Minor Spirit in Bondi blue, Wizard in dark orange, Bard
in hot pink, Sorcerer in dark red, and so on.

**You'll see:** your Active Spells window in bands of color, where a missing blue row reads as
"my Minor Spirit set has a gap" from across the desk.

> ⚠️ **Spell colors are matched by spell *number*, not name.** The lookup takes the effect's id
> as a number, so it only ever colors effects the game identifies numerically. **The first
> matching entry wins**, so if a number appears in two color entries, the one higher in the list
> is the one you get.

## Tips & gotchas

> ⚠️ **The displayed time does not tick down on the desktop.** Both desktop frontends draw the
> duration string the game sent, and the game only re-sends an effect dialog when something
> changes. A row can read `[02:14]` for a while and then jump. **The phone is the exception** —
> it converts durations to local deadlines and counts down every second. Treat the desktop
> number as "last known," and treat *presence* rather than the exact figure as the reliable
> signal.

> ⚠️ **Changing a window's Category wipes its contents.** Old-category effects are meaningless
> under a new feed, so switching a window from Buffs to Debuffs clears every row immediately.
> The window looks broken for a moment. It repopulates the next time the game sends that
> dialog — cast something, or run a `SPELL` check, and the rows return.

**The bar tracks the effect's own percentage, not the clock.** The fill comes from a value the
game sends with each effect, so it moves when the game updates the dialog rather than draining
smoothly. Read the bar as "roughly how much of this is left," and the number as the figure.

**Effects don't disappear on their own.** Nothing sweeps expired rows — an effect leaves the
window only when the game sends a fresh dialog for that category. An effect showing `[00:00]`
has almost certainly lapsed; the window is waiting for the game to say so.

**Rows appear in arrival order and are never sorted.** A new effect lands at the bottom and
holds that spot for as long as it runs, so positions stay stable while you're hunting. If you
want expiry-ordered effects, the phone's **Effects** sheet is the one surface that sorts.

**The time format differs between the desktop frontends.** The terminal brackets it and drops
seconds past the hour — `[12:34]` under an hour, `[03:06]` for three hours six minutes. The
GUI prints the game's raw string unbracketed, `03:06:54`. Same data, two presentations; an
effect the terminal shows as `[??:??]` is one whose duration isn't a clock value at all, such
as an indefinite blessing.

**A narrow window sacrifices the name before the time.** Squeeze the terminal window far enough
and the effect name truncates without an ellipsis; squeeze further and the name vanishes so the
time survives. The GUI clips the name the same way but gives it back on hover, showing
`Spirit Warding I - 03:06:54` as a tooltip.

**These feeds are the condition vocabulary.** **Effect active**, **Effect inactive**, and
**Effect time remaining** conditions each take a category, a name, and a match mode (**Exact**
or **Contains**), and drive hotbar button states, indicator icons, and hand icons. **Effect time
remaining** additionally compares against a threshold in seconds — which is how you build a
button that lights up when a buff has under thirty seconds left. Those conditions compute
remaining time live from an absolute expiry, so **they are accurate even while the window's
displayed number is stale.**

> ⚠️ **Conditions can disagree with the window, and the condition is right.** An effect past
> its expiry still draws in the window until the game clears it, but **Effect active** already
> reports it inactive. That's not a bug in either place — the condition does the arithmetic the
> renderer doesn't.

**Only the GUI can author conditions.** The terminal's hotbar editor round-trips existing states
untouched and shows `"{N} state(s) defined - edit in the GUI editor or hotbars.toml"`. Mobile
has no hotbars at all — the phone has macros, a separate system with no conditions.

## See also

- [Spells](./spells.md) — the full spell list, and `.spellwatch` for spotting what's *missing*
- [Missing Spells](./missing-spells.md) — watched spells that aren't currently running
- [Countdowns](./countdowns.md) — roundtime and cast time, the other clocks on your screen
- [Hotbars](./hotkeybar.md) — buttons whose states read these same effect feeds
- [Wire a hotbar for combat](../how-to/combat-hotbar.md) — building an **Effect time remaining**
  button next to an effects window
- [Make your health bar shout when you're hurt](../how-to/vitals-flash.md) — the same condition
  vocabulary applied to health
- [Indicators](./indicators.md) — icons driven by the same conditions
- [Color Palette](../customization/colors.md) — the spell color table behind `.spellcolors`

<details>
<summary>Config reference (TOML)</summary>

Written by the editors above. Hand-editing is for troubleshooting, not the normal path.

```toml
[[windows]]
name = "buffs"
widget_type = "active_effects"
category = "Buffs"
row = 0
col = 0
rows = 10
cols = 30
show_border = true
title = "Buffs"
```

**Widget fields** (`ActiveEffectsWidgetData`):

| Field | Type | Default | What it does |
|---|---|---|---|
| `category` | string | *(required)* | Which effect dialog this window shows. Exactly one of `Buffs`, `Debuffs`, `Cooldowns`, `ActiveSpells`. **Case- and spelling-exact**; an empty or unrecognized value renders nothing. |

`widget_type` must carry the underscore: `active_effects`. An unrecognized widget type falls
back to a `text` window rather than failing to load.

**Catalog presets.** All four ship at 10 rows by 30 columns under the **Active Effects** heading:

| Catalog row | `category` | Title |
|---|---|---|
| `buffs` | `Buffs` | Buffs |
| `debuffs` | `Debuffs` | Debuffs |
| `cooldowns` | `Cooldowns` | Cooldowns |
| `active_spells` | `ActiveSpells` | Active Spells |

A fifth seed, `active_effects_custom`, is not a catalog row — it is the **➕ Custom window… ▸
Active effects** creation flow, and it starts with an empty category.

**Where the data comes from.** The game pushes effects as dialog updates, one dialog per
category — `Active Spells` (with a space on the wire), `Buffs`, `Debuffs`, `Cooldowns`. Each
effect carries an id, a display name, a percentage, and a duration string. An effect missing any
of those is dropped rather than half-drawn. A dialog marked as a clear empties that category
outright; every other update adds or amends rows without removing any.

**Spell colors** (`colors.toml`). Matched on the effect's id as a number, first entry wins:

```toml
[[spell_colors]]
spells = [101, 102, 103, 104, 105, 107, 112, 115, 117, 120, 140]
color = "#0086b3"        # bar color; `bar_color` is the current name
text_color = "#909090"
```

| Field | Type | Default | What it does |
|---|---|---|---|
| `spells` | array of integer | `[]` | Spell numbers this entry colors. An explicit list, not a range. |
| `bar_color` | string | *(falls back to `color`)* | Fill color for the effect's bar |
| `color` | string | — | Legacy name for `bar_color`, still read |
| `text_color` | string | *(theme text)* | Color of the effect name |
| `bg_color` | string | — | Parsed and stored, but **not applied** to effect rows |

A character-level `spell_colors` list **replaces the global list wholesale** when it is
non-empty, rather than merging entry by entry.

**Conditions reading these feeds** (`hotbars.toml`, indicator and hand states):

```toml
[[bars.combat.buttons.states]]
when = { type = "effect_time", category = "Buffs", name = "Spirit Warding I", name_match = "exact", cmp = "<", seconds = 30 }
text_color = "#ff5555"
```

| Condition | Fields | True when |
|---|---|---|
| `effect_active` | `category`, `name`, `name_match` | The effect is present and unexpired. Effects with no parseable duration count as active while present. |
| `effect_inactive` | `category`, `name`, `name_match` | The effect is absent or past its expiry |
| `effect_time` | `category`, `name`, `name_match`, `cmp`, `seconds` | Remaining seconds compare true. **False when the effect is absent or has no parseable expiry.** |

`category` is one of `buffs`, `debuffs`, `cooldowns`, `active_spells`. `name_match` is `exact`
(default) or `contains`. `cmp` is `<`, `<=`, `>`, or `>=`.

**State ordering: the first matching state wins.** A broad condition listed above a narrow one
swallows it.

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`, `title`,
`locked` — apply as they do to any window.

</details>
