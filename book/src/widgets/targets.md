# Targets

> Every hostile creature in the room, with its status, in one list you can click to
> target — instead of re-reading the room description while something is hitting you.

## What it's for

Mid-fight the room description is the worst place to find a target. It scrolled away three
messages ago, it lists things you can't meaningfully hit next to the thing still swinging at
you, and it buries "stunned" inside a sentence.

The targets window is the short version: hostile creatures only, one per line, each with its
active statuses as short tags like `[stu,prn]`. Click one and you target it. Boss-tier and
challenging creatures come in their own colors, so the dangerous one in a pile of six is
obvious before you swing.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Entities**, and tick **Targets**. Use the row's
   **zone** control to place it — the **Right Bar** keeps it beside the text without covering
   it. (Typed equivalent: `.addwindow targets targets 92 3 28 12`.)
2. Right-click the window for its two per-window options: **Show filtered appendage count**
   and **Status position**.

<figure class="shot" data-shot="widgets/targets-window-section-options">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The targets window's right-click menu showing <b>Show filtered appendage count</b>, the <b>Status position</b> drop-down, and the <b>Global target settings…</b> link.</figcaption>
</figure>

→ **Expected result:** a list of the room's hostile creatures. Clicking a name sends
`target #<id>` and that row turns green with a `>` in front of it.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow targets targets 92 3 28 12` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow targets`
> alone prints usage and adds nothing.

`.editwindow targets` opens the form. The two targets options are labelled **Show Appendages**
(a checkbox) and **Status Pos:** (a drop-down reading **Left** or **Right**). `Tab` moves
between fields, `Ctrl+S` saves, `Esc` cancels.

