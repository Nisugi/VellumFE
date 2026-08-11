# Perception

> Your DragonRealms perception list parked on screen and sorted by urgency, so the spell about to
> lapse is at the top instead of buried in a wall of output.

## What it's for

Asking the game what you're perceiving costs a command and a screenful of text, and the reply
arrives in whatever order the game feels like. The thing you actually wanted — the one entry
that's fading — is somewhere in the middle of it.

A perception window parks that list somewhere permanent and **sorts it**, so the entries with the
most life left sit at the top and anything fading falls to the bottom. It refreshes itself when
the game sends a new list, and the entries stay clickable.

This is a **DragonRealms** window, and it is one of two — its counterpart is the DR
[Experience](./experience.md) window.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Character**, and tick **perception**. It
   arrives 20 rows by 40 columns, titled **Perceptions**.
   (Typed equivalent: `.addwindow perception perception 80 0 40 20`.)
2. There is nothing else to configure. This window has **no widget section** in its right-click
   menu — no feed to pick, no stream to set. Title, border, font and colors live under
   **Window** and **Appearance** as they do on any window.
3. Click an entry to ask the server for that object's verb menu.

<figure class="shot" data-shot="widgets/perception-gui-sorted-entries">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A <b>Perceptions</b> window with percentage entries at the top, <b>(OM)</b> and <b>(Cyclic)</b> entries beneath, and a <b>(Fading)</b> entry last.</figcaption>
</figure>

> ⚠️ **The GUI does not apply this window's three settings.** Short spell names, text
> replacements and sort direction are authored in the terminal and honored there. **The GUI always
> paints the list in the default order, unabbreviated and unreplaced.** The same layout renders
> two ways across frontends.

→ **Expected result:** a window titled **Perceptions** listing what you're perceiving, highest
percentage first. Empty until the game sends a list, where it reads **Nothing perceived.**
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow perception perception 80 0 40 20` — name, type, then column, row, width, height.
Run `.addwindow` with no arguments for a picker instead; **perception** is under **Character**,
and **it only appears there on a DragonRealms character**.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow perception`
> alone prints usage and adds nothing.

`.editwindow perception` opens the form, and **this is the frontend where the settings work.**
It offers three:

| Field | What it does |
|---|---|
| **Sort direction** | `descending` (default) puts the longest-lasting entries first; `ascending` flips it so anything fading floats to the top |
| **Use short spell names** | Swaps full spell names for their standard abbreviations, fitting a narrow window |
| **Text replacements** | A find/replace list applied to each row — opens its own sub-editor |

`Tab` moves between fields, `Space` toggles the short-names flag and cycles the sort direction,
`Ctrl+S` saves, `Esc` cancels.

**There is no stream or buffer field**, because both are fixed. Clicking an entry asks the server
for that object's verb menu.

