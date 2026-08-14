# Widgets

> Your layout is made of windows, and each window is wired to one thing the game
> tells you. This page is the map of what you can wire up.

## What a widget is here

A **window** is a box on your screen. A **widget** is what fills it — and every
widget type is bound to one specific feed coming off the wire. The compass window
draws the exits the game sent with the room. The health bar moves because the game
pushed a number named `health`. A targets window lists creatures because the room's
object line marked them bold.

That binding is the whole idea, and it has a practical consequence: **you do not
configure a widget to go find data — you place the window and the feed arrives.**
A bar with no feed id draws nothing at all, not an error. A window bound to a feed
your game never sends stays empty forever.

So the question this section answers is not "how do I make a health bar work." It
is "which of these thirty-odd things do I want on screen, and where do I click to
get it."

<figure class="shot" data-shot="widgets/widgets-catalog-all-categories">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Windows</b> catalog with every category collapsed, showing the groups — <b>Status</b>, <b>Progress Bars</b>, <b>Countdowns</b>, <b>Active Effects</b>, <b>Entities</b>, <b>Hands</b>, <b>Text Windows</b>, <b>Character</b>, <b>Navigation</b>, <b>Hotbars</b>, <b>Containers</b>, <b>Dialogs</b>, and <b>Other</b> — in the order the client lists them.</figcaption>
</figure>

## The roster

These are the groups the in-app **Windows** catalog uses, in the order it shows
them. Inside each group, rows are sorted alphabetically **by the window's title**,
which is not always the type string you would type — the **Type to add** column is
the string, and it is the one that matters at the command line.

**GS4** marks a widget offered only to GemStone IV characters; **DR** marks
DragonRealms-only. Everything unmarked appears for both.

### Status

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Dashboard | `dashboard` | A grid of many status lights in one window | [Dashboard](./dashboard.md) |
| Bleeding · Dead · Diseased · Hidden · Invisible · Joined · Kneeling · Poisoned · Prone · Sitting · Standing · Stunned · Webbed | `indicator` | One light per condition; the window's **name** is the status id | [Indicators](./indicators.md) |

The thirteen indicators sit in a nested **Indicators** fold inside this group, so
the **Dashboard** row isn't buried under them.

### Progress Bars

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Health · Mana · Stamina · Spirit | `progress` | One bar per vital | [Progress Bars](./progress-bars.md) |
| Concentration **DR** | `progress` | The DragonRealms concentration pool | [Progress Bars](./progress-bars.md) |
| Stance | `progress` | Your current stance, as a bar | [Progress Bars](./progress-bars.md) |
| `minivitals` **GS4** | `minivitals` | All your vitals as one compact strip | [Mini Vitals](./minivitals.md) |

