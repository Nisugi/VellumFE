# Play VellumFE on your phone

> By the end you'll have your character in your hand — either a real client you
> can log in from anywhere, or a second screen on the couch beside your PC.

## What you'll build

Two different things, and you probably want both eventually:

- **The app** — a client that logs into GemStone IV on its own. Sit on the bus,
  log in, play. It can also attach to your desk session when you'd rather pick
  the game up where you left it.
- **The browser** — a second screen. Nothing to install: point a phone, tablet
  or spare laptop at your PC on the same network and it mirrors the session
  you're already playing, with your keyboard still in charge.

Neither one is the "proper" way. They do different jobs.

<figure class="shot" data-shot="howto/phone-app-and-browser-side-by-side">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The same character in the native app (left) and in a tablet browser beside the desktop client (right).</figcaption>
</figure>

## Before you start

- **For the app:** iOS is a **beta — via TestFlight**; Android is **(in
  progress)**. See [iOS App](../frontends/ios.md) and
  [Android App](../frontends/android.md) for how to get the current build.
- **For the browser, and for pairing the app with your desk session:** the phone
  and the PC must be on the same network, and VellumFE's web server must be
  running and bound so LAN devices can reach it. That's Step 1 of the browser
  tab below.
- **Read this once before either route:** pairing tokens keep strangers out, but
  the traffic itself is plain HTTP. Use this on a network you trust — your home
  Wi-Fi, not a café's. To play from outside the house, put both devices on a
  Tailscale or WireGuard network and connect over that. **Never forward this
  port to the open internet.** `.webinfo` reprints this warning every time you
  run it, on purpose.

## Steps

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

**Serve the session, then pair a device with it.**

1. In the Launcher, open your connection's **Edit** form and expand
   **Advanced**.
2. Under **Web dashboard**, tick **Enable on port** and set a port — `8040` is
   a fine choice.
3. Set **Bind address** to `0.0.0.0`. The hint under the field reads
   **0.0.0.0 = allow LAN devices**, which is exactly the difference: the
   default `127.0.0.1` serves this PC only, and a phone will never reach it.
4. Save and launch the connection.

<figure class="shot" data-shot="howto/launcher-advanced-web-bind-lan">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>Launcher <b>Advanced</b> with <b>Enable on port</b> ticked and <b>Bind address</b> set to <code>0.0.0.0</code>, showing the <b>0.0.0.0 = allow LAN devices</b> hint.</figcaption>
</figure>

5. Once connected, type `.webinfo` in the command input. It prints two lines
   and then opens a pairing page in your browser:

   ```
   Web session URL (browser): http://192.168.1.21:8040/#token=…
   VellumFE app link: vellum://remote?host=192.168.1.21&port=8040&token=…&name=Rysk
   ```

6. **The pairing page shows two QR codes, and they are not interchangeable.**
   The browser QR carries the `http://` URL; the app QR carries the
   `vellum://` deep link. Scan the browser one with your phone's camera to open
   the browser client. Scan the app one from inside the app's **Scan QR to add**
   screen to save the session as a character. **Scanning the wrong code does
   nothing** — no error, no result, which is a confusing thirty seconds if you
   don't know to check.

<figure class="shot" data-shot="howto/webinfo-pairing-page-two-qr">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <code>.webinfo</code> pairing page with the browser QR and the app QR side by side, each under its own heading.</figcaption>
</figure>

→ **Expected result:** the browser QR opens a working session on the phone
within a couple of seconds; the app QR adds a named row to the app's
**Characters** list, showing `192.168.1.21:8040` with a live dot.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

The TUI has no Launcher — the Launcher is a graphical window and there is no
terminal equivalent. Enable the web server from the command line instead;
everything after that is identical.

1. Start your session with the web server switched on and bound for the LAN:

   ```
   vellum-fe --port 8000 --character Rysk --web-port 8040
   ```

   `--web-port` also forces the bind, so a phone can reach it. To make this
   permanent instead of per-launch, set `bind = "0.0.0.0"` under `[web]` in
   `config.toml`.
2. Type `.webinfo` in the command line. Both the browser URL and the app deep
   link print into your feed, followed by the security note, and the pairing
   page with its two QR codes opens in your default browser.
3. Scan the code matching your target — the browser QR for a browser, the app
   QR from inside the app's **Scan QR to add** screen.

