# Countdowns

> Roundtime as a shrinking bar in the corner of your eye — so you know when you can swing again
> without counting in your head.

## What it's for

Roundtime is the metronome of every fight, and the game gives it to you as a number buried in
the prompt. Counting it yourself means either firing commands early and eating the "…but you
are still recovering" line, or waiting too long and losing the exchange.

A countdown window turns that into something you don't have to read: a number plus a row of
blocks that empties as the seconds burn down. Peripheral vision handles it. When the blocks are
gone, you go.

The same widget covers cast time, stun, and anything else with a deadline — a Lich script can
push its own timer for a cooldown, a buff about to lapse, or a hunt window closing, and it draws
in exactly the same way.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Countdowns**, and tick **RT**. It arrives bound to the
   `roundtime` feed, titled **RT**, drawn in red.
2. Tick **casttime** as well if you cast — it arrives titled **Cast** in deep sky blue, so the
   two never get confused at a glance.
3. Set both rows' **zone** control to **Header** or **Footer**, or drag them wherever your eye
   naturally rests during combat.
4. Right-click either timer. The widget section holds **Timer id**, **Label**, **Fill color**,
   and **Stay visible at rest**. Changes apply live, with no Save button.

For a script-driven timer, use **➕ Custom window… ▸ Countdown**. VellumFE opens the new
window's menu immediately, because **a countdown with no Timer id renders as nothing**. Put the
script's id in that field and the timer starts working the next time the script fires.

<figure class="shot" data-shot="widgets/countdowns-gui-timer-menu-open">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A roundtime window right-clicked, with the widget section showing <b>Timer id</b>, <b>Label</b>, <b>Fill color</b>, and the <b>Stay visible at rest</b> checkbox.</figcaption>
</figure>

→ **Expected result:** swing a weapon and the RT window fills with blocks, then empties one
block per second until it clears.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

1. Place the timer with its full geometry:

   ```text
   .addwindow roundtime countdown 0 0 20 3
   ```

   **`.addwindow` takes one argument or six or more — never two through five.** `.addwindow
   roundtime` on its own prints usage and creates nothing. Run it bare to get a picker instead.
2. Type `.editwindow roundtime` to open the window form. The countdown fields are **Icon:** and
   **Countdown ID:**, plus the fill and background colors.
3. `Tab` moves between fields, `Ctrl+S` saves, `Esc` cancels — the footer reads
   `[Ctrl+S: Save] [Esc: Cancel]`.

**A countdown window falls back to matching on its own name.** Because you named this window
`roundtime`, it receives the roundtime feed even before you fill in **Countdown ID:**. That
convenience is countdown-only — progress bars have no such fallback — and it stops applying the
moment you name a window something else, so set **Countdown ID:** explicitly for anything that
isn't named after its feed.

> ⚠️ **The TUI form has no "stay visible at rest" control.** It *displays* the setting correctly
> once saved, but cannot author it. Tick **Stay visible at rest** in the GUI once, or set
> `show_when_zero = true` in `layout.toml`, and the terminal honors it from then on.

