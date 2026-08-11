# Tabbed Text

> Stack several streams into one window and let the tab badges tell you which one moved.

## What it's for

Giving every stream its own window works right up until you run out of screen. Speech,
thoughts, whispers, group chat, deaths, logons — six windows down a 40-column side bar leaves
nothing for the game itself, and most of them sit empty most of the time anyway.

A tabbed text window puts them in one frame with a tab strip. You read one tab at a time, and
the other tabs mark themselves unread when something lands. That's the real payoff: you stop
paying screen rent for streams that are quiet, without going blind to them. A whisper arrives
while you're watching combat, the **Whispers** tab lights up, and you click over when the
swing is done.

Tabs you'd rather not be nagged about — a chatty group channel, a logon feed — can be set
**Quiet**, so they still collect text but never mark themselves unread.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**1. Create the window.**

Click **Windows** in the top toolbar, then **➕ Custom window…** ▸ **Tabbed text**.

**2. Open the Tab Editor.**

Right-click the new window and choose **Edit tabs…** from the window's own section. This opens
the **Tab Editor**, a separate panel rather than an inline menu fold.

> ⚠️ **The Tab Editor is Save-buffered — the rest of the right-click menu is not.** Everywhere
> else in a window's menu your change applies the instant you make it. Here, nothing takes
> effect until you click **Save**. Closing the panel without saving discards your edits.

**3. Add your tabs.**

Two routes, and they work together:

- **The Known streams checklist.** Tick a stream to give it its own tab, named for the stream.
  Untick it to remove it from **every** tab. This is the fast way to build a window from
  scratch. The list covers every stream the client knows about — the well-known ones plus
  anything your sessions have ever declared — not only what has arrived today.
- **The grid.** One row per tab: **Name**, **Streams**, **Quiet**, **TS**, reorder, **Remove**.
  Use it to rename a tab, put several streams on one tab, or set the order. The **+** beside a
  row's Streams field lists streams seen **this session** and appends one to that tab.

Reorder with the **▲** and **▼** buttons on a row — the tab strip follows the grid's order.

<figure class="shot" data-shot="widgets/tabbed-text-tab-editor">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Tab Editor</b>: the grid of <b>Name</b> / <b>Streams</b> / <b>Quiet</b> / <b>TS</b> rows with reorder and <b>Remove</b>, the <b>Known streams</b> checklist below, and <b>Add tab</b> and <b>Save</b>.</figcaption>
</figure>

**4. Save.**

Click **Save**. Two rules are enforced: a tabbed window needs **at least one tab**, and
**every tab needs a name** — you'll get a message rather than a silent failure if either is
broken.

→ **Expected result:** the tab strip redraws with your tabs in order, and the next line on any
subscribed stream lands on its tab, badging that tab unread if it isn't the one you're reading.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

**1. Place the window.**

```text
.addwindow comms tabbedtext 80 0 40 20
```

`<name>` is a free label; `x` is the column and `y` the row. **`.addwindow` takes 1 argument or
6-plus — never 2 to 5**; with no arguments it opens a picker instead.

**2. Edit the tabs.**

Type `.editwindow comms`, move to the tab list, and work it with the keys shown in the footer:

```text
[A: Add]─[E: Edit]─[Del: Delete]─[Shift+↑/↓: Re-order]─[Esc: Back]
```

`A` adds a tab and `E` edits the selected one, both opening a small form with four fields in
order: **Name**, **Streams**, **Timestamps**, **Ignore activity**. `Tab` and `Shift+Tab` move
between fields.

On the **Streams** field, press **`Ctrl+P`** for a picker of streams seen this session —
footer `[Enter: Add stream]─[Esc: Back]`. It stays open after each add, so you can put several
streams on one tab in a row.

Press `Ctrl+S` to save the window, or `Esc` to cancel.

