# Make the game shout when something matters

> By the end you'll have a bounty line that turns gold and chimes, a web
> that lights an on-screen indicator, and the ambient noise you're tired of
> reading gone from the feed.

## What you'll build

Three rules, each one a different kind of "tell me":

1. **Color + sound** — `You have completed your task` prints in gold and plays a
   chime, so a bounty finishing lands even when you're reading something else.
2. **A status light** — being webbed switches on a custom status id, and an
   indicator window wired to that id lights up. This is the one worth learning:
   any line of game text can drive a visual, without the game ever sending a
   status update for it.
3. **Squelch** — the ambient weather chatter never reaches your main window
   again.

You'll prove all three with `.testline`, which feeds a fake game line through
the live highlight pipeline, so nothing here depends on finding a mob first.

<figure class="shot" data-shot="howto/highlights-three-rules-in-action">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The main window with a gold bounty line, a lit <b>WEBBED</b> indicator beside the vitals, and no weather chatter in the feed.</figcaption>
</figure>

## Before you start

- **For the sound rule only:** an audio file in `~/.vellum-fe/global/sounds/`.
  VellumFE creates that folder and seeds it on first run. Drop a `.wav` or
  `.mp3` in and it appears in the editor's **Sound** dropdown. The examples
  below use `chime.wav` — substitute the name of a file you actually have.
- **Coming from Wrayth or StormFront?** Convert your existing rules first
  instead of retyping them:

  ```
  vellum-fe import-highlights 70682.xml
  ```

  This runs and exits without connecting. It writes
  `70682-highlights.toml` beside the source file; add `--dry-run` to see what
  it would produce, or `--out <FILE>` to choose the destination.

## Steps

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**Rule 1 — a bounty that shows and chimes**

1. Click **Editors** in the top toolbar, then **Highlights**. The hub stays
   open, so you can keep working alongside it. (Or type `.highlights` in the
   command input.)
2. Click **Add highlight**.
3. Fill in the form:
   - **Name** — `bounty-done`
   - **Pattern** — `You have completed your task`
   - **Foreground** — `gold`
   - **Sound** — pick `chime.wav` from the dropdown
   - Tick **Entire line** so the whole line colors, not the matched words
4. Click **Save**.

<figure class="shot" data-shot="howto/gui-highlight-form-bounty">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The highlight form with <b>Name</b>, <b>Pattern</b>, <b>Foreground</b> and <b>Sound</b> filled in for the bounty rule.</figcaption>
</figure>

**Rule 2 — webbed lights an indicator**

5. You need somewhere for the light to live. Type
   `.addwindow webbed indicator 60 2 12 1` to place a one-row indicator window —
   **an indicator window's name is its status id**, so this one answers to
   `webbed`.
6. Back in the highlights editor, click **Add highlight** again:
   - **Name** — `webbed-on`
   - **Pattern** — `You are entangled in a web`
   - **Set status** — `WEBBED`
   - **Status duration** — leave empty so it stays lit until cleared
7. **Save**, then add the matching off-switch:
   - **Name** — `webbed-off`
   - **Pattern** — `You tear through the last of the webbing`
   - **Clear status** — `WEBBED`
8. **Save**.

**Rule 3 — hide the weather**

9. **Add highlight** once more:
   - **Name** — `quiet-weather`
   - **Pattern** — `A gentle breeze blows through`
   - Tick **Squelch**
10. **Save**.

**Prove all three without waiting for a mob**

11. Type each of these into the command input:

    ```
    .testline You have completed your task
    .testline You are entangled in a web
    .testline A gentle breeze blows through
    ```

→ **Expected result:** the first line prints in gold and the chime plays. The
second lights your **WEBBED** indicator. The third produces nothing at all —
the feed does not move, which is the squelch working.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**Rule 1 — a bounty that shows and chimes**

1. Type `.highlights`. The **highlight browser** opens over the screen, listing
   your rules grouped by category with a color sample for each.