→ **Expected result:** a bordered window titled `Perceptions` listing your perception entries in
your chosen order.
{{#endtab}}
{{#tab name="Mobile"}}

There is no perception surface on the phone, and the underlying feed is deliberately kept out of
the way: the perception stream is on the phone's hidden list, so it does not even get a stream
chip. The reasoning is that the data is built for a dedicated sorted window rather than for
reading as prose in a story pane.

What the phone offers in its place is the live effect picture. Open the **status drawer** (the
right drawer) and read its effect sections, or use the **Effects** sheet — and unlike the desktop
windows, **the phone ticks its times down in real time**, so an effect approaching zero is
visible as it happens. For the list itself, ask the game in the command input and read the reply
in the story pane.

→ **Expected result:** the **status drawer**'s effect sections show your running effects with
live-counting times. No sorted perception list appears.
{{#endtab}}
{{#endtabs}}

## Common setups

### A DragonRealms character column

Perception earns its space beside its sibling rather than alone.

1. Add **perception** and **experience** from the catalog's **Character** group — the two DR-only
   windows.
2. Send both to the same zone; the **Right Bar** works well.
3. Give perception the taller of the two — the list runs longer than the skill list does.

**You'll see:** one column reading what you're perceiving on top and what you're training below,
each refreshing on its own when the game sends a new list.

### Fitting a long list into a narrow window (terminal)

The default sort buries what's fading at the bottom, which is backwards when a narrow window
means you only see the top few rows.

1. `.editwindow perception`.
2. Set **Use short spell names** on.
3. Cycle **Sort direction** to `ascending`.
4. `Ctrl+S`.

**You'll see:** abbreviated names fitting a much narrower window, with the entry closest to
expiring now sitting in the top row where you'll notice it. Widen it back out and turn short
names off to read the full names again.

## Tips & gotchas

> ⚠️ **This window is DragonRealms only, and the two ways of adding it disagree about that.**
> Both pickers hide it from GemStone IV characters — it is absent from the GUI **Windows**
> catalog and the bare-`.addwindow` picker. The six-argument `.addwindow` form is not gated and
> will build the window for a GS4 character; you get an empty box, because GemStone IV never
> sends this feed. **When no game is set at all, VellumFE assumes GemStone IV**, so a Lich
> connection made without naming a game will hide this row — set the game if the row is missing
> on a DR character.

> ⚠️ **The window's three settings are honored in the terminal and ignored in the GUI.** Sort
> direction, short spell names and text replacements are read by the TUI renderer only. **A
> layout you author in the terminal and open in the GUI will look different** — same entries,
> default order, full names. Author for the frontend you play in.

**Sorting is by kind first, then by number, and it is not alphabetical.** Percentage entries rank
above everything, ordered by their own percentage. Below them come ongoing-magic `(OM)` entries,
then indefinite and cyclic ones, then anything the client doesn't recognize, then countable
`(roisaen)` entries by their count — and **anything marked `(Fading)` always sorts last.**
Descending order puts that ranking top-to-bottom; ascending inverts the whole thing.

**The window fills on a prompt, not the instant text arrives.** Entries accumulate as the game
sends them and are parsed, sorted and painted when the next prompt lands. A window that looks
briefly stale during a long burst catches up on its own.

**A fresh list replaces the old one wholesale.** The game clears the window before sending a new
set, so there is no scrollback and nothing accumulates. Scrolling up shows nothing earlier
because nothing earlier is kept.

**Highlight rules apply here.** Perception rows run through the same highlight engine as your
text windows, on the `perception` stream — so you can color a specific entry, or make one bold,
with an ordinary highlight rule.

**A text replacement that empties a row removes it.** If a replacement rule reduces a row to
nothing, the row is dropped rather than drawn blank. That is a legitimate way to hide entries you
never want to see — and an easy way to lose one by accident with an over-broad rule.

**Right-clicking a row opens the *window* menu, not a menu for that entry.** Left-click the entry
to ask the server for its verbs.

**The stream and buffer settings you may see referenced are fixed.** The window reads one feed
with a fixed cap, and neither editor offers them, because changing them was never wired up.

## See also

- [Experience (DragonRealms)](./experience.md) — the other DR-only window; its natural neighbour
- [Active Effects](./active-effects.md) — the structured buff, debuff and cooldown feeds
- [Text Windows](./text-windows.md) — how stream-fed windows work generally
- [Highlight Patterns](../customization/highlights.md) — coloring specific perception entries
- [Creating Layouts](../customization/layouts.md) — placing and saving the window

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog and `.addwindow`. Hand-editing is for troubleshooting, not the normal path.

```toml
[[windows]]
name = "perception"
widget_type = "perception"
row = 0
col = 80
rows = 20
cols = 40
show_border = true
title = "Perceptions"
sort_direction = "descending"
use_short_spell_names = false
text_replacements = []
```

### Widget fields

`widget_type = "perception"`. The type string is exactly `perception`; an unrecognized type does
not error, it quietly creates a **text** window instead.

| Field | Type | Default | What it does |
|---|---|---|---|
| `sort_direction` | `"descending"` or `"ascending"` | `"descending"` | Order by weight. **Terminal only** |
| `use_short_spell_names` | boolean | `false` | Substitute standard spell abbreviations. **Terminal only** |
| `text_replacements` | array of `{ pattern, replace }` | `[]` | Find/replace applied per row; an empty result drops the row. **Terminal only** |
| `stream` | string | `"percWindow"` | **Inert.** Fixed at the preset value; offered by no editor |
| `buffer_size` | integer | `100` | **Inert.** Fixed at the preset value; offered by no editor |

The three working fields are read by the terminal renderer only. The GUI paints the list in the
default order, unabbreviated and unreplaced.

### Sort weights

| Entry form | Weight | Position under `descending` |
|---|---|---|
| `(94%)` and other percentages | 3000 + the percentage | Top, ordered among themselves |
| `(OM)` — ongoing magic | 2000 | Below percentages |
| `(Indefinite)` / `(Cyclic)` | 1500 | Below OM |
| Anything unrecognized | 500 | Below indefinite |
| `(82 roisaen)` and other counts | the count itself | Below unrecognized when the count is small |
| `(Fading)` | 0 | Last |

`ascending` reverses the whole ordering.

### Catalog preset

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| `perception` | Character | Perceptions | 20 rows x 40 cols (floor: 5 x 20) | **DragonRealms only** |

The game gate hides the row from GemStone IV characters in both the GUI catalog and the
`.addwindow` picker. **An unset game counts as GemStone IV**, so the row is hidden by default
until the game is set. The six-argument `.addwindow` form is not gated and will build the window
regardless.

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Sort direction, short spell names, text replacements | ❌ no widget section in the right-click menu | `.editwindow perception` |
| Title, border, lock | Right-click ▸ **Window** | `.editwindow perception` |
| Font, text size, colors, frame | Right-click ▸ **Appearance** | `.editwindow perception` |

There is no `[perception]` config section — this window has no global settings. Standard window
keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`, `title`, `locked` — apply as
they do to any window.

### Empty states

| Frontend | Text |
|---|---|
| Desktop GUI | `Nothing perceived.` |
| Terminal (TUI) | (blank) |

</details>
