# Wire a hotbar for combat

> One-click attacks, stances, and spell prep — on buttons that dim, recolor, and count down as
> the fight changes.

## What you'll build

A row of combat buttons that isn't just a row of buttons. Attack dims itself while roundtime is
running and shows the seconds left. Hide goes green and reads "Hidden" once you're in the
shadows. Your 909 dims the moment you can't afford it, and swaps to a different command while
a cast is in flight. Every button also takes a hotkey, so the bar doubles as a set of bindings
you can read.

<figure class="shot" data-shot="howto/combat-hotbar-states-live">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>combat</b> bar mid-fight: <b>Attack</b> dimmed with "3s" counting down, <b>Hide</b> green and reading "Hidden", <b>909</b> dimmed for lack of mana.</figcaption>
</figure>

## Before you start

- **Condition states are authored in the Desktop GUI.** The TUI hotbar editor builds bars,
  buttons, hotkeys, and countdowns, but shows states as a count and leaves them untouched —
  it renders them faithfully, it just doesn't edit them. The Mobile tab explains what the
  phone offers instead.
- **Know the bar-to-window rule before you start:** a hotkeybar window displays the bar with
  **the same name as the window**. Name them to match or nothing appears.

## Steps

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**1 — Place the bar's window.**
Type `.addwindow combat hotkeybar 0 0 40 3` in the command input. This creates a hotkeybar
window named `combat`, which will display the bar named `combat`.

**2 — Create the bar.**
Open the **Editors** hub in the toolbar and click **Hotbars** (or type `.hotbars`). Add a bar
named `combat` and set **Title:** to `Combat`. Leave **Global (all characters)** ticked, or
untick it to give this character its own bar. The bar list tags each bar `[G]` for global and
`[C]` for character so you always know which you're editing.

**3 — Add an attack button that dims during roundtime.**
Click **Add button** and set **Label** to `Attack`, **Command** to `attack`, and **Hotkey** to
`f1` — or click **Capture**, which reads "Press a key..." while it waits, and press the key
itself. If the key is already spoken for you'll see *"Key is already bound by … - it wins over
this button."*; pick another.

Set **Countdown overlay** to **Roundtime** so the button shows the seconds left.

Now scroll to **States**, note the reminder beside it — *(first matching state styles the
button)* — and click **Add state**. **A new state arrives already set to `Roundtime active`
with dim ticked**, which is exactly this rule. Click **Save bar**.

**4 — Add a Hide button that reports being hidden.**
Click **Add button**: **Label** `Hide`, **Command** `hide`, **Hotkey** `alt+h`, **Tooltip**
`Attempt to hide`. Add a state, and on its condition row open the kind combo and choose
**Indicator**; set the id to `hidden` and leave **active** ticked. Under **Style while
active:**, set **Label** to `Hidden` and the foreground to `#80ff80`. A state's style can
replace the button's text, so the button tells you your own state rather than offering an
action you're already in.

Add a *second* state below it for **Roundtime active** with dim ticked. Order matters: hidden
sits above roundtime, so being hidden wins over being in roundtime. Click **Save bar**.

**5 — Add a spell button that knows what you can afford.**
Click **Add button**: **Label** `909`, **Command** `incant 909`. Set **Countdown overlay** to
**Casttime**.

Add a state and choose **Vital** on its condition row: vital `mana`, comparison `<`, value `9`,
unit combo set to **abs** — absolute points, not percent, because a spell's cost is a number of
mana and not a fraction of your pool. Tick dim under **Style while active:**.

**6 — Give that button a different command while casting.**
On a state's card, fill in **Command while active:** — the field hinting `(button command)`.
Whatever you type there is sent *instead of* the button's own command while that state matches.
This is literal text; anything dynamic belongs inside the command, such as a `;eq …` line that
Lich evaluates. Click **Save bar**.

**7 — Watch it before you commit.**
The **Preview:** row above the button list renders the bar live against your current game
state, using the same resolution the real widget uses. Step into roundtime and watch the
preview change. If the preview is wrong, the bar will be wrong.

