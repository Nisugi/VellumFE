# Changelog - v0.2.0-beta.11 (Complete)

All notable changes since v0.2.0-beta.1 (46 commits).

---

## 🎨 Color System & Terminal Compatibility

### Color Mode Support
- **Global color mode** for macOS Terminal.app and limited terminals
  - `--color-mode slot` flag for 256-color palette mode
  - `--color-mode direct` for 24-bit RGB true color (default)
  - Thread-local color mode respected by all parsing functions
  - Palette lookup via hashmap (hex → slot) for configured colors
- **ANSI slot colors (0-15)** for Profanity users with custom palettes
  - Presets now reference ANSI color names (e.g., "ANSI Green")
  - rgb_to_ansi_slot() function matches standard ANSI RGB values
- **Mode-aware color parsing** throughout entire codebase
  - All widgets route through centralized parse_color_to_ratatui()
  - Consistent color handling in slot vs direct modes
- **Color Palette browser** now shows slot numbers (e.g., "[2]" after hex)
- **`.setpalette` command** properly sends OSC 4 sequences in Slot mode

### Layout Migration
- **`migrate-layout` subcommand** converts old VellumFE layouts to current format
  - Handles deprecated fields (bar_fill, progress_id)
  - Provides fallback templates for missing widget types
  - Validates and upgrades layout structure

---

## 🪟 Window Management

### Window Commands
- **`.lockwindows` / `.lockall`** - Lock all windows to prevent accidental moves
- **`.unlockwindows` / `.unlockall`** - Unlock all windows for editing
- **`.reload layout`** - Reload auto-saved layout from disk
- **Window focus cycling** - Tab key cycles focused window for scrolling
  - Smart Tab behavior: '.' prefix triggers completion, otherwise cycles windows
  - Default focused window set to "main" on startup

### Menu System
- **4-level nested menu system** (popup → submenu → nested → deep)
  - Prevents stack overflow in debug builds
  - Menu commands use `menu:` prefix instead of dot commands
  - Proper rendering and keyboard navigation at all levels
- **Menu stack management** - Clear deep_submenu in all close handlers

### Layout System Improvements
- **Removed auto-scaling** on layout load
  - Windows use exact positions from layout.toml
  - No more automatic terminal size adjustments
  - `.resize` command available for manual redistribution
- **Fixed `.reload` breaking widget rendering**
  - Added `needs_widget_reset` flag to UiState
  - Widget caches cleared after reload/load/resize
  - Fixes generation counter mismatch

---

## 🎯 Highlight System

### Universal Highlighting
- **Shared HighlightEngine** (550+ lines in highlight_utils.rs)
  - Extracted from text_window.rs, eliminating 340+ lines of duplication
  - 5-step highlight algorithm with colors, bold, stream filtering, regex, capture groups
- **Highlights in ALL text-based windows**:
  - Text windows (main, thoughts, speech, etc.)
  - Inventory window
  - Spells window
  - Perception window
  - Room window
  - Active Effects widget
  - Targets widget
  - Players widget

### Replacement Support
- **Empty string replacement** for removing unwanted text
  - Example: Remove "roisaen/roisan" from duration text
  - `replace = ""` in highlights.toml
- **Regex text replacements** with auto-detection
  - Metacharacter detection (\\d+, \\w+, etc.)
  - Capture group support
- **`replace_enabled` toggle** - Disable replacements while keeping color highlights
- **Per-window highlight configuration** via window editor

### Highlight Commands
- **`.savehighlights` / `.loadhighlights`** - Manage highlight profiles
- **`.highlightprofiles`** - List saved highlight profiles
- **`.toggleignores`** - Toggle squelch patterns on/off

---

## 🎮 Widget Improvements

### Targets Widget
- **Uses room objs component** data for creature tracking
  - Displays creatures in the room with their status
  - Tracks current target from dDBTarget dropdown
  - Clickable creature links for targeting
  - Status abbreviation support with configurable position (start/end)
  - Truncation mode options for long names

