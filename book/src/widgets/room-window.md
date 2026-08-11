# Room Window

> Keep where you are on screen — name, description, what's on the ground, who's here, and
> which way out — so you never lose it to a wall of combat text.

## What it's for

The room scrolls past in your main window and then it's gone. Three creatures later you can't
remember whether that was the clearing with the north exit, and you're typing `look` again
mid-roundtime to find out.

The room window pins that information in place. It rewrites itself when you move and holds
still while you fight, so re-reading the room costs you a glance instead of a command. Every
noun in it stays clickable — the boulder, the exits, the person who just walked in.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

A room window is in your layout already — it's one of the six windows the default layout
ships. If you closed it, click **Windows** in the top toolbar and tick **Room** in the catalog.

To choose what it shows, right-click the room window. The widget section holds **Sections**
with four checkboxes: **Description**, **Objects**, **Players**, **Exits**. Untick one and it
disappears from the window as you click — settings apply live, and there is no Save button.

<figure class="shot" data-shot="widgets/room-sections-menu">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The room window's right-click menu with <b>Sections</b> expanded, showing <b>Description</b>, <b>Objects</b>, <b>Players</b>, and <b>Exits</b>.</figcaption>
</figure>

→ **Expected result:** the room window shows the name in bold, then the sections you left
ticked, reflowing as one paragraph. Unticking **Description** drops the prose and leaves the
name, the objects, and your exits.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

The room window is already in the default layout. To add one back, type
`.addwindow room room 0 0 80 10` — position and size are part of the command.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** Typing `.addwindow room`
> alone prints usage and adds nothing. Type `.addwindow` with no arguments for a picker
> instead.

To change what it shows, type `.editwindow room`. The full-screen form moves with `Tab` and
`Shift+Tab`, saves with `Ctrl+S`, and cancels with `Esc`. The four visibility toggles are on
that form.

→ **Expected result:** after `Ctrl+S`, the room window redraws with your chosen sections. The
window's border title still reads **Room** — that title is separate from the room's name.
{{#endtab}}
{{#tab name="Mobile"}}

The phone uses fixed chrome, so you don't place this window. The room lives in the **status
drawer** — swipe in from the right — as its **Room** section, which carries the room name in
bold followed by the description. The description keeps its colors and its clickable scenery,
so tapping a noun there works the same as tapping one in the text pane.

The top bar also shows the room name; tap it to swap between the room name and your character
name.

→ **Expected result:** opening the right drawer shows a **Room** heading with the current room
name and description, updating as you walk.
{{#endtab}}
{{#endtabs}}

## Common setups

### A compact "where am I" strip

You want the exits and who's here, but the prose is long and you've read it. Right-click the
room window, and under **Sections** untick **Description**. Then drag the window's bottom edge
up until it's three or four lines tall, and park it above your main text.

**You'll see:** the room name, the loose items, the other players, and `Obvious exits:` in a
block short enough to read without moving your eyes off the fight.

### A full room panel you can actually read

Keep all four sections ticked, then right-click ▸ **Appearance** ▸ **Text** and raise the text
size. Under **Appearance** ▸ **Frame**, turn the border off. Widen the window until the
description stops wrapping mid-sentence.

**You'll see:** a borderless block of room prose at a comfortable reading size, with every noun
in it still clickable.

## Tips & gotchas

> ⚠️ **`show_name` is not the border title, and the GUI ignores it.** In the terminal,
> turning it on adds the room name as a bold line *inside* the window — useful when you've
> hidden the border and lost the title with it. In the GUI the room name is always drawn as
> the first line of the content, so the setting has nothing left to do.

> ⚠️ **Objects continue the description's paragraph in the GUI, by design.** "You also see…"
> runs on from the end of the description rather than starting its own line, matching the
> Wrayth layout. If you untick **Description**, the objects become the first line instead.

**Clicking a noun does what the server said it should.** A link that carries a command sends
it. A link that is a plain noun asks the server for its verb menu and shows that menu where
you clicked. A URL opens your browser. You get the same three behaviors in the room window as
in your main text.

**Empty sections take no space.** A room with nobody in it doesn't reserve a blank line for
**Players**, so a window sized to a busy room will look under-filled in an empty one. Size it
for the rooms you hunt in.

**Room pictures are a VellumFE feature, not a game feed.** GemStone declares a `sprite` slot
on every room change and has never filled it. If you want art in this window, you map images
to room ids yourself — see [Inline Images](../customization/inline-images.md).

## See also

- [Compass](./compass.md) — the exits from this window, as a clickable rose
- [Players](./players.md) — a dedicated who's-here list, if the room window's line isn't enough
- [Text Windows](./text-windows.md) — the main feed the room would otherwise scroll away in
- [Map](./map.md) — where those exits lead
- [Build a hunting layout](../how-to/hunting-layout.md) — where the room window sits in a combat
  screen

<details>
<summary>Config reference (TOML)</summary>

`widget_type = "room"`. Set these through the right-click menu (GUI) or `.editwindow` (TUI);
this table is for reading a layout file or troubleshooting one.

| Field | Type | Default | What it does |
|---|---|---|---|
| `show_desc` | bool | `true` | Show the room description prose |
| `show_objs` | bool | `true` | Show loose items and creatures ("You also see…") |
| `show_players` | bool | `true` | Show other players ("Also here:") |
| `show_exits` | bool | `true` | Show the `Obvious exits:` line |
| `show_name` | bool | `false` | **TUI only** — add the room name as a bold line inside the content. The GUI always renders the name as the first content line and does not read this field. |
| `buffer_size` | integer | `10000` | Lines retained for scrollback |

`show_name` does not control the border title. That comes from the window's own `title`, which
the built-in template sets to `"Room"`.

```toml
[[windows]]
name = "room"
widget_type = "room"
title = "Room"
row = 0
col = 0
rows = 10
cols = 80
show_desc = true
show_objs = true
show_players = true
show_exits = true
```

</details>
