# VellumFE

A modern, feature-rich terminal client for [GemStone IV](https://www.play.net/gs4/).

![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)
![Tests](https://img.shields.io/badge/tests-1%2C003%20passing-brightgreen)
![Rust](https://img.shields.io/badge/rust-stable-orange)

## Features

- **Customizable Widget System** - Progress bars, countdowns, compass, hands, indicators, injury doll, active effects, and more
- **Tabbed Text Windows** - Route game streams to organized tabs (thoughts, combat, loot, etc.)
- **Highlight System** - Regex-based text highlighting with Aho-Corasick fast matching
- **Sound Alerts** - Play sounds on pattern matches with volume control
- **Direct eAccess Authentication** - Connect directly to GemStone IV without Lich proxy
- **Fully Themeable** - Complete color customization with preset themes
- **Layout Editor** - Interactive widget positioning and resizing (F2)
- **Comprehensive Testing** - 1,003 tests including end-to-end UI integration tests

## Quick Start

### Via Lich Proxy (Recommended)

```bash
# Start Lich with your character, then:
vellum-fe --port 8000 --character YourCharacter
```

### Direct Connection (Standalone)

```bash
vellum-fe --direct \
  --account YOUR_ACCOUNT \
  --password YOUR_PASSWORD \
  --game prime \
  --character CHARACTER_NAME
```

## Installation for Development. NOT NEEDED TO USE VellumFE

### Pre-built Binaries

Download from [Releases](https://github.com/Nisugi/vellum-fe/releases).

### Build from Source

```bash
# Clone the repository
git clone https://github.com/Nisugi/vellum-fe.git
cd vellum-fe

# Build release binary
cargo build --release

# Binary is at target/release/vellum-fe.exe
```

**Requirements:**
- Rust 1.70+ (stable)
- OpenSSL (for direct mode) - install via vcpkg on Windows

## Documentation

**[Full Documentation](https://nisugi.github.io/vellum-fe/)** - Comprehensive guides, tutorials, and reference

Quick links:
- [Getting Started](https://nisugi.github.io/vellum-fe/getting-started/)
- [Configuration Guide](https://nisugi.github.io/vellum-fe/configuration/)
- [Widget Reference](https://nisugi.github.io/vellum-fe/widgets/)
- [Keybind Actions](https://nisugi.github.io/vellum-fe/reference/keybind-actions.html)
- [Troubleshooting](https://nisugi.github.io/vellum-fe/troubleshooting/)

## Default Keybinds

| Key | Action |
|-----|--------|
| `F2` | Toggle layout editor |
| `F3` | Toggle highlight browser |
| `Page Up/Down` | Scroll main window |
| `Tab` | Cycle focus between widgets |
| `Ctrl+C` | Copy selected text |
| `Escape` | Close popups / cancel |

See [Keybind Reference](https://nisugi.github.io/vellum-fe/reference/keybind-actions.html) for complete list.

## Configuration

VellumFE uses TOML configuration files stored in `~/.vellum-fe/`:

```
~/.vellum-fe/
├── config.toml        # Main configuration
├── layout.toml        # Widget layout
├── keybinds.toml      # Key bindings
├── highlights.toml    # Text highlighting rules
└── colors.toml        # Theme colors
```

Example highlight:
```toml
[[highlights]]
pattern = "You are stunned"
fg = "bright_red"
bold = true
sound = "alert.wav"
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      Network Layer                       │
│            (Lich Proxy / Direct eAccess)                │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                    Parser (XML)                          │
│                Wrayth Protocol Handler                   │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                  Core (AppCore)                          │
│         State Management & Message Processing            │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                 TUI Frontend (Ratatui)                   │
│              Widget Rendering & Input                    │
└─────────────────────────────────────────────────────────┘
```

## Contributing

Contributions welcome! Please see [Contributing Guide](https://nisugi.github.io/vellum-fe/development/contributing.html).

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

- Forked from [VellumFE](https://github.com/Nisugi/VellumFE)
- Built with [Ratatui](https://ratatui.rs/) for terminal UI
- Inspired by [Profanity](https://github.com/jkindwall/profanity-beta)