### Players Widget
- **Displays players in the room** from room players component
  - Dual status support (prepended + appended statuses)
  - Clickable player names for interaction
  - Status abbreviation with configurable display

### Perception Window
- **Dedicated perception widget** for active effects display
- **Short spell names** via embedded ~600 spell abbreviation table
  - Converts "Spirit Warding I (101)" to "SpiritWard1"
  - Enable with `short_names = true` in config
- **Text replacements** with auto-detecting regex support
- **Configurable** via window editor with add/edit/remove patterns

### Container Window
- **ContainerWindow widget** for direct-connect users
  - Displays contents of bags, backpacks, containers
  - Container cache system in GameState
  - Parser support for container XML elements
  - Layout templates with configurations

### Experience Window (DragonRealms)
- **Experience widget** for DragonRealms skill tracking
  - Reads from `<component id='exp XXX'>` elements
  - ExpComponentState in GameState with field_order preservation
  - Left/center/right text alignment options
  - Change detection for efficient updates
  - Window editor support

---

## 🐉 DragonRealms Support

### Direct Connection
- **DragonRealms game codes**: dr, drplatinum, drfallen, drtest
  - Maps to proper codes: DR, DRX, DRF, DRT
  - Full eAccess authentication support
- **`--game` CLI argument** with dr/test options

### DR-Specific Features
- **Concentration progress bar** template (cyan/teal)
  - Replaces mana bar (DR uses concentration as 4th vital)
- **Experience tracking** via exp components
  - Component data layer with change detection
  - Frontend widget displaying all skills in order
- **Silent component updates** - DR exp components don't trigger prompts

---

## ⚙️ Configuration & Commands

### Reload System
- **`.reload` command** with granular options:
  - `.reload` / `.reload all` - Reload everything
  - `.reload highlights` / `.reload hl` - Reload highlights only
  - `.reload keybinds` / `.reload kb` - Reload keybinds only
  - `.reload settings` - Reload UI/connection/sound settings
  - `.reload colors` - Reload color presets
  - `.reload layout` - Reload auto-saved layout
- **MessageProcessor.apply_config()** for unified config refresh
- **Parser.update_event_patterns()** for runtime pattern updates

### Keybind Profiles
- **`.savekeybinds` / `.savekb [name]`** - Save keybinds as profile
- **`.loadkeybinds` / `.loadkb <name>`** - Load keybinds from profile
- **`.keybindprofiles` / `.kbprofiles`** - List saved profiles

### Configuration Improvements
- **Comprehensive config.toml documentation**
  - Restructured in logical order (connection → ui → sound → tts → streams → advanced)
  - Inline docs for every setting
  - Valid options listed for enum fields
  - File grew from 91 to 206 lines but now self-documenting
- **Global template directory** `~/.vellum-fe/global/templates/`
  - Preserves fully documented config.toml
  - Preserves layout_template.toml with all docs
  - Created automatically on first run
  - Reference docs (profile configs have comments stripped)
- **Layout template documentation**
  - Comprehensive layout_template.toml with all 19 widget types
  - Unicode icon reference
  - Theme color inheritance docs

---

## 🔊 Sound System

- **Sound queue architecture** in GameState
  - Pre-allocated capacity (5 sounds)
  - Core highlight engine queues sounds (no duplication)
  - Frontend drains and plays queued sounds
  - Removed AppCore::check_sound_triggers() duplication
  - Sounds trigger exactly when highlights match (no false positives)
- **`--nosound` flag** - Skip audio init on headless systems
  - Fixes 10-second startup delay without audio hardware
- **Audio config simplification**
  - Removed confusing `disabled` field (use `enabled = false`)
  - Moved startup_music from [ui] to [sound] section
  - Backwards-compatible migration

---

## 🌊 Stream Routing

### Configurable Routing
- **`[streams]` config section** for orphaned stream behavior:
  - `drop_unsubscribed` - List of streams to silently discard
  - `fallback` - Window name for orphaned streams (defaults to "main")
