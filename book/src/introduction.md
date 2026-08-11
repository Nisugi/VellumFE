# VellumFE

> Play GemStone IV in a client you can shape around your character — and be
> hunting in about five minutes.

## Why you'd use it

You already know the game. What you want is a screen that tells you what you
need mid-swing: vitals where your eye already goes, spells and effects visible
without a `SPELL` check, thoughts and combat in their own windows instead of
scrolling past you in one column.

VellumFE gives you that on **one core with five ways to play** — a terminal
client, a native desktop window, your phone's browser, an Android app, and an
iPhone app. The same character, the same highlights, the same settings, whether
you're at your desk or on a couch.

And you configure all of it **inside the client**. Every feature ships an
editor. Hand-editing a config file is a thing you may do, not a thing you have
to do.

<figure class="shot" data-shot="gui/intro-hunting-layout">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The Desktop GUI mid-hunt: main text left, vitals and roundtime
  along the bottom, active spells and the room's creatures docked right.</figcaption>
</figure>

## Get playing

Pick how you want to start. All three paths reach the same client.

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Download the release for your platform and extract it — see
   [Installation](./getting-started/installation.md).
2. **Double-click `vellum-fe`** (`vellum-fe.exe` on Windows). Running it with no
   arguments opens the graphical launcher.
3. Click **➕ New connection**. Choose **Lich** if you launch through Lich (fill
   in host and port), or **Direct** to reach the game without Lich (fill in
   account, game world, and character).
4. Tick **Save password** if you want it remembered — it goes into your OS
   credential store, never into a file.
5. Click **Save**, then **Launch**.

Saved connections run the **Desktop GUI** unless you change **Frontend** under
the connection's **Advanced** fold.

→ **Expected result:** a native VellumFE window opens, connects, and your
character's room description scrolls into the main text window.

<figure class="shot" data-shot="gui/intro-launcher-profiles">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The launcher's connection list, with one Lich connection and one
  Direct connection, each showing its character and game world.</figcaption>
</figure>

