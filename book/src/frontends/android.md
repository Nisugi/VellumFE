# Android App

A native Android app that runs the **entire VellumFE client on your
phone — no PC required**. The same Rust core as the desktop app runs as a
background service on the device, and the touch-first web client is the
screen. You log in with your play.net account directly from the phone.

## Getting It

Download `vellum-fe-android-arm64.apk` from
[GitHub releases](https://github.com/Nisugi/VellumFE/releases) and
sideload it — install steps, requirements (Android 8.0+, 64-bit), and
update options are in [Installation](../getting-started/installation.md#android).

## Logging In

The app opens on a login screen: enter account, password, character, and
game (all GemStone IV and DragonRealms worlds), or tap a saved profile.

Check **Remember this login** and the password is saved on the device,
encrypted with a hardware-backed key in the Android Keystore — the saved
file is unreadable off the device.

> **Lich never runs on the phone**, but scripted characters still work:
> the login screen's **Lich** tab attaches to a Lich session running on
> your PC (launch Lich with `--detachable-client`), and Lich's
> connect-a-device QR code prefills it. See
> [Connecting Through Lich](./web.md#connecting-through-lich).
> Alternatively, run the whole session on the PC and join it from the
> phone via the [mobile web frontend](./web.md).

## Playing

The interface is the [mobile web client](./web.md) — story pane, stream
chips, tappable exits and nouns, macro rail, side drawers with the injury
doll and character sheet, sounds, highlight and color editors. Everything
on that page applies here.

## Battery & Lifecycle

The session lives in a foreground service with a quiet status
notification ("Playing — session live", "Reconnecting…", …) that has a
**Stop** button.

- The app holds a wakelock **only while a session is active** — sitting
  at the login screen doesn't drain the battery.
- Swiping the app away mid-session **keeps playing**; swiping it away at
  the login screen stops the service.
- If the connection drops repeatedly with no input from you, it stops
  reconnecting rather than relogging all night — a forgotten phone winds
  down on its own.
- On first launch it asks once for a battery-optimization exemption so
  Android doesn't throttle the connection mid-hunt. If you decline, you
  can grant it later in system battery settings.

## Troubleshooting

- The UI runs in the system WebView. If the client is blank or stuck on
  "connecting…", update **Android System WebView** from the Play Store —
  on Android 8–9, update **Chrome**, which provides the engine there.
