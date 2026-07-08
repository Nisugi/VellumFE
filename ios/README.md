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

## TestFlight releases

Pushing a `v*.*.*-beta*` tag runs the `ios` job in
`.github/workflows/beta-release.yml`: device staticlib → `xcodegen` →
`xcodebuild archive` (manual signing: the distribution cert from the repo
secrets + the committed `ios/ci/VellumFE_AppStore.mobileprovision`) →
`-exportArchive` with `ios/ExportOptions.plist` (`destination = upload`),
which sends the build straight to App Store Connect. After Apple's
processing (usually minutes, plus a one-time review wait on the very first
build), it appears in the TestFlight app.

Signing is manual on purpose: automatic signing archives with a
*development* profile, which requires a registered device (a CI-only team
has none) and rejects a distribution-identity override as conflicting.
The provisioning profile expires **2027-07-07** (same day as the
distribution cert); regenerate both then — new cert → new p12 secrets,
then recreate the App Store profile via the ASC API or the developer
portal and commit it to `ios/ci/`.

Versioning: `CFBundleShortVersionString` is derived from the tag
(`v0.3.0-beta.3` → `0.3.0`); `CFBundleVersion` is the workflow run number,
so it strictly increases and never collides.

Repo secrets consumed (all set): `APPLE_TEAM_ID`,
`APPLE_DISTRIBUTION_P12_BASE64`, `APPLE_DISTRIBUTION_P12_PASSWORD`,
`APPSTORE_ISSUER_ID`, `APPSTORE_API_KEY_ID`, `APPSTORE_API_PRIVATE_KEY`.

Remaining one-time prerequisite: the app record `dev.vellumfe` must exist
in App Store Connect (`-allowProvisioningUpdates` auto-registers the App ID
and provisioning, but not the app record). If the upload fails with
"no suitable application records were found", create the app there first.

Privacy note: the first upload may trigger an ITMS-91053 email listing
required-reason API categories; amend `app/PrivacyInfo.xcprivacy` from it.
