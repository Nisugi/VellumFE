# iOS App

> The whole client on your iPhone — log in and play with no PC involved.
> **(beta — via TestFlight)**

## What it's for

The iOS app carries the same Rust core the desktop client runs, so the phone
talks to play.net by itself. Account, character, game world, **Connect** — you
are in, with nothing running at home.

And when something *is* running at home, the app plays that as well: it attaches
to a Lich session on your PC so a scripted character keeps its scripts, and it
pairs with a VellumFE session already running on your desktop to become a live
second screen for it. **The app is a full client and a pairing client — not one
or the other.**

<figure class="shot" data-shot="mobile/ios-story-pane">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The iOS app in a hunt: story pane with stream chips, vitals strip, floating compass, and the macro rail.</figcaption>
</figure>

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

There is no iOS app on the desktop — but the desktop prepares what the iPhone
will reach for.

1. **To let the iPhone mirror this session:** in the **VellumFE Launcher**, open
   the connection's **Advanced** fold, tick **Enable on port** under **Web
   dashboard**, and set **Bind address** to `0.0.0.0`. Launch, then type
   `.webinfo` and leave the pairing page on screen — the iPhone scans the
   **VellumFE app** QR from it.
2. **To let the iPhone attach to Lich:** launch Lich with `--detachable-client`
   and note the host and port.

→ **Expected result:** a pairing page showing two QR codes, plus a Lich host and
port to hand to the phone.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Same preparation from a terminal session:

1. Start with `--web-port 8040` and `bind = "0.0.0.0"` in the `[web]` block, then
   type `.webinfo`. The `VellumFE app link:` line begins `vellum://remote?` — it
   is exactly what the app QR encodes, so you can also type its host, port, and
   token into the phone by hand.
2. For Lich, launch it with `--detachable-client` and note host and port.

→ **Expected result:** the `vellum://remote?…` link in your terminal, ready to
scan or transcribe.
{{#endtab}}
{{#tab name="Mobile"}}

1. iOS builds ship through Apple's **TestFlight** while in beta — there is no
   downloadable file on the releases page. Joining instructions are in the
   release notes on
   [GitHub releases](https://github.com/Nisugi/VellumFE/releases).
2. Open the app. The login screen shows two tabs:
   - **`play.net`** — `play.net account`, `password`, `character`, a game
     selector covering every GemStone IV and DragonRealms world, and **Remember
     this login**. Tap **Connect**.
   - **`Lich`** — `host`, `port`, `label (optional)`, and **Remember this
     connection** (ticked by default), for attaching to Lich on your PC.
3. To pair with a desktop VellumFE session instead, tap the **person icon** in
   the login screen's action row to open **Characters**, then **Scan QR to add**
   and point the camera at the **VellumFE app** QR from `.webinfo`.
4. Tap the saved row to connect. The back control (accessibility label **Back to
   login**) returns to the two tabs.

<figure class="shot" data-shot="mobile/ios-characters-picker">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The iOS <b>Characters</b> picker: saved servers with live/offline dots, above <b>Scan QR to add</b> and <b>Add manually</b>.</figcaption>
</figure>

→ **Expected result:** game text scrolling on your iPhone — direct from
play.net, through your PC's Lich, or mirrored from your PC's VellumFE session,
depending on the route you took.
{{#endtab}}
{{#endtabs}}

> ⚠️ **The app shows two login tabs; a desktop browser shows three.** The
> browser's in-page **Remote** tab is hidden here deliberately, because the
> native **Characters** picker replaces it — the picker can scan pairing QR codes
> with the camera and store saved servers in the iOS Keychain, which a web form
> cannot. The capability moved; it was not removed.

## Common setups

### Carry your desk session to the couch

1. On the PC, launch the character with web enabled on `0.0.0.0` and run
   `.webinfo`.
2. On the iPhone, open **Characters** → **Scan QR to add** and scan the
   **VellumFE app** QR. The saved entry names itself after the character,
   because the link carries the character name.
3. Tap the new row.

→ The PC session keeps its own window open and playing while the iPhone shows
the same character live. Lich's one-client limit does not apply — the web layer
mirrors the session rather than replacing its client.

### Add a server by hand when there's no QR

1. **Characters** → **Add manually**. The sheet is titled **Add character**.
2. Fill in **Label (required, e.g. Rysk)**, **Host (e.g. 192.168.1.21)**,
   **Port (e.g. 8042)**, and **Pairing token (optional)**. Tap **Save**.
3. The row shows a dot: green and **live** when the machine answers, grey and
   **offline** when it does not.

→ You can see whether the PC is awake before tapping, instead of waiting out a
failed connection.

## Playing

The screen is the [browser client](./web.md) — story pane, stream chips,
tappable exits and nouns, macro tray, status drawer with the injury doll and
character sheet, sounds, and the highlight and color editors. Everything on that
page applies here, including the fact that **the phone's highlight editor does
handle redirects and squelch**.

## Backgrounding

iOS has no equivalent of Android's foreground service. When the app goes to the
background iOS suspends it and the game connection goes stale.

- On return, the session **reconnects automatically** and resumes where you left
  off. Expect reconnect-on-return rather than a keep-alive.
- Repeated drops with no input from you stop the reconnect loop, so a forgotten
  phone winds down instead of relogging all night.
- While you are on a paired desktop session, the app's embedded core sits idle —
  there is no game connection on the phone to go stale, and the usual web
  reconnect picks the mirror back up when you come back.

## Tips & gotchas

> ⚠️ **Deleting a saved character deletes its pairing token with it.** Swipe a
> row to delete, or tap **Edit** in the toolbar for the multi-select delete. To
> come back you re-scan the QR or re-enter the token.

- **Saved credentials never sit in plaintext.** Saved servers live in the iOS
  Keychain under service `dev.vellumfe.remote-server`, accessible only after the
  first unlock and only on that device (never in a backup). A remembered play.net
  password is sealed by the Rust core with a device master key.
- **Scanning the wrong QR does nothing.** The pairing page shows two: the
  **browser** one (`http://…/#token=…`) is for a browser; the **VellumFE app**
  one (`vellum://remote?…`) is for this app.
- **A `vellum://lich?host=…&port=…` link prefills the Lich tab and stops there.**
  It never auto-connects — you always press **Connect** yourself.
- **The iPhone needs a private path to your PC.** Home Wi-Fi, Tailscale, or a
  VPN. Never the open internet, for either the Lich port or the VellumFE web
  port.
- **The way back off a paired session** is ⚙ → **Leave this server (app login)**,
  which returns you to the two-tab login screen.

## See also

- [Browser client](./web.md) — the interface this app renders, and the `[web]`
  settings on the PC side.
- [Android app](./android.md) — the same client on Android.
- [The Launcher](../getting-started/launcher.md) — where **Web dashboard** and
  **Bind address** live on the desktop.
