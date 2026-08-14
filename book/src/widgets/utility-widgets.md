# Utility & Layout Widgets

> The five windows that hold your layout together rather than reporting on the game — and the
> four of them you can only add by typing the long form.

## What it's for

Most windows in this manual show you something about your character. These five do a job
instead: they pad a layout, host your typing, report on the client itself, or hand a panel to
something outside the game.

You reach for them rarely and deliberately. The catch is that four of the five are close to
invisible in the interface, which is what the rest of this page is mostly about.

> ⚠️ **Only `spacer` is in the catalog. The other four exist, but nothing offers them.**
> `command_input`, `performance`, `webui`, and `dialogpanel` are all valid window types, and
> none of them appears in the GUI **Windows** catalog or in the picker you get from a bare
> `.addwindow`. **The full six-argument `.addwindow` form is the only way to type one into
> existence** — and for two of them, typing it is the wrong move anyway (see below).
>
> A window already in your layout is a different matter: `command_input` and `spacer` both show
> up in the **hide** and **edit** pickers once they exist. It is only *adding* that is closed.

> ⚠️ **A misspelled widget type does not error — it silently gives you a text window.** There
> is no "unknown type" message. Three of these five accept two spellings, and the near-misses
> are exactly the ones you'd guess wrong:
>
> | Type | Also accepted | Guessing wrong gives you |
> |---|---|---|
> | `command_input` | `commandinput` | a text window |
> | `performance` | — | a text window |
> | `webui` | `lichui` | a text window |
> | `dialogpanel` | `dialog_panel` | a text window |
> | `spacer` | — | a text window |
>
> If you typed `.addwindow` and got an empty bordered box, check your spelling first.

## Set it up

This tabbed block covers the shared gesture — **adding a window that isn't in the catalog**.
Each widget's own section below adds what's specific to it.

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. For **spacer**, click **Windows** in the top toolbar, expand **Other**, and tick the
   **spacer** row. Repeat for as many as you need — each gets its own generated name.
2. For the other four, type the full form in the command input, for example
   `.addwindow gaps spacer 10 4 6 2` or `.addwindow perf performance 0 0 40 10`. **The catalog
   will not offer them**, and there is no right-click **Add** anywhere in the GUI.
3. Position it by dragging, and set borders and background through right-click ▸ **Appearance**
   like any other window.

<figure class="shot" data-shot="widgets/utility-widgets-gui-performance-window">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A <b>performance</b> window added by typed command, showing frame, render, and network timings beside a text window.</figcaption>
</figure>