→ **Expected result:** the timer draws as a right-aligned number followed by one block glyph per
remaining second, and vanishes when it reaches zero.
{{#endtab}}
{{#tab name="Mobile"}}

The phone's chrome is fixed, so you don't place timer windows there. Roundtime already has a
home: a bar in the phone's own chrome that reads **RT** with the remaining seconds, filling and
draining exactly as the desktop timer does.

That one bar covers cast time too, switching its label to **CT** and its color when a cast timer
is the longer of the two. You cannot split them into separate bars, retitle them, or add a bar
for a script-pushed timer — there is one timer surface and it is built in.

You can hide it: open the gear ⚙ and untick **RT label** in the chrome toggles. Custom timers,
colors, and stay-visible-at-rest are authored on the desktop and stored per character.

→ **Expected result:** the chrome bar reads `RT 5` and empties as roundtime runs out, switching
to `CT` while a spell is going off.
{{#endtab}}
{{#endtabs}}

## Common setups

### A combat corner that never moves

Timers that appear and disappear make your layout twitch, and a bar that jumps into existence is
harder to notice than one that was always there.

1. From the **Windows** catalog, tick **roundtime** and **casttime**, and set both zones to
   **Footer**.
2. Right-click each and tick **Stay visible at rest**.
3. Leave the default colors — red for RT, deep sky blue for CT.

→ Both windows sit in the footer permanently, reading `RT: 0` and `Cast: 0` with empty bars at
rest. When you swing, the red one fills and drains without a single element shifting position.

### A stun timer that works from the game's own text

Stun has no timer feed of its own. VellumFE ships regex patterns that read the game's stun
messages and drive a countdown from them, which is already wired up before you touch anything.

1. Tick **stuntime** in the **Windows** catalog. It arrives titled **Stun** in yellow, bound to
   the `stuntime` feed.
2. Get stunned. The shipped `[event_patterns]` rules match `You are stunned for 3 rounds`,
   multiply the captured rounds by five to get seconds, and push that into the timer. A matching
   rule on `You recover from being stunned.` clears it early.

→ A three-round stun fills the Stun window to fifteen seconds and drains it, and recovering
before it expires blanks the window immediately rather than letting it run down.

## Tips & gotchas

> ⚠️ **A countdown with no Timer id and a non-matching name renders as nothing.** The window is
> there, the border draws, and the inside stays empty forever because no feed reaches it. This is
> the single most common "my timer is broken" report, and it is why creating one from
> **➕ Custom window…** opens its config straight away.

> ⚠️ **Timer ids are case-sensitive.** `roundtime` works; `Roundtime` matches nothing.

**A timer window matches on its Timer id *or* its window name.** Both are checked, which makes
`.addwindow roundtime countdown …` work with nothing else configured. It also means a window you
name after a feed picks that feed up whether you meant it to or not — worth knowing before you
name a custom timer `casttime`.

**Timers hide at zero unless you tell them not to.** Default behavior is to vanish when they
expire, which keeps a busy layout quiet. **Stay visible at rest** keeps the label and a `0`
on screen with an empty bar, so the window never appears or disappears. Pick whichever bothers
you less; the setting is per window, so RT can stay put while a rarely-used timer hides.

**The block count is bounded by window width, not by the timer.** A sixty-second timer in a
twenty-column window shows the number `60` and fills every available cell — the blocks measure
"how much is left of what fits," while the number is always exact. Widen the window if you want
finer-grained visual resolution on long timers.

**Four things can drive a timer.** Native `roundtime` and `casttime` from the game; a
`[event_patterns]` rule whose `event_type` matches (this is how `stuntime` is fed, and
`stun`/`rt`/`ct` are aliased to `stuntime`/`roundtime`/`casttime`); or a script sending
`<vellumTimer id='...' value='epoch'/>`, where `value` is the absolute epoch second the timer
ends and `0` clears it.

**Countdown color is split across two settings.** **Fill color** in the widget section colors the
number and the blocks. The window's own background is under **Appearance ▸ Frame**. The shipped
presets set their color as the window's text color, which is why RT is red and CT is blue out of
the box.

## See also

- [Progress Bars](./progress-bars.md) — health, mana, and stamina, the other half of a combat
  corner
- [Mini Vitals](./minivitals.md) — all four vitals compactly, to pair with your timers
- [Hotbars](./hotkeybar.md) — buttons that gray out during roundtime, using the same **Roundtime
  active** and **Casttime active** conditions
- [Wire a hotbar for combat](../how-to/combat-hotbar.md) — timers and buttons working together
- [Highlight Patterns](../customization/highlights.md) — the pattern system `[event_patterns]`
  sits beside
- [Creating Layouts](../customization/layouts.md) — placing, zoning, and saving windows

<details>
<summary>Config reference (TOML)</summary>

Written by the editors above. Hand-editing is for troubleshooting, not the normal path.

```toml
[[windows]]
name = "roundtime"        # also acts as a fallback feed match
widget_type = "countdown"
id = "roundtime"          # the explicit feed binding
row = 0
col = 0
rows = 3
cols = 20
text_color = "#FF0000"
show_when_zero = true
```

**Widget fields** (`CountdownWidgetData`):

| Field | Type | Default | What it does |
|---|---|---|---|
| `id` | string | *(none)* | Countdown feed identifier (XML id). **Case-sensitive.** Falls back to the window `name` when unset. |
| `label` | string | *(window title)* | Text drawn as the timer's label. |
| `icon` | char | `█` | The glyph used for each remaining-second block. |
| `color` | string | *(theme text)* | Color of the number and the blocks. **Fill color** in the editors. |
| `countdown_background_color` | string | *(window bg)* | Background behind the timer. Accepts the old key `background_color` when reading. |
| `show_when_zero` | bool | `false` | Keep the timer visible at rest as `label: 0` with an empty bar, instead of hiding at zero. **Stay visible at rest** in the GUI menu. |

**Feed ids:** `roundtime`, `casttime`, `stuntime`, a custom `[event_patterns]` `event_type`, or
an id a script pushes via `<vellumTimer id='...' value='epoch'/>`.

**Preset defaults:** roundtime is titled **RT** in `#FF0000`, casttime **Cast** in `#00BFFF`,
stuntime **Stun** in `#FFFF00`. Each is 20 columns by 3 rows.

**Driving a timer from game text.** An `[event_patterns]` entry in `config.toml` reads the game's
own messages. Event types `stun`, `rt` and `ct` are aliased onto `stuntime`, `roundtime` and
`casttime`; anything else becomes a feed id of the same name. These ship enabled:

```toml
[event_patterns.stun_rounds]
pattern = '^\s*You are stunned for ([0-9]+) rounds?'
event_type = "stun"
action = "set"
duration = 0                 # 0 = use the captured value
duration_capture = 1         # 1-based regex capture group
duration_multiplier = 5.0    # rounds → seconds
enabled = true

[event_patterns.stun_recovery]
pattern = 'You recover from being stunned\.'
event_type = "stun"
action = "clear"
duration = 0
duration_multiplier = 1.0
enabled = true
```

**Driving a timer from a script.** Send `<vellumTimer id='dark-cataclyst' value='1764904999'/>`,
where `value` is the absolute epoch second the timer ends. A `value` of `0`, or any time in the
past, clears it. The tag never renders as text.

Standard window keys — `row`, `col`, `rows`, `cols`, `show_border`, `border_style`,
`text_color`, `locked` — apply as they do to any window.

</details>
