# Spells

> Your spell list parked on screen, so you can see what you know and what you're running without
> spending a command on `SPELL` in the middle of something.

## What it's for

The game hands you your spell list once, at login, and then expects you to remember it. Checking
costs a command and a screenful of text — which is a fine trade standing in town and a bad one
three seconds into a fight.

A spells window keeps that list visible. It's the game's own spell output given a permanent home:
styled the way the game styled it, with the same clickable links, sitting still while everything
else scrolls.

Its natural partner is the **Missing Spells** window. This window shows what you *have*; that one
shows what you're missing — the spells you told VellumFE to watch that aren't currently running.
Together they answer "am I fully spelled up?" without a single typed command.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**1. Add the window.**

Click **Windows** in the top toolbar, expand **Character**, and tick **spells**. It arrives 20
rows by 40 columns, titled **Spells**. **Categories start collapsed**, so expand the heading
before deciding the row is missing.

**2. Add its companion.**

Tick **missingspells** in the same **Character** category. It arrives smaller — 8 rows by 28
columns, titled **Missing Spells** — because it's usually empty and only needs room when
something's wrong.

**3. Tell it what to watch.**

The Missing Spells window is driven by a watch list, and it stays empty until you fill one.
Spell up the way you normally would, then type:

```text
.spellwatch add all
```

That snapshots everything currently running and watches it from then on.

<figure class="shot" data-shot="widgets/spells-gui-window-and-missing">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A <b>Spells</b> window beside a <b>Missing Spells</b> window, the latter listing two watched spells in amber after a buff lapsed.</figcaption>
</figure>

→ **Expected result:** the Spells window fills with your spell list, and Missing Spells reads
**All spells up** in green until one of your watched spells drops off.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**1. Place the windows.**

```text
.addwindow spells spells 80 0 40 20
.addwindow missing missingspells 80 20 28 8
```

**`.addwindow` takes one argument or six or more — never two through five.** Run it bare for a
picker.

> ⚠️ **The missing-spells type is `missingspells`, one word.** `missing_spells` with an
> underscore is not accepted, and an unrecognized type does not error — it quietly creates a
> **text** window instead. The bare-`.addwindow` picker also doesn't offer this type, so typing
> it correctly is the only route.

**2. Build the watch list.**

Spell up, then type `.spellwatch add all`. Bare `.spellwatch` lists what you're watching, marking
each entry `active` or `MISSING`.

**The terminal puts the count in the window's border.** When two watched spells are down, the
title reads **Missing Spells (2)** — so a glance at the border tells you the story even when the
window is scrolled or too short to show every row. The GUI doesn't do this.

