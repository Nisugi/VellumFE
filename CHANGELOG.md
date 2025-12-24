# VellumFE v0.2.0 Beta 11 Changelog

**Release Date**: 2024-12-23
**Upgrade Path**: v0.1.9-beta.5 → v0.2.0-beta.11

## Overview

This release represents a complete architecture rewrite (v0.2.0) with 56 commits of improvements from the last stable v0.1.x release. Major highlights include direct eAccess connection support, a new theme system, migration tools, and numerous widget improvements.

---

## 🚀 Major Features

### Direct eAccess Connection
**Connect directly to GemStone IV without Lich proxy!**

```bash
vellum-fe --direct \
  --account YOUR_ACCOUNT \
  --password YOUR_PASSWORD \
  --game prime \
  --character CHARACTER_NAME
```

- Full authentication flow with TLS
- eAccess login manager with persistent settings
- Game selection: GemStone IV (Prime/Plat/Test) and DragonRealms
- Character selection from account
- Secure password handling (not saved to disk)

### Theme System
**Unified color management across the application**

- Centralized theme configuration in `colors.toml`
- Preset color definitions for common elements
- Inheritance system: `@preset` references, `@base_window`, `@health_fg`
- Multiple theme support with hot-reload capability
- Clearer separation between UI colors and game text colors

### Migration Tools
**Import layouts from other clients**

```bash
# Migrate old VellumFE layouts
vellum-fe migrate-layout --src ~/.vellum-fe-old/ --out ~/.vellum-fe/

# Validate layout before use
vellum-fe validate-layout layout.toml
```

- Automatic layout format migration
- Configuration validation
- Dry-run mode for testing migrations
- Comprehensive error messages

---

## 🎮 New Widgets

### Container Window
- Displays contents of bags, backpacks, containers
- Container cache system in GameState
- Parser support for container XML elements
- Window editor support with templates

### Experience Window (DragonRealms)
- DragonRealms skill tracking from `<component id='exp XXX'>` elements
- Left/center/right text alignment options
- Change detection for efficient updates
- Window editor support

### Targets Widget
- Displays creatures in room from room objs component data
- Tracks current target from dDBTarget dropdown
- Clickable creature links for targeting
- Status abbreviation support (configurable position)
- Truncation modes for long names

### Players Widget
- Displays players in room from room players component
- Dual status support (prepended + appended)
- Clickable player names for interaction
- Status abbreviation with configurable display

---

## 🔧 Major Improvements

### Spells Window Enhancements
- **Spell clickability** with cmdlist.xml lookup
  - Left-click: Prepare spell (`PREPARE 101`)
  - Right-click: Cast immediately (`CAST 101`)
  - Shift+click: Evoke (`EVOKE 101`)
  - Ctrl+click: Release (`RELEASE`)
- **Double-buffer system** with change detection
- Proper stream handling with buffering
- `.addwindow` support (no restart required)

### Perception Window
- Short spell names via ~600 spell abbreviation table
  - "Spirit Warding I (101)" → "SpiritWard1"
  - Enable with `short_names = true`
- Text replacement with auto-detecting regex support
- Configurable via window editor (add/edit/remove patterns)

### Sound System Improvements
- **Sound queue architecture** in GameState
  - Core highlight engine queues sounds (no duplication)
  - Frontend drains and plays queued sounds
  - Sounds trigger exactly when highlights match
- **`--nosound` flag** for headless systems
  - Fixes 10-second startup delay without audio hardware
- Simplified audio config (removed confusing `disabled` field)
- Moved `startup_music` from `[ui]` to `[sound]` section

### Stream Routing
- **Configurable routing** via `[streams]` section:
  - `drop_unsubscribed` - Streams to silently discard
  - `fallback` - Window name for orphaned streams (default: "main")
- Spells stream properly routes to spells windows
- Inventory stream properly routes to inventory windows
- Fixed `.addwindow` not updating stream subscriptions

### Window Management
- **Right-click context menu** on window borders
  - Quick access to edit, close, bring to front
  - Alternative to menu navigation
- Improved widget cache synchronization
- Better window creation for all widget types

### Parser Improvements
- DragonRealms `<spell>` tag support
- Vitals caching with change detection
- Spell abbreviation table (600+ spells)
- Inventory/spell stream buffering
- Container XML parsing

