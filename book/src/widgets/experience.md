# Experience (DragonRealms)

> Every skill the game is tracking for you, listed and standing still — so you can see what's
> ripe to train without breaking rhythm to ask.

## What it's for

DragonRealms reports your training a skill at a time, and it reports a lot of skills. Asking
for the whole picture buries the screen; asking about one skill tells you about one skill.

This window keeps the game's own running tally on screen: one line per skill it is tracking,
each showing the value the game last sent, in the order the game established at login. When a
value changes the line changes, and nothing else moves. You read it in a glance and go back
to what you were doing.

**This is the DragonRealms window.** GemStone IV tracks experience as a level, a mind state
and a progress bar, and has an entirely different widget for it: see
[Experience (GemStone IV)](./gs4-experience.md). The two share a name in the catalog and
nothing else.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Character**, and tick **Experience**. It
   arrives 20 rows by 35 columns, titled **Experience**.
   (Typed equivalent: `.addwindow experience experience 0 0 35 20`.)
2. There is no widget section in this window's right-click menu — this widget has no settings
   of its own. Its border, title, colors and font are the standard window settings under
   **Window** and **Appearance**.

<figure class="shot" data-shot="widgets/experience-dr-skill-list">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A DragonRealms Experience window listing tracked skills, one <b>Name: value</b> line each, in the game's login order.</figcaption>
</figure>

→ **Expected result:** a scrollable list of `Skill: value` lines in a monospaced face. Before
the game sends anything it reads **No experience data yet.**
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow experience experience 0 0 35 20` — name, type, then column, row, width,
height. Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow experience`
> alone prints usage and adds nothing.

`.editwindow experience` opens the form, but this widget contributes no fields of its own —
you get the standard window settings only. `Tab` moves between fields, `Ctrl+S` saves, `Esc`
cancels.

→ **Expected result:** a bordered window titled `Experience` listing `Skill: value` lines.
**In the terminal the list does not scroll** — see the gotcha below. An empty feed reads
`(No experience data)`.
{{#endtab}}
{{#tab name="Mobile"}}

There is no DragonRealms experience surface on the phone — not a drawer section, not a stream
chip, not a wheel slice. The **status drawer** does carry an **Experience** section, but it is
built from the GemStone IV feed and stays empty for a DragonRealms character.

The phone still plays the game fully. Ask for your experience in the command input and read
the reply in the story pane like any other output; the **stream filter chips** row keeps that
reply findable if the fight is loud. For a standing list, use the desktop window.

→ **Expected result:** your typed experience command answers in the story pane. The **status
drawer**'s **Experience** section stays blank on a DragonRealms character.
{{#endtab}}
{{#endtabs}}

## Common setups

### A tall skills column you can actually read

This widget is a list, and a list wants height far more than it wants width.

1. Add **Experience** from the catalog's **Character** group.
2. Send it to the **Right Bar** and drag it to fill the column top to bottom. The list is
   long; every extra row is one more skill visible without scrolling.
3. Keep it around 35 columns. Lines are `Skill: value` and rarely need more.
4. In the GUI, drop the text size a step under **Appearance ▸ Text** to fit more lines.

**You'll see:** one column showing the game's whole tracked-skill tally at once, refreshing
itself line by line as values change, with nothing else on screen moving.

## Tips & gotchas

> ⚠️ **This window is DragonRealms only, and unlike most game gates this one is real.**
> Both pickers hide it from GemStone IV characters. Because VellumFE **treats an unset game
> as GemStone IV**, the row is hidden by default too — connecting through Lich without naming
> a game hides this window even on a DragonRealms character. Set the game, or use the full
> six-argument `.addwindow` form, which is not gated. Built on a GemStone IV character it
> stays permanently empty: that game never sends this feed.

> ⚠️ **The terminal list does not scroll; the GUI list does.** The GUI puts the lines in a
> scroll area, so a long list is fully reachable. **The TUI draws them as a plain block and
> clips at the bottom border** — lines past the window height are not shown and no key
> reaches them. In the terminal, size the window to the list rather than scrolling it.

**Empty until the game speaks.** Before the first update the GUI reads **No experience data
yet.** and the TUI reads `(No experience data)`. The game establishes which skills exist at
login and sends values as they change.

**The order is the game's, not alphabetical.** Skills appear in the sequence the game
declared at login and hold that position for the session. A skill that changes does not jump
to the top — the line updates in place, which is what makes the window readable at a glance.

**A skill with no value yet is not listed.** Only skills the game has actually sent a value
for get a line. The list can therefore grow during a session as more skills report in.

**The list is not cleared between characters in one run.** Skill names accumulate for the
life of the session, so if you have swapped characters without restarting, treat a
surprising entry with suspicion and restart to get a clean list.

**The window redraws only when a value actually changes.** VellumFE compares each incoming
value against the one it is showing and does nothing when they match. A window that looks
frozen is usually correct.

**Right-clicking a line opens the *window* menu.** These lines are plain text, not game
objects — there is nothing to click through to.

## See also

- [Experience (GemStone IV)](./gs4-experience.md) — the other game's experience widget,
  sharing the name and nothing else
- [Perception](./perception.md) — the other DragonRealms-only Character window
- [Progress Bars](./progress-bars.md) — including `concentration`, DragonRealms' own bar
- [Encumbrance](./encumbrance.md) — a single-purpose Character window offered in both games
- [Text Windows](./text-windows.md) — how scrolling, stream-fed windows work generally

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog and `.addwindow`. Hand-editing is for troubleshooting, not the normal
path.

```toml
[[windows]]
name = "experience"
widget_type = "experience"
title = "Experience"
row = 0
col = 0
rows = 20
cols = 35
min_rows = 5
min_cols = 20
show_border = true
align = "left"
```

### Widget fields

`widget_type = "experience"`. The type string is exactly `experience`; an unrecognized type
does not error, it quietly creates a **text** window instead.

This widget has exactly one field of its own.

| Field | Type | Default | What it does |
|---|---|---|---|
| `align` | string | `"left"` | `left`, `center` (or `centre`), or `right`. Anything else falls back to left. **Read only when the window is first built** — change it and reload the layout. Neither editor offers it, and the GUI list is always left-aligned regardless. |

Everything else is a standard window field: `row`, `col`, `rows`, `cols`, `show_border`,
`border_style`, `title`, `locked`. There is no `[experience]` config section — this window has
no global settings, and no widget section in the GUI right-click menu.

> **The shipped `layout_template.toml` is wrong about this widget.** Its commented example
> lists `stream`, `buffer_size`, `show_rates` and `compact_mode`. **None of those exist.**
> They are silently ignored if you copy them in. `align` is the only widget field.

### Catalog preset

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| **Experience** | Character | Experience | 20 rows x 35 cols (floor 5 rows, 20 cols) | DragonRealms only |

The gate hides the row from GemStone IV characters in both the GUI catalog and the
`.addwindow` picker. **An unset game counts as GemStone IV**, so the row is hidden by default
until the game is set to a DragonRealms instance. The six-argument `.addwindow` form is not
gated and builds the window regardless.

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Title, title bar, lock | Right-click ▸ **Window** / **Appearance ▸ Title bar** | `.editwindow experience` |
| Border, accent, background | Right-click ▸ **Appearance ▸ Frame** | `.editwindow experience` |
| Font, text size | Right-click ▸ **Appearance ▸ Text** | not offered |

**Word wrap and content alignment are not offered** under **Appearance ▸ Text** for this
widget — the lines are short and fixed-shape.

</details>