2. Type `.addhighlight` to open the rule form.
3. Fill in the fields — the form carries the same set the GUI does:
   - **Name** — `bounty-done`
   - **Pattern** — `You have completed your task`
   - **Foreground** — `gold`
   - **Sound** — `chime.wav`
   - Tick **Entire line**
4. Save the form.

**Rule 2 — webbed lights an indicator**

5. Type `.addwindow webbed indicator 60 2 12 1` to place the indicator window
   the status will drive. (`.addwindow` with no arguments opens a picker
   instead.)
6. `.addhighlight` again:
   - **Name** — `webbed-on`
   - **Pattern** — `You are entangled in a web`
   - **Set status:** — `WEBBED`
   - Leave **Status duration** empty
7. Save, then add the off-switch with **Pattern**
   `You tear through the last of the webbing` and **Clear status** `WEBBED`.

**Rule 3 — hide the weather**

8. `.addhighlight`, **Pattern** `A gentle breeze blows through`, tick
   **Squelch**, save.

**Prove all three**

9. Run the same three `.testline` commands:

    ```
    .testline You have completed your task
    .testline You are entangled in a web
    .testline A gentle breeze blows through
    ```

<figure class="shot" data-shot="howto/tui-highlight-browser-rules">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The TUI highlight browser listing the three new rules with their color samples.</figcaption>
</figure>