{{#endtab}}
{{#tab name="Terminal (TUI)"}}

From a terminal, connect through a running Lich:

    vellum-fe --port 8000 --character YourCharacter

Or reach the game with no Lich at all:

    vellum-fe --direct --account ACCOUNT --character YourCharacter --game prime

> **A hand-typed command line defaults to the terminal frontend.** The launcher's
> saved connections default to the GUI. Add `--frontend gui` (or `-f gui`) to a
> typed command line when you want the desktop window instead.

Omit `--password` on a direct connection and you're prompted for it securely
rather than putting it in your shell history.

→ **Expected result:** the terminal fills with the VellumFE layout — main text,
a command line at the bottom, vitals bars — and your character logs in.
{{#endtab}}
{{#tab name="Mobile"}}

There's no command line to type on a phone, so you either install the app or
point a browser at a session. Two routes, two jobs:

- **The app — play from anywhere.** Install the Android app or join the iOS
  TestFlight beta. Its login screen does everything: log in to play.net
  directly, attach to a Lich session, cold-start Lich over SSH, or pair with a
  VellumFE session running on your PC (**Characters** ▸ **Scan QR to add**).
- **A browser — a second screen beside your PC.** Give a desktop session
  `--web-port 8080 --web-bind 0.0.0.0`, then open
  `http://<your-pc-address>:8080/play`. Nothing to install, and it works just as
  well on a tablet or a spare laptop.

→ **Expected result:** the touch client loads with the main text pane, the
**status drawer** on the right, and the **macro tray** on the left.

Setup in detail: [Put VellumFE on your phone](./how-to/vellum-on-your-phone.md).
{{#endtab}}
{{#endtabs}}

## What you get

- **Windows you place.** Drag any window where you want it, size it, and it
  stays there. Add windows for thoughts, deaths, ESP, or any game stream you
  care about — see [Text Windows](./widgets/text-windows.md).
- **Highlights that do more than color.** A rule can recolor a line, play a
  sound, rumble your controller, replace the text, send the line to a different
  window, or hide it outright. See
  [Set up highlights and sound alerts](./how-to/highlights-and-sounds.md).
- **Connect your way.** Through Lich as a detachable client, or directly to
  play.net with no Lich running.
- **36 built-in themes**, ten of them accessibility variants — high-contrast
  (light and dark), deuteranopia, protanopia, tritanopia, monochrome,
  low-blue-light, photophobia, ADHD-focus, and reduced-motion.
- **Speech built in.** Windows can read new lines aloud, so a thought or a death
  reaches you without a glance. See [Speech](./features/speech.md).
- **Rebindable keys**, hotbars of command buttons, and gamepad support.
- **Graphics on the desktop** — skins, per-window frames, an injury doll, hand
  icons — with the terminal and phone rendering the same information as text.

## Where your settings live

Everything lands under `~/.vellum-fe`. Two layers matter:

- **`global/`** — settings shared by every character: highlights, keybinds,
  colors, hotbars, skins and images.
- **`profiles/<character>/`** — that one character's overrides of any of the
  above, plus their layout, history, and log.

Point the whole thing somewhere else with `--data-dir` or the `VELLUM_FE_DIR`
environment variable — handy for a portable install on a stick.

You change all of this in-app: type `.settings` for the settings editor, or
`.help` for every command. The full list is in the
[Command Reference](./reference/commands.md).

## Where to go next

- **[Getting Started](./getting-started/README.md)** — download, launcher, first
  connection.
- **[How-To Guides](./how-to/README.md)** — build a hunting layout, make vitals
  flash when you're hurt, wire a combat hotbar.
- **[Frontends](./frontends/README.md)** — what each of the five can and can't do.
- **[Widgets](./widgets/README.md)** — every window type, one page each.

## Getting help

- **Discord**: [discord.gg/6nKhWRTkSN](https://discord.gg/6nKhWRTkSN) — help,
  layout showcases, beta testing, release announcements.
- **GitHub Issues**:
  [github.com/Nisugi/VellumFE/issues](https://github.com/Nisugi/VellumFE/issues)
- **In-game**: find us on amunet.

<details>
<summary>Config reference (TOML)</summary>

You do not need this to play — every file below has an in-app editor. It's here
for troubleshooting and for people who like to read their config.

Base directory: `~/.vellum-fe`, overridden by `--data-dir <DIR>` or the
`VELLUM_FE_DIR` environment variable.

| Path | Holds |
|------|-------|
| `launcher.toml` | Saved launcher connections. **Never contains passwords.** |
| `global/config.toml` | Shared settings: connection, UI, sound, speech, web server |
| `global/highlights.toml` | Shared highlight, sound, squelch and redirect rules |
| `global/keybinds.toml` | Shared key bindings |
| `global/colors.toml` | Shared color palette, stream presets, spell colors |
| `global/hotbars.toml` | Shared hotbar button definitions |
| `global/controller.toml` | Gamepad binds, wheels, rumble, tuning (base layer) |
| `global/macros.toml` | Macro buttons for the phone/web client (a character's own copy overrides it) |
| `global/skins/` | One folder per skin, each with a `skin.toml` manifest |
| `global/images/` | Shared art pool: `icons`, `frames`, `dolls`, `compass`, `backgrounds`, `statusicons`, `hands` |
| `profiles/<character>/config.toml` | That character's overrides |
| `profiles/<character>/highlights.toml` | That character's own highlight rules |
| `profiles/<character>/keybinds.toml` | That character's own key bindings |
| `profiles/<character>/controller.toml` | That character's gamepad overrides |
| `profiles/<character>/layout.toml` | Auto-saved layout for that character |
| `profiles/<character>/history.txt` | Command history |
| `profiles/<character>/debug.log` | That session's log — the first place to look when something breaks |
| `layouts/` | Named layouts from `.savelayout`, shared by all characters |
| `highlights/` | Named highlight sets from `.savehighlights` |
| `keybinds/` | Named keybind profiles |
| `themes/` | Custom themes as `<name>.toml`, alongside the 36 built in |

Files are written atomically, and the previous version is kept alongside as
`<name>.toml.bak` — so a bad edit is one rename away from being undone.

</details>
