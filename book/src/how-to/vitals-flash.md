# Make your health bar shout when you're hurt

> One glance — or one sound — tells you you're at 30%, without reading a single number.

## What you'll build

A low-health alarm that works when your eyes are on the creature, not on your bars. You'll
wire three things that fire together at 25% health: a hotbar button that flips to red text on
a dark red field, an indicator window that lights up, and a sound plus a controller buzz on
the game's own wound messaging. By the end, dropping below a quarter health changes what your
screen looks like and what your chair feels like.

> ⚠️ **Nothing in VellumFE blinks or animates a vitals bar.** There is no flashing bar, no
> pulsing fill, no threshold color on the bar itself. The `minivitals` and `progress` windows
> expose Layout, Bar height, Bar text, Depleted color, and which bars show — and that is all.
> What this guide builds is louder and more reliable than a blink: a **color change** on a
> button, an **icon that lights**, a **sound**, and a **controller rumble**. Read the outcome
> as "your screen shouts," not "your bar flashes."

<figure class="shot" data-shot="howto/vitals-alert-triggered">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>Health at 22%: the <b>HP</b> hotbar button has flipped to red on dark red, and the <b>LOWHP</b> indicator window is lit beside the mini-vitals bars.</figcaption>
</figure>

## Before you start

- **The visual half of this guide is condition-driven, and only the Desktop GUI can author
  conditions.** The TUI hotbar editor covers a button's label, command, hotkey, tooltip,
  category, and countdown — it shows states as a count and tells you to edit them elsewhere.
  The TUI indicator editor edits id, title, icon, and active/inactive colors, with no
  conditions. The TUI *displays* everything you author in the GUI perfectly; it just doesn't
  build it. Each tab below says which parts it can do.
- **For the sound step:** put an audio file in `~/.vellum-fe/global/sounds/`. A bare name like
  `lowhp` works — the extension is tried for you.
- **For the rumble step:** a connected controller, plus a rumble pattern defined in the
  Controller editor's **Rumble** tab. Skip this step without one; the sound and colors work
  on their own.

