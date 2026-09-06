# Frontends

> One character, one core, six ways to sit down and play — pick the one that
> matches where you are right now.

## What it's for

You hunt at your desk, but you also want to check on a bounty from the couch, or keep a
character parked over SSH on a box that has no desktop at all. VellumFE gives you six
interfaces over **one core**, so connections, parsing, game state, and command dispatch
stay consistent while each frontend presents them for its own screen and controls.

The differences between the six are about **what the surface can physically do** — a
terminal cannot draw a skin image, a phone cannot let you drag a window to a new corner —
not about which one is the "real" client.

<figure class="shot" data-shot="gui/frontends-side-by-side">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The same character in three frontends at once: the Desktop GUI hunting layout, the same layout in a terminal, and the phone's status drawer.</figcaption>
</figure>

## The six at a glance

| Frontend | How you start it | Reach for it when | Maturity |
|---|---|---|---|
| [**Terminal (TUI)**](./tui.md) | `--frontend tui` (the CLI default) | You live in a terminal, play over SSH, or want the smallest possible footprint | Stable |
| [**Desktop GUI**](./gui.md) | `--frontend gui`, or launch a saved connection (**its** default) | You want mouse-first layout work, graphics, skins, and the stay-open toolbar hubs | Stable |
| [**Vellum Despana**](./despana.md) | Select **Despana** for a saved connection | You want a dense, customizable desktop workspace in a browser | Optional built-in |
| [**Mobile Web**](./web.md) | Enable the web server (`--web-port`, or `[web]` in `config.toml`), then open the address in a browser | Your PC is running the session and you want a second screen, or you want to play from a browser with no local UI at all | Stable |
| [**Android app**](./android.md) | Sideloaded APK | The whole client on an Android phone | **In progress** |
| [**iOS app**](./ios.md) | TestFlight | The whole client on an iPhone | **Beta — via TestFlight** |

Two of those rows carry a wrinkle worth stating plainly.

**"Mobile Web" is two different products depending on how you start it.** Run the TUI or GUI
with the web server on and the browser is a **sidecar** — a second screen driving the session
your PC already owns. Run `--frontend headless` and there is no local UI at all: the core plus
the web server *is* the client, and the browser gets a login screen. Same code, different
shape.

**The Android and iOS apps are not a third thing.** They are the same Rust core in a native
shell, showing the web client's UI in a WebView. What the shell adds is a saved
**Characters** picker — the one native screen — so a phone can scan a pairing QR code and
seal a saved server into the Keychain or Keystore. On a phone the login screen shows two
tabs (**`play.net`** and **`Lich`**) plus that picker, reached through the person icon; a
desktop browser shows three tabs, because it keeps the in-page **`Remote`** tab the phone
replaces.

## Set it up

Pick where you're playing, then start the client that way. Every one of these connects the
same character to the same game.

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Run `vellum-fe` with **no arguments** (or double-click it). The **VellumFE Launcher**
   window opens, headed **VellumFE** over **"Choose a connection to launch"**.
2. Click **➕ New connection**, fill in the **New connection** form, and click **Save**.
3. Click **Launch** on the row you just made.

To start the GUI without going through the Launcher, run
`vellum-fe --frontend gui --port 8000 --character YourName`.

Which frontend a saved connection uses is set per row under **Advanced** ▸ **Frontend** ▸
**GUI** / **Terminal** / **Despana**. **A saved connection defaults to GUI, but
the `--frontend` command-line flag defaults to `tui`** — the same character
started two ways lands in two different interfaces. That surprise is worth
knowing before you go looking for a bug.

<figure class="shot" data-shot="gui/frontends-launcher-frontend-picker">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A connection's <b>Advanced</b> section with the <b>Frontend</b> submenu open on <b>GUI</b> / <b>Terminal</b> / <b>Despana</b>.</figcaption>
</figure>

→ **Expected result:** a native window opens with your layout, and the top toolbar shows the
**Windows**, **Settings**, **Zones**, and **Editors** hubs.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

1. In a terminal, run `vellum-fe --port 8000 --character YourName`. `--frontend` defaults to
   `tui`, so you do not pass it.
2. To be explicit — in a script, or when you want a *saved* connection to open in a terminal
   rather than the GUI — run `vellum-fe --frontend tui --launch-profile "YourConnectionName"`.

There is **no Launcher in the terminal**: the Launcher is an egui window, so it exists in the
GUI only. Launching a saved connection from a terminal is `--launch-profile "<name>"`.