→ **Expected result:** `.webinfo` prints
`Web session URL (browser): http://…/#token=…` with your machine's LAN address
rather than `127.0.0.1`, and the phone connects to the session already running
in your terminal.
{{#endtab}}
{{#tab name="Mobile"}}

**Route A — the app, logging in on its own**

1. Open the app. The login screen shows two tabs.
2. **To log straight into the game**, stay on the **`play.net`** tab (the
   default). Enter your `play.net account`, `password` and `character`, choose
   your game from the dropdown, and tick **Remember this login** if you want it
   saved. Tap **Connect**.
3. **To attach to a Lich you're already running**, switch to the **`Lich`** tab
   and give it the `host` and `port`, plus a `label (optional)` so you recognize
   it later. **Remember this connection** is ticked by default.
4. **If that Lich might not be running yet**, fill in the `custom launch
   command (optional)` box inside the same **Lich** tab. Connecting probes the
   port, SSHes in to start Lich if it's down, then attaches. This lives inside
   the Lich tab — it is not a separate mode or a third tab.

**Route B — the app, attached to your desk session**

5. Tap the person icon, then **Characters**.
6. Tap **Scan QR to add** and scan the **app** QR from the `.webinfo` pairing
   page on your PC — the second of the two codes, not the browser one. The
   entry names itself after your character rather than showing a bare
   `host:port`, because the name rides along in the link.
7. No camera, or the PC is in another room? Tap **Add manually** and fill in
   **Add character**: **Label**, **Host**, **Port**, and the **Pairing token**
   copied from the `.webinfo` output.
8. Tap the row to connect.

> ⚠️ **The app's login screen shows two tabs; a desktop browser shows three.**
> The extra one in a browser is **Remote**. Nothing is missing on the phone —
> the app replaces that in-page tab with the native **Characters** picker,
> which can use the camera to scan pairing QR codes and seal saved servers into
> the Keychain or Keystore. The picker is the phone's Remote tab.

**Route C — the browser, as a second screen**

9. Open your phone's browser and go to the **Web session URL** from `.webinfo`,
   or scan the browser QR. The token is in the URL, so there's nothing to type.
10. Nothing to install, and the same URL works on a tablet or a spare laptop.

→ **Expected result:** the story feed streams live in your hand. Swipe from the
left edge for the **macro tray**, from the right for the **status drawer** with
its **Targets**, **Players** and **Room** sections, and long-press the wheel
puck for the touch wheel.
{{#endtab}}
{{#endtabs}}

## Make it yours

**Set the phone up as a glance-only second screen.** Leave your desktop client
running and open the browser URL on a tablet propped beside the keyboard. You
keep typing on the PC; the tablet shows the feed, the status drawer and your
group's condition without ever stealing focus. Nothing to install and nothing to
uninstall when you're done.

**Author on the couch.** The phone edits far more than people expect: macros,
touch-wheel slices, highlight rules **including redirects and squelch**, colors,
controller binds, and the entire desktop settings registry over the wire —
written to the hosting machine's config exactly as if you'd typed it there. See
[Make the game shout when something matters](./highlights-and-sounds.md) for the
phone path through the highlight editor.

**Know what stays at the desk.** The phone cannot author **layout or panel
placement**, **window resizing**, **game keybinds**, or **macro `hidden_when`
conditions**. Its chrome is fixed by design, so windows are not yours to move
there. Build the layout on the desktop; the phone renders what you built.

## When it doesn't work

**`.webinfo` says the web server is disabled.** The message is
`Web server is disabled. Enable [web] in config.toml or pass --web-port.` The
web server is off, not broken. Tick **Enable on port** in the Launcher's
**Advanced** section, or relaunch with `--web-port 8040`.

**`.webinfo` says the web server is not running.** A different message —
`Web server is not running (bind failed or still starting)` — and a different
cause. The server was asked to start but has no bound port. Usually another
program already holds that port; pick a different one. If you launched a moment
ago, give it a second and run `.webinfo` again.

**The URL says `127.0.0.1` and the phone can't load it.** That address means
"this PC only", and no other device can ever reach it. VellumFE tells you so
directly, printing
`Note: bind = "127.0.0.1" is this PC only. Set [web] bind = "0.0.0.0" so phones
on your LAN can connect.` Change **Bind address** to `0.0.0.0` and restart the
session — the bind is fixed at startup.

**The QR scans but nothing happens.** You almost certainly scanned the wrong one
of the two. The camera app handles the browser code (`http://…`); the app's
**Scan QR to add** screen handles the app code (`vellum://remote?…`). Crossed
over, both fail silently.

**The page loads but never connects.** Phone and PC must be on the same network.
Phones drop to mobile data when Wi-Fi looks unreliable, which silently ends the
LAN connection. Check the phone is really on your Wi-Fi, and that the PC's
firewall isn't blocking the port you chose.

**You want to play from outside the house.** Don't forward the port. Put both
devices on a Tailscale or WireGuard network and use the address that network
gives your PC — everything else on this page works unchanged, and the traffic is
encrypted by the tunnel.

## See also

- [Browser Client](../frontends/web.md) — the browser client in full: its two
  modes, the settings sheet, and everything the phone can author
- [iOS App](../frontends/ios.md) — TestFlight, the Characters picker, and
  backgrounding
- [Android App](../frontends/android.md) — installing the in-progress build,
  the Characters picker, and battery behavior
- [The Launcher](../getting-started/launcher.md) — the **Advanced** section that
  turns the web server on, and the separate SSH Launcher for cold-starting Lich
- [Make the game shout when something matters](./highlights-and-sounds.md) —
  authoring highlight rules from the phone
- [Command Reference](../reference/commands.md) — `.webinfo`
