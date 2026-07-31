# VellumFE

A modern, multi-frontend client for [GemStone IV](https://www.play.net/gs4/) — play in the terminal, in a native desktop GUI, or from your phone.

![License](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)
![Tests](https://img.shields.io/badge/tests-3%2C100%2B%20passing-brightgreen)
![Rust](https://img.shields.io/badge/rust-stable-orange)
[![Discord](https://img.shields.io/badge/discord-join%20us-5865F2?logo=discord&logoColor=white)](https://discord.gg/6nKhWRTkSN)

## Frontends

One core, four ways to play:

- **Terminal (TUI)** — the classic experience, built on [Ratatui](https://ratatui.rs/)
- **Desktop GUI** — native egui app with a Wrayth-style look, graphics skins, and in-app editors for every setting
- **Mobile Web** — the desktop client (or headless mode) serves a phone-friendly web UI; your phone becomes a second screen or a full client
- **Android / iOS apps** — the core and web UI bundled into native mobile apps

## Features

- **Customizable widget system** — progress bars, countdowns, compass, hands, injury doll, active effects, targets, players, inventory, spells, room window, and more
- **Maps and native travel** — in-client map rendering with click/tap-to-travel and a native `.go2` pathfinding engine
- **Tabbed text windows and stream routing** — route any game stream to any window, with in-app stream pickers on every frontend
- **Highlight system** — regex-based highlighting with Aho-Corasick fast matching, plus sound alerts
- **Hotkey bars** — command buttons with condition-driven styling and cooldown countdowns
- **Text-to-speech accessibility** — per-window speech, voice/rate control, gag and pronunciation rules, on desktop and phone
- **Fully themeable** — color themes plus optional GUI graphics skins; everything editable in-app, no config-file hand-editing required
- **Lich integration** — connect through Lich (including its WebUI panels rendered natively in the GUI) or skip it entirely
- **Direct eAccess authentication** — connect straight to GemStone IV with no proxy

## Quick Start

### Via Lich Proxy (Recommended)

```bash
# Start Lich with your character, then:
vellum-fe --port 8000 --character YourCharacter

# Or launch the desktop GUI:
vellum-fe --frontend gui --port 8000 --character YourCharacter
```

### Direct Connection (Standalone)

```bash
vellum-fe --direct \
  --account YOUR_ACCOUNT \
  --password YOUR_PASSWORD \
  --game prime \
  --character CHARACTER_NAME
```

## Installation

### Pre-built Binaries

Download from [Releases](https://github.com/Nisugi/VellumFE/releases).

### Build from Source

```bash
git clone https://github.com/Nisugi/VellumFE.git
cd VellumFE
cargo build --release
```

**Requirements:**
- Rust stable
- No TLS setup needed: Windows/macOS use the OS-native stack; Linux builds a bundled OpenSSL automatically (requires Perl, preinstalled on virtually all distros)

## Documentation

**[Full Documentation](https://nisugi.github.io/VellumFE/)** — guides, tutorials, and reference

Quick links:
- [Getting Started](https://nisugi.github.io/VellumFE/getting-started/)
- [Frontends](https://nisugi.github.io/VellumFE/frontends/)
- [Configuration Guide](https://nisugi.github.io/VellumFE/configuration/)
- [Widget Reference](https://nisugi.github.io/VellumFE/widgets/)
- [Command Reference](https://nisugi.github.io/VellumFE/reference/commands.html)
- [Troubleshooting](https://nisugi.github.io/VellumFE/reference/troubleshooting.html)

## Configuration

Settings live in TOML files under `~/.vellum-fe/`, but you rarely need to touch them — every setting has an in-app editor (GUI settings panels, TUI editors and dot-commands). Saves are atomic with automatic backups.

```
~/.vellum-fe/
├── config.toml        # Main configuration
├── layout.toml        # Widget layout
├── keybinds.toml      # Key bindings
├── highlights.toml    # Text highlighting rules
├── colors.toml        # Theme colors
├── hotbars.toml       # Hotkey bars
└── macros.toml        # Macros
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     Network Layer                       │
│         (Lich Proxy / Direct eAccess / WebUI)           │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                    Parser (XML)                         │
│               Wrayth Protocol Handler                   │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                  Core (AppCore)                         │
│        State Management & Message Processing            │
└──────┬───────────────┬───────────────┬──────────────────┘
       │               │               │
┌──────▼──────┐ ┌──────▼──────┐ ┌──────▼──────────────────┐
│ TUI         │ │ Desktop GUI │ │ Web Server              │
│ (Ratatui)   │ │ (egui)      │ │ (phone / Android / iOS) │
└─────────────┘ └─────────────┘ └─────────────────────────┘
```

## Community

Join the [VellumFE Discord](https://discord.gg/6nKhWRTkSN) — help and
setup questions, layout/skin/wheel showcases, accessibility and
controller talk, beta testing, and release announcements.

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for
development setup and contribution licensing terms.

```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- --port 8000
```

## License

Copyright © 2026 Nisugi

Licensed under the GNU General Public License v3.0 or later
([LICENSE](LICENSE)). You may use, modify, and redistribute VellumFE
freely; any distributed version, modified or not, must remain under the
same license with source code available.

## Code Signing Policy

Windows releases are signed with a certificate provided by the
[SignPath Foundation](https://signpath.org/), with free code signing
provided by [SignPath.io](https://about.signpath.io/).

- **Team roles**: [Nisugi](https://github.com/Nisugi) is the project
  author, reviewer, and release approver.
- **Signing process**: binaries are built from this repository's source
  by GitHub Actions ([beta-release.yml](.github/workflows/beta-release.yml));
  each release is manually approved for signing by the approver.
- **Privacy**: this program will not transfer any information to other
  networked systems unless specifically requested by the user or the
  person installing or operating it. See the [privacy policy](PRIVACY.md).

## Acknowledgments

- Built with [Ratatui](https://ratatui.rs/) and [egui](https://www.egui.rs/)
- Inspired by [ProfanityFE](https://github.com/matt-lowe/ProfanityFE) and Wrayth
- Thanks to the [Lich](https://github.com/elanthia-online/lich-5) community