→ **Expected result:** after `Ctrl+S`, your spell list draws in the Spells window, and the
Missing Spells window reads `All spells up` in green.
{{#endtab}}
{{#tab name="Mobile"}}

The phone's chrome is fixed, so you don't place these windows — the spell list has a built-in
home instead.

Open the **status drawer** (the right drawer) and scroll to the **Spells** section. It carries
the same list the desktop window shows, with the game's styling and working links intact. The
quickest route is the touch wheel's **Spells** slice, which opens the drawer and scrolls
straight to that section.

**There is no missing-spells surface on the phone.** The watch list is a desktop feature, and
nothing on the phone reports which watched spells are down. What the phone gives you instead is
the **status drawer**'s effect sections and the **Effects** sheet, where a buff that has lapsed
is absent from **Buffs** and its countdown ticks live on the way out — so you catch the
lapse by watching effects rather than by being told about the gap.

→ **Expected result:** the touch wheel's **Spells** slice opens the status drawer at the
**Spells** section, showing your spell list.
{{#endtab}}
{{#endtabs}}

## Common setups

### "Am I still spelled up?" answered without asking

The spells window tells you what you have. The gap is what costs you, and a gap is much harder
to see than a presence — you're scanning for something that isn't there.

1. Spell up completely in town, the way you'd want to hunt.
2. Type `.spellwatch add all`. VellumFE replies with something like
   `Watching 9 active spells (9 new).`
3. Add a **missingspells** window somewhere your eye passes often — beside your vitals is good.
4. Go hunt.

**You'll see:** a window reading **All spells up** in green for as long as everything holds. When
Spirit Warding lapses, that green line is replaced by an amber `101 Spirit Warding I` — an item
appearing where there was nothing, which is exactly the kind of change peripheral vision is good
at catching.

### Trimming the watch list to what you actually re-cast

`add all` is a blunt start and it watches things you don't care about — a bard's song you only
run in town, or a one-off from another player.

1. Type `.spellwatch` to list what you're watching, with each entry marked `active` or `MISSING`.
2. Drop the ones you won't re-cast, several at once:

   ```text
   .spellwatch rem [1605, 1617]
   ```

3. Add anything you missed the same way: `.spellwatch add [503, 513]`.

**You'll see:** the list shrinks to the spells you genuinely maintain, so the Missing Spells
window stops crying wolf about a buff you never intended to keep up.

## Tips & gotchas

> ⚠️ **The spells window is not a live "known spells" roster.** The game sends this list **once,
> at login**, and the window shows that snapshot. It is replaced wholesale when the game sends a
> fresh one, and it keeps no history and doesn't scroll back. If it looks stale or empty, run a
> `SPELL` check to make the game re-send.

> ⚠️ **Clicking a spell does not cast or prepare it.** A click on a plain spell link asks the
> *server* for that object's menu and shows you the verbs it offers. It's a two-step — click,
> then choose — not a one-click cast. Nothing fires from this window without you picking an
> entry.

**Adding the window after login still works.** VellumFE keeps the spell list it received and
replays it into a spells window you create later in the session, so you don't have to reconnect
to populate a window you just added.

**Missing Spells watches two categories, not four.** It compares your watch list against
**Active Spells** and **Buffs** only. Debuffs and cooldowns are deliberately excluded — a
cooldown that isn't running is normal, and reporting it as "missing" would make the window
useless. So a watched entry that only ever appears under Cooldowns will read as missing forever.

**`.spellwatch` takes numbers, and takes them in bulk.** `.spellwatch add 606`,
`.spellwatch add [101,103,107]`, and `.spellwatch add 101,103` all work. **A single unparseable
number rejects the whole command** rather than applying half of it, so a typo never leaves you
with a partially-updated list.

**There is no `.spellwatch clear`.** Clearing the list is `.spellwatch rem all`. `remove` works
anywhere `rem` does.

**The watch list is per character and survives restarts.** It's stored with that character's
session state, so each character keeps its own list without you managing files.

> ⚠️ **`add all` snapshots what's running *at that moment*.** Run it while half-spelled and you
> have codified being half-spelled — the window will cheerfully report **All spells up**. Spell
> up fully first, then snapshot.

**Missing Spells ignores your spell colors.** Its rows are always amber, regardless of what the
spell color table says. Spell colors reach the [Active Effects](./active-effects.md) windows,
not this one.

**Empty states are informative, so read them.** **All spells up** in green means the watch list
is satisfied. `.spellwatch add <n> to watch` in gray means you haven't built a list yet — the
window is working and has nothing to do.

## See also

- [Missing Spells](./missing-spells.md) — the watch-list window in full
- [Active Effects](./active-effects.md) — buffs, debuffs, cooldowns and active spells with their
  remaining time
- [Text Windows](./text-windows.md) — how stream-fed windows work generally
- [Hotbars](./hotkeybar.md) — buttons that gray out when you can't afford a spell, or when a buff
  has lapsed
- [Wire a hotbar for combat](../how-to/combat-hotbar.md) — a re-cast button that lights up when
  its buff drops
- [Color Palette](../customization/colors.md) — `.spellcolors` and the spell color table
- [Creating Layouts](../customization/layouts.md) — placing and saving these windows

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog and `.addwindow`. Hand-editing is for troubleshooting, not the normal
path.

```toml
[[windows]]
name = "spells"
widget_type = "spells"
row = 0
col = 80
rows = 20
cols = 40
show_border = true
title = "Spells"

[[windows]]
name = "missing"
widget_type = "missingspells"
row = 20
col = 80
rows = 8
cols = 28
show_border = true
title = "Missing Spells"
```

**Widget fields.** Neither widget has any of its own.

| Widget | Type string | Data | Where its content comes from |
|---|---|---|---|
| Spells | `spells` | `SpellsWidgetData` — no fields | Subscribed to the game's `Spells` stream, bound automatically at window creation. You do not set a stream. |
| Missing Spells | `missingspells` | `MissingSpellsWidgetData` — no fields | Derived: the per-character watch list minus everything currently in **ActiveSpells** and **Buffs** |

> The Missing Spells type string is **`missingspells`** — no underscore — and it is absent from
> the type list `.addwindow` advertises, though the window is real and the catalog creates it.

**Catalog presets**, both under the **Character** heading:

| Catalog row | Title | Size |
|---|---|---|
| `spells` | Spells | 20 rows x 40 cols |
| `missingspells` | Missing Spells | 8 rows x 28 cols (floor: 3 x 14) |

**`.spellwatch`** — usage is `.spellwatch add|rem <number> | [n,n,...] | all`, and bare
`.spellwatch` lists the watch list.

| Form | What it does |
|---|---|
| `.spellwatch` / `.spellwatch list` | Lists watched spells, each marked `active` or `MISSING`, headed `Watched spells (N missing):` |
| `.spellwatch add <n>` | Adds one spell number |
| `.spellwatch add [n,n,n]` | Adds several — also accepts `n,n` without brackets |
| `.spellwatch add all` | Watches everything currently in **ActiveSpells** and **Buffs**. Replies `Watching N active spells (M new).`, or `Nothing active in ActiveSpells/Buffs to add.` |
| `.spellwatch rem <n>` / `.spellwatch remove <n>` | Removes spells, same number forms |
| `.spellwatch rem all` | Clears the list. **There is no `clear` subcommand.** |

Numbers are parsed as whole numbers up to 65535; anything outside that, or any unparseable
element, rejects the entire command.

**Persistence.** The watch list lives in that character's session state under
`~/.vellum-fe/profiles/<Character>/session_cache.toml` (or under `$VELLUM_FE_DIR` when set), in
the `[character]` table as `watched_spells`. It's written by autosave when it changes, and
restored at login — there is no reason to edit it by hand.

**Display details.** Missing Spells rows are `<number> <name>` in amber `#d78700` in both desktop
frontends, and **the spell color table does not apply.** Empty states are `All spells up` in
green `#5f875f` and `.spellwatch add <n> to watch` in gray `#666666`. **The terminal appends the
count to the window title** (`Missing Spells (3)`); the GUI does not.

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`, `title`,
`locked` — apply as they do to any window.

</details>
