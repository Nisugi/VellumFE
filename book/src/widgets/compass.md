# Compass

> See every exit the room actually has, and click one to walk it — no guessing which
> directions the room accepts, no typing `n` into a wall.

## What it's for

The `Obvious exits:` line tells you where you can go, then scrolls away. So you guess, and the
game tells you that you can't go that way, and you guess again — which is a small tax you pay
in every unfamiliar room and a real one when something is chasing you.

The compass shows the current room's exits as a lit rose, updating the moment you arrive. Lit
directions are real exits; the rest stay faint. Click a lit one and you move. It's the fastest
way to feel out an unmapped area, and it never scrolls off.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Navigation**, and tick **Compass**. Use the row's
   **zone** control to send it somewhere it'll live — the **Right Bar** is a good home.
   (Typed equivalent: `.addwindow compass compass 92 15 28 7`.)
2. Right-click the compass to pick its art. The widget section has a **Compass art** drop-down
   listing **Skin default**, **None**, and any compass sets you've installed. **None** draws
   the built-in vector rose, which follows your theme colors.

Install more compass sets with `.jinx list` and `.jinx install`.

<figure class="shot" data-shot="widgets/compass-art-picker">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The compass window's right-click menu with the <b>Compass art</b> drop-down open on <b>Skin default</b>, <b>None</b>, and an installed set.</figcaption>
</figure>

→ **Expected result:** a rose appears with your current room's exits lit and the rest dimmed.
Clicking a lit direction walks you that way and the rose redraws for the new room.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow compass compass 92 15 28 7` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow compass`
> alone prints usage and adds nothing.

The terminal draws the compass as arrow glyphs in a small character grid — `▲` for north, `◀`
for west — with **up** and **down** as `↑` and `↓` and `o` in the middle for **out**. Available
exits take the active color (green by default), the rest go dark gray. `.editwindow compass`
opens the form for border and color; `Ctrl+S` saves, `Esc` cancels.

> ⚠️ **The terminal cannot draw compass art, and that's deliberate — not a missing feature.**
> Compass sets are images. The TUI renders glyphs and text, so the **Compass art** picker is
> GUI-only. Everything else — which exits are lit, clicking to move — works the same.

→ **Expected result:** a grid of arrows with your live exits colored. Clicking a lit arrow
sends that movement command.
{{#endtab}}
{{#tab name="Mobile"}}

You don't place windows on the phone, but you do get a compass: a small floating rose sits over
the text pane with the current room's exits lit. Tap a lit direction to walk it.

Two behaviors worth knowing. **It hides itself when the room reports no exits**, so a blank
spot over your text means the room has none rather than something being broken. And you can
move it — **press and hold it for about half a second** until it lifts, then drag it anywhere
over the text pane. The position is remembered on that device.

→ **Expected result:** tapping a lit direction moves you and the rose relights for the new
room. A long-press lifts the whole compass so you can park it out of your reading line.
{{#endtab}}
{{#endtabs}}

## Common setups

### A big rose in the corner of a hunting screen

Add the compass, send it to the **Right Bar**, then drag its corner out until it's as large as
you want. Right-click ▸ **Appearance** ▸ **Frame** and turn the border off so the art sits on
your background with nothing boxing it in.

**You'll see:** a large, borderless rose you can hit without aiming, with the room's real exits
lit.

### A thin strip that doesn't cost you screen

Drag the compass window's side edge inward until it's narrow. The rose holds its size and the
window just gets tighter around it, down to a 48-point floor — narrower than any other widget
is allowed to go.

**You'll see:** a compass small enough to tuck beside a vitals bar while staying clickable.

## Tips & gotchas

> ⚠️ **The compass window is free-form — it is not locked to a square.** The rose paints as a
> centered square of whichever side is shorter. Drag one edge and the window grows while the
> art holds its size and the extra space pads evenly around it. Drag both and the art scales
> up with the window.

> ⚠️ **Compass windows have a 48-point minimum width; every other widget stops at 120.** If
> you're wondering why the compass shrinks past where your other windows stop, that's why.

**"Skin default" and "None" are different answers.** **Skin default** takes whatever rose your
active skin ships, so changing skins changes your compass. **None** pins you to the built-in
vector rose, which follows your theme's link color for lit exits and stays put across skins.

**Up, down, and out are in the rose, not beside it.** Skinned sets have them drawn in; the
vector rose puts them at the hub. Nothing lives in a side column, which is what lets the window
shrink to 48 points and stay usable.

**A dim direction is not a broken compass.** The rose lights what the game reports for the
current room. Unlit means the game didn't list that exit — including hidden ones you haven't
found yet.

## See also

- [Room Window](./room-window.md) — the same exits as text, alongside the description
- [Map](./map.md) — where those exits lead, once you've walked them
- [Travel (.go2)](./travel.md) — walking somewhere further than one room
- [Skins (GUI Graphics)](../customization/skins.md) — where compass art comes from and how to
  install sets
- [Build a hunting layout](../how-to/hunting-layout.md) — placing the compass in a combat screen

<details>
<summary>Config reference (TOML)</summary>

`widget_type = "compass"`. The compass has no widget-specific data block — its colors come from
the window's own fields, and its art from the GUI **Compass art** picker (stored with your GUI
settings, not in the layout).

| Field | Type | Default | What it does |
|---|---|---|---|
| `active_color` | string | `green` | **TUI** — color for available exits |
| `inactive_color` | string | `dark gray` | **TUI** — color for unavailable directions |
| `show_border` | bool | `true` | Draw the window border |
| `border_style` | string | theme | Border style, for example `"rounded"` |

**Sizing.** The terminal draws into a small character grid; the built-in template is 9×5 to
leave room for a border. In the GUI, size is free-form and the minimum width is 48 points.

```toml
[[windows]]
name = "compass"
widget_type = "compass"
title = "Compass"
row = 15
col = 92
rows = 5
cols = 9
show_border = true
border_style = "rounded"
```

</details>