→ **Expected result:** a **Combat** bar on screen. `F1` attacks and the Attack button dims with
a countdown while roundtime runs. `Alt+H` hides, and the button turns green reading **Hidden**
once you are. The **909** button dims whenever you're under 9 mana.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**1 — Place the window and create the bar.**
Type `.addwindow combat hotkeybar 0 0 40 3`, then `.hotbars` to open the hotbar editor. Add a
bar named `combat`, then add buttons. The TUI editor covers **label**, **command**, **hotkey**
(with conflict validation), **tooltip**, **category**, and **countdown source** — so the whole
of steps 1–3's plain-button work, plus roundtime and casttime countdowns, happens here.

**2 — Author the condition states in the GUI once.**
**The TUI hotbar editor cannot build condition states.** A button that has them displays
`3 state(s) defined - edit in the GUI editor or hotbars.toml` and preserves them exactly on
save, so a TUI edit never damages GUI-authored conditions. Run the Desktop GUI once, follow its
steps 3–6, and return: `hotbars.toml` is shared between frontends.

**3 — Everything then renders in your terminal.**
The TUI draws each button's resolved state — recolored text, the state's replacement label,
dimming, and the countdown seconds — from the same evaluation the GUI uses. **Icons are the one
exception: the TUI always renders the text label**, so the **Face** setting (Text / Icon /
Icon + label) and any icon art are GUI-only. Give every button a label worth reading and the
bar works identically in both.

