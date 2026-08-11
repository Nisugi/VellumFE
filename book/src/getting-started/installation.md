# Installing VellumFE

> Get a working `vellum-fe` on your machine, past your operating system's
> defenses, in about two minutes.

## What it's for

You want to be in Icemule by tonight, not fighting a download. VellumFE ships
as a single self-contained binary per platform — no installer, no runtime to
install, no Ruby, no OpenSSL. Windows and macOS releases are code-signed, so
your OS should let them run without an argument. This page gets the file onto
your disk and proves it works; [The Launcher](./launcher.md) gets you into the
game.

<figure class="shot" data-shot="gui/install-releases-page">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The GitHub Releases page for a beta tag, with the platform assets
  and <code>SHA256SUMS.txt</code> listed under <b>Assets</b>.</figcaption>
</figure>

## Set it up

Every desktop release lives on the
[GitHub Releases page](https://github.com/Nisugi/VellumFE/releases). This is a
platform split rather than a frontend split — the same binary contains the
terminal frontend, the desktop GUI, and the web server, and which one you get
is decided at launch, not at download.

{{#tabs global="platform"}}
{{#tab name="Windows"}}

1. Download `vellum-fe-windows-x86_64.zip` from the latest release.
2. Right-click the zip and choose **Extract All…**. Put `vellum-fe.exe`
   somewhere permanent — `C:\Tools\VellumFE\` works; your Downloads folder
   does not, because you will move it later and break your shortcuts.
3. Double-click `vellum-fe.exe`.

The executable is signed through Azure Trusted Signing as part of the release
build, which is what stops Defender from flagging low-prevalence Rust binaries
as `Trojan:Win32/Cloxer`. If SmartScreen still shows a blue "Windows protected
your PC" panel on a brand-new release, click **More info** ▸ **Run anyway** —
reputation builds up per-version over the first days after a release.

→ **Expected result:** the **VellumFE Launcher** window opens with the heading
**VellumFE** and the line **Choose a connection to launch**. No black console
window sits behind it.
{{#endtab}}
{{#tab name="macOS"}}

1. Download `vellum-fe-macos.dmg` from the latest release for the app bundle,
   or `vellum-fe-macos-universal.zip` if you want the bare binary. The
   `-arm64` and `-x64` zips are single-chip builds; the universal one runs on
   both.
2. Open the dmg and drag **VellumFE** into **Applications**.
3. Double-click **VellumFE**.

Release dmgs are signed with a Developer ID certificate, notarized by Apple,
and stapled — so Gatekeeper approves the app offline, without the
right-click-**Open** dance older unsigned builds needed.

→ **Expected result:** the **VellumFE Launcher** window opens, headed
**VellumFE**, with **Choose a connection to launch** beneath it.
{{#endtab}}
{{#tab name="Linux"}}

1. Download `vellum-fe-linux-x86_64.tar.gz`.
2. Unpack it and put the binary on your `PATH`:

   ```bash
   tar -xzf vellum-fe-linux-x86_64.tar.gz
   install -m755 vellum-fe ~/.local/bin/vellum-fe
   ```

3. Run `vellum-fe` with no arguments.

The build targets glibc distributions. Install `libasound2` for sound, and your
desktop's clipboard libraries — most environments already ship both. There is no
signing story on Linux; verify the download against `SHA256SUMS.txt` on the
release page instead.

> ⚠️ **Running with no arguments opens the graphical launcher, which needs a
> display.** On a headless box, skip the launcher and pass connection flags
> directly — see [First Launch](./first-launch.md).

→ **Expected result:** the **VellumFE Launcher** window opens on a desktop
session; on a headless box you get a display error instead, which is your cue to
use flags.
{{#endtab}}
{{#tab name="Android"}}

The Android app **(in progress)** is a full client in its own right — it can log
in to play.net with no PC involved, attach to a Lich session, cold-start Lich
over SSH, or pair with a VellumFE session already running on your PC. It uses the
touch interface rather than the desktop GUI.

1. Download `vellum-fe-android-arm64.apk` onto the phone and tap it. Allow
   installs from the source when Android asks. Android 8.0 or newer.
2. On Android 8 and 9, update **Chrome** from the Play Store before first run.
   Chrome supplies the app's rendering engine on those versions, and an
   out-of-date engine leaves you staring at a red "connecting…" that never
   resolves.
3. Open VellumFE and log in on the app's own login screen. There is no
   connection list to import — the desktop Launcher and its `launcher.toml` stay
   on the desktop.
4. Leave the app's notification enabled. It is what holds the game connection
   open while the screen is off.

Updates install over the previous version — every release is signed with the
same key. Point [Obtainium](https://github.com/ImranR98/Obtainium) at this
repository for automatic update checks.

→ **Expected result:** VellumFE opens to its login screen, and after logging in
you land in the touch client. See [Android App](../frontends/android.md).
{{#endtab}}
{{#tab name="iOS"}}

iOS is a **TestFlight beta** and is not a downloadable asset on the releases
page — the release build uploads it straight to App Store Connect. You need a
TestFlight invitation; ask in [the Discord](https://discord.gg/6nKhWRTkSN) for
one, then install through Apple's TestFlight app.

Like Android, it is a full client with its own login screen — play.net direct,
Lich, SSH cold-start, or pairing with a session on your PC.

→ **Expected result:** VellumFE appears in TestFlight as an installable build.
See [iOS App](../frontends/ios.md).
{{#endtab}}
{{#endtabs}}

## Common setups

### Verify what you downloaded

Every release publishes `SHA256SUMS.txt` alongside the assets. Compare before
you run anything:

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing
```

On Windows PowerShell, `Get-FileHash vellum-fe-windows-x86_64.zip -Algorithm
SHA256` prints a hash to match by eye against the line in `SHA256SUMS.txt`.

→ You see `vellum-fe-windows-x86_64.zip: OK` (or a matching hash), which means
the archive is byte-for-byte the one CI built and signed.

### Keep configs somewhere other than your home directory

VellumFE writes everything it owns — settings, layouts, highlights, skins,
logs, saved connections — under `~/.vellum-fe/`. It is created on first run. To
put that elsewhere (a synced folder, a portable drive), point `VELLUM_FE_DIR` at
the location before starting:

```bash
VELLUM_FE_DIR=/mnt/sync/vellum vellum-fe
```

The same directory can also be set per connection, in the Launcher's
**Advanced** fold under **Data directory** — handy when one character's setup
should live apart from the rest.

→ Start VellumFE and the folder you named fills with `launcher.toml`,
`global/`, `profiles/`, and `vellum-fe.log` instead of your home directory.

### Build it yourself

You need a recent stable [Rust](https://rustup.rs/) toolchain — CI builds on
latest stable.

```bash
git clone https://github.com/Nisugi/VellumFE.git
cd VellumFE
cargo build --release
```

The binary lands at `target/release/vellum-fe`. Direct eAccess login uses your
OS's native TLS stack on Windows (SChannel) and macOS (Security.framework), so
there is nothing extra to install; Linux compiles a bundled OpenSSL during the
build, which needs Perl — present on effectively every distro.

→ `./target/release/vellum-fe --version` prints the version you just built.

## Tips & gotchas

> ⚠️ **A bare `vellum-fe` opens the Launcher; a `vellum-fe` with any flag does
> not.** The no-arguments case is what double-clicking produces, and it routes
> to the graphical launcher. The moment you pass even one flag you are on the
> command-line path, and **that path defaults to the terminal frontend, not the
> GUI** — add `--frontend gui` if you wanted a window. This catches almost
> everyone once. See [The Launcher](./launcher.md).

> ⚠️ **Extract the archive before running it.** Windows will happily run an exe
> from inside a zip preview, then fail confusingly when the process tries to
> spawn a session copy of itself.

- **Nothing prints to your terminal.** VellumFE logs to a file, because a
  terminal UI cannot share stdout with a log. Diagnostics live in
  `~/.vellum-fe/vellum-fe.log`, including panics. Raise the level with
  `RUST_LOG=debug`.
- **Moving the binary is fine; moving it after saving connections is fine too.**
  Connections are stored in `~/.vellum-fe/`, not next to the exe. The Launcher
  spawns sessions using its own current path, so a moved binary keeps working
  as long as you launch the moved copy.
- **The apps keep their own connections; they don't import your desktop ones.**
  Your `launcher.toml` stays on the desktop, and the phone saves its logins on
  the device. The apps *can* pair with a running desktop session (**Characters**
  ▸ **Scan QR to add**), and a browser can act as a second screen without
  installing anything — two routes for two jobs, both in
  [Put VellumFE on your phone](../how-to/vellum-on-your-phone.md).

## See also

- [The Launcher](./launcher.md) — saved connections and one-click sessions
- [First Launch](./first-launch.md) — connecting through Lich or directly
- [Android App](../frontends/android.md) · [iOS App](../frontends/ios.md)
- [Desktop GUI](../frontends/gui.md) · [Terminal (TUI)](../frontends/tui.md)

<details>
<summary>Config reference (TOML)</summary>

Installation itself writes no TOML — this is where the files it creates live.
The base directory is `~/.vellum-fe/`, or whatever `VELLUM_FE_DIR` names.

| Path | What it holds |
|---|---|
| `launcher.toml` | Saved connections for the Launcher. Never contains passwords. |
| `ssh-launcher.toml` | SSH target and per-character ports for the SSH Launcher. Never contains keys. |
| `vellum-fe.log` | The one log file, including panics and backtraces. |
| `global/` | Shared across characters: `highlights.toml`, `keybinds.toml`, `colors.toml`, `hotbars.toml`, `controller.toml`, `macros.toml`, plus `skins/`, `images/`, `data/`. |
| `profiles/<character>/` | Per-character `config.toml`, `layout.toml`, `highlights.toml`, `keybinds.toml`, `hotbars.toml`, `controller.toml`, `history.txt`, `widget_state.toml`, `debug.log`. |
| `layouts/` | Named layouts from `.savelayout`: TUI as `<name>.toml`, GUI as `<name>.json`. |
| `highlights/`, `keybinds/` | Named highlight and keybind sets. |
| `themes/` | Custom themes as `<name>.toml`. |

**Environment variables**

| Name | Default | What it does |
|---|---|---|
| `VELLUM_FE_DIR` | `~/.vellum-fe` | Base directory for every file above. Overridden by `--data-dir` and by a connection's **Data directory**. |
| `RUST_LOG` | `info` | Log level filter for `vellum-fe.log`. `debug` for troubleshooting. |

Every user-authored file is written atomically: VellumFE writes `<name>.tmp`,
copies the current file to `<name>.bak`, then renames. If a save ever goes
wrong, the previous version is one rename away.

</details>
