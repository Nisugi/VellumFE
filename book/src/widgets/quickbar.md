# Quickbar

> The game's own row of one-click commands, given a permanent home — look, search, inventory,
> attack, and whatever else the game or a script hands you, without typing any of them.

## What it's for

The game ships a set of clickable command bars: a general one, a combat one, an information one.
They exist whether or not you use them, and most of the time they go to waste because there is
nowhere for them to live.

A quickbar window is that home. It shows one bar at a time as a row of buttons, and clicking one
sends its command. Scripts push their own bars into the same place, which is how a Lich script
gives you buttons without you configuring anything.

**Reach for a quickbar when you want what the game and your scripts already offer. Reach for a
[hotbar](./hotkeybar.md) when you want buttons you designed yourself** — see the comparison
below, because the two look identical and behave nothing alike.

## Quickbar or hotbar?

|  | Quickbar | [Hotbar](./hotkeybar.md) |
|---|---|---|
| Who writes the buttons | **The game and your scripts**, pushed over the wire | **You**, in the `.hotbars` editor |
| Where they're stored | Arrive live; cached per character | `hotbars.toml`, global or per character |
| Change with your state | ❌ never | ✅ recolor, relabel, dim on conditions |
| Countdown overlays | ❌ | ✅ |
| Hotkeys | ❌ | ✅ per button |
| Several bars at once | ✅ one window switches between them | ✅ one window per bar |
| In-app editor | ❌ — custom bars are TOML-only | ✅ `.hotbars` |

In short: the quickbar surfaces what someone else made for you. The hotbar is where you build
your own. Most players eventually run one of each.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Hotbars**, and tick **quickbar**. It arrives 3
   rows by 120 columns with its title bar off, sized to sit as a strip across the top or bottom
   of your layout.
   (Typed equivalent: `.addwindow quickbar quickbar 0 0 120 3`.)
2. Click any button to send its command. A button whose label ends in `...` — **roleplay...**,
   **actions...** — asks the *server* for a menu and shows you the verbs it offers instead of
   firing immediately.
3. **When more than one bar exists, a dropdown appears at the left of the row.** Pick a bar from
   it to switch the window to that bar. With only one bar there is no dropdown, because there is
   nothing to switch to.

<figure class="shot" data-shot="widgets/quickbar-gui-bar-and-switcher">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A <b>quickbar</b> window showing the game's main bar — <b>look</b>, <b>search</b>, <b>inventory</b> — with the bar-switching dropdown open at the left listing <b>main</b>, <b>combat</b> and <b>information</b>.</figcaption>
</figure>

→ **Expected result:** a row of buttons across your layout. Clicking **look** sends `look` and
the room description appears in your main window.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow quickbar quickbar 0 0 120 3` — name, type, then column, row, width, height. Run
`.addwindow` with no arguments for a picker instead; **quickbar** is under **Hotbars**.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow quickbar`
> alone prints usage and adds nothing.

**Click a button to fire it.** That works whatever window has focus.

**The keyboard route needs focus first, and quickbars are excluded from `Tab` by default.**
With the window focused, `Left`/`Right` move the selection and `Enter` fires it. Out of the box
`Tab` skips straight past a quickbar, so to drive it by keyboard you must first remove
`"quickbar"` from `exclude` under `[ui.focus]` in `config.toml`. **Mouse clicking never needs
focus** and is the intended everyday gesture.

**The leftmost item is `>>`, the bar switcher.** Click it — or select it and press `Enter` — for
a popup listing every bar VellumFE knows, with the current one pre-selected. **The terminal
always shows this marker, even with one bar**; the GUI hides its dropdown until there are two.

`.editwindow quickbar` opens the form, but this widget has no settings of its own — only the
standard window fields.

