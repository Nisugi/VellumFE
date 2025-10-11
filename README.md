# profanity-rs

A modern, Rust-based terminal client for GemStone IV, built with [Ratatui](https://github.com/ratatui-org/ratatui). This is a complete rewrite of [ProfanityFE](https://github.com/elanthia-online/profanity) with enhanced features and performance.

![Screenshot](https://via.placeholder.com/800x400.png?text=Terminal+UI+Screenshot)

## Features

- **Dynamic Window Management** - Create, delete, move, and resize windows on the fly
- **Rich Widget Library** - 40+ pre-built widgets (text, progress bars, timers, compass, injury doll, active effects, targets, players)
- **Combat Tracking** - Scrollable target list with status indicators and current target highlighting
- **Player Tracking** - Scrollable player list showing all characters in the room with status
- **Spell Coloring** - Customize active spell/effect colors by spell ID for easy visual distinction
- **Mouse Support** - Click to focus, scroll to navigate, drag to move/resize
- **Text Selection** - Shift+drag to select and copy text
- **Stream Routing** - Game streams automatically route to appropriate windows
- **Layout Management** - Save and load custom window layouts
- **Performance Monitoring** - Real-time FPS, render times, network, and memory stats
- **XML Parsing** - Full support for GemStone IV's XML protocol
- **Live Configuration** - Most settings can be changed without restarting

## Quick Start

### 1. Build from Source

```bash
git clone https://github.com/yourusername/profanity-rs.git
cd profanity-rs
cargo build --release
```

The binary will be at `target/release/profanity-rs`.

### 2. Start Lich in Detached Mode

**Windows (PowerShell):**
```powershell
C:\Ruby4Lich5\3.4.5\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

**Linux/Mac:**
```bash
ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

### 3. Launch profanity-rs

```bash
./profanity-rs
```

The client will connect to Lich on `localhost:8000` by default.

## Documentation

**📖 [Read the full documentation in the Wiki](https://github.com/Nisugi/Profanitui/wiki)**

### Quick Links

- [Installation Guide](https://github.com/Nisugi/Profanitui/wiki/Installation)
- [Window Management](https://github.com/Nisugi/Profanitui/wiki/Window-Management)
- [Widget Reference](https://github.com/Nisugi/Profanitui/wiki/Widget-Reference) - All 40+ widgets documented
- [Commands Reference](https://github.com/Nisugi/Profanitui/wiki/Commands-Reference) - Complete list of dot commands
- [Configuration Guide](https://github.com/Nisugi/Profanitui/wiki/Configuration-Guide)
- [Troubleshooting](https://github.com/Nisugi/Profanitui/wiki/Troubleshooting)
- [Development Guide](https://github.com/Nisugi/Profanitui/wiki/Development-Guide)
- [Feature Roadmap](https://github.com/Nisugi/Profanitui/wiki/Feature-Roadmap)

## Creating Your First Window

```
.createwindow loot        # Create a loot window
.createwindow health      # Add a health bar
.createwindow compass     # Add a compass
.savelayout hunting       # Save your layout
```

## Configuration

On first run, a default config is created at `~/.profanity-rs/config.toml`. See the [Configuration Guide](https://github.com/Nisugi/Profanitui/wiki/Configuration-Guide) for details.

## Development

```bash
# Build for development
cargo build

# Run with debug logs
RUST_LOG=debug cargo run
# Logs: ~/.profanity-rs/debug.log

# Build for release
cargo build --release
```

See [Development Guide](https://github.com/Nisugi/Profanitui/wiki/Development-Guide) for architecture details and contribution guidelines.

## Requirements

- Rust 1.70+
- Lich (for connecting to GemStone IV)
- Terminal with mouse support (recommended: Windows Terminal, iTerm2, Alacritty)

## License

MIT License - See LICENSE file for details

## Credits

- Original [ProfanityFE](https://github.com/elanthia-online/profanity) by Shaelynne
- Built with [Ratatui](https://github.com/ratatui-org/ratatui)
- For [GemStone IV](https://www.play.net/gs4/) by Simutronics

## Links

- [GemStone IV](https://www.play.net/gs4/)
- [Lich Scripting Engine](https://github.com/elanthia-online/lich-5)
- [Ratatui Documentation](https://ratatui.rs/)
