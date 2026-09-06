# Browser Client

> Put a second screen beside your PC with nothing to install — your phone,
> tablet, or the spare laptop shows the same character you're already playing.

## What it's for

You're hunting at your desk and the good text is scrolling past the window you
can't see. Or you want to keep an eye on thoughts and your spell timers while
your hands are busy elsewhere in the house. VellumFE ships a small web server
inside the client: turn it on, open a browser on any device on your network, and
you get a touch-first view of the *same* session — same character, same streams,
same links to click.

Two shapes, both first-class:

- **A second screen for a running session (sidecar).** Your TUI or GUI keeps
  running on the PC; the browser joins that same character. This is the shape
  this page leads with.
- **The whole client in the browser.** Run VellumFE with no local UI at all and
  use `/play` as the browser client — [Headless mode](#headless-mode) below.
  For a dense desktop workspace, choose
  [**Vellum Despana**](./despana.md) for a saved connection in Vellum's
  native Launcher; it opens its authenticated `/despana` presentation
  automatically.

If you want the client living on your phone rather than served from your PC, the
[Android](./android.md) and [iOS](./ios.md) apps do that job instead. Neither
route is the lesser one — the app plays from anywhere, the browser sits beside
your PC.

<figure class="shot" data-shot="mobile/web-sidecar-second-screen">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>A phone browser showing the story pane, stream chips, and vitals strip beside a desktop GUI session running the same character.</figcaption>
</figure>

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

Serve the session from the connection you launch it with:

1. In the **VellumFE Launcher**, click **Edit** on the connection (or **➕ New
   connection**) and open the **Advanced** fold.
2. Tick **Enable on port** next to **Web dashboard** and set the port — `8040`
   is a good first choice.
3. Set **Bind address** to `0.0.0.0`. The hint next to the field says
   **0.0.0.0 = allow LAN devices**; the default `127.0.0.1` serves this PC only,
   so a phone will never reach it.
4. **Save**, then **Launch** the connection.
5. In the game input, type `.webinfo`. It prints
   `Web session URL (browser): http://192.168.1.50:8040/#token=…` and opens a
   pairing page with QR codes in your default browser.
6. Scan the **browser** QR with the phone's camera, or type that URL in.

<figure class="shot" data-shot="gui/web-launcher-advanced-web-dashboard">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The Launcher's <b>Advanced</b> fold with <b>Web dashboard</b> ticked on port 8040 and <b>Bind address</b> set to <code>0.0.0.0</code>.</figcaption>
</figure>

→ **Expected result:** the phone opens the game client already paired, showing
your character's live text. Your desktop window keeps playing exactly as before —
this is a mirror, not a handover.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

The Launcher is an egui window and does not open in a terminal, so set the port
on the command line or in `config.toml` instead:

1. Start with `vellum-fe --port 8000 --character Rysk --web-port 8040`.
   **`--web-port` enables the server but does not change `bind`** — for a phone
   to reach it, set `bind = "0.0.0.0"` in the `[web]` block (see the config
   reference at the bottom of this page).
2. Once connected, type `.webinfo`.
3. Read the printed `Web session URL (browser): …` line and open it on the phone,
   or scan the QR from the pairing page `.webinfo` writes.

→ **Expected result:** the phone shows the same character's text live, and your
terminal session is untouched.
{{#endtab}}
{{#tab name="Mobile"}}

The phone is the client here, not the host — there is nothing to enable on it.
You pair it once and it remembers:

1. Open the URL from `.webinfo` (or scan its QR). The token rides in the URL's
   `#token=` fragment, so the browser stores the pairing itself.
2. Use **Add to Home Screen** — the client is a PWA and installs like an app,
   opening straight to `/play`.

**One pairing covers all your characters on that device.** Unpaired connections
are refused and repeated bad attempts get locked out for a while.

→ **Expected result:** a home-screen icon that opens the game full-screen, with
no address bar and no re-pairing.
{{#endtab}}
{{#endtabs}}

## Common setups

### A phone beside the keyboard while you hunt

1. Launch your hunter in the desktop GUI with **Web dashboard** enabled on
   `8040` and **Bind address** `0.0.0.0`.
2. Run `.webinfo` and scan the browser QR.
3. On the phone, tap the **thoughts** stream chip so the story pane filters to
   thoughts only.
4. Swipe in from the right edge to open the **status drawer** and leave it on
   the **Targets** section.

→ You now watch thoughts and your target list on the phone while the desktop
window stays on room text and combat. Tapping a creature in the drawer opens its
attack/look/target menu and the command fires from your PC session.

### Several characters, one dashboard

Launch two or three characters with web enabled. Unpinned instances treat the
configured port as a *base* and walk upward to the next free one, so no two fight
over `8040`.

1. Open `http://192.168.1.50:8040/` — the bare root, not `/play`.
2. The **dashboard** lists one card per running session, health-checked and
   auto-refreshing.
3. Tap a card to play that character.

→ Switching characters is one tap, and a character you shut down drops off the
list on its own. If you want one character to keep a fixed address you can
bookmark straight to `/play`, set `pinned = true` in that character's profile
config — it then binds exactly that port or disables web for the session with a
loud warning, never a silent neighbor port.

## Playing from the browser

- **Read the game live**, with streams as filter chips carrying unread badges.
  Long-press a chip to reorder them; the order follows your character across
  devices.
- **Send commands** exactly as you would at the PC, dot-commands included. With a
  keyboard attached, Up/Down browse history; the ↻ button resends the last
  command and long-pressing it opens a history sheet. The newest matching history
  entry appears dim after the cursor — press Tab to accept it.
- **Tap links, nouns, and exits.** A direct link fires immediately; a plain noun
  opens the server's context menu in a bottom sheet. A mini compass floats over
  the text pane with live exits; hold it about half a second to lift and drag it
  somewhere else, and the spot is remembered per device.
- **Side drawers.** Swipe from the left edge for the **macro tray**; from the
  right for the **status drawer** — **Targets**, **Players**, **Room**, the
  injury doll, hands, and active effects with live countdowns.
- **Map.** A map button appears in the top bar once map data exists
  (`.mapdb download`). Tap a room to walk there with native `.go2`, drag to pan,
  pinch to zoom. See [Map](../widgets/map.md) and [Travel](../widgets/travel.md).
- **Reconnect gracefully.** The session resumes where you left off with a
  "missed output" marker for long gaps, and several devices can be connected at
  once.

## Settings on the phone

The gear ⚙ opens the settings sheet. What you can author from here:

- **Appearance** — theme presets, show/hide toggles for every piece of chrome,
  opacity sliders, and the **Aa** button for story text from 6 to 24 px. Theme,
  text size, and chip order roam with your character; chrome toggles and
  opacities stay per-device.
- **Client settings (saved on host)** — the full desktop settings registry over
  the wire, editable at character or global scope, written to the hosting
  machine's config exactly as if you had edited it there.
- **Highlight rules (this profile)** / **(global)** — add and edit rules with
  color pickers, a sound dropdown, and a live preview.
- **Colors (this profile)** / **(global)** — stream preset and prompt colors.
- **Streams (saved on host)**, **Touch wheel (long-press ring)**, **Controller
  (gamepad)**, **Speech**, and **SSH launcher (cold-start Lich)**.
- **Advanced** — raw TOML editors for highlights and colors with import/export,
  the practical way to move a desktop config onto the phone.

> ✅ **The phone's highlight editor does redirects and squelch.** Older
> documentation said it could not. The rule form has a **Squelch** checkbox and a
> redirect **Off / redirect only / redirect + copy** selector with a target
> field, so a phone-authored rule can send matching lines to another window or
> hide them outright.

What the phone genuinely cannot author: **layout and panel placement, window
resizing, game keybinds, and macro `hidden_when` conditions.** Those stay on the
desktop. Everything else on the list above is editable from the phone.

## Headless mode

For ordinary desktop play in **Vellum Despana**, run `vellum-fe` with no
arguments, edit the saved connection under **Advanced** ▸ **Frontend**, choose
**Vellum Despana**, and click **Launch**. The saved Vellum login is applied once and
the authoritative paired `/despana` URL opens automatically; you do not fill
out a second browser login form.

You can also start a browser-only session directly:

```bash
vellum-fe --frontend headless
```

This runs the core and the web server with **no local UI at all**. Once the web
server has actually bound, it prints tokenized URLs for Vellum's `/play`
surface and Despana's `/despana` surface using the real bound port. Open the
surface you want and the browser does the rest. Give it credentials (`--direct
--account … --character …`) to auto-connect, or give it nothing and `/play`
waits at the **login screen**.

The login screen is the same overlay the phone apps show. In a **desktop
browser** it has three tabs:

- **`play.net`** (default) — `play.net account`, `password`, `character`, a game
  selector covering every GemStone IV and DragonRealms world, and **Remember
  this login**. Submit is **Connect**.
- **`Lich`** — `host`, `port`, `label (optional)`, and **Remember this
  connection** (ticked by default). It attaches to a Lich session already
  running elsewhere; launch that Lich with `--detachable-client`. The tab also
  holds a `custom launch command (optional)` textarea: with one set, connecting
  probes the port first, SSHes to the host and runs the command if Lich is down,
  then attaches once the port opens. Configure the SSH side in ⚙ → **SSH
  launcher (cold-start Lich)**.
- **`Remote`** — points this browser at a *different* machine's VellumFE web
  server using its host, port, and pairing token.

> ⚠️ **A phone shows two tabs, not three.** In the Android and iOS apps the
> in-page **Remote** tab is hidden on purpose and a native **Characters** picker
> replaces it — reached from the person icon on the login screen. The picker
> exists so the app can scan pairing QR codes with the camera and seal saved
> servers into the Keychain or Keystore. Same capability, better home.

Headless sessions look after themselves: drops reconnect with backoff and typing
resets it; repeated drops with **no input from you** stop the reconnect loop so
an abandoned session winds down instead of relogging all night; a hung login is
retried by a watchdog; and `quit` returns to the login screen. Closing the
browser does not stop the independent headless process. Enter `.exit` to close
it completely, or stop only its specific process when the browser is
unreachable.

## Tips & gotchas

> ⚠️ **`bind = "127.0.0.1"` is this PC only.** The most common failure is a
> phone that cannot reach the URL at all. `.webinfo` tells you when this is the
> cause, printing: *bind = "127.0.0.1" is this PC only. Set [web] bind =
> "0.0.0.0" so phones on your LAN can connect.*

> ⚠️ **Security: keep the port on a network you trust.** Pairing keeps strangers
> out, but the traffic is plain HTTP. For play away from home use **Tailscale or
> WireGuard** — never forward this port to the open internet. `.webinfo` repeats
> this line every time you run it.

- **`.webinfo` refuses when there's nothing to pair.** "Web server is disabled"
  means the `[web]` block is off and you never passed `--web-port`. "Web server
  is not running (bind failed or still starting)" means the port was taken or
  the server is a beat behind — run it again.
- **The pairing page shows two QR codes.** The **browser** one carries an
  `http://…/#token=…` URL for any browser. The **VellumFE app** one carries a
  `vellum://remote?…` link only the Android or iOS app understands. Scanning the
  app QR into a plain browser does nothing.
- **A sidecar has no login screen.** The desktop session owns the connection, so
  the browser gets no account fields and no login music toggle. That screen
  appears only in headless mode and in the phone apps.
- **After editing `macros.toml` on the PC, run `.reloadmacros`** — connected
  phones update instantly rather than on next connect.
- **The wake button in the top bar keeps the phone's screen on.** Tap the title
  line to toggle between room name and character name.

## See also

- [Android app](./android.md) — the client living on your phone.
- [iOS app](./ios.md) — the same, on iPhone.
- [The Launcher](../getting-started/launcher.md) — where the **Web dashboard** fold
  lives.
- [Map](../widgets/map.md) and [Travel (.go2)](../widgets/travel.md) — what the phone's
  map button drives.
- [Skins (GUI Graphics)](../customization/skins.md) — the injury doll art the status drawer
  renders when the host session has a skin active.

<details>
<summary>Config reference (TOML)</summary>

The `[web]` block in `config.toml` (or a character's profile config). The
Launcher's **Advanced** fold and the phone's **Client settings (saved on host)**
sheet both write these fields, so hand-editing is a troubleshooting path, not the
intended one.

```toml
[web]
enabled = true
port = 8040
bind = "0.0.0.0"
pinned = false
```

| Field | Type | Default | What it does |
|---|---|---|---|
| `enabled` | bool | `false` | Turns the embedded web server on for this session. `--web-port <n>` enables it for one run without editing config. |
| `port` | u16 | `8040` | The HTTP + WebSocket port. When `pinned = false` this is a **base** port: an instance that finds it taken walks upward to the next free one, so several characters launch without any per-character config. |
| `bind` | string | `"127.0.0.1"` | The bind address. The default serves this machine only. Set `"0.0.0.0"` deliberately to let phones and tablets on your LAN connect. |
| `pinned` | bool | `false` | Binds exactly `port` or disables web for the session with a loud warning — never a silent neighbor port. Set this in a character's profile config when you want a stable `/play` bookmark for that character. |

Routes served: `/` is the multi-session dashboard, `/play` is the mobile and
sidecar game client, `/despana` is the optional desktop browser client, and
`/health` is a token-free reachability check (it is what puts the live or
offline dot next to a saved server in the phone apps' **Characters** picker).

</details>