→ **Expected result:** a bordered strip beginning with `>>`, then the game's buttons. Clicking
**inventory** sends `inven` and your items print in the main window.
{{#endtab}}
{{#tab name="Mobile"}}

There is no quickbar surface on the phone — not a drawer section, not a stream chip, not a wheel
slice. The bars the game sends never reach it.

The phone has its own answer to the same need, and it is better suited to a thumb: **macros**.
Open the **macro tray** (the left drawer) for your macro buttons — tap one to fire it,
long-press to edit, and use **＋ New button…** to create one with a label, a command, and a tap
mode. Macros live on the phone, are edited on the phone, and cover the same ground as a quickbar
button, with the difference that you choose what goes on them.

For a one-off command a quickbar button would have sent, the command input is always there.

→ **Expected result:** the **macro tray** opens from the left drawer with your macro buttons;
tapping one sends its command.
{{#endtab}}
{{#endtabs}}

## Common setups

### A command strip along the bottom of your layout

The quickbar's shape — wide and three rows tall, borders trimmed — is meant for one job.

1. Add **quickbar** from the catalog's **Hotbars** group.
2. Drag it to the bottom of your layout, above your command input, and stretch it to the full
   width.
3. Turn its title bar off (it ships off already) so it reads as a strip rather than a window.
4. Use the switcher to park it on **combat** before you hunt.

**You'll see:** one row of buttons under your text, with **attack**, **ambush**, **aim**,
**target** and **fire** a click away — and a switcher that puts the general bar back when you
return to town.

### Your own bar, alongside the game's

Custom quickbars are defined in `config.toml`, and they sit in the same switcher as the game's.
**This is the one part of VellumFE with no in-app editor** — there is no `.quickbars` command.

Add this to your `config.toml`:

```toml
[quickbars]
default = "quick-mine"

[[quickbars.custom]]
id = "quick-mine"
title = "Mine"
entries = [
  { type = "link", label = "loot", command = "loot" },
  { type = "sep" },
  { type = "link", label = "skin", command = "skin" },
  { type = "link", label = "search", command = "search" },
]
```

Restart, and the switcher gains a **Mine** entry — selected on arrival, because `default` names
it.

**You'll see:** your own three buttons in the same strip as the game's, switchable to and from
the combat bar like any other.

> ⚠️ **The `id` must be exactly `quick` or start with `quick-`.** Any other id is skipped
> silently at load with only a warning in the log — no error, no button, no explanation on
> screen. `id = "mine"` gives you nothing; `id = "quick-mine"` works.

## Tips & gotchas

> ⚠️ **A quickbar is not a hotbar and cannot be made into one.** Its buttons never change color,
> never dim during roundtime, never show a countdown, and never take a hotkey. If you want a
> button that reacts to your state, that is a [hotbar](./hotkeybar.md), and it is a different
> widget.

> ⚠️ **The bar switcher looks different in each frontend, and one of them hides.** The terminal
> always shows a `>>` marker at the left of the row and opens a popup list. The GUI shows a
> **dropdown** — but **only when two or more bars exist**. On a session with a single bar, the
> GUI quickbar looks like it has no switcher, because it doesn't need one.

**Two kinds of button, and the difference is what happens on click.** A plain button sends its
command straight to the game. A button ending in `...` is a *menu* button: clicking asks the
server what verbs apply and shows you a menu to pick from. Nothing fires until you choose.

**Empty is a real state and it tells you something.** A quickbar window with no bars reads **No
quickbars configured.** in the GUI. Since the game sends its bars once, at login, this normally
means you attached to an already-running session and missed the burst — see the next point.

**Bars are cached per character, so attaching to a running Lich still gets you buttons.**
VellumFE writes the bars it has seen into that character's session cache and restores them on the
next start. Where the cache is cold and the login burst was missed, it seeds the game's three
standard bars — **main**, **combat** and **information** — so the window is never blank on a
mid-session attach.

**Your active bar is remembered too.** The bar you last switched to is saved alongside the bars
themselves, so the window comes back on the one you were using rather than resetting to the first.

**Custom bars merge with the game's; they don't replace them.** Defining `quick-mine` adds a
switcher entry. Defining `quick` **replaces the game's main bar** with yours, because that id is
already taken.

**Entries missing required pieces are dropped one at a time.** A `link` with no `label` or no
`command`, or a `menulink` missing `exist` or `noun`, is skipped while the rest of the bar builds
normally. A bar that came out short is usually a half-written entry, not a broken file.

**Skip `selection_fg` and `selection_bg` if you see them in an old layout.** Selection colors
come from your theme, not from the window. Those keys do nothing.

## See also

- [Hotbars](./hotkeybar.md) — buttons you author, with conditions, countdowns and hotkeys
- [Wire a hotbar for combat](../how-to/combat-hotbar.md) — the authored equivalent of a combat
  quickbar
- [Text Windows](./text-windows.md) — where the commands you fire print their replies
- [Creating Layouts](../customization/layouts.md) — placing and saving the strip
- [Web & Mobile](../frontends/web.md) — the phone's macro tray, the quickbar's stand-in there

<details>
<summary>Config reference (TOML)</summary>

The window is written by the catalog and `.addwindow`. Custom bars are the exception in this
manual: they have no in-app editor, so `config.toml` is the only route.

### The window

```toml
[[windows]]
name = "quickbar"
widget_type = "quickbar"
row = 0
col = 0
rows = 3
cols = 120
show_border = true
show_title = false
title = "Quickbar"
```

`widget_type = "quickbar"`. The type string is exactly `quickbar`; an unrecognized type does not
error, it quietly creates a **text** window instead.

**Widget fields: none.** The content is entirely the bars the game, your scripts, and
`[quickbars]` supply.

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| `quickbar` | Hotbars | Quickbar | 3 rows x 120 cols, pinned to 3 rows, title bar off | Both |

Ungated — offered to GemStone IV and DragonRealms characters alike.

### Custom bars — `[quickbars]` in `config.toml`

```toml
[quickbars]
default = "quick-mine"       # which bar is active on start; must be a defined id

[[quickbars.custom]]
id = "quick-mine"            # required: exactly "quick", or starts with "quick-"
title = "Mine"               # optional; falls back to the id in the switcher
entries = [
  { type = "link", label = "loot", command = "loot", echo = "loot" },
  { type = "menulink", label = "roleplay...", exist = "qlinkrp", noun = "" },
  { type = "sep" },
]
```

| Field | Type | Default | What it does |
|---|---|---|---|
| `default` | string | none | Bar active on start. Ignored with a log warning when the id is unknown or malformed |
| `custom` | array of tables | `[]` | Bar definitions |
| `custom.id` | string | — | **Must be `quick` or start with `quick-`.** Any other value skips the whole bar with a log warning |
| `custom.title` | string | the id | Switcher label. Blank or whitespace falls back to the id |
| `custom.entries` | array | `[]` | Buttons, in display order |

**Entry types** (`type` is required):

| `type` | Required | Optional | Behavior |
|---|---|---|---|
| `link` | `label`, `command` | `echo` | Sends `command` to the game on click |
| `menulink` | `label`, `exist`, `noun` | — | Requests the server's verb menu for that object |
| `sep` / `separator` | — | — | Visual divider between buttons |

An entry missing a required field is skipped; the rest of the bar still builds.

### Where bars come from at runtime

| Source | Notes |
|---|---|
| The game | Sent once, at login, as `quick…` dialogs |
| Scripts | The same tags; a clear flag replaces the bar's contents, otherwise entries append |
| `[quickbars.custom]` | Applied at startup |
| Seeded fallback | Three standard bars seeded when the cache is cold on a mid-session attach |
| Switching | The game can change your active bar on its own |

**Only ids that are `quick` or begin with `quick-` are treated as quickbars.** Anything else on
the same wire tag is a dialog panel, not a quickbar button.

### Persistence

Bars, their order, and the active id are cached per character in:

```text
~/.vellum-fe/profiles/<Character>/session_cache.toml
```

That is what lets a mid-session attach to a running Lich still show buttons.

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Widget settings | none — no widget section in the right-click menu | none — `.editwindow` shows base fields only |
| Custom bar definitions | `config.toml` only — **no in-app editor** | `config.toml` only |
| Keyboard focus for arrow keys | n/a — the GUI drives the bar by mouse | remove `"quickbar"` from `[ui.focus]` `exclude` |
| Selection colors | from the active theme | from the active theme |

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`,
`border_sides`, `title`, `show_title`, `locked` — apply as they do to any window. Selection
colors are theme-owned; a `selection_fg` or `selection_bg` key in a layout file is read by
nothing.

</details>