→ **Expected result:** the **Combat** bar appears in your terminal with working hotkeys and
countdowns from TUI-authored config; after one GUI visit, the buttons also dim, recolor, and
relabel exactly as on the desktop.
{{#endtab}}
{{#tab name="Mobile"}}

**The phone has no hotbars — it has macros, which are a genuinely different thing.**

Its command-button surfaces are the **macro tray** (the left drawer) and the **macro rail**
along the bottom, plus floating buttons. Tap one to fire it; long-press to edit it. Create one
with **＋ New button…**, which opens an editor for a label, a command, a color, and a tap-mode
(send / type / type-then-send). These live in `macros.toml` on the PC.

What's missing is the condition vocabulary: a macro has no states, so it cannot dim during
roundtime, recolor when you're hidden, or gray out when mana runs low. The phone macro editor
also cannot set a macro's `hidden_when` conditions — label, command, color, and tap-mode are
the whole form. There's no way to author a `combat` hotbar from the phone, and no hotbar widget
for it to appear in.

Build the combat bar on desktop and keep the phone's macro rail for the handful of commands you
fire while away from your desk. For the alerting the phone *does* do well — sounds, rumble, and
lit status icons on game text — see
[Make your health bar shout when you're hurt](./vitals-flash.md); the phone's highlight editor
covers every field the desktop's does.

→ **Expected result:** tapping a macro on the rail sends its command immediately, in a flat
color that never changes — and the **combat** bar you built on desktop is waiting there when
you next play on your PC.
{{#endtab}}
{{#endtabs}}

## What the states look like on disk

You never need this file — the editor writes it — but three lines of it explain the ordering
rule better than a paragraph can. Here is the Hide button from step 4:

```toml
[[bars.buttons.states]]                            # checked FIRST
[bars.buttons.states.when]
type = "indicator"
id = "hidden"
active = true
[bars.buttons.states.style]
label = "Hidden"
fg = "#80ff80"

[[bars.buttons.states]]                            # checked only if the above missed
[bars.buttons.states.when]
type = "rt_active"
[bars.buttons.states.style]
dim = true
```

States are checked top to bottom and **the first match wins** — the second block never runs
while you're hidden. Swap the two blocks and a hidden character in roundtime shows a dim
"Hide" instead of a green "Hidden". In the editor the `^` and `v` buttons on each state card
are this same ordering; the file just makes it visible. Editors build one level of `all of` /
`any of` nesting; deeper trees are file-authored, render as *(nested group - edit in
hotbars.toml)*, and still evaluate correctly.

## Make it yours

**A melee bar that reads your hands.** Choose **Hand holds** on a condition row and set the hand
to `Right` with item type `weapon` to light your attack button only when you're actually armed.
Pair it with a **Hand empty** state carrying a **Command while active:** of `get my weapon`, and
one button both draws and swings depending on what you're holding.

**A caster bar that grays out what you can't cast.** Instead of writing a mana threshold per
spell, use **Spell affordable** with the spell's number. It reads the bundled spell table's
static costs against your current mana and fails closed — unknown numbers and formula-cost
spells evaluate false — so a button grays out rather than lying to you.

**A stance bar built from indicators.** Give each stance button an **Indicator** state on
`standing`, `kneeling`, `sitting`, or `prone`, styled bright, so the bar always shows your
current posture as the one lit button. The same eleven ids — `standing`, `kneeling`, `sitting`,
`prone`, `stunned`, `bleeding`, `hidden`, `invisible`, `webbed`, `joined`, `dead` — are
available in every condition builder in the product.

## When it doesn't work

**The window is there but empty.** The window name and the bar name must match exactly — a
hotkeybar window displays the bar of the same name, and there is no bar-picker in the window's
right-click menu (only **Edit hotbars…**, which opens this editor). Rename one to match the
other.

**The wrong state keeps winning.** The first matching state wins, so a broad condition placed
above a narrow one swallows it. Put "hidden" above "roundtime", and "health < 25%" above
"health < 50%". Reorder with each card's `^` and `v`, then **Save bar**.

**The hotkey does nothing.** Existing `keybinds.toml` bindings beat hotbar buttons. The editor
warns you at the time — *"Key is already bound by … - it wins over this button."* — and the
button list marks the row `(key conflict)`. Pick a free key, or rebind the keybind.

**A duplicated button's hotkey is gone.** That's deliberate: duplicating a key would guarantee a
conflict, so **Dup** clears the copy's hotkey. Give the copy its own.

**The button shows text where you wanted art.** Icons are GUI-only — the TUI always renders the
label. In the GUI, check **Face** is set to **Icon** or **Icon + label** and that the bar's
**Icon px** is large enough to see; the default is 24, and barbar-style art reads best between
32 and 64.

**Your global bar stopped responding to edits.** A per-character bar with the same name
**replaces** the global bar wholesale — it does not merge button by button. The bar list's
`[G]` / `[C]` badges tell you which copy exists; edit the `[C]` one, or delete it to fall back
to global.

**The state's command override isn't doing anything clever.** **Command while active:** is
literal text, not an expression. Put the logic in the command itself — a `;eq …` line, which
Lich intercepts and evaluates — rather than expecting the field to compute anything.

**Nothing saved.** **Save bar** is disabled until there are changes and shows "unsaved changes"
beside it while there are. If it's grayed out, your edit didn't register; click into the field
and change it again.

## See also

- [Hotbars](../widgets/hotkeybar.md) — the full field, state, icon, and countdown reference
- [Quickbar](../widgets/quickbar.md) — the lighter-weight command bar
- [Keybind Actions](../customization/keybinds.md) — the key syntax hotkeys use, and resolving
  conflicts
- [Indicators](../widgets/indicators.md) — the same condition vocabulary driving status icons
- [Hands](../widgets/hands.md) — hand icons, also condition-driven
- [Countdowns](../widgets/countdowns.md) — roundtime and casttime elsewhere in the layout
- [Active Effects](../widgets/active-effects.md) — the effect categories conditions read from
- [Skins (GUI Graphics)](../customization/skins.md) — registering the icon sheets buttons draw
  from
- [Make your health bar shout when you're hurt](./vitals-flash.md) — the same vocabulary
  applied to vitals
- [Build a hunting layout](./hunting-layout.md) — where the combat bar sits among your other
  windows
