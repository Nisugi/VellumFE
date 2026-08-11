# Indicators

> One glance tells you you're stunned, bleeding, hidden, or webbed — without hunting for it in
> text that already scrolled past.

## What it's for

The game announces a status once, in a line that's gone three messages later. Then you're
guessing. You cast into a stun you didn't know you had, or you walk out of hiding you thought
you'd lost, or you bleed out over four rooms because the message went by while you were reading
something else.

An indicator is a small window that lights when a status is on and goes dark when it's off.
It doesn't scroll, so the answer is always in the same spot on your screen — you learn where to
look and stop reading for it. Put the three or four that actually change your decisions
somewhere in your eyeline, and let the rest go.

The same machinery reaches further than the game's own statuses. A [highlight
rule](../customization/highlights.md) can light an indicator off any text you choose, which is
how you build alerts the game never offered you.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar. The catalog ships a row per standard status — tick
   **stunned**, **bleeding**, **webbed**, **hidden**, or whichever you want. Set each row's
   **zone** to send it somewhere it'll live.
   (Typed equivalent: `.addwindow stunned indicator 0 0 6 3`.)
2. To change the art, right-click the indicator and choose **Edit indicators…** in the widget
   section — or open the **Editors** hub ▸ **Indicators**, or type `.indicators`. All three
   open the same builder, a window titled **Indicator Icons**.
3. Pick a status in the left list. On the right, set its **Active icon (Y)** and, if you want
   something visible while the status is off, its **Inactive icon (N)**.
4. Click **Save all**.

<figure class="shot" data-shot="widgets/indicators-icon-editor">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Indicator Icons</b> editor: the status list on the left, and <b>Active icon (Y)</b>, <b>Inactive icon (N)</b>, and <b>Conditions (first match wins)</b> for the selected status on the right.</figcaption>
</figure>

→ **Expected result:** a small window that lights when the status turns on and goes dark when
it turns off. Get stunned and the **stunned** window lights the moment the game says so.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow stunned indicator 0 0 6 3` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow stunned`
> alone prints usage and adds nothing.

Type `.indicators` to open the template editor. Pick a status, press Enter, and edit its **Id**,
**Title**, **Icon**, **Active color**, and **Inactive color**. `Tab` and `Shift+Tab` move
between fields, `Ctrl+S` saves, `Esc` cancels. `.editwindow stunned` opens the window's own
form for border and placement.

> ⚠️ **The terminal editor has five fields and no conditions.** Id, title, icon, and the two
> colors — that's the whole form. Multi-condition icon switching is authored in the desktop GUI.
> **This is an authoring gap, not a display gap:** indicators light from the game and from
> `set_status` here exactly as they do in the GUI, and a layout carrying GUI-authored conditions
> keeps working in your terminal.