→ **Expected result:** a bordered list titled `Targets [03]`, the count updating as creatures
arrive and die. Clicking a row sends `target #<id>`.
{{#endtab}}
{{#tab name="Mobile"}}

You don't place windows on the phone. The same list is the **Targets** section at the top of
the **status drawer** — swipe in from the right, or use the touch wheel. It leads the drawer
deliberately, because mid-combat is when the drawer earns its keep. The current target carries
a `▸` and its statuses sit at the right of each row.

> ⚠️ **Tapping a target row on the phone opens the creature's verb menu — it does not target
> it outright.** On desktop the same click sends `target` immediately. On the phone you get the
> bottom sheet of server verbs and pick from there.

→ **Expected result:** the right drawer opens with **Targets** listing the room's hostiles, and
tapping one raises the verb sheet for that creature.
{{#endtab}}
{{#endtabs}}

## Common setups

### A narrow combat strip beside the text

Add the window, send it to the **Right Bar**, and drag it down to about twelve rows. Right-click
▸ **Status position** ▸ **Before the name**, so every row starts with its tags and the status
column lines up down the left edge.

**You'll see:** `[stu] a mud hog` stacked over `[stu,prn] a mud hog`, scannable in one glance
without reading the names.

### Colors that pick the dangerous one out of a crowd

Right-click ▸ **Global target settings…** to jump to **Settings ▸ Targets**. Set **Boss Color**
to something loud (`#ff5555` ships as the default) and **Challenging Color** to a warmer
`#ffaa55`. Save.

**You'll see:** in a room of six creatures, the two the game flagged as boss-tier render red and
the challenging one amber, while ordinary creatures stay in your normal text color.

### Proof the filter is working

Right-click a targets window and tick **Show filtered appendage count**. Fight a sorcerer, or
anything else that summons grasping limbs from the ground.

**You'll see:** `Appendages: 3` centered on the bottom border while the list itself stays clean
— three unkillable limbs hidden, and you can tell they were hidden rather than missed.

## Tips & gotchas

> ⚠️ **A creature is only listed once the game has sent a `<crtrStatus>` snapshot marking it
> hostile.** Until that arrives its hostility is *unknown*, and unknown creatures are excluded
> on purpose. A brand-new arrival can take a beat to appear, and a shopkeeper standing next to
> you never will. An empty list in a room full of prose is usually this, not a broken window.

> ⚠️ **The targets click sends `target` in the GUI and TUI, but opens the verb menu on the
> phone.** Same list, two different results from the same gesture.

> ⚠️ **The TUI puts a live count in the title (`Targets [03]`); the GUI does not.** The GUI
> window keeps whatever title you gave it.

**Grasping appendages are filtered, and kraken tentacles are not.** Spells like a sorcerer's
**Grasp of the Dead** (709) summon limbs that erupt from the ground and attack you. They're
targetable but can't be damaged, so listing them is pure clutter — arms, claws, limbs, pincers,
tentacles, and palpi are dropped from the list. The four **kraken tentacle** variants
(amaranthine, ghostly, grizzled, ancient) *are* real creatures you can kill, so they stay.
Nothing you configure changes that pair of rules.

**The appendage count is a count of *hostile* appendages.** A limb that never got a hostile
snapshot isn't counted, so the footer can read lower than the number actually flailing at you.

**Dead creatures leave the list entirely.** There is no dimmed corpse row here — that's the
[Players](./players.md) window's behavior. A creature that dies stops being listed.

**A status without an abbreviation falls back to its first three characters.** `stunned` is
mapped to `stu` out of the box; an unmapped `awake` renders `[awa]`. Statuses of three
characters or fewer pass through whole. Add your own pairs in **Settings ▸ Targets** ▸ **Status
abbreviations**.

**Several statuses show at once only when the game sends the structured feed.** `[stu,prn]`
comes from `<crtrStatus>`. When the client is falling back to reading the room text, one status
is the most it can know.

**`truncation_mode = "noun"` is not an always-on shortener.** It swaps the full name for the
bare noun only when a creature has a status *and* the name plus its tags would overflow the
window. Widen the window and full names come back.

**Global settings are global.** **Settings ▸ Targets** changes colors, truncation, excluded
nouns, and abbreviations for every targets window and the players window too. Only the two
options in the right-click menu are per-window.

## See also

- [Players](./players.md) — who else is here, and the `[ded]` styling for corpses
- [Items](./items.md) — the non-creature half of the same room feed
- [Room Window](./room-window.md) — the prose these lists are extracted from
- [Hotbars](./hotkeybar.md) — buttons that light up on creature and vitals conditions
- [Build a hunting layout](../how-to/hunting-layout.md) — placing targets in a combat screen

<details>
<summary>Config reference (TOML)</summary>

### Per-window (`layout.toml`)

`widget_type = "targets"`.

| Field | Type | Default | What it does |
|---|---|---|---|
| `entity_id` | string | `"targetcount"` | Entity feed id for the window |
| `show_body_part_count` | bool | `false` | Draw `Appendages: N` on the bottom border. GUI label **Show filtered appendage count**; TUI label **Show Appendages** |
| `status_position` | string | *(unset)* | `"start"` or `"end"`; overrides the global setting for this window only. Unset = follow the global |

### Global (`config.toml`, `[target_list]`) — edited in **Settings ▸ Targets**

| Field | Type | Default | What it does |
|---|---|---|---|
| `status_position` | string | `"end"` | `"start"` or `"end"` — which side of the name the tags sit on |
| `truncation_mode` | string | `"noun"` | `"full"` or `"noun"`. `"noun"` falls back to the bare noun only when a status is present and the line would overflow |
| `excluded_nouns` | list | `["arm", "coal"]` | Nouns never treated as targets (case-insensitive) |
| `boss_color` | string | `"#ff5555"` | Boss-tier creatures (AscensionBoss or MiniBoss) |
| `challenging_color` | string | `"#ffaa55"` | Creatures the game flags "challenging" |
| `dead_color` | string | `"#888888"` | Dead **players** in the players window — see [Players](./players.md) |

`[target_list.status_abbrev]` maps a full status name to a short tag. Twelve pairs ship by
default; unmapped statuses fall back to their first three characters.

```toml
[[windows]]
name = "targets"
widget_type = "targets"
title = "Targets"
row = 3
col = 92
rows = 12
cols = 28
show_border = true
show_body_part_count = true
status_position = "start"

[target_list]
status_position = "end"
truncation_mode = "noun"
excluded_nouns = ["arm", "coal"]
boss_color = "#ff5555"
challenging_color = "#ffaa55"
dead_color = "#888888"

[target_list.status_abbrev]
stunned = "stu"
frozen = "frz"
dead = "ded"
prone = "prn"
```

</details>
