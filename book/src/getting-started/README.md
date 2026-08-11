# Getting Started

> Three short pages take you from a downloaded archive to a character standing
> in Wehnimer's Landing.

## What this section does for you

You have the game and you probably have Lich. What you don't have yet is this
client on your machine, pointed at your character. That's the whole job of this
section, and it's three steps:

1. **[Installation](./installation.md)** — get the right archive for your
   platform (or the Android APK), extract it, and know where the binary should
   live. Building from source is here too, if you'd rather.
2. **[The Launcher](./launcher.md)** — the double-click path. Save a connection
   per character, keep the password in your OS credential store, and start a
   session with one click. Launch several to play several characters at once.
3. **[First Launch](./first-launch.md)** — the command-line path, choosing a
   frontend, and a tour of what's on screen once you're connected.

Read them in order the first time. After that, the Launcher page is the only one
you'll come back to.

## Before you start

- **A way into the game.** Either [Lich](https://lichproject.org/) running as a
  detachable client, or your play.net account details for a direct connection.
  VellumFE does both; you don't have to pick now.
- **For the terminal frontend**, a terminal with 256-color or true-color
  support. Windows Terminal, iTerm2, Kitty, Alacritty and modern GNOME Terminal
  all qualify.
- **For the desktop GUI**, nothing extra — it's a native window.
- **For Android**, version 8.0 or newer. On Android 8 and 9, update **Chrome**
  first: it provides the app's rendering engine there.

## Get connected

If you'd rather not read three pages first, this is the short version.

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Extract the download and **double-click `vellum-fe`**. With no arguments it
   opens the graphical launcher.
2. Click **➕ New connection**, pick **Lich** or **Direct**, and fill in the
   connection details for your character.
3. Click **Save**, then **Launch**.

Connections run the **Desktop GUI** by default; switch **Frontend** under
**Advanced** to get a terminal session instead.

→ **Expected result:** a VellumFE window opens, connects, and your room
description appears in the main text window.

<figure class="shot" data-shot="gui/getting-started-add-profile">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The launcher's <b>New connection</b> form in Direct mode, showing
  the account, game world, character and <b>Save password</b> controls.</figcaption>
</figure>

{{#endtab}}
{{#tab name="Terminal (TUI)"}}

With Lich already running as a detachable client on port 8000:

    vellum-fe --port 8000 --character YourCharacter

Without Lich:

    vellum-fe --direct --account ACCOUNT --character YourCharacter --game prime

> **A hand-typed command line runs the terminal frontend by default** — the
> launcher's saved connections run the GUI. Add `--frontend gui` when you want
> the desktop window from a typed line.

→ **Expected result:** the terminal fills with the default layout — main text,
vitals bars, a command line at the bottom — and your character logs in.

<figure class="shot" data-shot="tui/getting-started-first-connect">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A freshly connected terminal session at the default layout, room
  description in the main window and vitals along the bottom.</figcaption>
</figure>

{{#endtab}}
{{#tab name="Mobile"}}

There's no command line to type on a phone, so you either install the app or
point a browser at a session. Two routes for two jobs, both covered in
[Put VellumFE on your phone](../how-to/vellum-on-your-phone.md):

- **The app — play from anywhere.** Install the Android APK (see
  [Installation](./installation.md)) or join the iOS TestFlight beta
  **(beta — via TestFlight)**. Its login screen offers **play.net** and **Lich**
  tabs, and the person icon opens **Characters**, where **Scan QR to add** pairs
  the phone with a session running on your PC.
- **A browser — a second screen beside your PC.** Start a desktop session with
  `--web-port 8080 --web-bind 0.0.0.0`, then open
  `http://<your-pc-address>:8080/play` in the phone's browser. Nothing to
  install; your desktop keeps playing and the phone shows the same game.

→ **Expected result:** the touch client loads with the main text pane, the
**status drawer** on the right, and the **macro tray** on the left.
{{#endtab}}
{{#endtabs}}

## Once you're in

Two commands are worth knowing on day one:

- **`.help`** lists every dot-command, grouped by what it does.
- **`.settings`** opens the settings editor. In the Desktop GUI you can also use
  the **Settings** button in the top toolbar, which stays open while you change
  things.

> ⚠️ **`.quit` disconnects but leaves the window open — run it again to exit.**
> `.exit` closes VellumFE outright. Turn off **Keep Open On Quit** in
> **Settings ▸ UI** if you'd rather `.quit` close immediately.

> ⚠️ **`Ctrl+C` means different things in different frontends.** In the terminal
> it **copies** your selected text. In the Desktop GUI it **quits**. To leave a
> terminal session, use `.quit` or `.exit`.

## Then what

- **[How-To Guides](../how-to/README.md)** — the fastest route to a screen you
  like: [build a hunting layout](../how-to/hunting-layout.md), [make your health
  bar flash when low](../how-to/vitals-flash.md), [set up highlights and
  sounds](../how-to/highlights-and-sounds.md).
- **[Frontends](../frontends/README.md)** — what each of the five can do.
- **[Command Reference](../reference/commands.md)** — the full dot-command list.

## See also

- [Introduction](../introduction.md) — what VellumFE is and where its config lives
- [The Launcher](./launcher.md) — saved connections and keyring passwords in detail
- [First Launch](./first-launch.md) — every command-line flag, explained
