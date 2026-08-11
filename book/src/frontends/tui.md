# Terminal (TUI)

> Play GemStone IV wherever a terminal reaches — over SSH, on a Raspberry Pi, in a tmux split
> next to your scripts — with the same layout, highlights, and keybinds as the desktop client.

## What it's for

You already have a terminal open. The TUI puts your whole character in it: the same windows,
the same highlight rules, the same numpad movement macros as the Desktop GUI, drawn on a
character grid instead of pixels. It is what a hand-typed `vellum-fe` command gives you by
default, and it is the only frontend that survives a session over SSH on a machine with no
desktop at all.

It is not a stripped-down mode. Mouse works — for links, scrolling, selection, dragging
windows, and right-click menus. And the TUI **keeps its own window editor**, a full-screen
form you open with `.editwindow`; the GUI's separate editor was retired in favor of a
right-click menu, so this form has no direct counterpart over there.

<figure class="shot" data-shot="tui/tui-hunting-layout">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A hunting layout in a 24-bit-color terminal: main text left, vitals and roundtime across the top, thoughts and combat stacked at right.</figcaption>
</figure>

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

The GUI does not open the terminal frontend, but it does decide whether a *saved connection*
opens in one. In the **VellumFE Launcher**, click **Edit** on a connection, open **Advanced**
▸ **Frontend**, and choose **Terminal**. Save.

**A saved connection defaults to GUI**, while the `--frontend` command-line flag defaults to
`tui` — so a character you always launched from a terminal will open in a window the first
time you launch it from the Launcher instead.

<figure class="shot" data-shot="gui/tui-launcher-frontend-terminal">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The Launcher's <b>Advanced</b> ▸ <b>Frontend</b> submenu with <b>Terminal</b> selected for a saved connection.</figcaption>
</figure>

→ **Expected result:** clicking **Launch** on that row starts the character in a terminal
rather than a native window.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

1. Connect through Lich: `vellum-fe --port 8000 --character YourName`. You do not pass
   `--frontend`, because `tui` is already the default.
2. Or connect straight to play.net with no Lich:
   `vellum-fe --direct --account YOURACCOUNT --game prime --character YourName`. Omit
   `--password` and you'll be prompted for it without it landing in your shell history.
3. On a terminal limited to 256 colors, add `--color-mode slot --setup-palette` so VellumFE
   reprograms the palette to your configured colors at startup.
4. Launch a connection you saved in the GUI without leaving the terminal:
   `vellum-fe --frontend tui --launch-profile "YourConnectionName"`.

