# VellumFE

A modern, multi-frontend client for [GemStone IV](https://www.play.net/gs4/) — play in the terminal, in a native desktop GUI, or from your phone.

![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)
![Tests](https://img.shields.io/badge/tests-3%2C100%2B%20passing-brightgreen)
![Rust](https://img.shields.io/badge/rust-stable-orange)

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

## Contributing

Contributions welcome!

```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- --port 8000
```

## License

Licensed under either of:
- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

## Acknowledgments

- Built with [Ratatui](https://ratatui.rs/) and [egui](https://www.egui.rs/)
- Inspired by [ProfanityFE](https://github.com/matt-lowe/ProfanityFE) and Wrayth
- Thanks to the [Lich](https://github.com/elanthia-online/lich-5) community