<figure class="shot" data-shot="widgets/tabbed-text-tui-tab-list">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The TUI window editor's tab list for a <code>tabbedtext</code> window, footer reading <code>[A: Add]─[E: Edit]─[Del: Delete]─[Shift+↑/↓: Re-order]─[Esc: Back]</code>.</figcaption>
</figure>

→ **Expected result:** the tab strip shows your tabs in the order you set, and a tab marks
itself unread when text arrives on it while you're reading another.
{{#endtab}}
{{#tab name="Mobile"}}

**The phone has no tabbed window, because its whole story pane already works this way.**
Chrome is fixed, so you don't place or configure windows here — but the job a tabbed window
does is exactly what the **stream filter chips** above the story pane do.

Each stream in play gets a chip. Tap one to filter the pane to that stream, and a numeric
**unread badge** on a chip counts what has arrived since you last looked — the same signal a
tab's unread mark gives you on the desktop. **Long-press a chip to reorder** the row, which is
the equivalent of dragging tabs into the order you read them; that order follows your account
rather than the device.

The chip row hides when there's only the story stream, so it appears as soon as a second
stream fires. Streams that feed the **status drawer** and the phone's fixed chrome rather than
reading as prose — room, spells, bounty, society, experience — get no chip by design.

Build and tune the tabbed window on the desktop; the phone presents the same session through
chips.

→ **Expected result:** a chip row above the story pane, one chip per active stream, each
badging itself as lines arrive, and tapping one filters the pane to that stream.
{{#endtab}}
{{#endtabs}}

## Common setups

### A communication hub

One window carrying every conversational channel, with the noisy one muted.

Make a **Tabbed text** window and open **Edit tabs…**. Build four rows:

| Name | Streams | Quiet | TS |
|---|---|---|---|
| Speech | `speech` | | ✓ |
| Thoughts | `thoughts` | | ✓ |
| Whispers | `whisper` | | ✓ |
| Group | `group` | ✓ | |

Timestamps on the three you'll read back through; **Quiet** on **Group** so a busy party
channel doesn't badge the window every few seconds. Click **Save**.

**You'll see:** one window where four used to go, with **Whispers** badging itself the moment
someone whispers to you — and **Group** collecting quietly, unread mark never lighting.

### An activity feed for a hunting screen

Combat, deaths, and arrivals in one 40-column pane so your side bar keeps room for targets and
a compass.

Tabs: **Combat** on `combat`, **Deaths** on `death`, and **Arrivals** on `logons` with
**Quiet** ticked — you want to be able to check who walked in without being pulled away from a
fight every time somebody logs on.

In the TUI, that's `.addwindow activity tabbedtext 80 0 40 12`, then `.editwindow activity` and
`A` three times.

**You'll see:** your fight in the front tab, a **Deaths** tab that badges when something dies
near you, and an **Arrivals** tab that fills silently until you go looking.

## Tips & gotchas

> ⚠️ **The Tab Editor is Save-buffered; the rest of the window menu applies live.** This is the
> one place in the GUI's per-window settings where your changes are not already in effect. If
> tab edits "didn't take," you closed the panel without clicking **Save**.

**Unticking a stream in Known streams removes it from *every* tab, not just the one you're
looking at.** If that untick leaves a tab with no streams, the tab goes away. Tabs that were
already empty — a row you just added and haven't filled — are left alone. Remove a stream from
one tab only by editing that row's **Streams** field instead.

**The two stream pickers in the Tab Editor are different lists.** The **Known streams**
checklist is the full universe the client knows about. The per-row **+** offers only streams
seen **this session**. A stream you expect that hasn't fired yet appears in the checklist but
not in the **+** menu.

**Quiet is per tab, and it's about the unread mark, not the text.** A Quiet tab still receives
and buffers everything; it just never badges. That's the setting you want for a channel you
check on purpose rather than react to.

**`buffer_size` on a tabbed window is per tab**, not shared across the window, and it defaults
to 10,000 lines.

> ⚠️ **Word wrap on a tabbed window is GUI-only.** For a plain text window, **Appearance ▸ Text
> ▸ Word wrap** writes to the shared layout definition and travels to the TUI. A tabbed window
> has no `wordwrap` field, so the setting is remembered per GUI tab and does not reach
> `layout.toml` or the terminal.

**Three tab-switching actions exist, and none has a default key.** **Next Tab**, **Previous
Tab**, and **Next Unread Tab** live under the **Tabs** category in the keybinds editor. Bind
**Next Unread Tab** if you like — it jumps straight to whatever just badged, which is the whole
point of the badges.

**A subscribed tab outranks stream routing.** A tab that lists a stream receives it regardless
of any `[streams.routes]` entry; routes only decide where *unsubscribed* streams go. Check
effective destinations in the **Streams** panel — **Editors** ▸ **Streams & Custom Windows**,
or `.streams`.

> ⚠️ **In the Terminal (TUI), `Ctrl+C` copies your selection — it does not quit.** In the
> Desktop GUI the same combination **quits**. Leave the TUI with `.quit` or `.exit`.

## See also

- [Text Windows](./text-windows.md) — one stream, one window, and the full routing rules
- [Build a hunting layout](../how-to/hunting-layout.md) — where a tabbed window earns its keep
  on a small screen
- [Desktop GUI](../frontends/gui.md) — the Windows catalog and the window right-click menu
- [Terminal (TUI)](../frontends/tui.md) — `.editwindow` and the editor's key map
- [layout.toml](../configuration/layout-toml.md) — every window field and its default

<details>
<summary>Config reference (TOML)</summary>

A tabbed window is `widget_type = "tabbedtext"`, with one `[[windows.tabs]]` block per tab.
Reference only — **Edit tabs…** and `.editwindow` write all of this.

```toml
[[windows]]
name = "comms"
widget_type = "tabbedtext"
row = 0
col = 80
rows = 20
cols = 40
buffer_size = 3000

[[windows.tabs]]
name = "Speech"
streams = ["speech"]
show_timestamps = true

[[windows.tabs]]
name = "Thoughts"
streams = ["thoughts"]
show_timestamps = true
timestamp_position = "start"

[[windows.tabs]]
name = "Group"
streams = ["group"]
ignore_activity = true
```

### Window fields

| Field | Type | Default | What it does |
|---|---|---|---|
| `tabs` | array of tab tables | `[]` | The tab list, in display order. |
| `buffer_size` | integer | `10000` | Scrollback lines kept **per tab**. |
| `tab_bar_position` | string | `"top"` | Where the tab strip sits. |
| `tab_separator` | bool | `false` | Draw a separator between tabs. |
| `tab_active_color` | string | theme | Color of the active tab label. |
| `tab_inactive_color` | string | theme | Color of inactive tab labels. |
| `tab_unread_color` | string | theme | Color of a tab with unread content. |
| `tab_unread_prefix` | string | none | Marker prefixed to an unread tab's label. |

The six presentation fields from `tab_bar_position` down have no editor UI today — they are
TOML-only. The Tab Editor covers names, streams, Quiet, timestamps, and order.

### Tab fields

| Field | Type | Default | What it does |
|---|---|---|---|
| `name` | string | required | The tab's label. Saving with an unnamed tab is refused. |
| `streams` | array of string | `[]` | Stream ids feeding this tab. **Preferred form.** |
| `stream` | string | none | Legacy single-stream field. **When both are set, `streams` wins.** |
| `show_timestamps` | bool | inherits `ui.show_timestamps` | Per-tab timestamp override. |
| `timestamp_position` | `"start"` / `"end"` | inherits `ui.timestamp_position` (`"end"`) | Per-tab timestamp placement. |
| `ignore_activity` | bool | `false` | The **Quiet** checkbox — this tab never marks itself unread. |

A tab with an empty `streams` array and no `stream` receives nothing. Saving through the GUI
Tab Editor writes `streams` and clears the legacy `stream` field, so any tab you touch there
is migrated to the plural form.

</details>
