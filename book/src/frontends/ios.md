# iOS App (Beta)

A native iOS app that runs the **entire VellumFE client on your
iPhone — no PC required**. Like the [Android app](./android.md), the same
Rust core as the desktop app runs inside the app, and the touch-first web
client is the screen.

## Getting It

iOS builds ship through Apple's **TestFlight** while in beta — they are
not downloadable files on the GitHub releases page. Requires iOS 16 or
newer. See the release notes on
[GitHub releases](https://github.com/Nisugi/VellumFE/releases) for how to
join the beta.

## Logging In

The login screen is the same as the other mobile frontends: enter
account, password, character, and game (all GemStone IV and DragonRealms
worlds), tap a saved profile, or switch to the **Lich** tab to attach to
a Lich session running on your PC (see
[Connecting Through Lich](./web.md#connecting-through-lich)).

Check **Remember this login** and the password is sealed with a key held
in the iOS Keychain (device-bound, never in backups) — the saved file is
unreadable off the device.

Scanning a `vellum://` QR code opens the app with the Lich tab
prefilled. Lich 6's connect-a-device panel will display one (not yet
available); until then, enter the host and port manually.

## Playing

The interface is the [mobile web client](./web.md) — story pane, stream
chips, tappable exits and nouns, macro rail, side drawers with the injury
doll and character sheet, sounds, highlight and color editors. Everything
on that page applies here.

## Backgrounding

iOS has no equivalent of Android's foreground service: when the app goes
to the background, iOS suspends it and the game connection goes stale.
When you come back, the session **reconnects automatically** and resumes
where you left off — expect reconnect-on-return rather than Android's
keep-alive. As on the other mobile frontends, if the connection drops
repeatedly with no input from you, the app stops reconnecting rather than
relogging all night.
