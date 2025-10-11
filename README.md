# VellumFE

A modern, high-performance terminal frontend for GemStone IV, built with [Ratatui](https://github.com/ratatui-org/ratatui). VellumFE is a complete rewrite of [ProfanityFE](https://github.com/elanthia-online/profanity) with enhanced features, blazing performance, and modern architecture.

![Screenshot](https://via.placeholder.com/800x400.png?text=Terminal+UI+Screenshot)

## Features

- **Custom Highlights** - Ultra-fast regex and literal string matching with Aho-Corasick (40x faster!)
- **Dynamic Window Management** - Create, delete, move, and resize windows on the fly
- **Rich Widget Library** - 40+ pre-built widgets (text, progress bars, timers, compass, injury doll, active effects, targets, players)
- **Combat Tracking** - Scrollable target list with status indicators and current target highlighting
- **Player Tracking** - Scrollable player list showing all characters in the room with status
- **Spell Coloring** - Customize active spell/effect colors by spell ID for easy visual distinction
- **Mouse Support** - Click to focus, scroll to navigate, drag to move/resize
- **Text Selection** - Click and drag to select text, auto-copy to clipboard (Shift+drag for native terminal selection)
- **Stream Routing** - Game streams automatically route to appropriate windows
- **Layout Management** - Save and load custom window layouts
- **Performance Monitoring** - Real-time FPS, render times, network, and memory stats
- **XML Parsing** - Full support for GemStone IV's XML protocol
- **Live Configuration** - Most settings can be changed without restarting

## Quick Start

### 1. Build from Source

```bash
git clone https://github.com/Nisugi/VellumFE.git
cd vellum-fe
cargo build --release
```

The binary will be at `target/release/vellum-fe`.

### 2. Start Lich in Detached Mode

**Windows (PowerShell):**
```powershell
C:\Ruby4Lich5\3.4.5\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

**Linux/Mac:**
```bash
ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

### 3. Launch VellumFE

```bash
# Basic launch (uses default config)
./vellum-fe

# With command-line options
./vellum-fe --port 8000 --character YourCharName --links true

# View all options
./vellum-fe --help
```

**Command-Line Options:**
- `--port` / `-p` - Port to connect to (default: 8000)
- `--character` / `-c` - Character name (loads character-specific config)
- `--links` - Enable link highlighting (default: false)

The client will connect to Lich on `localhost:8000` by default.

## Documentation

**📖 [Read the full documentation in the Wiki](https://github.com/Nisugi/VellumFE/wiki)**

### Quick Links

- [Installation Guide](https://github.com/Nisugi/VellumFE/wiki/Installation)
- [Window Management](https://github.com/Nisugi/VellumFE/wiki/Window-Management)
- [Widget Reference](https://github.com/Nisugi/VellumFE/wiki/Widget-Reference) - All 40+ widgets documented
- [Commands Reference](https://github.com/Nisugi/VellumFE/wiki/Commands-Reference) - Complete list of dot commands
- [Configuration Guide](https://github.com/Nisugi/VellumFE/wiki/Configuration-Guide)
- [Troubleshooting](https://github.com/Nisugi/VellumFE/wiki/Troubleshooting)
- [Development Guide](https://github.com/Nisugi/VellumFE/wiki/Development-Guide)
- [Feature Roadmap](https://github.com/Nisugi/VellumFE/wiki/Feature-Roadmap)

## Creating Your First Window

```
.createwindow loot        # Create a loot window
.createwindow health      # Add a health bar
.createwindow compass     # Add a compass
.savelayout hunting       # Save your layout
```

## Configuration

VellumFE uses a directory-based config structure for multi-character support:

**Config Locations:**
- `~/.vellum-fe/configs/default.toml` - Default configuration
- `~/.vellum-fe/configs/<character>.toml` - Character-specific configs
- `~/.vellum-fe/layouts/default.toml` - Default window layout
- `~/.vellum-fe/layouts/<character>.toml` - Character-specific layouts
- `~/.vellum-fe/layouts/auto_<character>.toml` - Autosaved layouts (highest priority)
- `~/.vellum-fe/debug.log` - Debug log (or `debug_<character>.log` if using `-c`)

On first run, default configs are automatically created. See the [Configuration Guide](https://github.com/Nisugi/VellumFE/wiki/Configuration-Guide) for details.

## Development

```bash
# Build for development
cargo build

# Run with debug logs
RUST_LOG=debug cargo run

# Run with character-specific config and debug log
cargo run -- --character Zoleta
# Logs: ~/.vellum-fe/debug_Zoleta.log

# Build for release
cargo build --release
```

See [Development Guide](https://github.com/Nisugi/VellumFE/wiki/Development-Guide) for architecture details and contribution guidelines.

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
