# Missing Spells

> A window that stays blank until something you rely on drops off — then names it. The gap in
> your defenses, reported instead of noticed.

## What it's for

Seeing what you *have* is easy. Seeing what you're *missing* is the hard part, because you're
scanning for something that isn't there — and a lapsed buff looks exactly like a screen with
nothing wrong.

This window inverts that. You tell VellumFE which spells you care about, and it shows you only
the ones that aren't running. Most of the time it reads **All spells up** in green and you
ignore it. The moment Spirit Warding falls off mid-hunt, an amber line appears where there was
nothing — a change in the corner of your eye, which is the one thing peripheral vision is
genuinely good at.

The watch list is yours, per character, and it survives restarts. Building it is one command.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**1. Add the window.**

Click **Windows** in the top toolbar, expand **Character**, and tick **missingspells**. It
arrives 8 rows by 28 columns, titled **Missing Spells** — small on purpose, since it is empty
most of the time.

**2. Spell up the way you'd want to hunt.**

The next step snapshots what's running *right now*, so get yourself fully covered first.

**3. Build the watch list.**

```text
.spellwatch add all
```

VellumFE replies with something like `Watching 9 active spells (9 new).`

<figure class="shot" data-shot="widgets/missing-spells-gui-amber-rows">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A <b>Missing Spells</b> window listing <b>101 Spirit Warding I</b> and <b>107 Spirit Warding II</b> in amber after two wards lapsed, beside a green <b>All spells up</b> state for comparison.</figcaption>
</figure>

→ **Expected result:** the window reads **All spells up** in green. Let a watched buff expire
and that line is replaced by an amber `101 Spirit Warding I`.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**1. Place the window.**

```text
.addwindow missing missingspells 80 20 28 8
```

