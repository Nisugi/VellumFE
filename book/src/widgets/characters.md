# Characters

> Six clients open, and one of them is dying. See which one without alt-tabbing.

## What it's for

Running several characters means several VellumFE windows, and the one that needs you is
always the one you are not looking at. The Characters window puts a card on screen for every
other VellumFE running on this machine — health, roundtime, status, room — so a health bar
dropping in another client is something you *see* rather than something you find.

It also answers the other multi-account question: who is with whom. Grouped characters share
an enclosing frame with the leader listed first, and solo characters stand alone. When the
roster behind a frame is a guess rather than a parsed `group` reply, the window says
**roster unconfirmed** instead of presenting the guess as fact.

**Nothing to turn on.** Every VellumFE registers itself when it starts, and each one finds
its siblings on its own. Add the window and the cards appear.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Character**, and tick **Characters**.
   The catalog is collapsed, so expand the group first.
   (Typed equivalent: `.addwindow characters multiaccount 0 0 46 12`.)
2. Right-click the window ▸ **Edit Window…** for the **Card contents** section — which rows
   each card shows, how many columns a card has, card order, and card width. See
   [Shape the card](#shape-the-card) below.

<figure class="shot" data-shot="widgets/characters-gui-wall">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A Characters window with three cards: two inside a shared group frame led by <b>⚑ Alice</b>, one standing alone.</figcaption>
</figure>

→ **Expected result:** one card per other VellumFE running on this machine, plus a
gold-bordered card for yourself marked **(you)**. With no siblings running you get
"No other VellumFE sessions on this machine." and "Cards appear automatically as you connect
more clients."

{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**The cards are a GUI widget.** You can place the window in the TUI, but it will not draw
cards — it prints "Multi-account cards are available in the GUI frontend." in gray. The cards
lean on per-card injury dolls and bar art the terminal has no equivalent for.

If you run one character in the TUI and others in the GUI, that TUI session still *publishes*
its status: its card shows up on every GUI client's wall. Watching is what the terminal can't
do, not being watched.

To read the wall from a terminal-only setup, open the `/characters` page in a browser — see
[The Characters wall in a browser](#the-characters-wall-in-a-browser).

<figure class="shot" data-shot="widgets/characters-tui-notice">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A <code>multiaccount</code> window in the TUI showing its gray GUI-only notice.</figcaption>
</figure>

→ **Expected result:** the window appears with the GUI-only notice, and this character's card
continues to show on your GUI clients.

{{#endtab}}
{{#tab name="Mobile"}}

The phone has no Characters window in its fixed layout — it has the whole wall as its own
page instead. Turn on the phone server (**Settings ▸ Web ▸ Web Server**), run `.webinfo` to
get the pairing URL, open it on the phone, and tap **All characters at a glance →** on the
sessions page that loads.

The page draws the same clustered cards from the same feeds, with a **⚙ display** button for
which rows to show. It updates live.

<figure class="shot" data-shot="widgets/characters-mobile-wall">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <code>/characters</code> wall on a phone: a grouped pair and a solo card stacked vertically.</figcaption>
</figure>

→ **Expected result:** a **Characters** page listing every running session as a card, dimmed
for any session that has dropped.

{{#endtab}}
{{#endtabs}}

## Common setups

### A trouble strip above your main window

Add the window, set it about 12 rows by 46 columns, and leave the default rows on: roundtime
with the status icons beside it, vitals, and the injury doll. Turn **Your own card** off in
**Edit Window…** so the strip is only other people, and drag it to the top of the screen.

**You'll see:** a row of narrow cards where a red health bar or an `X` glyph is visible from
across the room, and your own client's vitals aren't duplicated in it.

### Doll on the left, numbers on the right

In **Edit Window…** set **Card columns** to `2`, set the **Size weights** to `1` and `1.4`,
then use the per-row column buttons to send **Injury doll** to column 1 and leave everything
else in column 2. Raise **Card width** to about 240 px.

**You'll see:** each card becomes a wide tile with a figure on the left and the bars stacked
beside it, rather than a tall column you scroll.

## Shape the card

**Edit Window…** ▸ **Card contents** offers:

| Control | What it does |
|---|---|
| **Your own card** | Include a card for this character, gold-bordered, sorted first |
| **Room id in header** | Room number top-right, red when that character is not in your room; hover for the room name |
| **Vitals as numbers (51/51)** | Absolute vitals where the peer reported them, percentages otherwise |
| **Only show effects matching:** | Comma-separated name fragments, e.g. `sleep, bind, web`. Empty shows everything |
| **Max per card** | 1–12 effects drawn per card; the rest collapse to `+N more` |
| **Card columns** | 1, 2, or 3 columns inside each card, with **Size weights** for their relative widths |
| **Rows (top to bottom)** | Tick which rows draw, reorder with ⬆/⬇, and **Share line with the row above** for compact ones |
| **Card order** | **Group** (keeps clusters adjacent), **Name**, or **Connected** |
| **Card width** | 90–320 px |

The ten rows are **Roundtime**, **Status icons**, **Vitals**, **Hands / casting**,
**Debuffs & cooldowns**, **Mind state**, **Stance**, **Field experience**, **Encumbrance**,
and **Injury doll**. **Vitals** and **Injury doll** never share a line — put things beside
them with columns instead.

## Reading a card

### Status glyphs

Status is a strip of single characters, borrowed from Lich's groupbar: with six cards on
screen, width is the scarce resource. Hover any glyph for its name. If you have a skin
installed, its icon art replaces the letter.

| Glyph | Condition | Color |
|---|---|---|
| `X` | Dead | red |
| `S` | Stunned | gold |
| `!` | Bleeding | crimson |
| `W` | Webbed | silver |
| `P` | Prone | orange |
| `K` | Kneeling | orange |
| `s` | Sitting | orange |
| `H` | Hidden | grey |
| `i` | Invisible | light blue |
| `T` | Poisoned | green |
| `D` | Diseased | brown |
| `J` | Joined (in a group) | deep sky blue |

Standing is deliberately not drawn — a glyph on every card at all times carries no
information. Any other condition the game reports without a glyph of its own still appears,
as its first letter.

### Frames and headers

- **Gold border, name in gold, `(you)`** — your own card, the reference point.
- **Red border, red room number** — that character is in a different room from you.
- **Shared frame around several cards** — the game has these characters grouped.
- **⚑ Name** above a frame — the group's leader, listed first. **⚑ Name (not yours)** means
  the leader is someone else's character.
- **⚑ grouped** with no name — the game says these are grouped but no roster has been parsed.
  Type `group` in that character's client to confirm.
- **roster unconfirmed** in amber — one of these characters joined a group without seeing the
  full roster, so membership may be incomplete.

### When a card goes uncertain

A card that is connected is current, however quiet it has been — updates arrive on change, so
an AFK character's silence means nothing changed, not that the data aged.

When a sibling's connection drops, its card **dims and keeps its last known values** rather
than vanishing, with an amber `●` beside the name. Hover it for "Disconnected — last known
values" (or "No update recently"). After two minutes with no reconnect, the card is dropped.

Values a character has never reported read as unknown rather than zero: an unreported stance
shows `Stance —` with "Not reported by this character yet" on hover, because "Stance 0%" would
read as fully offensive, which is a lie.

## The Characters wall in a browser

The same wall exists as a web page at `/characters`, served by the built-in web server. Any
running instance serves it, and the page dials every registered session itself, so it works
from whichever port you reach.

1. Turn the phone server on: **Settings ▸ Web ▸ Web Server** (it is separate from the
   Characters window, which needs no setup of its own).
2. Type `.webinfo`. It prints a **Web session URL** with the pairing token in it.
3. Open that URL, then follow **All characters at a glance →**.

The page has its own **⚙ display** panel for which rows to draw; those choices are stored in
that browser and do not touch your desktop window.

> ⚠️ **`.webinfo` warns you if `bind = "127.0.0.1"` — that is this PC only.** Set
> `bind = "0.0.0.0"` under `[web]` so phones on your LAN can reach it, and use
> Tailscale or WireGuard off-LAN. Never expose the port to the open internet.

## Tips & gotchas

> ⚠️ **Same machine only.** Discovery reads `~/.vellum-fe/web-sessions/`, a directory on this
> computer, and dials each sibling over loopback. Characters running on a second PC never
> appear, however they are connected.

**The window shows nothing until a second VellumFE is running.** One client alone gets your
own card and the "No other VellumFE sessions on this machine." note. New clients take up to
five seconds to appear — discovery re-reads the registry on that interval.

**Turning this off is a Settings toggle, not a window action.** **Settings ▸ Web ▸
Multi-Account Status** controls both halves: switch it off and this session stops publishing
its status *and* stops reading anyone else's. Deleting the window changes nothing about what
is shared.

> ⚠️ **This never opens you to the network.** With the phone server off, the sidecar binds
> `127.0.0.1` regardless of what `bind` says, so enabling multi-account status cannot expose
> a session to the LAN even if `bind` was left at `0.0.0.0` from an earlier phone setup.

**Injury dolls use *your* art, not theirs.** Peers send a wound map, not artwork, so a
character running custom doll art appears on the doll you have installed.

**Debuffs and cooldowns are the default effect categories.** Active spells and buffs are long
lists that would bury a 150 px card. Add them in the TOML appendix's `effect_categories` if
you want them, and lean on the filter box.

**Recoloring a condition needs a restart to reach these cards.** They honor your
`[[indicator_template]]` colors, but read them once per session.

**The self card is always first.** It sorts ahead of every sibling under **Group** order, so
it is a fixed reference point rather than something to hunt for.

## See also

- [Injury Display](./injury-doll.md) — the full-size version of the card's doll row
- [Progress Bars](./progress-bars.md) — the dedicated bars for your own character
- [Active Effects](./active-effects.md) — the unfiltered view of the effects a card caps
- [Web Client](../frontends/web.md) — the phone server behind the `/characters` wall

<details>
<summary>Config reference (TOML)</summary>

### Per-window (`layout.toml`)

`widget_type = "multiaccount"`. Default size 12 rows × 46 cols, minimum 6 × 24.

| Field | Type | Default | What it does |
|---|---|---|---|
| `show_vitals` | bool | `true` | Health/mana/stamina/spirit bars |
| `show_rt` | bool | `true` | Roundtime and casttime, interpolated from the peer's clock |
| `show_status` | bool | `true` | The colored glyph strip |
| `show_injuries` | bool | `true` | Injury doll, drawn with your installed art |
| `show_mind` | bool | `false` | Mind state bar |
| `show_stance` | bool | `false` | Combat stance bar |
| `show_field_exp` | bool | `false` | Unabsorbed field experience, urgent at/near cap |
| `show_encumbrance` | bool | `false` | Encumbrance bar |
| `show_room` | bool | `true` | Room id in the card header |
| `show_self` | bool | `true` | Include a card for this character |
| `show_absolute_vitals` | bool | `true` | `51/51` instead of `51%` where reported |
| `show_hands` | bool | `false` | Hands and the spell being prepared |
| `show_effects` | bool | `true` | Debuffs and cooldowns |
| `effect_categories` | list | `["Debuffs", "Cooldowns"]` | Which categories to draw, in order |
| `effect_filter` | list | `[]` | Case-insensitive name fragments; empty shows everything |
| `max_effects` | int | `4` | Cap per card after filtering; the rest become `+N more` |
| `row_order` | list | `[]` | Row ids top to bottom; unlisted rows keep default order |
| `merged_rows` | list | `["status"]` | Row ids drawn on the line above them |
| `card_column_weights` | list | `[1.0]` | Relative column widths; the length is the column count |
| `card_row_columns` | table | `{}` | Row id to 0-based column index |
| `sort_by` | string | `"group"` | `"group"`, `"name"`, or `"port"` |
| `columns` | int | `0` | Cards per row before wrapping; 0 fits as many as the width allows |
| `card_width` | float | `150.0` | Card width in points |

Row ids for `row_order`, `merged_rows`, and `card_row_columns`: `rt`, `status`, `vitals`,
`hands`, `effects`, `mind`, `stance`, `field_exp`, `encumbrance`, `injuries`.

### Global (`config.toml`, `[web]`) — edited in **Settings ▸ Web**

| Field | Type | Default | What it does |
|---|---|---|---|
| `multiaccount` | bool | `true` | Share status with, and read status from, sibling instances. Binds to loopback only |
| `enabled` | bool | `false` | The phone server. Required for the `/characters` wall from another device |

With `multiaccount = true` and `enabled = false`, the sidecar runs bound to `127.0.0.1`
whatever `bind` says.

```toml
[[windows]]
name = "characters"
widget_type = "multiaccount"
title = "Characters"
row = 0
col = 0
rows = 12
cols = 46
show_self = false
card_width = 240.0
card_column_weights = [1.0, 1.4]
sort_by = "group"

[windows.card_row_columns]
injuries = 0
vitals = 1
rt = 1

[web]
multiaccount = true
```

</details>
