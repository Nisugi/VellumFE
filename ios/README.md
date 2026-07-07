# VellumFE iOS shell

A thin SwiftUI/WKWebView shell over the Rust core running headless in-process,
serving the embedded web UI on `http://127.0.0.1:<port>` — the same
architecture as the Android app (`android/`). The Swift sources mirror the
Kotlin shell: `CoreBridge.swift` ↔ `VellumCore.kt`, `CryptoKeys.swift` ↔
`CryptoKeys.kt`, `ContentView.swift` ↔ `MainActivity.bootAndLoad`.

There is no `CoreService` analog: iOS has no foreground services. Backgrounding
suspends the process and the game TCP session goes stale; on return the
headless reconnect supervisor restores it. Expect **reconnect-on-return**,
unlike Android's keep-alive.

## Building on a Mac

Prereqs: Xcode, Rust with the iOS targets, XcodeGen.

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
brew install xcodegen
```

Build (from the repo root — the Rust staticlib must exist before Xcode links):

```sh
cargo build --release -p vellum-ios --target aarch64-apple-ios      # device
cargo build --release -p vellum-ios --target aarch64-apple-ios-sim  # simulator
cd ios && xcodegen generate && open VellumFE.xcodeproj
```

Select your iPhone (or a simulator) as the destination and Run. Signing is
automatic on team `37YA6ZBNY2`. After editing `project.yml`, re-run
`xcodegen generate` — the `.xcodeproj` and `app/Info.plist` are generated and
gitignored; never hand-edit them.

Rebuilding after Rust or web-asset changes (`src/frontend/web/assets/` is
embedded via `include_str!`): re-run the `cargo build` for your destination,
then build in Xcode as usual.

## Debugging

- Rust core logs: Xcode console (os_log subsystem `dev.vellumfe.core`), or
  Console.app filtered on the same subsystem.
- Web UI: Safari on a paired Mac → Develop → your iPhone → the VellumFE page
  (WKWebView inspection is enabled in DEBUG builds only).
- Core file logs live under the app container:
  `Application Support/vellum/` (Xcode → Devices → download container).

## One-time distribution chores (Phase I3)

Documented in the paperwork guide; summary of repo secrets consumed by CI:
`APPLE_TEAM_ID` (set), `APPLE_DISTRIBUTION_P12_BASE64`,
`APPLE_DISTRIBUTION_P12_PASSWORD`, `APPSTORE_ISSUER_ID`,
`APPSTORE_API_KEY_ID`, `APPSTORE_API_PRIVATE_KEY`.
