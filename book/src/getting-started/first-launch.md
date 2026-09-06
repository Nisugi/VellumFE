# First Launch

> Get your character on screen from a command line — through Lich, or straight to
> play.net — and know what you're looking at when the room description lands.

## What it's for

You already know how you connect: Lich on a port, or account-and-password to the
game. This page is the command-line route for both, so you can put VellumFE
behind a Lich launcher entry, a shell alias, or a shortcut and stop thinking
about it. The last section is a tour of the screen you land on — which window is
which, where you type, how you scroll back to the thing that killed you.

If you'd rather click a saved connection than type flags, use
[the Launcher](./launcher.md); it covers the same ground with stored passwords
and can open the native GUI, Terminal, or Vellum Despana browser surface.
Running `vellum-fe` with **no arguments at all** opens it.

> ⚠️ **The same character, started two ways, lands in two different interfaces.**
> A hand-typed command line defaults to the **terminal UI**; a new saved
> Launcher connection defaults to the **GUI**. A saved connection can instead
> select **Despana**, which opens the paired Vellum Despana browser surface
> automatically. Pass
> `--frontend gui` (or `-f gui`) to make a manual command line match the native
> GUI; use `--frontend headless` when you want a browser-only session.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Start Lich as you always do, and note the port it prints.
2. Run VellumFE with the GUI frontend pointed at that port:

   ```bash
   vellum-fe --frontend gui --port 8000 --character Rysk
   ```

   `--port` is Lich's listening port; `--host` defaults to `127.0.0.1`, so you
   only pass it when Lich runs on another machine.
3. To skip Lich entirely, connect through play.net's eAccess login instead:

   ```bash
   vellum-fe --frontend gui --direct --account myaccount --character Rysk --game prime
   ```

   Leave `--password` off. VellumFE prompts for it in the terminal
   (`Password for account myaccount:`) with the characters hidden, so the
   password never enters your shell history.

<figure class="shot" data-shot="gui/first-launch-default-layout">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A freshly connected GUI session: the <b>main</b> game feed, the <b>Room</b> window, and the command input along the bottom, with the <b>Windows / Settings / Zones / Editors</b> toolbar hubs above.</figcaption>
</figure>

→ **Expected result:** a desktop window opens, the game text starts scrolling in
the main feed, and typing `look` in the input at the bottom returns the room
description.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

1. Start Lich, then run VellumFE with no `--frontend` flag — **the terminal UI is
   the command line's default**:

   ```bash
   vellum-fe --port 8000 --character Rysk
   ```

2. For a direct play.net login, add `--direct` and your account details:

   ```bash
   vellum-fe --direct --account myaccount --character Rysk --game prime
   ```

   Omit `--password` and VellumFE prompts for it with echo off before the UI
   takes over the terminal.
3. `--account`, `--password`, and `--game` are only accepted alongside
   `--direct`. Passing them on their own is rejected before the client starts.

<figure class="shot" data-shot="tui/first-launch-connected-session">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A connected terminal session showing the bordered <b>main</b>, <b>Room</b>, and <b>thoughts</b> windows with the command line on the bottom row.</figcaption>
</figure>