→ **Expected result:** your layout draws on the character grid, the command input takes the
cursor, and game text scrolls in the main window.
{{#endtab}}
{{#tab name="Mobile"}}

The phone has no terminal frontend, and the apps do not run one. What it can do is **join a
TUI session running on your PC**: start the TUI with the web server on
(`vellum-fe --port 8000 --character YourName --web-port 8080`), type `.webinfo` for the
pairing URL and QR code, then open that URL in the phone browser or scan the QR from the
app's **Characters** picker (person icon ▸ **Characters** ▸ **Scan QR to add**).

The phone renders the session with its own fixed chrome — the right **status drawer**'s
**Targets**, **Players**, and **Room** sections — not your terminal layout. Layout, palette,
and window-editor work stay on the machine running the TUI.

→ **Expected result:** the phone shows the same game text and vitals as the terminal, and a
command typed on either lands in the game once.
{{#endtab}}
{{#endtabs}}

## Working with windows

Everything in this section is mouse-and-keyboard in the terminal itself.

**Move a window** by left-dragging its **title bar** — the top row, and only the top row.
(The GUI lets you drag from anywhere; the TUI reserves the body for text selection.) A window
locked in place can still be grabbed there, but will not move.

**Resize a window** by dragging its **right column**, **bottom row**, or the **bottom-right
corner cell**. Two details save you a puzzled minute: the bottom row only acts as a handle
when the window is taller than two rows, and the right column only when it is wider than one
— so a single-row bar's bottom row stays content, not a grab handle. A locked window resizes
nowhere.

**Right-click a title bar** for **Close Window**, **Edit Window...**, and **Open Menu**.
(**Close Window** is not offered on `main` or `command_input`.) Right-clicking the
performance overlay opens its own metrics toggle menu instead.

**Add, hide, delete, and restore** windows by command:

| Goal | Command |
|---|---|
| Add a window | `.addwindow` for a picker, or `.addwindow <name> <type> <x> <y> <w> [h]` |
| Hide a window, keep it in the layout | `.hidewindow` / `.hidewin [name]` (no name opens a picker) |
| Truly remove a window from the layout | `.deletewindow` / `.delwindow <name>` — stashed so it can be restored |
| Bring a hidden window back | `.menu` ▸ **Windows** submenu |
| Save / load a layout | `.savelayout [name]` · `.loadlayout [name]` · `.layouts` to list |
| Refit the layout after resizing the terminal | `.resize` |

**`.deletewindow` truly deletes in both the TUI and the GUI**, and `.hidewindow` hides in
both — there is no asymmetry to remember. Both refuse to remove your only main-feed window.
There is no standalone `.showwindow`; restoring is the `.menu` route or `.loadlayout`.

## The window editor

`.editwindow` (or `.editwin`) opens a full-screen form for the focused window, or
`.editwindow <name>` for a specific one — the same form the title-bar right-click reaches via
**Edit Window...**. It is keyboard-driven, and the bottom border always tells you the keys
for wherever you are.

| Key | Does |
|---|---|
| `Tab` / `Shift+Tab` | Next / previous field |
| `Ctrl+S` | Save and close |
| `Esc` | Cancel, or back out of a sub-editor |
| `Ctrl+P` | **On the Streams field only** — open a picker of every stream seen this session |

The base footer reads `[Ctrl+S: Save] [Esc: Cancel]`. Step into a sub-editor and it changes to
match: reorderable lists show `[Ctrl+S: Save]─[Shift+↑/↓: Reorder]─[Esc: Cancel]`, and the
stream picker shows `[Enter: Add stream]─[Esc: Back]`.

That `Ctrl+P` picker is the one to remember. Subscribing a window to a stream normally means
typing an id you have to already know; on the Streams field it lists what Lich has actually
pushed this session, and `Enter` appends the highlighted id to the field. It does nothing on
any other field, so if it seems dead, check which field has focus.

<figure class="shot" data-shot="tui/tui-window-editor-stream-picker">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The TUI window editor on the Streams field with the <code>Ctrl+P</code> seen-streams picker open, footer reading <code>[Enter: Add stream]─[Esc: Back]</code>.</figcaption>
</figure>

## Keys you get out of the box

These are the defaults in `keybinds.toml`, and every one of them is rebindable with
`.keybinds`.

**Text and windows**

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | Switch focused window (dot-command completion takes priority when one is offered) |
| `PageUp` / `PageDown` | Scroll the focused window one page (20 lines) |
| `Alt+PageUp` / `Alt+PageDown` | Scroll one line |
| `Up` / `Down` | Previous / next command in history |
| `Ctrl+F` | Start search |
| `Ctrl+PageUp` / `Ctrl+PageDown` | Previous / next search match |
| `Esc` | Close a menu, picker, or editor; exit the current mode |
| `F12` | Toggle the performance overlay |

**Numpad movement, pre-bound**

`num_1`–`num_9` walk the compass the way the keypad is laid out (`1`=southwest, `2`=south,
`3`=southeast, `4`=west, `5`=`out`, `6`=east, `7`=northwest, `8`=north, `9`=northeast).
`num_0` goes down and `num_decimal` goes up. `num_plus` sends `look`, `num_minus` sends
`info`, `num_multiply` sends `exp`, and `num_divide` sends `health`.

**Editing the command line** — these are fixed terminal conventions rather than entries in
`keybinds.toml`: `Ctrl+A` select all, `Ctrl+C` copy, `Ctrl+X` cut, `Ctrl+V` paste, `Ctrl+Z`
undo, `Ctrl+Shift+Z` redo, `Ctrl+E` jump to end, `Ctrl+U` clear the line, `Ctrl+W` delete the
word behind the cursor.

**As you type**, the newest history entry sharing your prefix appears as muted text from the
cursor onward. `Tab` accepts it. When there is no suggestion and no dot-command completion
pending, `Tab` falls back to switching windows. The suggestion uses the theme's
`text_secondary` color; set `completion_color` on the command-input window to change it.

## Selecting, copying, and clicking

Drag across text to select it, and **it copies to your clipboard the moment you release the
mouse** — no second keystroke. That is `selection_auto_copy`, on by default. Copy is
plain text by design; styling is deliberately not carried to the clipboard.

Clicking game text does what the markup says: a URL opens your browser, a command link sends
its command, a coordinate link runs its command list, and a plain noun **asks the server for
its context menu** and shows what comes back. **Ctrl+drag** a link onto a hand, a container,
an inventory row, or another link to send `_drag`, which is how you move an item without
typing a target.

## Common setups

### A 256-color terminal that still looks right

Some terminals — an older `xterm`, a locked-down remote console — cap at 256 colors, and true
color output turns muddy. Start with `vellum-fe --port 8000 --character YourName --color-mode
slot --setup-palette`. Slot mode maps your configured colors into palette slots and
`--setup-palette` programs them into the terminal at startup. If you change colors mid-session,
run `.setpalette` to reprogram, and `.resetpalette` to hand the terminal's own palette back
before you exit.

**You'll see:** your named highlight colors rendering as the exact hues you chose in
`.colors`, instead of the nearest muddy approximation.

### A layout that survives a terminal resize

Arrange your windows by dragging title bars and edges, then save with
`.savelayout hunting`. When you later stretch the terminal or drop into a different-sized
tmux pane, run `.resize` to refit the saved arrangement to the new dimensions rather than
rebuilding it. `.layouts` lists what you have saved, and `.loadlayout hunting` brings it
back — including on the Desktop GUI, which reads the same pool.

**You'll see:** your combat window still occupying the right third of the screen after the
pane changes size, rather than clipped off the edge or stranded in the middle.

## Tips & gotchas

> ⚠️ **In the TUI, `Ctrl+C` copies your selection — it does not quit.** A raw-mode terminal
> owns its own keys, so nothing intercepts it. To leave, type `.quit` or `.exit`. **In the
> Desktop GUI the same combination quits**, because there the OS delivers it beneath the key
> layer. This is the one difference most likely to bite you when you switch frontends.

> ⚠️ **`.quit` disconnects but keeps the client open by default.** Run it again, or use
> `.exit`, to close. Change it with `ui.keep_open_on_quit` in Settings.

- **The terminal cannot render images, and that is by design, not a missing feature.**
  Skins, per-window background art, and the graphical injury doll, compass, and hand icons
  are GUI-only forever — there are no pixels inside a character cell. The TUI shows the same
  underlying information with glyphs and color, so nothing about your character is hidden
  from you. If a layout you built in the GUI has skin frames, they load without complaint and
  simply have nothing to draw.
- **`.setpalette` needs slot mode.** On `direct` (true color) it has nothing to program.
  Pair it with `--color-mode slot`.
- **Reset the palette before you leave a shared terminal.** `.resetpalette` restores the
  terminal's own colors; without it a multiplexer session can keep your palette after you
  disconnect.
- **Use a Nerd Font** if you want the default countdown glyphs and compass rendering exactly
  as designed. Without one they fall back to plainer characters.
- **There is no Launcher in the terminal.** It is an egui window, so it is GUI-only. Start a
  saved connection from a terminal with `--launch-profile "<name>"`.
- **A one-row window has no bottom resize handle.** That is deliberate — its bottom row is
  content. Resize it from the right column instead, or adjust it in `.editwindow`.
- **`.menu` has no default keybind.** Type it, or use **Open Menu** from a title-bar
  right-click.

## See also

- [Frontends overview](./README.md) — all five, and how to choose
- [Desktop GUI](./gui.md) — the mouse-first counterpart with skins and toolbar hubs
- [Mobile Web](./web.md) — joining a TUI session from your phone
- [Widgets](../widgets/README.md) — what can go in a window
- [Configuration Files](../configuration/README.md) — keybinds, highlights, colors

<details>
<summary>Config reference (TOML)</summary>

**Command-line flags that matter to the TUI**

| Flag | Type | Default | What it does |
|---|---|---|---|
| `--frontend` | `tui` \| `gui` \| `headless` | `tui` | The TUI is the default; pass it explicitly to override a saved connection. |
| `--color-mode` | `direct` \| `slot` \| `indexed` | from `config.toml` | `direct` = 24-bit RGB; `slot` = 256-color custom palette; `indexed` = standard 256-color, the safe fallback. |
| `--setup-palette` | flag | off | Runs `.setpalette` at startup. Use with `--color-mode slot`. |
| `--launch-profile <NAME>` | string | — | Runs a saved connection from `launcher.toml` in this frontend. |
| `--port` / `--host` | u16 / string | `8000` / `127.0.0.1` | Lich proxy target; overrides `config.toml`. |
| `--direct` | flag | off | Connect to play.net without Lich; enables `--account`, `--password`, `--game`. |
| `--nosound` | flag | off | Skips audio device initialization entirely. |

**Settings the TUI reads** (`config.toml`)

| Field | Type | Default | What it does |
|---|---|---|---|
| `ui.selection_auto_copy` | bool | `true` | Copy a drag-selection to the clipboard on mouse-up. Turn off and selection stays visible without copying. |
| `ui.keep_open_on_quit` | bool | `true` | `.quit` disconnects but leaves the client open; `.quit` again or `.exit` closes it. |
| `ui.color_mode` | `"direct"` \| `"slot"` \| `"indexed"` | `"direct"` | Terminal color rendering, same values as `--color-mode`. |

**Per-window field used above** (`layout.toml`)

| Field | Type | Default | What it does |
|---|---|---|---|
| `completion_color` | color name or hex | theme `text_secondary` | Color of the inline history suggestion in the command-input window. |

**TUI-only dot-commands**

| Command | What it does |
|---|---|
| `.setpalette` | Load your palette colors into the terminal (slot color mode). |
| `.resetpalette` | Restore the terminal's default palette. |
| `.resize` | Refit the layout to the current terminal size. |
| `.transparent` | Toggle transparent window backgrounds. |

The in-app `.help` footer states the same split: commands marked `(GUI)` need the desktop
GUI, and `.setpalette` / `.resetpalette` / `.resize` are TUI-only.

</details>
