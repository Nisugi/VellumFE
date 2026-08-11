# Text Windows

> Give a stream its own window so the traffic you care about stops scrolling past in the middle
> of a fight.

## What it's for

GemStone sends far more than room descriptions down the wire. Thoughts, deaths, logons, ESP,
bounty updates, society chatter, and your own combat all arrive as separate *streams* — and by
default most of them stack into one column with everything else. The result is familiar: a
death message you wanted to read gets three lines of swing-and-miss on top of it before your
eye lands on it.

A text window is one scrollable pane bound to one or more of those streams. Point a window at
`death` and death messages go there and only there. Point one at a Lich script's own stream id
and you have a purpose-built readout for that script. Your story text gets quieter, and the
things you were squinting for get a place to sit still.

This is also how **custom windows** work. There is no separate widget for "a window for my
script" — a custom window is a text window subscribed to whatever stream id the script pushes.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**1. Create the window.**

Click **Windows** in the top toolbar, then **➕ Custom window…** ▸ **Text**. The catalog stays
open while you work.

Many streams already have a stock row in the catalog — **Thoughts**, **Deaths**, **Logons** —
so scroll the list first and tick the row if it's there. **Categories start collapsed**, so
expand a heading before deciding a row is missing. Use **➕ Custom window…** when you want a
window for a stream that has no stock row, such as a Lich script's own id.

**2. Point it at a stream.**

Right-click the new window and open the **Window** section. Type stream ids into the
**Streams** field, comma-separated, and press **Enter** to commit.

You don't have to know the ids by heart. Click the **+** beside the field for a menu of every
stream that has actually arrived this session, and pick one to append it.

**3. Set the buffer.**

**Buffer lines** in the same section caps how much scrollback the window keeps. A busy
combat window is fine at a few hundred; a thoughts window you want to read back through wants
more.

<figure class="shot" data-shot="widgets/text-windows-window-section-streams">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The window right-click menu's <b>Window</b> section, with the <b>Streams</b> field, its <b>+</b> seen-stream picker open, and <b>Buffer lines</b> below.</figcaption>
</figure>

Everything in this menu **applies live — there is no Save button.** Text fields commit when you
press **Enter** or when focus leaves the field.

The rest lives one fold deeper. **Window ▸ Advanced** holds **Compact**, **Timestamps**, and
**Timestamp at line start**. Font, text size, **Word wrap**, and content alignment are under
**Appearance ▸ Text**.

→ **Expected result:** the window appears, and the next line the game sends on that stream
lands in it instead of your main window.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**1. Place the window.**

Type `.addwindow <name> text <x> <y> <w> [h]` — for example:

```text
.addwindow thoughts text 0 0 40 10
```

`<name>` is a free label you'll use later with `.editwindow` and `.deletewindow`; `x` is the
column and `y` the row; height defaults to 10 if you omit it.

**`.addwindow` takes 1 argument or 6-plus — never 2 to 5.** With no arguments it opens a
picker. `.addwindow thoughts` on its own prints the usage line and adds nothing.

**2. Point it at a stream.**

Type `.editwindow thoughts` to open the full-screen window editor. `Tab` and `Shift+Tab` move
between fields, `Ctrl+S` saves, `Esc` cancels.

Move to the **Streams** field and press **`Ctrl+P`** for a picker of every stream seen this
session. The footer reads `[Enter: Add stream]─[Esc: Back]`. The picker **stays open after
each add**, so you can stack several streams into one window without reopening it.

If nothing has arrived yet on any custom stream, the picker won't open and the editor says
**"No custom streams seen yet this session."** That's the picker having nothing to offer, not a
broken key — type the id into the field by hand, or wait for the stream to fire once.

<figure class="shot" data-shot="widgets/text-windows-tui-stream-picker">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The TUI window editor with the <b>Streams</b> field focused and the <code>Ctrl+P</code> seen-stream picker open, footer reading <code>[Enter: Add stream]─[Esc: Back]</code>.</figcaption>
</figure>

