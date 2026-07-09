# Mobile Web

A touch-first client that runs in any browser, served by an embedded web
server. It works two ways:

- **Second screen for a desktop session** — the TUI or GUI keeps running
  on your PC and the phone joins the *same* character (a sidecar).
- **The whole interface** — [headless mode](#headless-mode) runs just the
  core plus the web server; you log in and play entirely from the
  browser. (The [Android](./android.md) and [iOS](./ios.md) apps package
  exactly this into a phone app.)

## Enabling (Desktop Sidecar)

In `config.toml`:

```toml
[web]
enabled = true
port = 8040
bind = "0.0.0.0"   # required for phones on your LAN (default 127.0.0.1)
pinned = false     # see "Multiple characters" below
```

Or one-off from the CLI: `--web-port 8040` (this enables the server but
does **not** change `bind` — set that in config to reach it from a phone).

## Pairing Your Phone

Access requires a pairing token. In-game:

```
.webinfo
```

This prints the session URL (with the token included) and opens a QR code
in your browser — scan it with the phone. One pairing covers all your
characters; the phone remembers the token, so this is a one-time step per
device. Unpaired connections are refused, and repeated bad attempts are
locked out temporarily.

> **Security**: pairing keeps strangers out, but the traffic is plain
> HTTP on your LAN. For off-LAN play use Tailscale/WireGuard — never
> expose the port to the open internet.

## The Dashboard

- `/` is a **dashboard**: one card per running character session, health-
  checked and auto-refreshing — tap a card to play it.
- `/play` is the game client itself.
- "Add to Home Screen" works — the client is a PWA and installs like an app.

## Multiple Characters

Run several VellumFE instances and they share the web setup automatically:
each instance walks upward from the base port to find a free one, and all
of them appear on the dashboard. If you want a specific character to have
a **stable port** (for a direct `/play` bookmark), set `pinned = true` in
that character's profile config — it then binds exactly that port or
disables web for the session with a loud warning, never a silent neighbor
port.

## Playing from the Browser

- **Read the game** live, with streams as filter chips (unread badges;
  long-press a chip to reorder — remembered per device).
- **Send commands** — identical to typing at the PC, including
  dot-commands. With a keyboard, Up/Down browse command history; the ↻
  button resends the last command, and long-pressing it opens a history
  sheet.
- **Tap links, nouns, and exits** — context menus open in a bottom
  sheet; a mini **compass** floats over the text pane (exits light up,
  tap to move). It starts bottom-right; hold it about half a second to
  lift and drag it anywhere — the spot is remembered per device.
- **Side drawers** — swipe from the screen edges (or tap the handles):
  the left drawer is a vertical **macro tray**; the right is a **status
  panel** with the injury doll, injuries list, hands, character sheet
  (experience, encumbrance, bounty, society), active effects with live
  countdowns, and **tap-to-target** — tap a creature to get its
  attack/look/target menu.
- **Vitals, hands, RT/CT** — a status strip with live countdowns.
- **Macro buttons** — the macro rail, menu buttons, and draggable
  floating buttons from [macros.toml](../configuration/macros-toml.md);
  create and edit them from the phone with the rail's **+** button. A
  button's **On tap** behavior can also *type into the input* instead of
  sending, so word buttons ("go", "second", "door") compose phrases —
  *Type, then send* submits the composed line in one tap.
- **Sound alerts** — highlight sounds play in the browser (toggle in
  Settings; the first sound may need one tap due to autoplay rules).
- **Reconnect gracefully** — resumes where you left off, with a "missed
  output" marker for long gaps. Multiple devices can connect at once.

## Settings on the Phone

The gear button (also reachable from the login screen) opens Settings:

- **Appearance** — four theme presets (Vellum dark, OLED black, high
  contrast, parchment), show/hide toggles for every piece of chrome
  (macro bar, compass, vitals, hands, RT label, effect pills, chips),
  and opacity sliders for floating buttons, drawers, and bottom sheets.
  The **Aa** button sets story text from 6 to 24 px. All per-device.
- **Highlight editor** — add/edit highlight rules with color pickers, a
  sound dropdown, and a live preview; fields the form doesn't cover
  (redirects, squelch, ...) are preserved for desktop editing.
- **Colors editor** — stream preset and prompt colors with native
  pickers.
- **Advanced** — raw TOML editors for highlights and colors (profile or
  global) with **import file / export** — the practical way to move a
  desktop config onto the phone.

Edits save to the same config files the desktop uses and apply live.

## Headless Mode

```bash
vellum-fe --frontend headless
```

Runs the core and web server with **no local UI** — it prints the ready
`/play` URL (token included) at startup, and the browser does the rest.
Give it credentials (`--direct --account ... --character ...`) to
auto-connect, or give it nothing and it waits at the **browser login
screen**: enter account/password/character/game, or tap a saved profile
(shared with the [desktop Launcher](../getting-started/launcher.md)'s
`launcher.toml`). "Remember this login" saves the password securely.

### Connecting Through Lich

The login screen has a **play.net / Lich** toggle. The Lich tab attaches
to a Lich session already running on another machine — so a headless
host, the Android app, or the iOS app can still play a fully scripted
character:

- Launch Lich with `--detachable-client` on your PC, then enter its
  host and port (an optional label names the saved entry). Saved Lich
  connections reattach with one tap, no password.
- **Coming in Lich 6** (not yet available): Lich's WebUI adds a
  connect-a-device panel showing a `vellum://` **QR code / link**;
  scanning or tapping it opens the login screen with the Lich tab
  prefilled. It never auto-connects — you always press Connect. The
  app understands `vellum://` links today; Lich 6 adds the panel that
  displays them.
- The detachable port is unauthenticated, so the form warns when the
  host doesn't look private. Keep it to home Wi-Fi, Tailscale, or a
  VPN — never the open internet.
- If the link drops, the session reattaches to the same Lich target
  automatically.

### Remote: a desktop session on your phone (apps only)

Inside the [Android](./android.md) and [iOS](./ios.md) apps the login
screen has a third tab, **Remote**. Instead of running a session on the
phone, it points the app at a **desktop VellumFE**'s web server — the
same sidecar the browser gets, with the app's install-once, QR-pairing,
secure-storage experience. Your PC session keeps its TUI/GUI screen; the
phone becomes a live second screen for it. Lich's one-frontend limit
doesn't apply — the web sidecar mirrors the session rather than
replacing its client.

- On the PC, run `.webinfo`: the pairing page now shows two QR codes.
  Scan the **VellumFE app** one with the phone camera — it opens the
  app's Remote tab prefilled (host, port, and pairing token). It never
  auto-connects; you always press Connect. Or type the address and token
  by hand.
- **Remember this server** keeps the pairing in the phone's Keychain
  (iOS) / Keystore-sealed storage (Android); reconnecting is one tap
  from then on.
- While on a remote server, the app's embedded core sits idle — there is
  no game connection on the phone to go stale in the background; the web
  client's usual reconnect picks the mirror back up when you return.
- The way back: **⚙ Settings → Leave this server (app login)**.
- Same network rule as everything else on this page: home Wi-Fi,
  Tailscale, or a VPN — never the open internet.

Headless sessions manage themselves:

- Drops reconnect automatically with backoff; typing again resets it.
- If the connection drops repeatedly with **no input from you**, it
  stops reconnecting — an abandoned session winds down instead of
  relogging all night.
- If login hangs, a watchdog retries it.
- `quit` (or the logout button) returns to the login screen.

The login screen only appears in headless and app (Android/iOS) mode — a
desktop sidecar session is controlled from the desktop, as before.

## Tips

- After editing `macros.toml` on the PC, run `.reloadmacros` — connected
  phones update instantly.
- The wake button in the top bar keeps the phone's screen on; tap the
  title line to toggle between room name and character name.