- **Default drop_unsubscribed list**:
  - Communication: speech, whisper, talk, conversation
  - Scripts: targetcount, playercount, targetlist, playerlist
  - Prevents "[0]" noise from Lich scripts
- **Stream subscriber map** built at startup with O(1) lookup
- **Unknown streams fall back to main** window correctly

---

## 🎯 Spells Window

### Complete Implementation
- **Parser support** for inline `<stream id="Spells">` tags
- **Double-buffer system** with line accumulation
  - Fixes blank lines between spell entries
  - Each `<stream>` tag accumulates segments into line buffer
  - Flush on `</stream>` creates complete line
- **Click handling** with cmdlist.xml lookup
  - Individual spell clicks use coord field (e.g., `prepare 608`)
  - Spell circle clicks execute `_spell` commands (e.g., `_spell _spell_ranger`)
  - Automatic fallback to context menu for links without coord
- **Change detection** for efficient updates

---

## 🔧 Bug Fixes

### Critical Fixes
- **Blank line preservation** in game output
  - Empty strings from server no longer skipped
  - Fixes bounty, look, and other formatted outputs
- **Stream reset on prompt skip**
  - current_stream resets to "main" when prompt skipped
  - Prevents text disappearing after silent_prompt match
- **Consecutive prompt display**
  - Skip logic now checks main text presence
  - Prevents duplicate `R>` prompts from script XML updates
- **Division by zero guard** in vitals update

### Scroll & Text Window Fixes
- **Tab completion** and PageUp/PageDown scrolling
  - Routed through frontend's scroll_window()
  - Fixes visual sync between data and frontend layers
- **Scroll behavior improvements**
  - Track wrapped line count per logical line
  - Adjust scroll position when old lines removed
  - Fix memory leak from unbounded wrapped_lines growth
  - Reset scroll state on clear() and validate after rewrap_all()
- **Scroll indicator** `[N]` on tabbed window separator line
- **Title area cleared** before redraw (prevents artifacts)
- **Skip title rendering** when show_border is false

### Parser & Component Fixes
- **All DR components marked as silent updates**
  - Frequent exp/skill components don't trigger prompts
  - chunk_has_silent_updates set before early return
- **Speech/talk/whisper stream duplication fix**
  - Fixed when no target window exists
- **Unread tab markers** implementation completed
- **NextUnreadTab keybind** action implemented

---

## ⚡ Performance Optimizations

### Text Processing
- **Pre-compiled regex patterns** in TextReplacement
  - Compiled at creation time, not on every application
  - Significant CPU reduction during high-throughput text
- **Aho-Corasick automaton** for spell name matching
  - O(n) matching instead of O(n * patterns)
  - Faster with many highlights/replacements active
- **Faster attribute extraction**
  - Replaced regex with string parsing
- **LazyLock for compass regex** - Compile once
- **Minor text window rendering optimizations**

---

## 🎨 UI/UX Improvements

### Search
- **Search bar inherits command_input visual settings**
  - Matches borders, border style, sides, background color
  - No longer hardcoded borderless paragraph
- **Search placeholder text**
  - Dimmed placeholder when empty (matches highlight form)
  - Disappears when typing starts

### Window Editor
- **Text replacements editor** for perception windows
  - Regex pattern support (auto-detected)
  - Empty string replacement handling
- **Highlight form refresh** after saves

### General UI
- **selection_auto_copy** config option for automatic clipboard
- **Performance stats** can be enabled in config
- **Removed non-existent config references** (sound.disabled, poll_timeout_ms)

---

## 🗑️ Removed Features

- **Map widget** removed (abandoned feature)
  - Cleaned up from WidgetType enum
  - Removed from parsing and validation
  - Removed test references

---

## 🏗️ Code Quality

### Refactoring
- **Comprehensive codebase cleanup**
  - Fixed all 60 panic! statements in parser.rs tests
  - Converted if-let-else-panic to let-else pattern
  - Replaced UNIX_EPOCH unwrap with unwrap_or_else
  - Replaced unsafe unwraps with expect() + clear messages
- **sync_simple_widget! macro** for reducing boilerplate
  - Standard pattern for widget synchronization
  - Documented usage examples
