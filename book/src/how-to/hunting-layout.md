# Build a hunting layout

> By the end you'll have a combat screen — vitals, roundtime, targets, compass, hands — saved
> under a name you can load on any character, in either desktop frontend.

## What you'll build

VellumFE ships a deliberately plain screen. The default layout is **six windows**: `main`,
`command_input`, `thoughts`, `speech`, `room`, and `society`. That is a reading screen. This
guide turns it into a fighting screen.

You'll finish with your story text still front and center, a vitals bar and a roundtime timer
across the top where you can read them without leaving the text, and a right-hand column
carrying targets, a compass, and your hands. Then you'll save it as `hunting`, which makes it
reproducible — on your next character, after a bad drag, or on the terminal.

<figure class="shot" data-shot="howto/howto-hunting-layout-finished">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The finished layout: story text filling the center, <b>minivitals</b> and <b>RT</b> in the <b>Header</b> zone, and <b>Targets</b>, <b>Compass</b>, <b>Left Hand</b>, and <b>Right Hand</b> stacked down the <b>Right Bar</b>.</figcaption>
</figure>

## Before you start

- **You're connected and looking at a session.** If not, see
  [First Launch](../getting-started/first-launch.md).
- **Two of these windows are GemStone IV only.** **minivitals** and **Reserve** are gated to
  GS4 and won't appear in the catalog on a DragonRealms character. Use the individual
  **Health**, **Mana**, **Stamina**, and **Spirit** bars there instead — they're ungated, and
  step 2 tells you how.
- **Nothing here needs Lich**, and nothing here edits a file by hand.

## Steps

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**1. Turn on the zones you're going to fill.**

Click **Zones** in the top toolbar and switch on **Header** and **Right Bar**. The hub stays
open, so you can toggle both and watch the canvas shrink around them. (Typed equivalent:
`.header on` and `.rightbar on`.)

→ **Expected result:** a strip appears across the top of the window and a column down the
right side. Both are empty, and your story window has resized to fit between them.

**2. Add the windows.**

Click **Windows** in the toolbar. The catalog opens and stays open — you're going to add six
windows without closing it once.

Two things to know before you start ticking. **Rows are labeled by the window's title, not its
internal name**, so you're looking for **RT**, not "roundtime", and **Injuries**, not
"injury_doll". And **the categories start collapsed** — expand a heading before you go hunting
for a missing row.

Tick each of these, and set its **zone** on the same row using the control at the row's right
edge:

| Tick this row | Set its zone to |
|---|---|
| **minivitals** | **Header** |
| **RT** | **Header** |
| **Targets** | **Right Bar** |
| **Compass** | **Right Bar** |
| **Left Hand** | **Right Bar** |
| **Right Hand** | **Right Bar** |

**On DragonRealms**, there is no **minivitals** row. Tick **Health**, **Mana**, **Stamina**,
and **Spirit** instead and send all four to **Header** — you'll get four separate bars where
GS4 gets one combined widget.

The zone control is a submenu rather than a drop-down list, so it opens a nested list beside
the row. That's deliberate: a true combo box counts as a click outside the toolbar menu and
would close the whole catalog before you could pick anything.

<figure class="shot" data-shot="howto/howto-windows-catalog-hunting-rows">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Windows</b> catalog with categories expanded, <b>minivitals</b> and <b>RT</b> ticked, and a row's zone submenu open on <b>Right Bar</b>.</figcaption>
</figure>

→ **Expected result:** six windows appear as you tick them — two in the header strip, four in
the right column — and start showing live data immediately. The compass lights the exits of
the room you're standing in.

**3. Arrange them.**

Click outside the catalog to dismiss it. Now **drag each window from anywhere on its body** —
no modifier, no need to find a title bar. Resize by dragging any edge or corner.

Edges snap: to the zone's bounds, to the other windows in the same zone, and optionally to a
grid, with a guide line drawn at each match. Put **minivitals** at the left of the header and
**RT** at the right, then stack the right bar in the order you want to read it — **Targets**
on top, **Compass** under it, the two hands at the bottom.

**Hold Shift while you drop** to suspend snapping entirely and place a window exactly under
the pointer. That's the escape hatch when a snap keeps pulling the compass flush against an
edge you want it away from.

**4. Pin the compass so it stops moving.**

Right-click the compass ▸ **Arrange** ▸ **Advanced** ▸ tick **Fixed size**. Do the same for
both hand windows.

Fixed size keeps a window's exact width and height when the app window resizes or you zoom, so
only its position adapts. A compass rose and a hands widget both look wrong stretched; the
targets list wants to grow. Leave **Targets** unfixed.

**5. Save it.**

Type `.savelayout hunting` in the command input.

**There is no button for this.** Saving a layout is typed, in the GUI exactly as in the
terminal — the catalog says so itself when you have no layouts yet.

→ **Expected result:** a confirmation in your story window. `.layouts` now lists `hunting`, and
`.loadlayout hunting` rebuilds this screen from scratch at any point later.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