### Highlight System
- **New commands**:
  - `.savehighlights` / `.loadhighlights` - Manage highlight profiles
  - `.highlightprofiles` - List saved profiles
  - `.toggleignores` - Toggle squelch patterns
- Sound triggering moved to core (no frontend duplication)

---

## 📖 Documentation

### New Documentation
- **Migration guides** for Profanity, Wizard FE, StormFront users
- **Comprehensive layout templates** (`layout_template.toml`)
  - All 19 widget types documented
  - Unicode icon reference
  - Theme color inheritance examples
- **Cookbook recipes** for common setups
- **Troubleshooting guides** for platform-specific issues

### Updated Documentation
- Complete widget reference with examples
- CLI option documentation
- Stream routing configuration
- Direct eAccess setup guide
- Theme system guide

---

## 🏗️ Architecture Changes

### Core-Data-Frontend Separation
**Complete rewrite with three-layer architecture:**

- **Core layer**: Business logic, no frontend dependencies
- **Data layer**: Pure data structures, shared types
- **Frontend layer**: TUI (ratatui) and GUI (egui) implementations

This enables:
- Cleaner separation of concerns
- Easier testing
- Future GUI frontend
- Better code organization

### Widget Data vs Rendering
- Widget state stored in `data/widget.rs`
- Rendering logic in `frontend/tui/*.rs`
- Core layer never imports from frontend

---

## 🐛 Notable Bug Fixes

- Fixed `.addwindow` creating windows that don't receive updates
  - Inventory windows now populate immediately (no restart)
  - Spells windows now populate immediately (no restart)
  - Stream subscriptions properly updated
- Fixed startup delay on headless systems (add `--nosound`)
- Fixed widget synchronization issues
- Fixed stream routing for dynamically added windows
- Fixed highlight sound duplication
- Fixed parser handling of DragonRealms-specific tags

---

## 💻 CLI Enhancements

### New Flags
- `--direct` - Direct eAccess connection mode
- `--account` / `--password` - Direct mode credentials
- `--game` - Game selection (prime/plat/test/dr)
- `--nosound` - Skip audio initialization

### New Subcommands
- `validate-layout [FILE]` - Validate layout configuration
- `migrate-layout` - Migrate old VellumFE layouts
  - `--src <DIR>` - Source directory
  - `--out <DIR>` - Output directory (optional)
  - `--dry-run` - Test migration without changes
  - `-v` - Verbose output

---

## 📊 Statistics

- **56 commits** since v0.1.9-beta.5
- **1,003+ tests** passing
- **19 widget types** supported
- **~600 spell abbreviations** in database
- **10+ migration guide** pages

---

## ⚙️ Configuration Changes

### New Config Sections

**`[streams]` section** in config.toml:
```toml
[streams]
drop_unsubscribed = []  # Streams to silently discard
fallback = "main"       # Default window for orphaned streams
```

**`[sound]` section updates**:
```toml
[sound]
enabled = true          # Replaces old 'disabled' field
startup_music = ""      # Moved from [ui] section
```

### Backwards Compatibility
- Old `[ui] startup_music` automatically migrated to `[sound]`
- Old `disabled` field converted to `enabled = !disabled`
- Existing layouts work without changes

---

## 🔄 Upgrade Notes

### From v0.1.9-beta.5 to v0.2.0-beta.11

1. **Backup your configuration**:
   ```bash
   cp -r ~/.vellum-fe/ ~/.vellum-fe.backup/
   ```

2. **Update binary**:
   - Download latest release
   - Replace old binary

3. **Validate configuration** (optional):
   ```bash
   vellum-fe validate-layout ~/.vellum-fe/layout.toml
   ```

4. **Review new features**:
   - Try direct eAccess connection (`--direct`)
   - Explore new widgets (Container, Targets, Players)
   - Check out new commands (`.savehighlights`, etc.)

### Breaking Changes
**None** - This release is backwards compatible with v0.1.9-beta.5 configurations.

---

## 🙏 Acknowledgments

Thanks to all testers and contributors who helped make this release possible!

- GemStone IV community for feedback and bug reports
- Lich maintainers for the excellent proxy interface
- Ratatui contributors for the TUI framework

---

## 📝 See Also

- [Full detailed changelog](CHANGELOG_DETAILED.md) - All 56 commits
- [Migration guides](book/src/migration/README.md)
- [Documentation](https://nisugi.github.io/vellum-fe/)
- [GitHub Issues](https://github.com/nisugi/vellum-fe/issues)