- **Removed unused code**
  - list_highlights()
  - Duplicate decode_icon()
  - Dead imports throughout codebase

### Testing
- **All tests passing**: 1,026/1,027 ok
  - 928 unit tests
  - 34 parser tests
  - 57 integration tests
- **Speech stream duplicate test** added
- **Highlight engine tests** for silent_prompt functionality

### Documentation
- **TESTING_HIGHLIGHTS.md** guide added
  - 8 detailed test scenarios with examples
  - Performance monitoring instructions
  - Troubleshooting guide
  - Complete configuration examples
- **GemStoneIV XML elements** documentation

---

## 🔧 Build & CI

- **Vendored OpenSSL** for CI builds
  - Compiles from source for consistent builds
  - No system OpenSSL dependency
- **Cargo.toml in .gitignore** for local/CI split

---

## 📋 Config Changes

### Breaking Changes
- Map widget removed (update layout.toml if present)
- `sound.disabled` → `sound.enabled = false`
- `tts.speak_whispers` → `tts.speak_speech` (alias provided)

### New Config Sections
- `[streams]` with drop_unsubscribed and fallback
- `[highlights]` with replace_enabled toggle
- `connection.account`, `connection.password`, `connection.game` fields
- `ui.selection_auto_copy` option
- `ui.performance_stats_enabled` option

### Removed Fields
- `ui.show_timestamps` (use per-window setting)
- `sound.disabled` (use sound.enabled)
- `tts.voice` (unused)
- `poll_timeout_ms` (unused)

---

## 🎯 Command Reference

### New Commands (since beta.1)
- `.reload [all|highlights|hl|keybinds|kb|settings|colors|layout]`
- `.savekeybinds` / `.savekb [name]`
- `.loadkeybinds` / `.loadkb <name>`
- `.keybindprofiles` / `.kbprofiles`
- `.savehighlights` / `.loadhighlights`
- `.highlightprofiles`
- `.toggleignores` / `.ignores`
- `.lockwindows` / `.lockall`
- `.unlockwindows` / `.unlockall`
- `.setpalette` (improved in Slot mode)

### CLI Flags
- `--color-mode <slot|direct>`
- `--nosound`
- `--profile <name>` (separate config from --character)
- `--game <prime|plat|test|dr|drplatinum|drfallen|drtest>`

### Subcommands
- `migrate-layout --src <DIR> [--out <DIR>] [--dry-run] [-v]`
- `validate-layout [LAYOUT_FILE]`

---

## 📊 Statistics

- **46 commits** since v0.2.0-beta.1
- **1,026/1,027 tests passing**
- **Major focus areas**:
  - Color system & terminal compatibility
  - Universal highlighting
  - Window management
  - DragonRealms support
  - Stream routing
  - Performance optimization
  - Spells window implementation

---

## 🙏 Migration Guide

### From beta.1 to beta.11

1. **Update layout.toml**:
   - Remove any `map` widgets (deprecated)
   - Consider using `migrate-layout` command for automatic conversion

2. **Update config.toml**:
   - Change `sound.disabled = true` to `sound.enabled = false`
   - Change `tts.speak_whispers` to `tts.speak_speech`
   - Review new `[streams]` section for custom routing

3. **Color mode**:
   - macOS Terminal.app users: try `--color-mode slot`
   - If colors look wrong, check `~/.vellum-fe/global/templates/config.toml`

4. **Check templates**:
   - New documented templates in `~/.vellum-fe/global/templates/`
   - Use as reference for all configuration options

5. **DragonRealms users**:
   - Update vitals layout to use "concentration" instead of "mana"
   - Add experience window for skill tracking

6. **Try new features**:
   - `.lockwindows` to prevent accidental moves
   - `.reload highlights` to test highlight changes without restart
   - Highlight replacement to clean up text (e.g., remove "roisaen")

---

## 🐛 Known Issues

- None currently reported

---

**Full commit range**: v0.2.0-beta.1...v0.2.0-beta.11