## Steps

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**1 — Put a vitals window on screen (if you don't have one).**
Click the **Windows** button in the top toolbar and tick **minivitals**. Right-click it to set
**Bar text** to **Health: 99%** so the number is there when you do want it.

**2 — Add the alarm button's bar.**
Type `.addwindow vitals hotkeybar 0 0 20 3`. The window name is the binding — **a hotkeybar
window shows the bar of the same name**, so this window will display a bar called `vitals`.

**3 — Build the bar and its low-health state.**
Open the **Editors** hub in the toolbar and click **Hotbars** (or type `.hotbars`). Add a bar
named `vitals`, then click **Add button**. Set:

- **Label** — `HP`
- **Command** — `health`

Leave **Global (all characters)** ticked so every character gets the alarm.

**4 — Add the state that changes the button's appearance.**
Scroll to **States** — the note beside it reads *(first matching state styles the button)*.
Click **Add state**. A new state arrives preset to `Roundtime active` and dim; change it:

- In the **When** row, leave the group set to **all of**.
- On the condition row, open the kind combo and choose **Vital**.
- Set the vital combo to **health**, the comparison to **<**, the number to **25**, and the
  unit combo to **%**.
- Under **Style while active:**, set the foreground to `#ff4040` and the background to
  `#400000`.

Click **Save bar**.

**5 — Add a second, earlier warning above it.**
Click **Add state** again, set it to **Vital · health · < · 50 · %**, and give it a yellow
foreground (`#ffd040`). Now use the `^` button to move the **25%** state *above* the **50%**
state. **Order is everything: the first state whose condition matches wins**, so if 50% sits
first it will match at 22% too and you'll never see red. Click **Save bar**.

**6 — Light an indicator on the game's own wound text.**
Type `.addwindow LOWHP indicator 0 0 6 3`. **An indicator window's name is its status id.**
Then open the **Editors** hub ▸ **Highlights** (or type `.highlights`) and click
**Add highlight**. Fill in:

- **Pattern** — `You are (badly hurt|gravely injured|at death's door)`
- **Foreground** — `#ff4040`
- **Sound** — `lowhp`
- **Rumble** — pick your pattern from the combo
- **Set status** — `LOWHP`
- **Status duration** — `20`

Tick **Global (all characters)** and click **Save**. `set_status` is the trick that makes any
game text drive a visual alert: the id lights every indicator *and* dashboard cell with that
name, riding the same machinery the server's own status icons use. **Status duration** clears
it after 20 seconds so it doesn't stick on after you heal.

→ **Expected result:** at full health the **HP** button sits in its normal color and the
**LOWHP** indicator is dark. Drop under 50% and the button turns yellow; drop under 25% and it
turns red on dark red. When the game calls you badly hurt, the sound plays, the controller
buzzes, and the **LOWHP** indicator lights for 20 seconds.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**The TUI shows every part of this alarm. It can author two of the three parts itself.**

**1 — Place the windows.**
Type `.addwindow minivitals minivitals 0 0 40 3`, then
`.addwindow vitals hotkeybar 0 4 20 3`, then `.addwindow LOWHP indicator 0 8 6 3`. The
hotkeybar window's name binds it to the bar named `vitals`; the indicator window's name is its
status id.

**2 — Build the sound-and-status half — the TUI does this fully.**
Type `.highlights` to open the highlight browser and add a rule with pattern
`You are (badly hurt|gravely injured|at death's door)`, foreground `#ff4040`, sound `lowhp`,
a rumble pattern, **set status** `LOWHP`, and **status duration** `20`. The TUI highlight form
covers every field the GUI form does — sounds, rumble, redirects, squelch, and all three
status actions.

**3 — Create the bar and its button.**
Type `.hotbars` to open the hotbar editor. Add a bar named `vitals` and a button with label
`HP` and command `health`. The editor covers label, command, hotkey, tooltip, category, and
countdown source.

**4 — For the color-changing states, use the GUI once.**
**The TUI hotbar editor cannot build condition states.** When a button has them it shows
`2 state(s) defined - edit in the GUI editor or hotbars.toml` and leaves them untouched when
you save, so nothing you author elsewhere is ever lost. Run the Desktop GUI once, follow its
steps 3–5 above, and come back — `hotbars.toml` is shared, and the TUI renders the resulting
colors on the button exactly as authored. The same applies to indicators: `.indicators` in the
TUI edits id, title, icon, and active/inactive colors, while multi-condition icon switching is
authored in the GUI.

→ **Expected result:** the sound, rumble, and lit **LOWHP** indicator work immediately from
TUI-authored config. After one GUI visit, the **HP** button also turns yellow under 50% and
red under 25% in your terminal.
{{#endtab}}
{{#tab name="Mobile"}}

**The phone shows a low-health alarm; it authors half of one.**

The phone's vitals are fixed chrome — four bars reading `HP`, `MP`, `ST`, `SP` with a percent,
in the **status drawer**. You can't add a bar, resize one, or place a hotbar, and there is no
threshold styling on the phone's bars.

**What the phone can author is the whole sound-and-status half**, which is the half that
actually reaches you mid-fight. Open the gear ⚙ ▸ **Highlight rules (this profile)**, tap
**＋ New rule**, and fill in the same fields as the desktop form — pattern, colors, **Sound**,
**Sound volume**, **Rumble**, and **Set status** / **Status duration** / **Clear status**. The
phone highlight editor covers every field the desktop editors do, redirects and squelch
included; a compile-time test cross-checks the phone's form against the desktop field list so
it cannot fall behind.

What the phone genuinely cannot do is hotbars. Its equivalent surface is the **macro tray**
and **macro rail**, driven by `macros.toml` — buttons with a label, command, color, and
tap-mode, with no condition vocabulary behind them. Author the condition-driven visuals on
desktop; they'll be waiting on the same character next time you play there.

→ **Expected result:** a rule saved on the phone plays your sound and buzzes the phone's paired
controller when the game says you're badly hurt, and the same rule lights the **LOWHP**
indicator when you next open that character on desktop.
{{#endtab}}
{{#endtabs}}

## Make it yours

**Alarm on the wound, not the number.** Health percent lags a big hit — you can be at 40% with
a rank-3 chest wound and in more danger than someone at 20% clean. Add a state using the
**Injury** condition instead: area `chest`, comparison `>=`, level `2`. Levels 1-3 are wounds
and 4-6 are scars, so `>= 2` catches a serious wound and ignores old scars. Put it above your
percentage states and it wins whenever it matches.

**One alarm for "actually in trouble."** A single low number rarely means death; a low number
*while stunned* does. Add a state, set its group combo to **all of**, and click **+ condition**
twice to build: **Vital · health · < · 40 · %** and **Indicator · stunned · active**. Only the
combination lights it, so the alarm stays rare enough that you believe it.

**Make it an absolute number.** Percentages mislead at low levels, where 25% can be nine hit
points. Switch the Vital row's unit combo from **%** to **abs** and set the value to a real
number of hit points you know is dangerous for your character.

## When it doesn't work

**The button turned yellow but never red.** Your 50% state is above your 25% state. The first
matching state wins, and 22% satisfies "< 50%" perfectly well, so the yellow rule fires and
the red one is never consulted. Use the `^` button on the red state's card to move it above the
yellow one, then **Save bar**. Order the widest condition last, always.

**The button never changes at all.** Check three things in order: the button has at least one
entry under **States** (an empty state list means nothing can ever match); you clicked
**Save bar** and the "unsaved changes" note is gone; and the hotkeybar window's *name* matches
the bar's name exactly. A window named `combat` will not display a bar named `vitals`.

**The indicator never lights.** The indicator window's name is its status id, and it must match
the highlight's **Set status** value. `.addwindow LOWHP indicator …` pairs with **Set status**
`LOWHP`. Case doesn't matter — ids are compared case-insensitively — but spelling does.

**The indicator lights and never goes out.** You left **Status duration** empty, which means
"stay on until something clears it." Either set a duration in seconds, or add a second highlight
rule matching your recovery text with that id in **Clear status**.

**The sound stays silent.** The file must be in `~/.vellum-fe/global/sounds/`. A bare name works
and the extension is tried for you, so `lowhp` finds `lowhp.wav` — but a name with a typo finds
nothing and fails quietly. Check the folder, and confirm sound is enabled in **Settings**.

**Your states vanished after editing in the TUI.** They didn't. The TUI hotbar editor
deliberately round-trips states untouched and displays them as a count, so a TUI save never
destroys GUI-authored conditions. If they're genuinely gone, check whether a per-character bar
is shadowing your global one — a character bar with the same name **replaces the global bar
wholesale** rather than merging with it. The editor's bar list shows `[G]` for global and `[C]`
for character so you can tell which one you're editing.

**The rumble combo is empty.** Rumble patterns come from the Controller editor's **Rumble**
tab. Define one there first and it appears in the highlight form's combo.

## See also

- [Mini Vitals](../widgets/minivitals.md) — every option on the vitals window, including
  Depleted color and bar order
- [Progress Bars](../widgets/progress-bars.md) — individual health/mana/stamina/spirit windows
- [Hotbars](../widgets/hotkeybar.md) — the full button, state, and countdown reference
- [Indicators](../widgets/indicators.md) — indicator windows and custom status ids
- [Dashboard](../widgets/dashboard.md) — the grid form of the same status machinery
- [Highlight Patterns](../customization/highlights.md) — every highlight field, including
  `set_status`, `status_duration`, and `clear_status`
- [Sound Alerts](../customization/sounds.md) — the sounds directory and volume
- [Controller & Gamepad](../customization/controller.md) — defining rumble patterns
- [Wire a hotbar for combat](./combat-hotbar.md) — the same condition vocabulary, applied to
  attacks and spell prep
- [Set up highlights and sound alerts](./highlights-and-sounds.md) — the highlights editor in
  depth
