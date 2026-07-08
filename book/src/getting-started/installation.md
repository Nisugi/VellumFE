# Installation

## Download

Download the latest release from [GitHub Releases](https://github.com/Nisugi/VellumFE/releases).

| Platform | File |
|----------|------|
| Windows | `vellum-fe-windows.zip` |
| macOS | `vellum-fe-macos.tar.gz` |
| Linux | `vellum-fe-linux.tar.gz` |
| Android | `vellum-fe-android-arm64.apk` |
| iOS | TestFlight (beta) — not a release download; see [iOS App](../frontends/ios.md) |

Extract the archive and place `vellum-fe` (or `vellum-fe.exe`) somewhere in your PATH.

## Android

The Android app is a full standalone client: it connects directly to the
game (no PC required) and uses the touch-first web interface.

1. Download `vellum-fe-android-arm64.apk` onto the phone and tap it;
   allow installs from the source when Android asks. Requires Android
   8.0 or newer.
2. On Android 8–9, update **Chrome** in the Play Store first — Chrome
   provides the app's rendering engine there, and an out-of-date engine
   shows a blank client with a red "connecting…" that never resolves.
3. Open VellumFE and log in. Saving the login stores the profile in the
   app's private storage on the phone.
4. Keep the app's notification enabled: it is what keeps the game
   connection alive while the screen is off.

Updates install in place over the previous version (releases share one
signing key). For automatic update checks, add this repository to
[Obtainium](https://github.com/ImranR98/Obtainium).

See [Android App](../frontends/android.md) for logging in, battery
behavior, and everything else about playing on the phone.

## Building from Source

Requires [Rust](https://rustup.rs/) 1.70+.

```bash
git clone https://github.com/Nisugi/VellumFE.git
cd VellumFE
cargo build --release
```

The binary will be at `target/release/vellum-fe`.

### TLS Dependencies

Direct eAccess authentication uses your operating system's native TLS stack
(SChannel on Windows, Security.framework on macOS) — no extra setup needed.
On Linux, a bundled OpenSSL is compiled automatically during the build; it
only requires Perl, which virtually every distro ships by default.

## Verify Installation

```bash
vellum-fe --version
```

Should display the version number (e.g., `vellum-fe 0.3.0-beta.4`).

## Configuration Directory

On first run, VellumFE creates `~/.vellum-fe/` with default configuration files.

You can override this location with the `VELLUM_FE_DIR` environment variable:

```bash
export VELLUM_FE_DIR=/custom/path
```
