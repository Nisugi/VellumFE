# VellumFE

**A modern, high-performance multi-frontend client for GemStone IV**

---

## What is VellumFE?

VellumFE is a feature-rich terminal client designed specifically for [GemStone IV](https://www.play.net/gs4/), the legendary text-based MMORPG by Simutronics. Built from the ground up in Rust, VellumFE delivers:

- **60+ FPS rendering** with sub-millisecond event processing
- **Fully customizable layouts** with pixel-perfect window positioning
- **Rich theming support** with 24-bit true color
- **Multiple connection modes** (Lich proxy or direct eAccess authentication)
- **Modern TUI** built on ratatui with planned GUI support via egui

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              VellumFE                                    │
│                                                                          │
│  ┌─────────────────────────┐  ┌────────────────────────────────────┐   │
│  │      Main Window        │  │         Room Description           │   │
│  │                         │  │  [Obvious exits: north, east, out] │   │
│  │  A goblin attacks!      │  └────────────────────────────────────┘   │
│  │  > attack goblin        │  ┌──────────┐  ┌──────────┐              │
│  │  You swing at a goblin! │  │ ◄ N ►    │  │ HP: 100% │              │
│  │                         │  │   S      │  │ MP:  87% │              │
│  └─────────────────────────┘  └──────────┘  └──────────┘              │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ >                                                                │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

## Philosophy

VellumFE is built on these core principles:

### 1. Performance First
Every design decision prioritizes smooth, responsive gameplay. Generation-based change detection, lazy text wrapping, and efficient memory management ensure VellumFE never gets in your way.

### 2. Customization Without Limits
Your client should look and behave exactly how you want. Every window can be positioned, sized, styled, and configured independently. Create layouts for hunting, merchanting, roleplaying, or anything else.

### 3. Modern Architecture
A clean separation between Core (game logic), Data (state), and Frontend (rendering) allows for multiple frontends (TUI today, GUI tomorrow) while keeping the codebase maintainable.

### 4. Developer Friendly
Written in idiomatic Rust with comprehensive documentation. Want to add a new widget type? Extend the parser? Create a custom browser? The architecture supports it.

## Feature Highlights

### Layouts
Design your perfect interface with TOML-based layout files. Position windows by row/column or pixel coordinates. Nest windows, create tabs, configure borders and colors per-window.

### Highlights
Apply colors and styles to game text with regex patterns. Highlight creature names, player speech, spell effects, or anything else. Fast literal matching via Aho-Corasick for high-frequency patterns.

### Keybinds
Bind any key combination to game commands, client actions, or macros. Full modifier support (Ctrl, Alt, Shift). Action-based system supports scrolling, navigation, text editing, and custom commands.

### Themes
Complete color control with presets, palettes, and per-widget overrides. Ship with dark and light themes, or create your own.

### Sound
Audio alerts for game events with configurable triggers. Text-to-speech support for accessibility.

### Connection Modes
Connect via Lich for scripting integration, or directly authenticate with eAccess for standalone operation.

## Quick Start

```bash
# Via Lich (default)
vellum-fe --port 8000

# Direct connection
vellum-fe --direct --account YOUR_ACCOUNT --character CharName
```

See [Installation](./getting-started/installation.md) for detailed setup instructions.

## Documentation Structure

This documentation is organized for multiple audiences:

| Section | Audience | Purpose |
|---------|----------|---------|
| [Getting Started](./getting-started/README.md) | New users | Installation, first launch, quick tour |
| [Configuration](./configuration/README.md) | All users | Config file reference |
| [Widgets](./widgets/README.md) | All users | Widget types and properties |
| [Customization](./customization/README.md) | Power users | Layouts, themes, highlights |
| [Tutorials](./tutorials/README.md) | All users | Step-by-step guides |
| [Cookbook](./cookbook/README.md) | Power users | Quick recipes for specific tasks |
| [Architecture](./architecture/README.md) | Developers | System design and internals |
| [Development](./development/README.md) | Contributors | Building, testing, contributing |
| [Reference](./reference/README.md) | All users | Complete reference tables |

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/nisugi/vellum-fe/issues)
- **Discussions**: [GitHub Discussions](https://github.com/nisugi/vellum-fe/discussions)
- **In-Game**: Find us on the amunet channel

## License

VellumFE is open source software. See the repository for license details.

---

*Ready to dive in? Start with [Installation](./getting-started/installation.md).*