→ **Expected result:** a glyph that appears in its active color when the status turns on, and
an empty window when it's off.
{{#endtab}}
{{#tab name="Mobile"}}

The phone uses fixed chrome, so you don't place indicator windows — statuses appear as small
text badges in the **status row**, the strip carrying your hands and vitals above the command
input. They light up as `STUN`, `BLEED`, `WEB`, `HIDDEN`, `INVIS`, `KNEEL`, `SIT`, `PRONE`, and
`DEAD`, and a badge is absent rather than dim when its status is off.

> ⚠️ **The phone's badge set is fixed and shorter than the desktop's.** Nine statuses have
> badges. **Poisoned, diseased, joined, and standing have none, and custom `set_status` ids
> never appear on the phone at all** — a rule you write on your phone will light the indicator
> when you next open that character on desktop, but not on the phone itself.

→ **Expected result:** a `STUN` badge appears in the status row while you're stunned and
disappears when it wears off.
{{#endtab}}
{{#endtabs}}

## Common setups

### A row of afflictions above your input line

Add **bleeding**, **stunned**, **webbed**, and **poisoned** from the catalog and send all four
to the same zone, side by side. Right-click each ▸ **Appearance** ▸ **Frame** and turn the
border off so they read as one strip instead of four boxes.

Because an inactive indicator draws nothing, the strip is empty when you're fine. Take a wound
that bleeds and exactly one icon appears.

**You'll see:** blank space when you're healthy, and a single lit icon the moment something is
wrong — the emptiness is the signal.

### An icon that changes with severity, not only on and off

Right-click your **bleeding** indicator ▸ **Edit indicators…**, select `BLEEDING`, and use
**Conditions (first match wins)** to add two conditions, in this order:

1. **Injury** — area `chest`, comparison `>=`, level `3` → pick your loudest image.
2. **Injury** — area `chest`, comparison `>=`, level `1` → pick a quieter image.

Click **Save all**.

**You'll see:** the quiet icon on a scratch and the loud one on a serious wound — severity you
can read from across the room, off one window.

### An alert the game never gave you

An indicator's name is its status id, so create the window first and then point a highlight at
it. Type `.addwindow AMBUSH indicator 0 0 8 3`, then open **Editors** ▸ **Highlights**, add a
rule matching the text you care about, and put `AMBUSH` in **Set status** with a **Status
duration** of `15`.

**You'll see:** the `AMBUSH` window light for fifteen seconds whenever that text arrives, then
clear itself. [Make your health bar shout when you're hurt](../how-to/vitals-flash.md) builds
exactly this alongside a sound and a controller buzz.

## Tips & gotchas

> ⚠️ **An indicator window's NAME is its status id. There is no picker, anywhere.**
> `.addwindow stunned indicator …` shows the `stunned` status because the window is called
> `stunned`. The **Edit indicators…** editor changes what each id *looks like* — it never
> changes which id a window shows. To point a window at a different status, make a new one with
> that name. Hotbar windows bind by name the same way.

> ⚠️ **An indicator never adds a status it doesn't already have — a dashboard does.** If you
> `set_status` an id and nothing lights, an indicator window with that exact name probably
> doesn't exist. A [dashboard](./dashboard.md) grows a new cell on its own the first time an
> unknown id fires; an indicator can't, because the window *is* the id. This is the single most
> useful difference between the two widgets.

> ⚠️ **Conditions are GUI-only; lighting is not.** The GUI editor has **Conditions (first match
> wins)**; the terminal editor has Id, Title, Icon, and the two colors. Both frontends *display*
> whatever you authored, and both light from the game and from `set_status`. Author conditions
> once in the GUI and your terminal renders the result.

**An inactive indicator draws nothing at all — it doesn't dim.** Inactive art is opt-in, in both
frontends. Set an **Inactive icon (N)** if you want something visible while the status is off;
otherwise the window is empty, and `inactive_color` has nothing to color. An empty window is
usually what you want: only real statuses take up visual space.

**Ids are matched without regard to case, but spelling is exact.** `stunned`, `STUNNED`, and
`Stunned` are the same status. `stuned` is a different one that will never light.

**The first matching condition wins.** Conditions are checked top to bottom, so a broad one
above a narrow one swallows it. Put **Injury >= 3** above **Injury >= 1**, never the other way
around. When nothing matches, the window falls back to its plain active or inactive icon.

**The standard ids are:** `standing`, `kneeling`, `sitting`, `prone`, `stunned`, `bleeding`,
`hidden`, `invisible`, `webbed`, `joined`, and `dead`, plus the afflictions `poisoned` and
`diseased`. Those same ids are the vocabulary of the **Indicator** condition in hotbars and
hand icons.

**`.testline` proves a rule without waiting for a mob.** Type `.testline` followed by the text
you're matching and it runs through the live highlight pipeline, lighting the indicator if your
rule works.

**Grayscale is a global choice, with per-status overrides.** The **Indicator Icons** editor's
**Global icon art** fold carries an **Icon set** picker and a **Grayscale when inactive**
checkbox, which desaturates a configured inactive sprite rather than fading it. Install more
icon sets with `.jinx`.

## See also

- [Dashboard](./dashboard.md) — the same statuses in one grid, and the widget that auto-adds
  unknown ids
- [Make your health bar shout when you're hurt](../how-to/vitals-flash.md) — `set_status`
  driving an indicator, with sound and rumble
- [Highlight Patterns](../customization/highlights.md) — `set_status`, `status_duration`, and
  `clear_status` in full
- [Hands](./hands.md) — the same condition vocabulary, on hand icons
- [Hotbars](./hotkeybar.md) — the same conditions, styling buttons
- [Injury Display](./injury-doll.md) — wounds by body part, where a bleeding indicator only says
  "somewhere"
- [Skins (GUI Graphics)](../customization/skins.md) — where indicator icon art comes from

<details>
<summary>Config reference (TOML)</summary>

`widget_type = "indicator"`. Set these through **Edit indicators…** (GUI) or `.indicators` and
`.editwindow` (TUI).

| Field | Type | Default | What it does |
|---|---|---|---|
| `indicator_id` | string | the window's `name` | Status to track. Matched case-insensitively. |
| `icon` | string | none | Glyph or text shown when active |
| `active_color` | string | `#00ff00` | Color while the status is on |
| `inactive_color` | string | `#555555` | Color while off — only visible if an inactive icon is set |
| `default_status` | string | none | Legacy field, kept for old layouts |
| `default_color` | string | none | Legacy field, kept for old layouts |

**Binding.** A window created by name gets `indicator_id` set to its own `name`. Writing a
different `indicator_id` into the file is honored, but nothing in either UI does that — treat
the window name as the binding.

**Icons and conditions** live in the shared indicator templates (edited in **Indicator Icons**),
not in the window. That's why two windows with the same id always look alike, and why editing a
template updates every window and dashboard cell using that id at once.

```toml
[[windows]]
name = "stunned"
widget_type = "indicator"
title = "Stunned"
indicator_id = "STUNNED"
row = 0
col = 0
rows = 2
cols = 1
active_color = "#00ff00"
show_border = false
```

</details>