→ **Expected result:** the window appears at the coordinates you gave. A spacer shows as blank
space; a performance window immediately starts printing timings.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type the full form: `.addwindow <name> <type> <col> <row> <width> [height]`. For example
`.addwindow gaps spacer 10 4 6 2`, or `.addwindow perf performance 0 0 40 10`.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow performance`
> alone prints usage and adds nothing. The usage line it prints lists only a handful of types
> and is out of date — it does not mean the type you wanted is invalid.

Run `.addwindow` with no arguments for the picker. **The picker offers `spacer` and nothing else
from this page** — it is built from the same catalog the GUI uses.

`.editwindow <name>` opens the window's form. What it exposes varies sharply by widget, and each
section below says which fields you get.

→ **Expected result:** the window appears at the column and row you gave. Height defaults to 10
when you leave the sixth argument off.
{{#endtab}}
{{#tab name="Mobile"}}

The phone uses fixed chrome, so none of these five is placeable there — and four of the five
have no phone equivalent to redirect you to, because they're desktop plumbing.

The one that does: **the command input is always present on the phone**, as the input line
below the story pane. You never add or position it; it is part of the chrome.

For the client's own health figures, and for the Lich WebUI, use a desktop session — the phone
carries neither. Dialog panels reach the phone only as ordinary text in the story pane, through
whichever stream the game sent them on.

→ **Expected result:** you type into the phone's built-in command input, which is there from the
moment you connect and cannot be moved or removed.
{{#endtab}}
{{#endtabs}}

## Common setups

### spacer — layout padding

The only widget here that is in the catalog, and the only one most players will use.

A spacer draws nothing. It occupies grid space so the windows around it land where you want
them, which is how you get a gap between two bars or push a strip to one side without giving
those windows sizes that fight you on the next resize.

**It never draws a border and never draws a title** — the preset turns both off, deliberately.
It respects your theme background rather than punching a hole in the layout.

**Its minimum size is 1 by 1**, the smallest of any widget. That matters: a thin alignment
spacer survives a resize. Everything else in the layout floors at 5 cols by 3 rows, so a narrow
strip of any other type gets clamped up and shoves the layout along after it.

Spacers are dynamically named — `spacer_1`, `spacer_2` — and they show up in the hide and edit
pickers under **Other**. **`.editwindow` on a spacer offers no widget fields**, only the
standard geometry and border ones.

**You'll see:** blank space that holds its place, letting the windows on either side keep the
sizes you gave them.

### command_input — your typing line as a real window

Every frontend always has somewhere to type. So why would you add one?

**Because the command input is a normal dockable window, and the always-there version is a
fallback.** In the desktop GUI the input is a placeable window like any other — you can move it,
resize it, style it, and park it in a shell zone. The GUI watches whether that window actually
rendered this frame; if it didn't — because the definition is missing, the window is hidden, or
its zone is collapsed — it paints a **fixed panel at the bottom of the screen instead**, so the
input can never be lost. That fallback panel is not movable and has no grip.

So: the input you see by default is either your window or the safety net, and adding or styling
a `command_input` window is how you take charge of which.

The default layout already ships a `command_input` window, so in practice you edit that one
rather than adding a second.

**Settings.** This is the widest widget form on this page. **TUI `.editwindow`** exposes
**Prompt icon**, **Prompt icon color**, **Text color**, **Cursor color**, **Cursor background**,
and **Completion color** — the color of the greyed history suggestion. **The GUI has no widget
section for it**, so those six fields are terminal-authored; the GUI reads them from the layout.

**You'll see:** a prompt character in your chosen color ahead of the cursor, and a dimmed
history suggestion completing as you type.

### performance — the client's own vitals

A window of frame timings, render and UI times, text-wrap cost, network and parse throughput,
event counts, CPU, and memory. It reports on VellumFE, not on your character.

Reach for it when the client feels slow and you want to know which part is slow — or when
you're reporting a problem and want numbers to attach.

**Metrics collection runs at all times**, so the window is accurate the moment you open it;
there's nothing to enable first. Every section has its own toggle, and all ten default to on.

**TUI `.editwindow`** gives you an **Edit metrics** entry for those toggles. **The GUI has no
widget section**, so which metrics show is terminal-authored.

**The [Performance Monitor reference](../reference/performance.md) documents what every number
means** — read it there rather than guessing from the labels.

**You'll see:** live timings that move as you play, with the slow section standing out from the
rest.

### webui — a Lich script's own panel

A WebUI window hosts a page published by a Lich script, so a script can give you real controls
inside your layout instead of printing text at you.

> ⚠️ **Do not add this one with `.addwindow`.** Type **`.webui <script/page>`** instead — for
> example `.webui creaturebar/main`. That builds the window *and binds it to the page*, sizing
> it from the page's own hint. `.webui` with no argument opens a picker of registered pages, and
> `.webui off` closes the panel.
>
> A window you add by hand with `.addwindow … webui …` has an empty page binding and shows
> nothing, with no way to fill it in: **neither frontend's editor exposes the page field.**

Windows made this way are named `webui:<page>`, which is why you may see that prefix in your
window lists.

**You'll see:** a script's own interface — buttons, lists, whatever it published — sitting in
your layout as a native window.

### dialogpanel — a resident game dialog, kept on screen

The game sends some interfaces as dialogs rather than text: the combat panel and similar
resident controls. A dialog panel window renders one of them by id, from the accumulated store
of dialog data the client keeps.

> ⚠️ **These arrive on their own — you don't add them.** When the game opens a resident dialog,
> the client records it and builds the panel, placing it from the dialog's own declared position
> and size hints. It then appears in your **Windows** list, where you tick it to show and untick
> it to hide.
>
> A `dialogpanel` you add by hand with `.addwindow` has an empty dialog id and renders nothing,
> and **no editor in either frontend exposes that id** — the game defines these controls.

**Game-created panels are session windows.** They don't persist to your layout the way a text
window does, and a panel that was placed by the game reopens where the game's hints put it,
unless you've moved it and it saved a position for that id.

Effect dialogs are a separate story with their own widget — see
[Active Effects](./active-effects.md), which is fed by four named dialogs and is what you
actually want for buffs, debuffs, cooldowns, and active spells.

**You'll see:** the game's own panel as a window you can place, rather than as text scrolling
past in your story pane.

## Tips & gotchas

> ⚠️ **"Not in the catalog" is not the same as "not supported".** All five render fully and
> reload from your layout file correctly. What four of them lack is a way to *add* them by
> pointing — and for `webui` and `dialogpanel`, that's because a different route creates them
> properly bound.

> ⚠️ **Hiding the command input means two different things.** In the desktop GUI the window
> goes away and the fixed fallback bar takes over. In the terminal the flag is saved but **the
> input stays on screen**, with a message explaining why: *"Command input hidden in the layout
> (GUI shows its fallback bar); the TUI keeps it visible."* One layout, two behaviors, on
> purpose.

> ⚠️ **Bare `.hidecontainers` closes dialog panels too.** It reports the count as "container
> window(s)", which is misleading — it closes every session window, panels included. Name a
> container explicitly if you only meant to close a bag.

**Use a spacer rather than oversizing a neighbor.** A spacer's 1 by 1 floor is what lets it stay
thin. Padding a layout by making the window next to it wider works until the next resize
redistributes the space and moves everything after it.

**The editors are lopsided here, and the terminal usually wins.** `command_input` and
`performance` both have real terminal forms and **no GUI widget section at all**. If a setting
on this page seems to have no home in the desktop GUI, that is why — author it once in the
terminal and the GUI reads it from the layout.

## See also

- [Performance Monitor](../reference/performance.md) — what every metric in the performance
  window means
- [Active Effects](./active-effects.md) — the dialog-fed widget you probably want instead of a
  raw dialog panel
- [Containers](./container-windows.md) — the other session-only windows `.hidecontainers` reaches
- [Creating Layouts](../customization/layouts.md) — placing, saving, and restoring all of these
- [Widgets](./README.md) — the full roster, and the three routes for adding any of them

<details>
<summary>Config reference (TOML)</summary>

All five accept the standard window keys — `row`, `col`, `rows`, `cols`, `show_border`,
`border_style`, `border_sides`, `title`, `show_title`, `locked` — as any window does. Only
`spacer` can be created from the catalog.

### Type strings and aliases

| Widget | `widget_type` | Alias | In the catalog? | Minimum size |
|---|---|---|---|---|
| Spacer | `spacer` | — | ✅ yes | 1 col x 1 row |
| Command input | `command_input` | `commandinput` | ❌ no | 5 x 3 (default floor) |
| Performance | `performance` | — | ❌ no | 20 cols x 4 rows |
| WebUI panel | `webui` | `lichui` | ❌ no | 5 x 3 (default floor) |
| Dialog panel | `dialogpanel` | `dialog_panel` | ❌ no | 14 cols x 4 rows |

An unrecognized `widget_type` falls back to **text** with no error.

### spacer

```toml
[[windows]]
name = "spacer_1"
widget_type = "spacer"
row = 10
col = 4
rows = 2
cols = 6
show_border = false
show_title = false
```

No widget fields. Borders and titles are off by design.

### command_input

```toml
[[windows]]
name = "command_input"
widget_type = "command_input"
row = 44
col = 0
rows = 1
cols = 120
locked = true
prompt_icon = ">"
```

| Field | Type | Default | What it does |
|---|---|---|---|
| `prompt_icon` | string | none | Character or short string before the cursor |
| `prompt_icon_color` | string | none | Its color |
| `input_text_color` | string | none | Typed text color. Reads a legacy `text_color` key from old files |
| `cursor_color` | string | none | Cursor foreground |
| `cursor_background_color` | string | none | Cursor background |
| `completion_color` | string | none | The greyed inline history suggestion |

Preset pins the window to exactly 1 row and ships it `locked = true`.

### performance

```toml
[[windows]]
name = "performance"
widget_type = "performance"
rows = 10
cols = 40
show_fps = true
show_memory = true
```

| Field | Type | Default | What it does |
|---|---|---|---|
| `enabled` | bool | `true` | Draw the window's contents |
| `show_fps` | bool | `true` | Frame rate |
| `show_render_times` | bool | `true` | Render timings |
| `show_ui_times` | bool | `true` | UI build timings |
| `show_wrap_times` | bool | `true` | Text-wrap cost |
| `show_net` | bool | `true` | Network throughput |
| `show_parse` | bool | `true` | Parser throughput |
| `show_events` | bool | `true` | Event counts |
| `show_cpu` | bool | `true` | CPU usage |
| `show_memory` | bool | `true` | Memory usage |

### webui

```toml
[[windows]]
name = "webui:creaturebar/main"
widget_type = "webui"
page = "creaturebar/main"
```

| Field | Type | Default | What it does |
|---|---|---|---|
| `page` | string | `""` | The `script/page` id this window hosts. **Written by `.webui`**; no editor exposes it |

### dialogpanel

```toml
[[windows]]
name = "combat"
widget_type = "dialogpanel"
dialog_id = "combat"
```

| Field | Type | Default | What it does |
|---|---|---|---|
| `dialog_id` | string | `""` | Which dialog's controls to render. **Set by the client** when the game opens the dialog; no editor exposes it |

### Where the settings live

| Widget | GUI widget section | TUI `.editwindow` |
|---|---|---|
| `spacer` | none | no widget fields |
| `command_input` | **none** | six color and prompt fields |
| `performance` | **none** | **Edit metrics** toggles |
| `webui` | none | none — page set by `.webui` |
| `dialogpanel` | none | none — the game defines the controls |

</details>
