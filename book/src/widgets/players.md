# Players

> See who else is in the room at a glance, and click a name to interact without typing it.

## What it's for

The room's "Also here:" line is one long sentence that scrolls away, and it's the line you most
often want back — you're deciding whether to loot in front of someone, looking for the person
you're grouping with, or checking whether the body on the floor is your friend.

The players window keeps that roster on screen and adds what the sentence buries: statuses as
short tags, and corpses dimmed with a `[ded]` marker so you can tell "someone's here" from
"someone died here." Click a name and the game's own verb menu opens on that person.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Entities**, and tick **Players**. Set the row's
   **zone** — the **Right Bar** under your targets window works well.
   (Typed equivalent: `.addwindow players players 92 16 28 8`.)
2. There are no per-window options. Everything that shapes these rows — status side,
   abbreviations, and the dead color — lives in **Settings ▸ Targets** and is shared with the
   [Targets](./targets.md) window.

<figure class="shot" data-shot="widgets/players-gui-room-roster">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A Players window in the Right Bar listing three names, one dimmed with a <b>[ded]</b> tag.</figcaption>
</figure>

→ **Expected result:** a list of everyone else in the room. Clicking a name opens that player's
verb menu at your cursor.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow players players 92 16 28 8` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow players`
> alone prints usage and adds nothing.

`.editwindow players` opens the form. The only widget-specific field is the entity id; border,
colors, and title are the usual ones. `Tab` moves between fields, `Ctrl+S` saves, `Esc`
cancels.

→ **Expected result:** a bordered list titled `Players [02]`, the count tracking arrivals and
departures. Clicking a name opens that player's verb menu.
{{#endtab}}
{{#tab name="Mobile"}}

You don't place windows on the phone. The same roster is the **Players** section of the
**status drawer** — swipe in from the right, or fire the touch wheel's **Players** slice, which
opens the drawer and scrolls straight to it. Tapping a name opens that player's verb menu as a
bottom sheet, the same as a desktop click.

→ **Expected result:** the right drawer opens with **Players** listing everyone in the room, and
tapping a name raises the verb sheet for that player.
{{#endtab}}
{{#endtabs}}

## Common setups

### A who's-here strip under your targets

Add the players window, send it to the **Right Bar**, and drag it to about eight rows so it sits
directly under [Targets](./targets.md). Both read from the same room feed, so they update
together.

**You'll see:** hostiles above, people below, in one column you can check without moving your
eyes off the fight.

### Corpses that read as corpses

Right-click a targets window ▸ **Global target settings…** to reach **Settings ▸ Targets**. Set
**Dead Color** to a dim gray (`#888888` is the default) and confirm `dead` maps to `ded` under
**Status abbreviations**. Save.

**You'll see:** a fallen player renders as `Regyy [ded] [prn]` in gray, clearly separate from the
living names above it, while still being clickable.

## Tips & gotchas

> ⚠️ **Right-clicking a player row opens the *window* menu, not a menu for that player.**
> Right-click anywhere in a VellumFE window is always the window's own menu. To act on a person,
> **left**-click the name — that's what asks the server for their verb menu.

> ⚠️ **The TUI puts a live count in the title (`Players [02]`); the GUI does not.**

**Two statuses can stack, from two different places.** A status written before the name in the
room text (`a stunned Regyy`) and one written after it (`Regyy (prone)`, or the verbose "who is
lying down") are tracked separately, so a row can carry both: `Regyy [stu] [prn]`.

**When a player is dead, the `[ded]` tag always leads.** A dead, prone player reads
`Regyy [ded] [prn]` regardless of which other statuses apply.

**Dead here means the room roster called them a body.** The client reads the roster's "the body
of …" phrasing. It is not a health reading, and it does not follow players you can't see.

**Statuses without an abbreviation fall back to their first three characters.** An unmapped
`awake` renders `[awa]`; anything three characters or shorter passes through whole.

**Status side is global, and this window has no override.** **Settings ▸ Targets** ▸ **Status
Position** moves the tags to the front or the back for players *and* targets together. Only the
targets window can override it per-window.

## See also

- [Targets](./targets.md) — the hostile half of the same room feed, and the home of the shared
  `[target_list]` settings
- [Items](./items.md) — objects on the ground from that same feed
- [Room Window](./room-window.md) — the roster as prose, with its **Players** section toggle
- [Build a hunting layout](../how-to/hunting-layout.md) — where this sits in a combat screen

<details>
<summary>Config reference (TOML)</summary>

### Per-window (`layout.toml`)

`widget_type = "players"`. This widget has exactly one field of its own.

| Field | Type | Default | What it does |
|---|---|---|---|
| `entity_id` | string | `"playercount"` | Entity feed id for the window |

Everything else is a standard window field (`title`, `show_border`, `border_color`, `row`,
`col`, `rows`, `cols`).

### Global (`config.toml`, `[target_list]`) — edited in **Settings ▸ Targets**

The players window reads three of the shared target settings:

| Field | Type | Default | What it does |
|---|---|---|---|
| `status_position` | string | `"end"` | `"start"` or `"end"` — which side of the name the tags sit on |
| `dead_color` | string | `"#888888"` | Text color for dead players (the `[ded]` rows) |
| `status_abbrev` | table | 12 pairs | Full status name to short tag; unmapped statuses use their first three characters |

The remaining `[target_list]` fields (`truncation_mode`, `excluded_nouns`, `boss_color`,
`challenging_color`) affect the targets list only — see [Targets](./targets.md).

```toml
[[windows]]
name = "players"
widget_type = "players"
title = "Also Here"
row = 16
col = 92
rows = 8
cols = 28
show_border = true

[target_list]
status_position = "end"
dead_color = "#888888"

[target_list.status_abbrev]
dead = "ded"
prone = "prn"
stunned = "stu"
```

</details>