→ **Expected result:** the terminal repaints into bordered windows, the game feed
fills the largest one, and your cursor sits on the command line at the bottom.
{{#endtab}}
{{#tab name="Mobile"}}

Phones have no command line to type flags into, so none of the switches above
apply. You connect on the app's own **login overlay** instead:

- The **`play.net`** tab is the direct-login equivalent — fields for
  `play.net account`, `password`, `character`, a Game selector, and a
  **Remember this login** checkbox. Submit with **Connect**.
- The **`Lich`** tab is the `--port` equivalent — `host`, `port`, and an optional
  `label`, with **Remember this connection** ticked by default.
- To play through a VellumFE session already running on your PC, tap the person
  icon and open **Characters**, then **Scan QR to add** or **Add manually**. Your
  PC produces the pairing QR with `.webinfo`.

A desktop browser shows a third **`Remote`** tab in place of that picker.

→ **Expected result:** the login overlay disappears and the game feed fills the
screen, with the **status drawer** on the right and the **macro tray** on the
left.
{{#endtab}}
{{#endtabs}}

## Common setups

### Recipe 1 — Make Lich launch VellumFE for you

In Lich's launcher, add VellumFE as a custom frontend and set the command to:

```
path\to\vellum-fe.exe --frontend gui --port %port% --key %key%
```

Lich substitutes `%port%` with the port it opened and `%key%` with the login key
it received from the game; `--key` hands that key straight to the game server, so
you are never asked for credentials. Drop `--frontend gui` from the line if you
want the terminal UI.

**Outcome:** picking your character in Lich opens VellumFE already in the room
you logged out in — no password prompt, no port to copy.

### Recipe 2 — One set of settings, several characters

`--character` is your *login* name. `--profile` is your *config* name, and it
picks which folder under `~/.vellum-fe/profiles/` holds your layout, highlights,
keybinds, and hotbars. When you leave `--profile` off, it silently copies
`--character` — which is why every new character starts with a blank-slate
layout.

Point three characters at one shared config folder:

```bash
vellum-fe --port 8000 --character Rysk    --profile hunting
vellum-fe --port 8000 --character Nisugi  --profile hunting
vellum-fe --port 8000 --character Vellum  --profile hunting
```

**Outcome:** all three log in under their own names, and all three read and write
`~/.vellum-fe/profiles/hunting/` — so a layout you save on one is there when you
log in as the next.

## Tips & gotchas

> ⚠️ **In the terminal UI, `Ctrl+C` copies your selection — it does not quit.**
> In the GUI, `Ctrl+C` **quits**. To leave either one, type `.quit`. Note that
> `.quit` disconnects but keeps the window open — run it again, or use `.exit`,
> to close the client outright.

> ⚠️ **`--profile` is not `--character`.** Omitting `--profile` makes your config
> folder follow your character name. If you rename the flag you pass, you land in
> a different folder and your layout looks lost. It is not lost — it is under the
> old profile name in `~/.vellum-fe/profiles/`.

> ⚠️ **The DragonRealms world names are spelled differently in two places.** On
> the command line they are hyphenated: `dr`, `dr-platinum`, `dr-fallen`,
> `dr-test`. In `config.toml` and in saved Launcher connections they are not:
> `dr`, `drplatinum`, `drfallen`, `drtest`. A misspelled value in `config.toml`
> does not error — it falls back to GemStone IV Prime, and you find out at the
> character list.

**The tour — what you're looking at once you're in.** The default layout places
six windows:

| Window | What lands in it |
|---|---|
| **main** | The game feed — room descriptions, combat, everything unrouted |
| **Room** | Room name, description, and exits, kept current in place |
| **thoughts** | The `thoughts` stream (ESP) |
| **speech** | The `speech` stream |
| **society** | The `society` stream |
| **command input** | Where you type, along the bottom |

Everything else — vitals bars, a compass, an injury doll, hotbars — is a window
you add yourself. In the GUI that's the **Windows** button in the top toolbar,
which opens a stay-open catalog with a checkbox per window. In the terminal,
type `.addwindow` with no arguments to get a picker. See
[Widgets](../widgets/README.md).

**Where things are.** Your command line is the bottom row of the screen; press
`Enter` to send, `Up` and `Down` to walk back through history, and `Ctrl+R` to
repeat your last command. **Scrolling back** is `PageUp` / `PageDown` a page at a
time and `Alt+PageUp` / `Alt+PageDown` a line at a time in the terminal, or the
mouse wheel and scrollbar in the GUI. `Tab` moves focus between windows in the
terminal; in the GUI you click the window you want.

**Getting help.** Anything you type starting with a dot is a client command, not
game input. Three worth knowing on day one:

- `.help` — the full dot-command list, grouped by section
- `.menu` — the main menu tree (the GUI also has toolbar hubs)
- `.settings` — the in-app settings editor

**Copying text.** In the terminal, drag-select and the text is on your clipboard
the moment you release. In the GUI, drag-select (double-click for a word, triple
for a line) and press `Ctrl+C`. Copy is plain text in both, deliberately.

**Direct mode and TLS.** Direct connections use your operating system's own TLS
stack, so Windows and macOS need nothing extra. On Linux, see
[Installation](./installation.md).

## See also

- [Installation](./installation.md) — getting the binary, and Linux TLS notes
- [The Launcher](./launcher.md) — saved connections, stored passwords, per-connection frontend
- [Vellum Despana](../frontends/despana.md) — the optional desktop browser
  workspace
- [Command Reference](../reference/commands.md) — every dot-command
- [keybinds.toml](../configuration/keybinds-toml.md) — rebinding anything in the tour above
- [Widgets](../widgets/README.md) — the windows you add to the default six

<details>
<summary>Config reference (TOML)</summary>

Everything on this page can live in `config.toml` instead of the command line.
Per-profile config is `~/.vellum-fe/profiles/<profile>/config.toml`; shared
defaults are `~/.vellum-fe/global/config.toml`.

**`[connection]`**

| Field | Type | Default | What it does |
|---|---|---|---|
| `host` | string | `"127.0.0.1"` | Address to connect to (Lich's host) |
| `port` | integer | `8000` | Port to connect to (Lich's listening port) |
| `character` | string | *(unset)* | Character name, used for Lich proxy selection and direct login |
| `account` | string | *(unset)* | play.net account, direct connections only |
| `password` | string | *(unset)* | **Stored in plain text.** Leave it unset and answer the prompt instead |
| `game` | string | `"prime"` | `prime`, `platinum`, `shattered`, `test`, `dr`, `drplatinum`, `drfallen`, `drtest`. An unrecognized value falls back to `prime` |

**CLI-vs-config precedence.** A command-line switch always wins over the file.
For each field the order is:

- **host / port** — `--host` / `--port`, then `[connection]`
- **account** — `--account`, then `connection.account`, then an error
- **password** — `--password`, then `connection.password`, then the hidden terminal prompt
- **character** — `--character`, then `connection.character`, then an error
- **game** — `--game`, then `connection.game`, then `prime`

A saved Launcher connection applied with `--launch-profile <NAME>` sits between
the two: it fills the same fields these switches would, but any switch you type
explicitly alongside it still wins. `--launch-profile` cannot be combined with
`--direct`, `--key`, or `--launcher`.

**Other switches this page uses**

| Switch | Default | What it does |
|---|---|---|
| `--frontend` / `-f` | `tui` | `tui`, `gui`, or `headless` (no local UI; use Vellum at `/play` or Vellum Despana at `/despana`) |
| `--key` | *(unset)* | Lich's `%key%` login key, passed to the game server |
| `--profile` | falls back to `--character` | Which folder under `profiles/` holds your config |
| `--data-dir` | `~/.vellum-fe` | Moves the whole config tree; equivalent to the `VELLUM_FE_DIR` environment variable |
| `--config` | *(unset)* | Read one specific `config.toml` by path |
| `--nosound` | off | Skips audio device initialization entirely |
| `--color-mode` | from config | `direct` (true color), `slot` (256-color custom palette), `indexed` (256-color standard) |
| `--setup-palette` | off | Programs the terminal palette at startup; pair with `--color-mode slot` |
| `--web-port` / `--web-bind` | from `[web]` | Turns on the embedded web server for phone/browser play |
| `--launcher` | off | Opens the graphical Launcher — also what a no-argument run does |

**Subcommands** (these run and exit instead of connecting):

- `vellum-fe validate-layout [FILE]` — check a layout file, or your current one
- `vellum-fe migrate-layout --src <DIR> [--out <DIR>] [--dry-run]` — convert old-format layouts
- `vellum-fe import-highlights <FILE.xml> [--out FILE] [--dry-run]` — convert Wrayth/StormFront highlights to TOML

</details>
