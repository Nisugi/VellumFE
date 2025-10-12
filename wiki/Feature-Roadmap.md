# Feature Roadmap

This roadmap outlines planned features for vellum-fe, organized by priority. Features are subject to change based on community feedback and development priorities.

## Table of Contents

- [Priority Levels](#priority-levels)
- [P0: Critical Features](#p0-critical-features)
- [P1: High Priority](#p1-high-priority)
- [P2: Medium Priority](#p2-medium-priority)
- [P3: Low Priority](#p3-low-priority)
- [P4: Experimental](#p4-experimental)
- [Contributing](#contributing)

---

## Priority Levels

| Priority | Timeline | Description |
|----------|----------|-------------|
| **P0** | Next release | Critical features, major bugs |
| **P1** | 1-3 months | High-value features |
| **P2** | 3-6 months | Quality-of-life improvements |
| **P3** | 6+ months | Nice-to-have features |
| **P4** | Future | Experimental, needs research |

---

## P0: Critical Features

These features are the highest priority and will be implemented first.

### Clickable Links and Context Menus

**Status:** ✅ Completed

Wrayth-style clickable links with hierarchical context menus for game objects.

**Features:**
- Click any word in a game object link to open context menu
- 588 commands loaded from `cmdlist1.xml` (look, examine, get, drop, etc.)
- Hierarchical 3-level menu system:
  - Main menu → Category submenu (e.g., roleplay) → Nested submenu (e.g., swear)
  - Categories with underscore become submenus
  - Category 0 always appears at end
- Multi-word link prioritization ("raven feather" over "raven")
- Recent links cache (last 100) for efficient lookups
- Menu request/response protocol with server
- Secondary noun support for held items (`%` placeholder)
- Menu text formatting (removes `#` and `@`, substitutes `%`)
- Mouse and keyboard navigation (arrows, Enter, Esc)
- All menu levels visible simultaneously
- Menus auto-position at click location with bounds checking
- Stream discard for missing windows

**Implementation:**
- Link detection from `<a exist="..." noun="...">` XML tags
- Text window caches recent links with accumulated text
- Parser handles `<menuResponse>` with `<mi coord="..." noun="..."/>` tags
- PopupMenu widget renders overlays with solid backgrounds
- App manages 3 menu levels (main, submenu, nested_submenu)

**Usage:**
- Left-click any word in a link to open menu
- Arrow keys to navigate, Enter to select
- Esc or Left to go back one level
- Right or Enter on submenu items to drill down

**Related:** See [Mouse and Keyboard Guide](https://github.com/Nisugi/VellumFE/wiki/Mouse-and-Keyboard)

---

### Active Effects System

**Status:** Partially implemented

Track and display active spells, buffs, and debuffs.

**Features:**
- Parse `<spell>` XML tags for active spells
- Display spell names with remaining duration
- Color-code by type:
  - Green: Buffs (defensive spells, enhancements)
  - Red: Debuffs (negative effects)
  - Blue: Utility spells
- Show cooldown timers for abilities
- Support multiple display modes (list, grid, compact)

**Widgets:**
- `activeeffects` - List of active spells/effects
- `missingspells` - Track missing defensive spells (planned)

**Usage:**
```
.createwindow activeeffects
```

**Related:** Issue #XX

---

### Target and Player Widgets

**Status:** Planned

Display targets and players in the current room.

#### Targets Widget

**Compact mode:**
- Display: `Targets [XX]` with count
- Click to expand to full list

**Expanded mode:**
- Show header with count
- List all targets
- Color-code by threat level (if available)
- Show target health bars (if available)
- Support filtering/sorting

**Usage:**
```
.createwindow targets
```

#### Players Widget

**Compact mode:**
- Display: `Players [XX]` with count
- Parse player arrival/departure
- Update count in real-time

**Expanded mode:**
- List all players in room
- Show player names
- Optionally show profession/level (if available)
- Color-code by group/friend status
- Support player notes/tags

**Usage:**
```
.createwindow players
```

**Related:** Issue #XX

---

### Parser Improvements

**Status:** In progress

Improve XML parsing reliability and accuracy.

#### Blank Line Handling

**Issue:** Some blank lines in game output are not preserved correctly.

**Fix:**
- Preserve intentional blank lines (e.g., from `mana` command)
- Distinguish between XML artifact blanks and content blanks
- Add test cases for various scenarios

#### Prompt Handling

**Issue:** Prompts may appear twice when thoughts stream is active.

**Fix:**
- Track active stream destinations
- Skip duplicate prompts when thoughts stream is active
- Only show prompt in main if thoughts window doesn't exist
- Test with various stream combinations

#### Missing XML Tags

**Add support for:**
- `<spell>` tags for active effects (partially done)
- Room object data (more thorough parsing)
- `<nav>` tags (better handling)
- `<resource>` tags (if applicable)
- Inventory updates

#### DialogData Extraction

**Improve:**
- Extract nested tags more reliably
- Handle malformed/incomplete dialogData
- Better error recovery

**Related:** Issue #XX, Issue #XX

---

### Highlighting System

**Status:** Planned

Regex-based text highlighting for combat, loot, and custom patterns.

**Features:**
- Load highlight patterns from config
- Support foreground/background colors
- Support underline/bold/italic
- Priority system for overlapping highlights
- Enable/disable per highlight

**Configuration format:**
```toml
[[highlights]]
pattern = "^You.*"
fg = "#ffff00"
bold = true
priority = 10

[[highlights]]
pattern = "\\d+ silver"
fg = "#c0c0c0"
priority = 5
```

**Preset categories:**
- Combat messages (attacks, damage)
- Player names
- Important items/loot
- NPC names
- Room directions
- Spell casts

**Related:** Issue #XX

---

### Complete Status Indicators

**Status:** Partially implemented

Add missing status effect indicators.

**Implemented:**
- Poisoned
- Diseased
- Bleeding
- Stunned
- Webbed

**Planned:**
- Kneeling
- Sitting
- Prone/Dead
- Invisible
- Hidden
- Silenced
- Other common debuffs

**Usage:**
```
.createwindow poisoned
.createwindow stunned
.createwindow status  # Dashboard with all indicators
```

**Related:** Issue #XX

---

## P1: High Priority

High-value features that improve usability and functionality.

### Experience Window

**Status:** Planned

Track skill experience and learning progress.

**Features:**
- Parse skill experience data
- Display format: `SkillName: ranks percent% [mindstate/34]`
- Color-code by skill category:
  - Armor skills
  - Weapon skills
  - Magic skills
  - Survival skills
  - Lore skills
- Color-code mindstate:
  - 0: White (not learning)
  - 1-10: Cyan (light learning)
  - 11-20: Green (moderate)
  - 21-30: Yellow (heavy)
  - 31-34: Red (saturated)
- Show pulsing indicator for actively learning skills
- Track skill gains over time
- Support skill filtering/grouping
- Sort by category or mindstate

**Usage:**
```
.createwindow experience
```

**Related:** Issue #XX

---

### Macro Support

**Status:** Planned

Execute command sequences with a single keypress.

**Configuration:**
```toml
[[macros]]
key = "f1"
commands = ["stance defensive", "guard"]

[[macros]]
key = "f2"
commands = ["prep 701", "cast at %target"]
variables = { target = "last_target" }

[[macros]]
key = "ctrl+h"
commands = ["drink my potion", "eat my rations"]
```

**Features:**
- Macro trigger keys/sequences
- Command sequences (multiple commands)
- Variables and substitution
- Conditional macros based on state
- Delays between commands
- Macro recording mode

**UI:**
- List active macros (`.macros`)
- Edit/create macros (`.editmacro`)
- Enable/disable macros (`.togglemacro`)
- Test macro execution

**Related:** Issue #XX

---

### Keybind Support

**Status:** Partially implemented (configuration only)

Map keys to game commands or client actions.

**Configuration:**
```toml
[[keybinds]]
key = "f1"
action = "command"
command = "look"

[[keybinds]]
key = "num_8"
action = "command"
command = "north"

[[keybinds]]
key = "ctrl+l"
action = "loadlayout"
layout = "combat"
```

**Features:**
- Map keys to commands
- Support modifier keys (Ctrl, Alt, Shift)
- Function key support (F1-F12)
- Number pad for movement
- Configurable per character/global
- Conflict detection
- Show keybind help/cheatsheet

**Common defaults:**
- F1-F12: Customizable commands
- Number pad: Directional movement
- Ctrl+1-9: Quick macros

**Related:** Issue #XX

---

### Autocomplete System

**Status:** Planned

Autocomplete commands, names, and directions.

**Features:**
- Complete from command history
- Complete known commands
- Complete room directions
- Complete visible NPC/player names
- Tab completion UI
- Context-aware autocomplete
- Fuzzy matching
- Learn from usage patterns

**Usage:**
```
Type: lo<Tab>
→ look

Type: go n<Tab>
→ go north

Type: whi<Tab>
→ whisper (if players nearby match)
```

**Related:** Issue #XX

---

### Stun Handler Script

**Status:** Planned

Automatically update stun timer from game events.

**Features:**
- Parse stun messages from combat
- Update stun timer widget automatically
- Handle stun recovery messages
- Configurable stun detection patterns

**Related:** Issue #XX

---

## P2: Medium Priority

Quality-of-life improvements and polish features.

### Timestamp Support

**Status:** Planned

Add optional timestamps to text windows.

**Features:**
- Configurable timestamp format
- Per-window timestamp enable/disable
- Timestamp color customization
- 12/24 hour format option

**Configuration:**
```toml
[[ui.windows]]
name = "main"
show_timestamps = true
timestamp_format = "[%H:%M:%S]"
timestamp_color = "#888888"
```

**Related:** Issue #XX

---

### Window Management Improvements

**Status:** Planned

Make window management easier and more intuitive.

#### Window Snapping

**Features:**
- Snap to terminal edges when moving
- Snap to other windows
- Configurable snap distance
- Visual guides while dragging

#### Window Groups/Tabs

**Features:**
- Tab similar windows together
- Switch between tabs
- Detach tabs to separate windows
- Tab bar UI

#### Window Presets

**Features:**
- Save/load window arrangements
- Quick-switch between layouts (hotkey)
- Per-character layouts
- Export/import layouts

**Related:** Issue #XX

---

### Persistent Command History

**Status:** Planned

Save command history across sessions.

**Features:**
- Save history to file
- Load history on startup
- Search history (Ctrl+R style)
- Configurable history size limit
- Clear history command (`.clearhistory`)

**Configuration:**
```toml
[ui]
history_file = "~/.vellum-fe/history.txt"
history_size = 1000
```

**Related:** Issue #XX

---

### Terminal Title Updates

**Status:** Planned

Update terminal title with game state.

**Features:**
- Show current room
- Show character name
- Show health/mana percentages
- Show active status effects
- Configurable format

**Configuration:**
```toml
[ui]
terminal_title_format = "vellum-fe - {character} - {room} - HP: {health}%"
```

**Related:** Issue #XX

---

### Enhanced Configuration

**Status:** Planned

Improve configuration management.

**Features:**
- Configuration validation (check for errors on load)
- Configuration migration tool (upgrade old configs)
- Better error messages for config issues
- Per-character configurations
- Profile system (combat, RP, travel, etc.)

**Related:** Issue #XX

---

## P3: Low Priority

Nice-to-have features for future releases.

### Rich Text Rendering

**Status:** Planned

Support more text styling options.

**Features:**
- Bold, italic, dim
- Strikethrough
- Multiple underline styles
- 24-bit color support (if terminal supports)
- Better gradient support

**Related:** Issue #XX

---

### Platform Testing

**Status:** Ongoing

Ensure compatibility across platforms.

**Tasks:**
- Test on Windows (primary platform)
- Test on Linux (various distributions)
- Test on macOS
- Document platform-specific quirks
- CI/CD for all platforms

**Tested terminals:**
- Windows: Windows Terminal, Alacritty
- Linux: GNOME Terminal, Konsole, Kitty, Alacritty
- macOS: iTerm2, Alacritty, Terminal.app

**Related:** Issue #XX

---

### Documentation Improvements

**Status:** Ongoing

Expand and improve documentation.

**Tasks:**
- Complete API documentation
- Widget configuration examples
- Macro/keybind examples
- Highlight configuration guide
- Performance tuning guide
- Video tutorials
- FAQ

**Related:** Issue #XX

---

### Performance Optimization

**Status:** Ongoing

Optimize rendering and memory usage.

**Tasks:**
- Profile rendering performance
- Optimize hot paths
- Reduce allocations
- Benchmark against ProfanityFE
- Memory usage optimization
- Reduce CPU usage during idle

**Related:** Issue #XX

---

## P4: Experimental

Experimental features requiring significant research and development.

⚠️ **Note:** These features should be developed on a separate branch to avoid performance impact on main.

### Clickable Links and Context Menus

**Status:** ✅ Completed (moved to P0)

Make text elements clickable with context menus and drag-and-drop functionality.

**Completed features:**
- Wrayth-style clickable links with hierarchical context menus
- 588 commands from cmdlist1.xml
- Drag and drop items to containers or empty space
- Smart link detection with multi-word priority
- Mouse and keyboard navigation

---

### Advanced UI Features

**Status:** Research phase

Explore advanced UI capabilities.

**Possible features:**
- Split panes (horizontal/vertical)
- Floating windows (pop-up style)
- Modal dialogs
- Forms/input wizards
- Charts and graphs (experience over time)
- Mini-map visualization

**Challenges:**
- Ratatui limitations
- Terminal capabilities
- Complexity vs. usability

**Related:** Issue #XX

---

## Contributing

Want to help build these features?

### How to Contribute

1. **Pick a feature** from this roadmap
2. **Check GitHub issues** for related discussions
3. **Comment on the issue** to claim it
4. **Read the [Development Guide](https://github.com/Nisugi/VellumFE/wiki/Development-Guide)**
5. **Create a feature branch** and start coding
6. **Submit a pull request** when ready

### Feature Requests

Have an idea not on this roadmap?

1. **Search existing issues** to avoid duplicates
2. **Create a feature request** on GitHub
3. **Describe the use case** and expected behavior
4. **Discuss with the community**

### Priority Changes

Priorities may change based on:
- Community feedback
- Bug severity
- Development resources
- Technical dependencies

---

## Release Schedule

**Current version:** 0.1.0 (Initial release)

**Planned releases:**
- **0.2.0** - P0 features (Active effects, targets, parser improvements)
- **0.3.0** - P1 features (Experience, macros, keybinds)
- **0.4.0** - P2 features (QoL improvements)
- **1.0.0** - Stable release with all core features

**Release frequency:** Every 2-3 months (target)

---

## Completed Features

Track completed features from this roadmap:

- ✅ Dynamic window management
- ✅ Mouse support (move, resize, scroll)
- ✅ Text selection mode
- ✅ Stream routing
- ✅ Layout save/load
- ✅ Progress bars (health, mana, etc.)
- ✅ Countdown timers (roundtime, casttime, stun)
- ✅ Compass widget
- ✅ Injury doll widget
- ✅ Status indicators (poisoned, diseased, etc.)
- ✅ Dashboard widget
- ✅ Performance monitoring
- ✅ Hand widgets (left/right hand display)
- ✅ Basic active effects widget
- ✅ **Clickable links and context menus (Wrayth-style)**
- ✅ **Drag and drop for items (put in containers, drop)**

---

[← Previous: Development Guide](https://github.com/Nisugi/VellumFE/wiki/Development-Guide) | [Back to Wiki Home →](https://github.com/Nisugi/VellumFE/wiki/Home)
