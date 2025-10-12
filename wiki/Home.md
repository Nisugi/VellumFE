# Welcome to VellumFE

A modern, high-performance terminal UI client for GemStone IV, written in Rust and built with Ratatui.

## What is VellumFE?

VellumFE is a complete rewrite of the popular [ProfanityFE](https://github.com/elanthia-online/profanity) client, bringing modern performance, enhanced features, and a robust architecture to GemStone IV players.

## Key Features

### Window Management
- **Dynamic Windows** - Create, delete, move, and resize windows at any time
- **40+ Widget Types** - Pre-built widgets for every need
- **Mouse Support** - Full mouse control for drag, resize, scroll, and text selection
- **Clickable Links** - Wrayth-style context menus with 588 commands for game objects
- **Layout System** - Save and load custom window arrangements

### Rich Widget Library
- **Text Windows** - 12 pre-configured stream windows (main, thoughts, speech, etc.)
- **Progress Bars** - Visual vitals tracking (health, mana, stamina, spirit, encumbrance, stance, etc.)
- **Countdown Timers** - Roundtime, casttime, and stun tracking
- **Compass** - Visual exit display
- **Injury Doll** - Graphical wound/scar display
- **Hands** - What you're holding and prepared spells
- **Status Indicators** - Icon-based status effects (poison, disease, bleeding, stunned, webbed)
- **Active Effects** - Spell/buff/debuff tracking with durations
- **Performance Stats** - Real-time FPS, render times, network, and memory monitoring

### Stream Routing
Game output is automatically divided into named streams (main, thoughts, speech, familiar, etc.) and routed to appropriate windows. Create custom windows with your own stream routing.

### Performance
Built in Rust for speed and reliability:
- Fast rendering with double-buffering
- Efficient text wrapping and layout
- Real-time performance monitoring
- Handles thousands of lines without lag

### Configuration
- **TOML-based config** - Human-readable configuration files
- **Live updates** - Most settings apply without restart
- **Per-character layouts** - Different setups for different characters
- **Highlights** - Regex-based text highlighting with Aho-Corasick optimization and sound support
- **Keybinds** - Custom keyboard shortcuts with 24 built-in actions and macro support

## Getting Started

1. **[Installation](https://github.com/Nisugi/VellumFE/wiki/Installation)** - Build from source
2. **[Quick Start](https://github.com/Nisugi/VellumFE/wiki/Quick-Start)** - Launch and connect
3. **[Window Management](https://github.com/Nisugi/VellumFE/wiki/Window-Management)** - Create your first windows
4. **[Widget Reference](https://github.com/Nisugi/VellumFE/wiki/Widget-Reference)** - Explore all available widgets

## Documentation

- [Installation](https://github.com/Nisugi/VellumFE/wiki/Installation)
- [Quick Start](https://github.com/Nisugi/VellumFE/wiki/Quick-Start)
- [Window Management](https://github.com/Nisugi/VellumFE/wiki/Window-Management)
- [Widget Reference](https://github.com/Nisugi/VellumFE/wiki/Widget-Reference)
- [Layout Management](https://github.com/Nisugi/VellumFE/wiki/Layout-Management)
- [Commands Reference](https://github.com/Nisugi/VellumFE/wiki/Commands-Reference)
- [Configuration Guide](https://github.com/Nisugi/VellumFE/wiki/Configuration-Guide)
- [Stream Routing](https://github.com/Nisugi/VellumFE/wiki/Stream-Routing)
- [Mouse and Keyboard](https://github.com/Nisugi/VellumFE/wiki/Mouse-and-Keyboard)
- [Text Selection](https://github.com/Nisugi/VellumFE/wiki/Text-Selection)
- [Highlight Management](https://github.com/Nisugi/VellumFE/wiki/Highlight-Management)
- [Keybind Management](https://github.com/Nisugi/VellumFE/wiki/Keybind-Management)
- [Spell Colors](https://github.com/Nisugi/VellumFE/wiki/Spell-Colors)
- [Targets and Players](https://github.com/Nisugi/VellumFE/wiki/Targets-and-Players)
- [Troubleshooting](https://github.com/Nisugi/VellumFE/wiki/Troubleshooting)
- [Development Guide](https://github.com/Nisugi/VellumFE/wiki/Development-Guide)
- [Feature Roadmap](https://github.com/Nisugi/VellumFE/wiki/Feature-Roadmap)

## Quick Example

```bash
# Start Lich in detached mode (wait 5-10 seconds)
ruby ~/lich5/lich.rbw --login YourChar --gemstone --without-frontend --detachable-client=8000

# Launch vellum-fe
./vellum-fe

# Create some windows
.createwindow loot
.createwindow health
.createwindow mana
.createwindow compass
.createwindow active_spells

# Save your layout
.savelayout hunting
```

## Requirements

- **Rust 1.70+** - For building from source
- **Lich** - Ruby scripting engine for GemStone IV
- **Terminal with mouse support** - Windows Terminal, iTerm2, Alacritty recommended

## Community & Support

- **Issues** - [GitHub Issues](https://github.com/Nisugi/VellumFE/issues)
- **Troubleshooting** - See [Troubleshooting Guide](https://github.com/Nisugi/VellumFE/wiki/Troubleshooting)
- **Contributing** - See [Development Guide](https://github.com/Nisugi/VellumFE/wiki/Development-Guide)

## Credits

- **Original ProfanityFE** - by Shaelynne
- **Ratatui** - Terminal UI framework
- **GemStone IV** - by Simutronics

---

**Next:** [Installation Guide](https://github.com/Nisugi/VellumFE/wiki/Installation) →