The terminal has no shell zones — no header strip, no side bars. You place windows on the grid
by coordinate instead, then adjust with the mouse. The result is the same screen; the route to
it is arithmetic rather than menus.

**1. Add the windows.**

Type `.addwindow` with no arguments to open the widget picker, choose a widget, and it lands
at a default spot you then drag. To place precisely, use the full form —
`.addwindow <name> <type> <x> <y> <w> [h]`, where `x` is the column and `y` the row.

On a 120x40 terminal, this fills the top strip and the right column:

```text
.addwindow minivitals minivitals 0 0 40 3
.addwindow roundtime countdown 40 0 14 3
.addwindow targets targets 92 3 28 12
.addwindow compass compass 92 15 28 7
.addwindow left hand 92 22 28 3
.addwindow right hand 92 25 28 3
```

The `<name>` is a free label of your choosing — it identifies the window for `.editwindow`
and `.deletewindow` later. The widget you get comes from `<type>`.

**The argument count is 1 or 6-plus, with nothing valid in between.** `.addwindow compass` on
its own prints the usage line and adds nothing — the command either opens the picker (no
arguments) or places a window (name, type, position, and size all supplied). Omit only the
final height and it defaults to 10.

**On DragonRealms**, drop the `minivitals` line — that widget is GS4-only. Use four progress
bars across the top instead:

```text
.addwindow health progress 0 0 28 3
.addwindow mana progress 28 0 28 3
.addwindow stamina progress 56 0 28 3
.addwindow spirit progress 84 0 28 3
```

→ **Expected result:** each window draws where you put it and starts filling with live data.
The compass lights the exits of your current room.

**2. Move and resize them.**

**Drag the title bar — the top row, and only the top row.** The TUI reserves a window's body
for text selection, so a drag that starts in the middle selects text rather than moving the
window. (**This differs from the GUI, which drags from anywhere.**)

**Resize from the right column, the bottom row, or the bottom-right corner cell.** Two details
save you a puzzled minute: the bottom row is only a handle when the window is taller than two
rows, and the right column only when it's wider than one. A three-row roundtime window resizes
fine; a one-row bar has no bottom handle at all, by design — its bottom row is content.

**3. Fine-tune anything the mouse can't reach.**

Right-click a title bar ▸ **Edit Window...** for the full-screen form, or type
`.editwindow <name>`. `Tab` moves between fields, `Ctrl+S` saves, `Esc` cancels.

**4. Save it.**

Type `.savelayout hunting`.