Custom bars are not a catalog row — see [Route 2](#route-2-the-custom-window-menu-gui).

### Countdowns

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| RT | `countdown` | Roundtime, counting down | [Countdowns](./countdowns.md) |
| Cast | `countdown` | Cast time | [Countdowns](./countdowns.md) |
| Stun | `countdown` | Stun time | [Countdowns](./countdowns.md) |
| Pulse **GS4** | `countdown` | Time until the next mana/health pulse | [Countdowns](./countdowns.md) |

### Active Effects

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Active Spells | `active_effects` | Spells currently on you, with time left | [Active Effects](./active-effects.md) |
| Buffs | `active_effects` | Beneficial effects | [Active Effects](./active-effects.md) |
| Debuffs | `active_effects` | Harmful effects | [Active Effects](./active-effects.md) |
| Cooldowns | `active_effects` | Abilities you're waiting on | [Active Effects](./active-effects.md) |

All four are the same widget with a different **Category** setting. One type
string, four presets.

### Entities

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Targets | `targets` | Hostile creatures in the room; click to target | [Targets](./targets.md) |
| Players | `players` | Who else is here, with status tags | [Players](./players.md) |
| Items | `items` | Objects on the ground | [Items](./items.md) |

### Hands

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Left Hand · Right Hand · Spell | `hand` | What you're holding, and what you have prepared | [Hands](./hands.md) |

### Text Windows

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Story | `text` | The main game feed | [Text Windows](./text-windows.md) |
| Thoughts · Speech · Bestiary · Announcements · Loot · Death · Logons · Familiar · Ambients · Bounties · Society Tasks | `text` | One window per stream, so it stops interrupting your story pane | [Text Windows](./text-windows.md) |
| Chat | `tabbedtext` | Several streams as tabs in one window | [Tabbed Text](./tabbed-text.md) |

### Character

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Inventory | `inventory` | What you're carrying | [Inventory](./inventory.md) |
| Containers **GS4** | `containers` | Your whole inventory as one tree, bags and all | [Containers Window](./containers-window.md) |
| Characters | `multiaccount` | Your other logged-in characters, at a glance | *(no page yet)* |
| Reserve **GS4** | `reserve` | What the game is holding aside for you | [Reserve](./reserve.md) |
| Spells | `spells` | Your known-spells snapshot from login | [Spells](./spells.md) |
| Missing Spells | `missingspells` | Watched spells that are *not* currently up | [Missing Spells](./missing-spells.md) |
| Injuries | `injury_doll` *(or `injuries`)* | A body diagram of your wounds and scars | [Injury Display](./injury-doll.md) |
| Encumbrance | `encum` | How weighed down you are | [Encumbrance](./encumbrance.md) |
| Experience **GS4** | `gs4_experience` | Level, mind state, progress to next level | [Experience (GemStone IV)](./gs4-experience.md) |
| Experience **DR** | `experience` | Your skill list and its field experience | [Experience (DragonRealms)](./experience.md) |
| Perceptions **DR** | `perception` | Sorted perception entries | [Perception](./perception.md) |

> Both Experience widgets carry the title **Experience** and both live in
> **Character**, but they are different widgets on opposite game gates. You only
> ever see the one for your game.

### Navigation

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Room | `room` | Room name, description, exits, and what's in it | [Room Window](./room-window.md) |
| Compass | `compass` | The exits, as a rose you can click | [Compass](./compass.md) |
| Map | `map` | A live mini map; click a room to walk there | [Map](./map.md) |

### Hotbars

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Actions | `hotkeybar` | Buttons **you** design, that recolor on your state | [Hotbars](./hotkeybar.md) |
| Quickbar | `quickbar` | Buttons the **game and your scripts** push at you | [Quickbar](./quickbar.md) |

The **Actions** row is the hotbar widget — the title differs from the type string.

### Containers

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| *(one row per bag you've opted in)* | `container` | The contents of one specific container | [Container Windows](./container-windows.md) |

> **`container` and `containers` are different widgets.** Singular = one window per
> bag, listed here. Plural = one window holding everything as a tree, and it lives
> in **Character**, not in this group. See
> [Containers Window](./containers-window.md).

Container rows are not fixed. A bag appears as a row once the game has shown you
inside it this session, and ticking the row is what creates the window. There is
no `container` row in a fresh catalog.

### Dialogs

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Betrayer **GS4** | `betrayer` | Blood points and the Betrayer item list | [Betrayer](./betrayer.md) |

### Other

| Row reads | Type to add | What it's for | Page |
|---|---|---|---|
| Bestiary Browser **GS4** | `bestiaryview` | The bundled creature codex, as browsable pages | *(no page yet)* |

**Both bestiary rows are the same feature wearing two faces**, and both read the
same bundled codex. The **Bestiary** row under *Text Windows* is where `.bestiary`
prints its answers — ask a question, get styled lines. This row is the browsable
version of that codex, with pages instead of a scrollback. Place either, or both.

The stream is the fallback, not the lesser option: with no bestiary window in your
layout, `.bestiary` output lands in your main window instead.

### Not in the catalog at all

Six types exist and work, but no picker offers them. The only way to place one is
to type the full `.addwindow` form.

| Type to add | What it's for | Page |
|---|---|---|
| `command_input` *(or `commandinput`)* | The input line itself. Your layout already has one. | [Utility & Layout Widgets](./utility-widgets.md) |
| `spacer` | Blank filler that reserves space in a layout | [Utility & Layout Widgets](./utility-widgets.md) |
| `container` | One bag's contents — reachable by ticking a bag row, never by a fixed catalog entry | [Containers](./container-windows.md) |
| `performance` | Frame timings and message throughput | [Performance Monitor](../reference/performance.md) |
| `webui` *(or `lichui`)* | A Lich WebUI page drawn as a native window | [Utility & Layout Widgets](./utility-widgets.md) |
| `dialogpanel` *(or `dialog_panel`)* | A resident game dialog, addressed by its id | [Utility & Layout Widgets](./utility-widgets.md) |

## How to add one

There are four routes, and they are not interchangeable — one of them isn't
something you do at all.

### Route 1: the Windows catalog (GUI)

Click **Windows** in the top toolbar. The catalog stays open while you work.
**Every category is collapsed when it opens**, so expand the group first, then tick
the row. Ticking shows the window; unticking hides it. Each row also has a **zone**
dropdown that places the window without your dragging it.

This is the route to teach a new player. It never asks for a type string, so none
of the title-versus-type traps below can bite.

### Route 2: the Custom window menu (GUI)

Six of the widgets are blanks rather than presets — a bar with no feed, a text
window with no stream. Those are not catalog rows. They live under **➕ Custom
window…** at the top of the catalog, labelled **Text**, **Tabbed text**,
**Progress bar**, **Countdown**, **Entity list**, and **Active effects**.

Creating one **auto-opens its settings menu**, because a custom bar or timer with
no feed id renders as nothing until you give it one.

### Route 3: `.addwindow` (works everywhere, including the GUI input)

`.addwindow` takes **one argument or six or more — never two through five.**

- `.addwindow` with **no arguments** opens a picker, grouped by the same
  categories.
- `.addwindow <name> <type> <x> <y> <width> [height]` creates a window directly.

> ⚠️ **`.addwindow players` on its own adds nothing.** Two-to-five arguments prints
> a usage line and stops. This surprises people constantly, because that middle
> form looks like the obvious one.

The `<name>` is a free label of your choosing — it does **not** have to be a
catalog key, and the window is built from `<type>`, not from a preset. The
exceptions where the name is load-bearing are called out below.

### Route 4: the game does it for you

Some windows are not something you add. When the game first sends the feed, the
client creates the window itself and binds it. **GemStone IV's Experience window
works this way** — the first time the game opens its experience dialog, the window
appears. Re-sends never duplicate it.

The same discovery machinery is why a stream you've never seen before can turn up
as an offered row later in the session.

## The rules that apply to every widget

These hold for every type in the roster, so the individual pages don't repeat them.

> ⚠️ **An unrecognized type string does not error — it silently gives you a
> `text` window.** Type `.addwindow x activeeffects 0 0 30 10` and you get a blank
> text box, not a complaint. The type strings are exact: `active_effects` (never
> `activeeffects`), `missingspells` (never `missing_spells`), `encum` (never
> `encumbrance`). If a new window came up empty and boring, check your spelling
> before you check the feed.

> ⚠️ **Catalog rows are labelled by the window's TITLE, and several titles differ
> from the type string.** The row that reads **Encumbrance** is type `encum`. The
> row that reads **Actions** is type `hotkeybar`. **Story** is `text`, **RT** is
> `countdown`, **Injuries** is `injury_doll`. Clicking the row is always safe;
> typing what the row says is not.

> ⚠️ **Right-click is always the WINDOW menu.** There is no per-row context menu
> anywhere — not on a target, not on an item, not on an inventory line.
> **Left-click** a row to act on that thing; right-click to configure the window
> holding it.

**A few types accept two spellings.** `injury_doll` and `injuries` are the same
widget; so are `command_input` and `commandinput`, `webui` and `lichui`,
`dialogpanel` and `dialog_panel`. Most types accept exactly one spelling, so don't
generalize from these four.

**Not every widget has a settings section.** The GUI's right-click menu grows a
widget-specific section only for types that have their own settings. Seventeen do:
**Countdown, Progress, Active Effects, Room, Targets, GS4 Experience,
Encumbrance, MiniVitals, Tabbed Text, Hotkeybar, Indicator, Map, Injury Doll,
Compass, Hand, Dashboard, Characters**. Every other type — including Inventory,
Reserve, Container, Containers, Bestiary Browser, Players, Items, Spells, Missing
Spells, Quickbar, Perception, Betrayer, Spacer, and the DragonRealms Experience
widget — has **no widget section**, and
that is not a bug. Their appearance and framing still live under **Appearance**;
they have nothing else to configure.

**A window's name is usually a label, but twice it is a binding.** An `indicator`
window's name **is** the status id it watches — there is no picker. A `hotkeybar`
window shows the bar whose name matches the window's name. For a `countdown`, the
name works as a fallback feed id, which is why `.addwindow roundtime countdown …`
works with nothing else set. **A `progress` window has no such fallback** —
naming one `health` does nothing at all; it needs the **Bar id** set.

**Feed ids are case-sensitive**, and a widget bound to nothing draws nothing.

**Game gating hides rows; it does not block the command.** Both pickers filter by
your game, but the six-argument `.addwindow` form does not check. You can force a
GS4-only window onto a DragonRealms character and it will build, sit there, and
stay empty. **An unset game counts as GemStone IV**, which is why GS4 rows show up
when you connect through Lich without naming a game.

**Placing, moving, and removing works the same for all of them.** Drag to move,
drag an edge to resize, `.hidewindow` to hide, `.deletewindow` to truly remove
(deleted windows are stashed and restorable from **↩ Restore deleted…**). None of
that varies by widget type.

## See also

- [Creating Layouts](../customization/layouts.md) — placing, saving, and loading arrangements of these windows
- [Build a hunting layout](../how-to/hunting-layout.md) — a worked example that picks eight of them and puts them somewhere
- [Command Reference](../reference/commands.md) — `.addwindow`, `.hidewindow`, `.deletewindow`, and the rest
- [layout.toml](../configuration/layout-toml.md) — the file all of this writes

<details>
<summary>Config reference (TOML)</summary>

Every window in your layout is one `[[windows]]` block in `layout.toml`. The
catalog and `.addwindow` write these for you; hand-editing is for troubleshooting.

These keys are shared by every widget type. Type-specific keys are documented in
each widget's own page.

| Field | Type | Default | What it does |
|---|---|---|---|
| `name` | string | — | Unique id. Load-bearing for `indicator`, `hotkeybar`, and `countdown`. |
| `widget_type` | string | `"text"` | Which widget fills the window. Unknown values fall back to `text` silently. |
| `row` | integer | `0` | Top edge |
| `col` | integer | `0` | Left edge |
| `rows` | integer | `10` | Height |
| `cols` | integer | `40` | Width |
| `title` | string | per-type | Border title, and the label the GUI catalog sorts and displays by |
| `show_border` | boolean | `true` | Draw the frame |
| `border_style` | string | `"single"` | `single`, `double`, `rounded`, `thick`, `quadrant_inside`, `quadrant_outside` |
| `border_color` | string | theme | Hex color, or `-` to take the theme's |
| `locked` | boolean | `false` | Refuse move and resize |

```toml
[[windows]]
name = "targets"
widget_type = "targets"
title = "Targets"
row = 0
col = 92
rows = 10
cols = 28
show_border = true
```

</details>