That is name, type, then column, row, width, height. Run `.addwindow` with no arguments for a
picker instead — **missingspells** is one of the rows it offers, under **Character**.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow missingspells`
> alone prints usage and adds nothing.

> ⚠️ **The type string is `missingspells`, one word.** `missing_spells` with an underscore is
> rejected — and a rejected type does not raise an error. It quietly builds a **text** window
> instead, which looks like a working window that never fills.

**2. Spell up, then build the list.**

```text
.spellwatch add all
```

Bare `.spellwatch` lists what you're watching, marking each entry `active` or `MISSING`.

**The terminal puts the count in the window's border.** With two watched spells down the title
reads **Missing Spells (2)**, so the border tells the story even when the window is too short to
show every row. **The GUI does not do this** — there, the rows are the only signal.

`.editwindow missing` opens the form, but this widget has no settings of its own: the fields you
see are the standard window ones.

→ **Expected result:** a bordered window reading `All spells up` in green, whose title gains a
count the moment a watched spell drops.
{{#endtab}}
{{#tab name="Mobile"}}

There is no missing-spells surface on the phone — no drawer section, no stream chip, no wheel
slice. The watch list is a desktop feature, and nothing on the phone reports which watched
spells are down.

What the phone gives you instead is the live picture rather than the gap. Open the **status
drawer** (the right drawer) and read its effect sections, or use the **Effects** sheet: a buff
that is about to lapse ticks its countdown down in real time there, which the desktop windows do
not do. You catch the lapse by watching a timer approach zero rather than by being told after
the fact.

`.spellwatch` typed into the phone's command input still edits the list — it is stored on the
host, so a list you build on the phone shows up in your desktop window next time you sit down.

→ **Expected result:** the **status drawer**'s effect sections show your running buffs with
live-ticking times. No window names what is missing.
{{#endtab}}
{{#endtabs}}

## Common setups

### The set-and-forget hunting watch

This is the whole feature in four steps, and it is worth doing once properly rather than
tinkering with it later.

1. Spell up completely in town, exactly the way you want to hunt.
2. Type `.spellwatch add all`.
3. Put a **missingspells** window somewhere your eye already passes — directly beside your vitals
   is the best spot, because that is where you look when something goes wrong anyway.
4. Go hunt.

**You'll see:** a small green **All spells up** for as long as everything holds. When a ward
falls, that green line is replaced by amber `101 Spirit Warding I`. Re-cast it and the window
returns to green on its own — no command, no confirmation.

### Trimming the list to what you actually re-cast

`add all` is a deliberately blunt start, and it will happily watch things you have no intention
of maintaining: a bard's song you only run in town, a one-off somebody else cast on you.

1. Type `.spellwatch` to see the list, each entry marked `active` or `MISSING`.
2. Drop several at once:

   ```text
   .spellwatch rem [1605, 1617]
   ```

3. Add anything the snapshot missed the same way: `.spellwatch add [503, 513]`.

**You'll see:** the list shrinks to the spells you genuinely maintain, and the window stops
crying wolf about a buff you never intended to keep up. `.spellwatch` confirms each edit with a
running total — `Removed 2 spells from the watch list (7 left).`

### Watching one specific thing before a hard fight

You do not have to watch everything. Watching one spell makes this a single-purpose alarm.

1. Clear the list: `.spellwatch rem all`.
2. Watch the one that matters: `.spellwatch add 414`.

**You'll see:** a window that is green until that one spell drops, then shows exactly one amber
line. Nothing else can trigger it.

## Tips & gotchas

> ⚠️ **`add all` snapshots what is running at that instant.** Run it while half-spelled and you
> have codified being half-spelled — the window will cheerfully report **All spells up** while
> you stand there missing three wards. Spell up fully *first*, then snapshot.

> ⚠️ **The type string is `missingspells`, and a typo does not error.** An unrecognized widget
> type silently falls back to a **text** window. If you typed `missing_spells` you now have a
> text window that will never show anything, not a broken missing-spells window.

**It watches two effect categories, not four.** Your watch list is compared against **Active
Spells** and **Buffs** only. **Debuffs and Cooldowns are deliberately excluded** — a cooldown
that isn't running is normal, and reporting it as "missing" would make the window useless. The
consequence worth knowing: a watched number that only ever appears under Cooldowns reads as
missing forever.

**Absence is the test, not expiry.** Nothing here counts down. The game clears and re-sends a
whole category whenever it changes, so a spell vanishing from that feed is what marks it
missing. That makes the window accurate even though the times shown in your
[Active Effects](./active-effects.md) windows sit frozen.

**Numbers, in bulk, all-or-nothing.** `.spellwatch add 606`, `.spellwatch add [101,103,107]`,
and `.spellwatch add 101,103` all work — brackets are optional and spaces inside them are fine.
**A single unparseable number rejects the entire command** rather than applying half of it, so a
typo never leaves you with a partially-updated list. It prints the usage line and nothing
changes.

> ⚠️ **There is no `.spellwatch clear`.** Clearing the list is **`.spellwatch rem all`**.
> `remove` works anywhere `rem` does.

**Names come from a table first, the live feed second, and the number last.** A watched spell
shows as `101 Spirit Warding I` when VellumFE knows the number, as whatever name the game last
used for it this session when it doesn't, and as a bare `64001` when it has never seen it named.
A row that is all digits means the number is real but unrecognized — usually a script-pushed
custom effect.

**The rows ignore your spell colors.** Missing-spells rows are always amber, whatever the spell
color table says. Spell colors reach [Active Effects](./active-effects.md) windows; they do not
reach this one.

**Read the two empty states — they mean different things.** **All spells up** in green means the
watch list is satisfied. `.spellwatch add <n> to watch` in gray means you have not built a list
yet: the window is working correctly and has nothing to do.

**The list is per character and survives restarts.** It rides that character's session state, so
each character keeps its own without you managing anything. Nothing needs saving.

**The window is small by design.** It ships 8 rows by 28 columns and floors at 3 by 14. If you
watch twenty spells and twenty drop at once, the terminal's title count still tells you the
total even when only a few rows fit.

## See also

- [Spells](./spells.md) — the login-time spell list this window's gaps are measured against
- [Active Effects](./active-effects.md) — the live buff/debuff/cooldown feeds this window reads
- [Hotbars](./hotkeybar.md) — a re-cast button that restyles itself when a buff lapses
- [Wire a hotbar for combat](../how-to/combat-hotbar.md) — turning a detected gap into one click
- [Color Palette](../customization/colors.md) — `.spellcolors` and why they stop at this window
- [Creating Layouts](../customization/layouts.md) — placing and saving the window

<details>
<summary>Config reference (TOML)</summary>

Written by the catalog and `.addwindow`. Hand-editing is for troubleshooting, not the normal
path.

```toml
[[windows]]
name = "missingspells"
widget_type = "missingspells"
row = 20
col = 80
rows = 8
cols = 28
show_border = true
title = "Missing Spells"
```

### Widget fields

`widget_type = "missingspells"` — **one word, no underscore.** An unrecognized type does not
error; it quietly creates a **text** window.

The widget carries **no fields of its own**. Its content is derived: the per-character watch list
minus everything currently present in the **ActiveSpells** and **Buffs** effect feeds.

### Catalog preset

| Catalog row | Category | Title | Size | Game |
|---|---|---|---|---|
| `missingspells` | Character | Missing Spells | 8 rows x 28 cols (floor: 3 x 14) | Both |

Ungated — it appears in the GUI **Windows** catalog and the bare-`.addwindow` picker for both
GemStone IV and DragonRealms characters.

### `.spellwatch`

Usage: `.spellwatch add|rem <number> | [n,n,...] | all`.

| Form | What it does |
|---|---|
| `.spellwatch` / `.spellwatch list` | Lists the watch list, each entry marked `active` or `MISSING`, headed `Watched spells (N missing):`. With an empty list it prints the usage hint instead. |
| `.spellwatch add <n>` | Adds one spell number |
| `.spellwatch add [n,n,n]` | Adds several. Brackets optional (`101,103` works); spaces inside them are fine |
| `.spellwatch add all` | Watches everything currently in **ActiveSpells** and **Buffs**, deduped and sorted ascending. Replies `Watching N active spells (M new).`, or `Nothing active in ActiveSpells/Buffs to add.` |
| `.spellwatch rem <n>` / `.spellwatch remove <n>` | Removes spells; same number forms |
| `.spellwatch rem all` | Clears the list, replying `Cleared N watched spells.` **There is no `clear` subcommand.** |

Numbers parse as whole numbers up to 65535. Anything outside that range, any non-numeric element,
and an empty or `[]` argument reject the **entire** command with the usage line. Entries keep
**add order**, and adding a number already on the list is a no-op that still reports the total.

### Persistence

The watch list lives in that character's session cache:

```text
~/.vellum-fe/profiles/<Character>/session_cache.toml
```

(or the same path under `$VELLUM_FE_DIR` when that is set), in the `[character]` table as
`watched_spells`. It is written by autosave when it changes and restored at login. There is no
reason to edit it by hand.

### Display

| State | Text | Color |
|---|---|---|
| Watched spell not active | `<number> <name>`, e.g. `101 Spirit Warding I` | amber `#d78700` |
| Watch list satisfied | `All spells up` | green `#5f875f` |
| No watch list yet | `.spellwatch add <n> to watch` | gray `#666666` |

Rows appear in **watch-list order** (the order you added them), not sorted. **The spell color
table does not apply.** **The terminal appends a count to the window title** — `Missing Spells
(3)` — and drops it when nothing is missing; the GUI never adds a count.

### Where the settings live

| Setting | Desktop GUI | Terminal (TUI) |
|---|---|---|
| Widget settings | none — no widget section in the right-click menu | none — `.editwindow` shows base fields only |
| Title, border, colors, lock | Right-click ▸ **Window** / **Appearance** | `.editwindow <name>` |

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`, `title`,
`locked` — apply as they do to any window.

</details>