→ **Expected result:** your layout draws on the character grid, the command input takes focus,
and game text begins scrolling in the main window.
{{#endtab}}
{{#tab name="Mobile"}}

The phone does not start a session by itself unless you want it to — it usually joins one.

- **Second screen for the PC you're already playing on:** start the TUI or GUI with the web
  server on (`vellum-fe --port 8000 --character YourName --web-port 8080`), then run
  `.webinfo` to get the pairing URL and QR code. Open that URL on the phone, or scan the QR
  in the app's **Characters** picker (person icon ▸ **Characters** ▸ **Scan QR to add**).
- **Play with no PC involved:** in the app, use the **`play.net`** tab to log in directly, or
  the **`Lich`** tab to attach to a Lich you're running elsewhere.

You cannot add, move, or resize windows here — the phone's chrome is fixed by design and
renders from a snapshot the host streams. What you *can* author from the phone is
substantial: macros, touch-wheel slices, highlights (including redirects and squelch),
colors, controller binds, and the full settings registry. Layout work belongs on the desktop.

→ **Expected result:** the phone shows your game text, and swiping in the right **status
drawer** reveals **Targets**, **Players**, and **Room** for the same character your PC is
playing.
{{#endtab}}
{{#endtabs}}

## Common setups

### One character, desk and couch

Start the GUI from the Launcher and enable the web server on the same run. Play at the desk
normally. When you get up, run `.webinfo`, scan the QR with your phone, and the phone joins
the *same* session — the same roundtime, the same room, the same active spells. Nothing
syncs, because nothing needed to: there is one session and two views of it.

**You'll see:** your health bar moving on the phone at the same instant it moves on the
monitor, and a command typed on either one landing in the game once.

### A character parked on a headless box

On a server or a Raspberry Pi with no desktop, run
`vellum-fe --frontend headless --port 8000 --character YourName --web-port 8080 --web-bind 0.0.0.0`.
There is no local UI at all. Point any browser on your network at that box's address and the
browser is the entire client — including its own login screen, since there's no desktop
session to attach to.

**You'll see:** the browser presenting a login overlay rather than immediately showing a
game already in progress — that is how you tell headless from sidecar at a glance.

## Tips & gotchas

> ⚠️ **`Ctrl+C` means two different things.** In the **Terminal (TUI)** it **copies your
> selection** and does **not** quit. In the **Desktop GUI** it **quits**. To leave the client
> from the terminal, use `.quit` or `.exit`. This is the single biggest trap when you move
> between the two.

> ⚠️ **`.quit` may not close the window.** By default it disconnects but leaves the client
> open — run it again, or use `.exit`, to close. Turn this off with `ui.keep_open_on_quit` in
> Settings.

- **The two "Launchers" are unrelated.** The **Launcher** is the graphical connection list
  you get by running with no arguments. The **SSH Launcher** is an in-session panel
  (`.launcher` / `.launch <character>`) that cold-starts a headless Lich on another machine.
  They share a word and nothing else.
- **The terminal cannot render images, and that is deliberate.** Skins, per-window background
  art, and the graphical injury doll, compass, and hand icons are GUI-only forever — a
  character cell has no pixels to put them in. The TUI renders the same information with
  glyphs and color. Nothing is missing from your *character*; only the artwork is.
- **Layouts are shared, appearance is not entirely.** `.savelayout` and `.loadlayout` work in
  both desktop frontends against the same pool, so a layout you build in the GUI opens in the
  TUI. Skin frames inside it simply have nothing to draw on the terminal side.
- **Saving a layout is always typed.** There is no GUI button for it — `.savelayout <name>`
  in either desktop frontend. The Windows catalog says so itself.

## See also

- [Terminal (TUI)](./tui.md) — the terminal frontend in full
- [Desktop GUI](./gui.md) — toolbar hubs, right-click window menu, skins
- [Vellum Despana](./despana.md) — desktop browser workspace
- [Mobile Web](./web.md) — sidecar and headless browser modes
- [Android app](./android.md) · [iOS app](./ios.md)
- [Configuration Files](../configuration/README.md) — shared configuration

<details>
<summary>Config reference (TOML)</summary>

Frontend selection is a command-line and per-connection concern; these are the settings that
decide which face you get and how it reaches the network.

**Command line**

| Flag | Type | Default | What it does |
|---|---|---|---|
| `--frontend` | `tui` \| `gui` \| `headless` | `tui` | Which interface to run. `headless` runs the core plus web server with no local UI. |
| `--launcher` | flag | off (on when run with no arguments) | Opens the graphical Launcher. GUI only. |
| `--launch-profile <NAME>` | string | — | Runs a saved connection by name from `launcher.toml`. Conflicts with `--direct`, `--key`, `--launcher`. |
| `--port` | u16 | `8000` | Lich proxy port. Overrides `config.toml`. |
| `--host` | string | `127.0.0.1` | Lich proxy host. Overrides `config.toml`. |
| `--character` | string | — | Character to log in as (direct mode); also the fallback config-directory name. |
| `--profile` | string | falls back to `--character` | Config-directory name, kept separate from the login name. |
| `--direct` | flag | off | Connects to play.net without Lich. Enables `--account`, `--password`, `--game`. |
| `--web-port <PORT>` | u16 | — | Enables the embedded web server on this port. Overrides `[web]`. |
| `--web-bind <ADDR>` | string | — | Address the web server binds to. Overrides `[web]`. |
| `--data-dir <DIR>` | path | `~/.vellum-fe` | Config directory root. Also settable via `VELLUM_FE_DIR`. |

**Per-connection** (`launcher.toml`, written by the Launcher)

| Field | Type | Default | What it does |
|---|---|---|---|
| `frontend` | `"gui"` \| `"tui"` | `"gui"` | Which frontend this saved connection launches. **Note the mismatch with the `--frontend` CLI default of `tui`.** |
| `web_client` | `"despana"` | unset | Selects the optional Despana browser frontend while retaining the native GUI fallback for older builds. |
| `save_password` | bool | `false` | Stores the password in the OS credential store (service id `vellum-fe`, keyed by the lowercased account). Never written to a file. |

**Session behavior** (`config.toml`)

| Field | Type | Default | What it does |
|---|---|---|---|
| `ui.keep_open_on_quit` | bool | `true` | After `.quit`, disconnect but keep the window open. Run `.quit` again or `.exit` to close. |

</details>
