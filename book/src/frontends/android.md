# Android App

> The whole client on your phone — log in, hunt, and script from a bus stop with
> no PC involved. **(in progress)**

## What it's for

You want to play GemStone IV without being at your desk. The Android app carries
the same Rust core the desktop client runs, so the phone connects to play.net by
itself: enter your account, pick a character, and you are in the game. Nothing
has to be running at home.

When something *is* running at home, the app plays that too. It attaches to a
Lich session on your PC so a scripted character keeps all its scripts, and it can
pair with a VellumFE session already running on your desktop and become a live
second screen for it. **The app is a full client and a pairing client — not one
or the other.**

<figure class="shot" data-shot="mobile/android-story-pane">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The Android app mid-hunt: story pane with stream chips, the vitals strip, the floating compass, and the macro rail.</figcaption>
</figure>

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

There is no Android app on the desktop — but the desktop is where you prepare
the two things the phone will reach for.

1. **To let the phone mirror this session:** open the connection's **Advanced**
   fold in the **VellumFE Launcher**, tick **Enable on port** under **Web
   dashboard**, and set **Bind address** to `0.0.0.0`. Launch, then type
   `.webinfo` and leave the pairing page open — the phone scans the **VellumFE
   app** QR from it.
2. **To let the phone attach to Lich:** launch Lich with `--detachable-client`
   and note the host and port you gave it.

→ **Expected result:** a pairing page on screen with two QR codes, and a Lich
host and port written down. The phone does the rest.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Same preparation, from the terminal:

1. Start the session with `--web-port 8040` and `bind = "0.0.0.0"` in the
   `[web]` block, then type `.webinfo`. Note the `VellumFE app link:` line — it
   begins `vellum://remote?` and is what the app QR encodes.
2. For Lich, launch it with `--detachable-client` and note host and port.

→ **Expected result:** the `vellum://remote?…` link printed in your terminal
session, ready to scan or type into the phone.
{{#endtab}}
{{#tab name="Mobile"}}

1. Download `vellum-fe-android-arm64.apk` from
   [GitHub releases](https://github.com/Nisugi/VellumFE/releases) and sideload
   it. Requirements and update options are in
   [Installation](../getting-started/installation.md). **Android 8.0 or newer,
   64-bit.**
2. Open the app. The login screen shows two tabs:
   - **`play.net`** — `play.net account`, `password`, `character`, a game
     selector covering every GemStone IV and DragonRealms world, and **Remember
     this login**. Tap **Connect**.
   - **`Lich`** — `host`, `port`, `label (optional)`, and **Remember this
     connection** (ticked by default), for attaching to Lich on your PC.
3. To pair with a desktop VellumFE session instead, tap the **person icon** in
   the login screen's action row to open **Characters**, then **⧉  Scan QR to
   add** and point the camera at the **VellumFE app** QR from `.webinfo`.
4. Tap the saved row to connect. **‹  Back to login** returns to the two tabs.

<figure class="shot" data-shot="mobile/android-characters-picker">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The Android <b>Characters</b> picker: saved servers with live/offline dots, above <b>⧉  Scan QR to add</b> and <b>＋  Add manually</b>.</figcaption>
</figure>

→ **Expected result:** the game text scrolling on your phone — from play.net
directly, from your PC's Lich, or mirrored from your PC's VellumFE session,
depending on which route you took.
{{#endtab}}
{{#endtabs}}

> ⚠️ **The app shows two login tabs; a desktop browser shows three.** The
> browser's in-page **Remote** tab is hidden here on purpose, because the native
> **Characters** picker replaces it — the picker can use the camera to scan
> pairing QR codes and can seal saved servers into the Android Keystore, which a
> web form cannot. Nothing is lost; it moved.

## Common setups

### Take your desk session with you

1. On the PC, launch your character with web enabled on `0.0.0.0` and run
   `.webinfo`.
2. On the phone, open **Characters** → **⧉  Scan QR to add** and scan the
   **VellumFE app** QR. The entry names itself after the character, because the
   link carries the character name.
3. Tap the new row.

→ Your PC session keeps its own GUI window open and playing; the phone shows the
same character live. Lich's one-client limit does not bite here — the web layer
mirrors the session rather than replacing its client.

### Add a server by hand when there's no QR to scan

1. **Characters** → **＋  Add manually**. The dialog is titled **Add character**.
2. Fill in **Label (required, e.g. Rysk)**, **Host (e.g. 192.168.1.21)**,
   **Port (e.g. 8042)**, and **Pairing token (optional)**. Tap **Save**.
3. The row appears with a dot: green and **live** when the machine answers,
   grey and **offline** when it does not.

→ You can tell at a glance whether the PC is up before you tap, without waiting
on a failed connection.

## Playing

The screen is the [browser client](./web.md) — story pane, stream chips,
tappable exits and nouns, macro tray, status drawer with the injury doll and
character sheet, sounds, and the highlight and color editors. Everything on that
page applies here, including the fact that **the phone's highlight editor does
handle redirects and squelch**.

## Battery & lifecycle

The session lives in a foreground service with a quiet status notification
("Playing — session live", "Reconnecting…") carrying a **Stop** button.

- The wakelock is held **only while a session is active** — sitting at the login
  screen does not drain the battery.
- Swiping the app away mid-session **keeps playing**. Swiping it away at the
  login screen stops the service.
- Repeated drops with no input from you stop the reconnect loop, so a forgotten
  phone winds down instead of relogging all night.
- On first launch it asks once for a battery-optimization exemption so Android
  does not throttle the connection mid-hunt. Decline it and you can grant it
  later in system battery settings.

## Tips & gotchas

> ⚠️ **On Android 8 and 9, update Chrome — not just Android System WebView.**
> Those releases take the rendering engine from Chrome. If the client is blank or
> stuck on "connecting…", update Chrome from the Play Store first. On Android 10
> and newer, update **Android System WebView**.

- **Deleting a saved server** is the **✕** on its row, which asks **"Delete
  {name}?"** with the note *"Removes the saved server (and its pairing token)."*
  Deleting the entry deletes the pairing — you re-scan to come back.
- **Saved credentials never sit in plaintext.** A remembered play.net password is
  sealed by the Rust core with a device master key, and saved servers live in
  `remote.bin` sealed by an Android Keystore key. Neither is readable off the
  device, and neither uses SharedPreferences.
- **Scanning the wrong QR does nothing.** The pairing page shows two: the
  **browser** one (`http://…/#token=…`) is for a browser, the **VellumFE app**
  one (`vellum://remote?…`) is for this app.
- **A `vellum://lich?host=…&port=…` link prefills the Lich tab and stops there.**
  It never auto-connects — you always press **Connect** yourself.
- **The phone needs a private path to your PC.** Home Wi-Fi, Tailscale, or a VPN.
  Never the open internet, for either the Lich port or the VellumFE web port.

## See also

- [Browser client](./web.md) — the interface this app renders, and the `[web]`
  settings on the PC side.
- [iOS app](./ios.md) — the same client on iPhone.
- [Installation](../getting-started/installation.md) — sideloading and updates.
- [The Launcher](../getting-started/launcher.md) — where **Web dashboard** and
  **Bind address** live on the desktop.