→ **Expected result:** gold bounty line plus chime, the **WEBBED** indicator
switches on, and the weather line never appears.
{{#endtab}}
{{#tab name="Mobile"}}

**The phone authors highlights in full** — including redirects and squelch.
This is a real path, not a fallback to the desktop. A compile-time check
cross-references the phone's form against the desktop field list, so the phone
cannot silently fall behind.

1. Tap the gear **⚙** in the top bar to open the settings sheet.
2. Tap **Highlight rules (this profile)** for rules that follow this character,
   or **Highlight rules (global)** for rules that apply everywhere.
3. Tap **＋ New rule**.
4. Fill in the form. It has a live **Sample game text** preview at the top that
   restyles as you type:
   - **Name** — `bounty-done`
   - **Pattern (text or regex)** — `You have completed your task`
   - **Text color** — tap the swatch and pick gold
   - Tick **Color entire line**
   - **Sound** — choose `chime.wav` from the dropdown
5. Save, then repeat for the other two rules:
   - `webbed-on` — pattern `You are entangled in a web`, **Set status**
     `WEBBED`
   - `quiet-weather` — pattern `A gentle breeze blows through`, tick
     **Squelch (hide matching lines)**
6. Type the three `.testline` commands into the command bar at the bottom, the
   same as on desktop.

<figure class="shot" data-shot="howto/mobile-highlight-rule-form">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The phone's highlight rule form, with the live <b>Sample game text</b> preview above the color pickers and the <b>Squelch</b> checkbox below.</figcaption>
</figure>

> ⚠️ **The phone has its own sound switch, separate from the host's.** The
> settings sheet's **Sound alerts: on — tap to mute** toggle is per-device.
> Muting it silences the phone while your PC keeps chiming — which is usually
> what you want on the couch, and confusing if you forget it's there.

→ **Expected result:** the rules save to the hosting machine's config, exactly
as if you'd typed them there. The gold line and the lit **WEBBED** indicator
appear on your desktop client too.
{{#endtab}}
{{#endtabs}}

## Make it yours

**Send lines somewhere else instead of hiding them.** Squelch is absolute — the
line is gone. If you'd rather keep it but move it out of the way, set
**Redirect to** to another window's name. The mode selector decides what
happens to the original: **redirect only** removes it from the main feed,
**redirect copies** leaves it in place and duplicates it. A rule with pattern
`asks you to` and **Redirect to** `thoughts` puts merchant chatter in your
thoughts window while your hunting feed stays clean.

**Make a status clear itself.** The webbed rule above stays lit until the
matching **Clear status** rule fires — right if you have a reliable "you're
free" line, wrong if you don't and it sticks on forever. Put `30` in **Status
duration** instead and the status switches itself off after thirty seconds
whether or not a clear rule ever matches. The two approaches combine: whichever
happens first wins.

**Rewrite noisy text rather than removing it.** **Replace** rewrites the matched
text in place and understands `$1`-style capture groups. A rule with pattern
`(\w+) just bit you!` and **Replace** `>>> $1 BIT YOU <<<` keeps the
information and makes it impossible to skim past. Set **Window** to scope the
rewrite to one window while the colors still apply everywhere.

**Buzz the controller.** If you play with a gamepad, the **Rumble** dropdown
fires a named rumble pattern on match — a low-health rule you feel rather than
read. Patterns come from the Controller editor's Rumble tab.

## When it doesn't work

**The rule never fires.** Confirm with `.testline` before blaming the rule:
`.testline` injects your text verbatim into the live pipeline, so if the rule
doesn't fire there, the pattern is wrong. The most common cause is a regex
metacharacter taken literally — `(`, `)`, `[`, `]`, `.`, `?`, `*` and `+` all
have meaning. To match a literal `(2/3)` you need `\(2/3\)`. If your pattern
is plain text with no regex in it at all, tick **Fast parse** — it uses literal
matching, which is both faster and immune to this problem.

**Colors don't apply but the sound plays** (or the reverse). The `[highlights]`
section has four independent global switches, all on by default:
`sounds_enabled`, `replace_enabled`, `redirect_enabled` and `coloring_enabled`.
One being off disables that entire category across every rule while the others
keep working. Check them in **Settings** before rewriting a rule that was fine
all along.

**No sound at all.** Three things in order: the file must be in
`~/.vellum-fe/global/sounds/`; `[sound] enabled` must be `true`; and the master
`[sound] volume` (default `0.7`) multiplies against each rule's **Sound
volume**, so a low master value quietly mutes everything. On the phone, also
check the sheet's own **Sound alerts** toggle.

**One sound machine-guns, or a repeated line only chimes once.** Both are
`cooldown_ms` (default `500`). It's a minimum gap between repeats of *the same
sound* — a pattern that matches ten lines in a burst plays once, not ten times.
The cooldown is tracked per sound, so two different rules with two different
files never suppress each other. Raise it if a spammy pattern still chatters;
lower it if you're genuinely missing fast repeats.

**The indicator doesn't light.** The id in **Set status** must match the
indicator window's id — matching ignores case, so `webbed` and `WEBBED` are the
same id, but `web` is not. Confirm the window is actually on screen; a status
firing at a hidden window looks identical to a rule that never matched.

> ✅ **A dashboard adds unknown ids for you.** If your **Set status** id
> doesn't exist yet, a dashboard widget appends it and lights it. An
> *indicator* window does not — it only flips an id it already has. So a
> dashboard is the faster way to see a new status working before you commit to
> placing a dedicated indicator for it.

**Your rules vanished when you switched characters.** Rules save per-profile by
default. Tick **Global (all characters)** in the GUI form, or use the
**(global)** list on the phone, for rules that should follow you everywhere.

## See also

- [Highlight Patterns](../customization/highlights.md) — every rule field, its
  type and default
- [Sound Alerts](../customization/sounds.md) — the sound system, the seeded
  files, and the `[sound]` settings
- [Indicators](../widgets/indicators.md) — placing and configuring the window a
  `set_status` rule drives
- [Dashboard](../widgets/dashboard.md) — the grid that auto-adds unknown status
  ids
- [Command Reference](../reference/commands.md) — `.testline`, `.highlights`,
  `.addhighlight`
- [Browser Client](../frontends/web.md) — the full list of what the phone can
  and can't author
- [Make your health bar shout when you're hurt](./vitals-flash.md) — the same
  machinery applied to vitals
