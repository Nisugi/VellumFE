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
- **Clickable Links** - Wrayth-style context menus on game objects (right-click or click items)
- **Stream Routing** - Game streams automatically route to appropriate windows
- **Layout Management** - Save and load custom window layouts with `.resize` auto-scaling
- **Performance Monitoring** - Real-time FPS, render times, network, and memory stats
- **XML Parsing** - Full support for GemStone IV's XML protocol
- **Live Configuration** - Most settings can be changed without restarting
- **Sound Support** - Highlight-based sound triggers with configurable volume

## Quick Start

### 1. Download the Latest Release

Download the latest `vellum-fe.exe` from the [Releases](https://github.com/Nisugi/VellumFE/releases) page.

### 2. Start Lich in Detached Mode

**Windows (PowerShell):**
```powershell
# Note: Replace 3.4.x with your actual Ruby version (e.g., 3.4.2, 3.4.5, etc.)
C:\Ruby4Lich5\3.4.x\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

**Linux/Mac:**
```bash
ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

Wait 5-10 seconds for Lich to fully connect before launching VellumFE.

### 3. Launch VellumFE

**Windows:**
```powershell
.\vellum-fe.exe --port 8000 --character YourCharName --links true
```

**Linux/Mac:**
```bash
./vellum-fe --port 8000 --character YourCharName --links true
```

**Command-Line Options:**
- `--port` / `-p` - Port to connect to (default: 8000)
- `--character` / `-c` - Character name (loads character-specific config)
- `--links` - Enable clickable links (default: true)

The client will connect to Lich on `localhost:8000` by default.

## Essential Commands

**Getting Started:**
```
.menu                         # Open the main menu - start here!
.help                         # Show all available commands
```

The `.menu` command is your main entry point - it provides easy access to all configuration options, window management, and settings.

**Quick Commands:**
```
.settings                     # Edit all configuration options
.highlights                   # Manage text highlights
.uicolors                     # Customize UI colors and themes
.keybinds                     # Configure keyboard shortcuts
.windows                      # List all active windows
.savelayout [name]            # Save your current layout
.resize                       # Auto-scale layout to current terminal size
```

**Example Workflow:**
1. Launch VellumFE and connect to Lich
2. Type `.menu` to open the main menu
3. Configure windows, highlights, and settings through the menu
4. Type `.savelayout` to save your configuration
5. Type `.resize` whenever you change terminal size

## Documentation

**📖 [Full Documentation Wiki](wiki/)**

The `/wiki/` directory contains comprehensive documentation:

- [Getting Started](wiki/Getting-Started.md) - Installation and first-time setup
- [Configuration](wiki/Configuration.md) - Complete configuration reference
- [Commands](wiki/Commands.md) - All dot commands documented
- [Windows and Layouts](wiki/Windows-and-Layouts.md) - Window management guide
- [Window Types](wiki/Window-Types.md) - All 40+ widget types
- [Highlights](wiki/Highlights.md) - Creating and managing highlights
- [Keybinds](wiki/Keybinds.md) - Keybind system and built-in actions
- [Mouse Controls](wiki/Mouse-Controls.md) - Mouse operations and clickable links
- [Themes and Colors](wiki/Themes-and-Colors.md) - Color customization with 7 pre-made themes
- [Troubleshooting](wiki/Troubleshooting.md) - Common issues and solutions
- [FAQ](wiki/FAQ.md) - 50+ frequently asked questions

**Advanced Topics:**
- [Advanced: Streams](wiki/Advanced-Streams.md) - Stream routing deep dive
- [Advanced: Characters](wiki/Advanced-Characters.md) - Multi-character configuration
- [Advanced: XML](wiki/Advanced-XML.md) - XML protocol reference

## Configuration

VellumFE uses a directory-based config structure for multi-character support:

**Config Locations (Windows):**
- `C:\Users\<you>\.vellum-fe\configs\default.toml` - Default configuration
- `C:\Users\<you>\.vellum-fe\configs\<character>.toml` - Character-specific configs
- `C:\Users\<you>\.vellum-fe\layouts\default.toml` - Default window layout
- `C:\Users\<you>\.vellum-fe\layouts\<character>.toml` - Character-specific layouts
- `C:\Users\<you>\.vellum-fe\layouts\auto_<character>.toml` - Autosaved layouts (highest priority)
- `C:\Users\<you>\.vellum-fe\sounds\` - Sound files for highlight triggers
- `C:\Users\<you>\.vellum-fe\debug.log` - Debug log (or `debug_<character>.log` if using `-c`)

**Config Locations (Linux/Mac):**
- `~/.vellum-fe/configs/` - Configuration files
- `~/.vellum-fe/layouts/` - Window layouts
- `~/.vellum-fe/sounds/` - Sound files
- `~/.vellum-fe/debug.log` - Debug log

On first run, default configs are automatically created.

## Mouse Controls

- **Click Title Bar** - Drag to move window
- **Click Edges/Corners** - Drag to resize window
- **Click Text** - Focus window
- **Scroll Wheel** - Scroll through text history
- **Click and Drag Text** - Select and copy to clipboard
- **Shift + Drag** - Native terminal selection (bypasses VellumFE)
- **Click Links** - Open context menu (when `--links` enabled)

## Building from Source

Only needed if you want to contribute or modify the code:

```bash
git clone https://github.com/Nisugi/VellumFE.git
cd VellumFE
cargo build --release
```

The binary will be at `target/release/vellum-fe` (or `vellum-fe.exe` on Windows).

**Development:**
```bash
cargo build                   # Build for development
cargo run                     # Run with default settings
cargo run -- --character Test # Run with character-specific config
RUST_LOG=debug cargo run      # Run with debug logging (Linux/Mac)
```

See [CLAUDE.md](CLAUDE.md) for complete architecture documentation.

## Requirements

- **Lich** - Required for connecting to GemStone IV ([Lich 5](https://github.com/elanthia-online/lich-5))
- **Terminal with Mouse Support** - Recommended: Windows Terminal, iTerm2, Alacritty, Kitty
- **Windows/Linux/Mac** - Cross-platform support

## Troubleshooting

**Connection Issues:**
- Make sure Lich is running in detached mode (`--detachable-client=8000`)
- Wait 5-10 seconds after starting Lich before launching VellumFE
- Check the port matches (default: 8000)
- See [Troubleshooting Guide](wiki/Troubleshooting.md) for more

**Configuration Issues:**
- Delete `~/.vellum-fe/` directory to reset to defaults
- Check debug logs in `~/.vellum-fe/debug.log`
- Use `.settings` command to view/edit all config options

## Support

- **Issues/Bugs**: [GitHub Issues](https://github.com/Nisugi/VellumFE/issues)
- **Wiki**: [Documentation Wiki](wiki/)
- **Discord**: [GemStone IV Discord](https://discord.gg/gemstone) - #lich channel

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