→ **Expected result:** after `Ctrl+S`, the window draws where you placed it and the stream's
next line arrives in it.
{{#endtab}}
{{#tab name="Mobile"}}

**The phone doesn't place windows** — its chrome is fixed, and it renders from a snapshot the
host streams to it. There are no handles to drag and no catalog to tick.

Streams still get their own reading surface. They appear as **stream filter chips** in a row
above the story pane: tap a chip to show only that stream, and a numeric **unread badge** on a
chip tells you how much has arrived while you were reading something else. **Long-press a chip
to reorder** the row, and the order follows your account rather than the device.

The chip row hides itself when only the story stream exists, so it appears the moment a second
stream fires. A handful of streams never get a chip because they feed the **status drawer** and
the phone's own chrome instead of reading as prose — room, spells, bounty, society, and
experience among them.

So build your windows on the desktop and let the phone present the same session its own way.

→ **Expected result:** a chip appears above the story pane for each stream in play, badging
itself as new lines land, and tapping one filters the pane to that stream.
{{#endtab}}
{{#endtabs}}

## Common setups

### A quiet death log you can actually read back

Deaths matter and they're easy to miss. Give them a window with timestamps so you can tell
whether that was thirty seconds ago or twenty minutes.

In the GUI: **Windows** ▸ **➕ Custom window…** ▸ **Text**, then right-click it ▸ **Window** ▸
set **Streams** to `death` and **Buffer lines** to `500`. Open **Window ▸ Advanced** and tick
**Timestamps**, then tick **Timestamp at line start** so the time leads each line and the names
stay aligned down the left.

In the TUI: `.addwindow deaths text 80 0 40 8`, then `.editwindow deaths`, put `death` in
**Streams**, tick timestamps, `Ctrl+S`.

**You'll see:** a short column of death notices, each stamped with its time, sitting still
while your main window scrolls.

### A bounty window that stops eating a third of your screen

Bounty text is verbose. **Compact** rewrites it into 1-4 lines, which turns a paragraph into a
glanceable objective.

Make a text window subscribed to `bounty`, then right-click ▸ **Window** ▸ **Advanced** ▸ tick
**Compact**.

**You'll see:** your current task condensed to a few lines — and the change applies to the
bounty already on screen, not only the next update.

> ⚠️ **Compact only affects the `bounty` stream.** It is not a general blank-line remover or a
> whitespace trimmer. Ticking it on a thoughts or combat window changes nothing at all, which
> looks like a broken checkbox but is the flag correctly doing nothing outside its one job.

## Tips & gotchas

**A window that subscribes to a stream always wins.** Stream routing has a strict order: a
subscribed window first, then a `[streams.routes]` entry, then the fallback window, then `main`
as a last resort. So adding a stream to a window's **Streams** field is all it takes — you
never also have to write a route.

**Routes only decide where an *unsubscribed* stream goes.** Set one when you want a stream
discarded, forced to main, or parked in a window that isn't subscribed to it. To see the whole
picture at once, open the **Streams** panel — **Editors** ▸ **Streams & Custom Windows**, or
type `.streams`. It lists every known stream with its *effective* destination and lets you
change it in place.

> ⚠️ **A route pointing at a window that doesn't exist does not create one.** Windows are never
> auto-created or auto-opened to satisfy a route; the stream quietly falls through to your
> fallback window instead. If a route seems ignored, check the window name still exists.

**A hidden window still fills its buffer.** Hiding is presentation only — text routed to a
hidden window accumulates and is waiting when you show it again. That's the difference between
**Hide** and **Delete**.

> ⚠️ **`.deletewindow` truly removes a window in both desktop frontends; `.hidewindow` only
> hides it.** There is no GUI-versus-TUI asymmetry here. Delete stashes the window so
> **Windows** ▸ **↩ Restore deleted…** can bring it back with its streams intact, and both
> frontends refuse to remove your only main-feed window.

> ⚠️ **In the Terminal (TUI), `Ctrl+C` copies your selection — it does not quit.** In the
> Desktop GUI the same combination **quits**. Leave the TUI with `.quit` or `.exit`.

**Word wrap is not purely cosmetic here.** For a text window the **Appearance ▸ Text ▸ Word
wrap** checkbox writes to the shared layout definition, so it travels with the window into the
TUI and into `layout.toml`. Most other Appearance settings are GUI-only.

**Per-window timestamp settings override the global one.** `ui.timestamp_position` sets the
default for everything; a window that has its own **Timestamp at line start** state ignores it.
Change the global in **Settings** and a window you've already customized will not follow.

**Scrolling:** `PageUp` / `PageDown` move a 20-line page in the focused TUI window,
`Alt+PageUp` / `Alt+PageDown` move a single line, and the mouse wheel scrolls whatever is under
the cursor in both desktop frontends. A window auto-scrolls with new text unless you have
scrolled back, so reading history doesn't get yanked out from under you.

## See also

- [Tabbed Text](./tabbed-text.md) — several streams sharing one window, with unread badges
- [Build a hunting layout](../how-to/hunting-layout.md) — placing and saving a full screen
- [Desktop GUI](../frontends/gui.md) — the Windows catalog and the full right-click menu
- [Terminal (TUI)](../frontends/tui.md) — `.editwindow`, title-bar dragging, resize handles
- [layout.toml](../configuration/layout-toml.md) — every window field and its default

<details>
<summary>Config reference (TOML)</summary>

A text window is `widget_type = "text"`. Reference only — the **Streams** field and
`.editwindow` write all of this for you.

```toml
[[windows]]
name = "deaths"
widget_type = "text"
streams = ["death"]
row = 0
col = 80
rows = 8
cols = 40
buffer_size = 500
show_timestamps = true
timestamp_position = "start"
```

### Text widget fields

| Field | Type | Default | What it does |
|---|---|---|---|
| `streams` | array of string | `[]` | Stream ids this window subscribes to. A subscribed window outranks every `[streams.routes]` entry. |
| `buffer_size` | integer | `10000` | Lines of scrollback kept in memory. |
| `wordwrap` | bool | `true` | Wrap long lines to the window width. Shared with the GUI's **Appearance ▸ Text ▸ Word wrap**. |
| `show_timestamps` | bool | `false` | Prefix or suffix each line with its arrival time. |
| `timestamp_position` | `"start"` / `"end"` | inherits `ui.timestamp_position` (`"end"`) | Where the timestamp sits. Set here, it overrides the global setting for this window only. |
| `compact` | bool | `false` | Condenses verbose **bounty** text to 1-4 lines. **No effect on any other stream.** |

Position and size (`row`, `col`, `rows`, `cols`) and the appearance fields are common to all
windows — see [layout.toml](../configuration/layout-toml.md).

### Stream routing (`config.toml`)

Routing decides where a stream goes when **no** window subscribes to it.

```toml
[streams]
fallback = "main"

[streams.routes]
bounty = "window:bounty"
ambients = "discard"
loot = "main"
```

| Field | Type | Default | What it does |
|---|---|---|---|
| `fallback` | string | `"main"` | Window that receives any unsubscribed, unrouted stream. |
| `routes.<stream>` | `"discard"` / `"main"` / `"window:<name>"` | none | Per-stream destination for unsubscribed streams. Any other value is rejected when the config loads. |
| `room_in_main` | bool | `true` | DragonRealms: keeps `<streamWindow id='room'>` from switching the current stream. |

A `window:` route naming a window that doesn't exist falls back to `fallback`, then `main`. The
older `drop_unsubscribed` list is migrated automatically to `routes.<id> = "discard"` on load.

### Common stream ids

| Stream | Content |
|---|---|
| `main` | Primary game output |
| `speech` | Player dialogue |
| `thoughts` | ESP / telepathy |
| `death` | Death messages |
| `familiar` | Familiar messages |
| `logons` | Login and logout notices |
| `society` | Society messages |
| `bounty` | Bounty information |
| `loot` | Loot messages |
| `announcements` | Announcements |
| `ambients` | Ambient messages |

`streams` is not limited to this list. Any id a Lich script pushes can feed a text window —
that is exactly what a custom window is.

</details>