→ **Expected result:** `.layouts` lists `hunting`. After you stretch the terminal or move to a
different tmux pane, `.resize` refits the arrangement to the new dimensions instead of leaving
windows clipped off the edge — and `.loadlayout hunting` brings the whole screen back.
{{#endtab}}
{{#tab name="Mobile"}}

**The phone can't build a layout, and that's a design decision rather than a missing feature.**
Its chrome is fixed: the client renders from a snapshot the host streams, and there are no
handles to drag, no zones to fill, and no catalog to tick.

What that costs you is less than it sounds, because the phone shows the same information
through surfaces of its own:

- **Targets**, **Players**, and **Room** live in the right **status drawer**, along with the
  injury doll, your hands, and active effects — the same content as the windows you'd have
  placed in a Right Bar.
- **Vitals** render in the phone's own fixed chrome.
- The **macro tray** in the left drawer holds your combat commands as tappable buttons, which
  is where a hotbar's job goes on a touchscreen. See
  [Wire a hotbar for combat](./combat-hotbar.md).

So build this layout on the desktop, save it with `.savelayout hunting`, and connect the phone
to that session — the same character, its own presentation. Start the desktop client with the
web server on (`--web-port 8080`), run `.webinfo` for the pairing URL and QR code, then open
the URL in your phone browser or scan the QR from the app's **Characters** picker (person icon
▸ **Characters** ▸ **Scan QR to add**).

Plenty of customization *is* yours from the phone — macros, touch-wheel slices, highlights
including redirects and squelch, colors, controller binds, and the full settings registry.
Window placement is the part that stays on the desktop.

→ **Expected result:** the phone shows the same game text and vitals as your desktop session,
with targets and room information a drawer-swipe away, and a command typed on either device
lands in the game once.
{{#endtab}}
{{#endtabs}}

## Make it yours

**If you fight in melee, trade the compass for the injury doll.** A caster watches exits and
mana; a melee character watches wounds. Untick **Compass** in the catalog and tick
**Injuries**, sending it to **Right Bar**. The GUI draws it as a body diagram colored by wound
and scar severity, and with a skin that ships doll art you can place the wound dots yourself
through **Calibrate doll…** in the doll window's right-click section. In the TUI, swap the
`.addwindow compass` line for `.addwindow injuries injury_doll 92 15 28 8`.

**You'll see:** a body diagram that colors a limb the moment you take a wound there, instead
of you parsing the wound line out of scrolling text.

**If you're a spellcaster, add the effects trio to a Left Bar.** Turn on the left zone
(**Zones** ▸ **Left Bar**, or `.leftbar on`) and send **Buffs**, **Debuffs**, and **Cooldowns**
there — three separate catalog rows, each an active-effects window carrying one category.
Keeping them apart from the right column means your defensive spell timers never compete with
your target list for the same glance.

**You'll see:** three stacked lists counting down independently, so a lapsing 401 is visible
before it drops rather than after.

**If your terminal is small, put your side windows in a tabbed window instead of a column.**
Four windows down a 40-column right bar leaves nothing for text. Add one **tabbedtext** window
and give it the streams you'd have split out, then switch tabs with a click. In the GUI, the
tab list is **Edit tabs…** in the window's own right-click section; in the TUI it's the
`.editwindow` form.

**You'll see:** one window's worth of screen doing four windows' work, at the cost of only
seeing one at a time.

## When it doesn't work

**A window you ticked in the catalog doesn't appear anywhere.** Check its zone. If the zone is
one you have switched off, the window is real and placed but the strip holding it isn't drawn.
Open **Zones** and turn that zone on, or move the window with right-click ▸ **Arrange** ▸
**Move to** ▸ **Center**.

**You can't find a row in the catalog.** Two causes. **The categories start collapsed** — click
a category heading to expand it before concluding the row is absent. And **rows are sorted and
labeled by title**, so **RT** files under R, not under "roundtime" — search for what the window
is called on screen, not what you'd type.

**minivitals or Reserve is genuinely missing.** Both are GemStone IV only and are filtered out
of the catalog on a DragonRealms character. That's gating working correctly, not a bug. Use the
individual **Health** / **Mana** / **Stamina** / **Spirit** bars.

**`.addwindow compass` prints a usage line and adds nothing.** The command takes either **no**
arguments (which opens the picker) or **all six** — name, type, x, y, width, and optionally
height. A partial command falls through to the usage message. Type `.addwindow` alone and pick
from the list instead.

**A `.addwindow` window landed at the top-left corner in the wrong size.** Numbers that fail to
parse fall back silently rather than erroring — x and y to `0`, width to `40`, height to `10`.
Check for a stray character in the coordinates, delete it with `.deletewindow <name>`, and
retype the line.

**Dragging a TUI window's middle selects text instead of moving it.** That's correct behavior:
**the TUI moves a window from its title bar only**, unlike the GUI, which drags from anywhere.
Grab the top row.

**A TUI window won't resize from its bottom edge.** If it's two rows tall or shorter, its
bottom row is content, not a handle. Resize from the right column, or set the size in
`.editwindow`.

**A window snaps flush against an edge you want it away from.** **Hold Shift while dropping**
to suspend snapping for that gesture. If a window keeps drifting back to an edge on later
resizes, it has a persistent anchor — right-click ▸ **Arrange** ▸ **Release Anchors** to forget
it. The window stays exactly where it is and stops following. If snapping misbehaves in a way
you can't explain, `.snapdebug` writes a trace to `~/.vellum-fe/vellum-fe.log`.

**You resized the app window and everything is proportionally wrong.** In the GUI, right-click
the windows that shouldn't stretch ▸ **Arrange** ▸ **Advanced** ▸ **Fixed size** — a compass and
a hands widget want it, a text window doesn't. In the TUI, run `.resize` to refit the layout to
the terminal's new dimensions.

**You deleted a window and want it back.** In the GUI, **Windows** ▸ **↩ Restore deleted…**
returns it with its position, streams, and widget type intact. **That button only appears when
something is waiting to be restored**, so an empty stash means it's genuinely not there. In the
TUI, restore via `.loadlayout hunting` — which is the strongest argument for saving early and
saving often.

> ⚠️ **`.deletewindow` truly removes a window and `.hidewindow` only hides it — in both desktop
> frontends.** There is no asymmetry between the GUI and the TUI to remember. Both refuse to
> remove your only main-feed window, so you cannot strand yourself without game text.

> ⚠️ **In the Terminal (TUI), `Ctrl+C` copies your selection — it does not quit.** In the
> Desktop GUI the same combination **quits**. Leave the TUI with `.quit` or `.exit`. This is
> the difference most likely to bite you while you're moving between frontends mid-build.

## See also

- [Desktop GUI](../frontends/gui.md) — the Windows catalog, zones, snapping, and the full
  right-click menu
- [Terminal (TUI)](../frontends/tui.md) — title-bar dragging, the resize handles, and
  `.editwindow`
- [Creating Layouts](../customization/layouts.md) — `.savelayout` / `.loadlayout` and the
  shared layout pool
- [layout.toml](../configuration/layout-toml.md) — every window field, its type, and its
  default
- [Mini Vitals](../widgets/minivitals.md) · [Countdowns](../widgets/countdowns.md) ·
  [Targets](../widgets/targets.md) · [Compass](../widgets/compass.md) ·
  [Hands](../widgets/hands.md)
- [Injury Display](../widgets/injury-doll.md) · [Active Effects](../widgets/active-effects.md)
  — the two variations above
- [Wire a hotbar for combat](./combat-hotbar.md) — the next thing to add to this screen
