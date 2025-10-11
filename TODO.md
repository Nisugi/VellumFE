# Profanitui TODO List

## High Priority Features

### Active Effects System (effectmon)
- [x] Create ActiveEffects widget for displaying active spells/buffs/debuffs
  - [x] Parse `<spell>` XML tags for active spells
  - [x] Parse buff/debuff indicators from game
  - [x] Display spell names with remaining duration
  - [x] Color-code by type (buff=green, debuff=red, utility=blue)
  - [x] Show cooldown timers for abilities
  - [x] Support multiple display modes (list, compact)

- [ ] Create MissingSpells widget (future enhancement)
  - [ ] Track known spell list per character/profession
  - [ ] Highlight missing defensive spells
  - [ ] Highlight missing offensive spells
  - [ ] Configurable spell priority/grouping
  - [ ] Visual warnings for critical missing buffs

### Target System (targetlist)
- [x] Create Targets widget - compact mode
  - [x] Display format: `Targets [XX]` with count
  - [x] Parse target data from game
  - [x] Update count in real-time
  - [x] Click to expand to full list

- [x] Create Targets widget - expanded mode
  - [x] Show header with count
  - [x] List all targets beneath header
  - [x] Color-code by threat level if available
  - [x] Show target health bars if available
  - [x] Support filtering/sorting

### Players Widget (targetlist)
- [x] Create Players widget - compact mode
  - [x] Display format: `Players [XX]` with count
  - [x] Parse player arrival/departure
  - [x] Update count in real-time

- [x] Create Players widget - expanded mode
  - [x] List all players in room
  - [x] Show player names
  - [x] Optionally show profession/level if available
  - [x] Color-code by group/friend status
  - [x] Support player notes/tags

### Experience Window (exp)
⚠️ **Note**: Need to research ProfanityFE behavior before implementing
- [ ] Create Experience/Skills widget
  - [ ] Parse skill experience data
  - [ ] Display format: `SkillName: ranks percent% [mindstate/34]`
  - [ ] Color-code by skill category (armor, weapon, magic, survival, lore)
  - [ ] Color-code mindstate (0=white, 1-10=cyan, 11-20=green, 21-30=yellow, 31-34=red)
  - [ ] Sort skills by category or mindstate
  - [ ] Show pulsing indicator for actively learning skills
  - [ ] Track skill gains over time
  - [ ] Support skill filtering/grouping

### Percentage/Stats Window (perc)
- [ ] Create general stats display widget
  - [ ] Parse and display various percentage-based stats
  - [ ] Support custom formatting per stat type
  - [ ] Configurable refresh rate

## Parser Improvements

### XML Parser Enhancements
- [x] Improve `<pushStream>` / prompt handling
  - [x] Track active stream destinations
  - [x] Skip duplicate prompts after popStream
  - [x] Works with all streams (thoughts, speech, etc.)

- [ ] Add missing XML tag support
  - [ ] Parse `<spell>` tags for active effects
  - [ ] Parse room object data more thoroughly
  - [ ] Handle `<nav>` tags better
  - [ ] Support `<resource>` tags (if applicable)
  - [ ] Parse inventory updates

- [ ] Improve dialogData extraction
  - [ ] Extract nested tags more reliably
  - [ ] Handle malformed/incomplete dialogData
  - [ ] Better error recovery

## Indicator System

### Complete Missing Indicators
- [ ] Add all status effect indicators
  - [ ] Kneeling
  - [ ] Sitting
  - [ ] Prone/Dead
  - [ ] Invisible
  - [ ] Hidden
  - [ ] Silenced
  - [ ] Other common debuffs

- [ ] Create indicator templates for each new indicator
- [ ] Add to status dashboard options
- [ ] Document indicator IDs in README

## Scripting & Automation

### Stun Handler Script
- [ ] Create script to set stun countdown from game events
- [ ] Parse stun messages from combat
- [ ] Update stun timer widget automatically
- [ ] Handle stun recovery messages

### Macro Support
- [ ] Design macro configuration format
  - [ ] Macro trigger keys/sequences
  - [ ] Macro command sequences
  - [ ] Support for variables/substitution
  - [ ] Conditional macros based on state

- [ ] Implement macro execution engine
  - [ ] Parse macro configuration
  - [ ] Execute macro sequences
  - [ ] Handle delays between commands
  - [ ] Support macro recording mode

- [ ] Create macro UI
  - [ ] List active macros
  - [ ] Edit/create macros
  - [ ] Enable/disable macros
  - [ ] Test macro execution

### Keybind Support
- [x] Design keybind system
  - [x] Map keys to commands
  - [x] Support modifier keys (Ctrl, Alt, Shift)
  - [x] Function key support
  - [x] Numpad key support
  - [x] Configurable per character/global

- [x] Implement keybind handler
  - [x] Capture key events
  - [x] Execute bound commands (actions + macros)
  - [x] Support macro text with \r for enter
  - [x] HashMap-based keybind lookup

- [x] Common keybind defaults
  - [x] F1-F12 for common commands
  - [x] Number pad for directional movement
  - [x] Customizable combat hotkeys

⚠️ **Known Limitation**: Shift + Numpad combinations not supported (terminal/crossterm limitation)

## Highlighting & Text Features

### Advanced Highlighting System
- [x] Implement regex-based highlighting
  - [x] Load highlight patterns from config
  - [x] Support foreground/background colors
  - [x] Support bold styling
  - [x] Aho-Corasick optimization for fast_parse mode
  - [x] Color entire line option

- [x] Create highlight configuration format (TOML)
  - [x] Pattern definitions
  - [x] Color specifications
  - [x] fast_parse option for literal patterns
  - [x] color_entire_line option

- [ ] Highlighting presets (user can create custom)
  - [ ] Document example patterns for combat messages
  - [ ] Document example patterns for player names
  - [ ] Document example patterns for important items/loot
  - [ ] Document example patterns for NPC names
  - [ ] Document example patterns for room directions
  - [ ] Document example patterns for spell casts

### Sound System
- [x] Add cross-platform sound support (using `rodio` crate)
  - [x] Add `rodio` dependency to Cargo.toml
  - [x] Create `~/.vellum-fe/sounds/` directory structure
  - [x] Support WAV, MP3, OGG, FLAC formats
  - [x] Async/non-blocking sound playback
  - [x] Volume control configuration
  - [x] Global sound enable/disable toggle

- [x] Integrate sounds with highlights
  - [x] Add `sound` field to highlight config entries
  - [x] Play sound when highlight pattern matches
  - [x] Support per-highlight volume override
  - [x] Prevent sound spam (cooldown/debounce)
  - [x] Fallback gracefully if sound file missing

- [ ] Sound presets and examples
  - [x] Document sound file naming conventions (README in defaults/sounds/)
  - [x] Create framework for embedding default sounds in binary
  - [ ] Include sample sounds for common events (infrastructure ready, needs actual sound files)
  - [ ] Create sound pack repository/sharing format (future enhancement)
  - [ ] Support character-specific sound overrides (future enhancement)

### Highlight Management UI
- [ ] Create in-app highlight management system
  - [ ] `.addhighlight` command - Create new highlight with interactive prompts
  - [ ] `.edithighlight <name>` command - Edit existing highlight
  - [ ] `.removehighlight <name>` command - Delete highlight
  - [ ] `.listhighlights` command - Show all configured highlights
  - [ ] `.testhighlight <name>` command - Test pattern against recent text
  - [ ] Hot reload highlights without restarting VellumFE
  - [ ] Auto-save highlights to config file
  - [ ] Validation for regex patterns (catch errors before saving)
  - [ ] Visual preview of highlight colors
  - [ ] Support for creating sound-enabled highlights

### Text Selection
⚠️ **Note**: Investigate ratatui capabilities for custom text selection
- [ ] Implement VellumFE-aware text selection
  - [ ] Research ratatui support for custom selection handling
  - [ ] Override terminal's native text selection (Shift+Mouse)
  - [ ] Respect window boundaries (don't select across windows)
  - [ ] Select text within single focused window only
  - [ ] Copy to clipboard with proper line breaks
  - [ ] Visual selection highlighting
  - [ ] Support multi-line selection within window
  - [ ] Fallback to native selection if VellumFE selection disabled

### Terminal Size Management & Responsive Layouts
⚠️ **Note**: Currently crashes if terminal smaller than layout dimensions
- [ ] Terminal size detection and management
  - [ ] Detect terminal dimensions on startup
  - [ ] Investigate setting terminal size programmatically before launching
  - [ ] Handle terminal resize events gracefully (don't crash)
  - [ ] Show error/warning if terminal too small for layout
  - [ ] Minimum terminal size requirements (e.g., 80x24)

- [ ] Responsive layout system
  - [ ] Create default layouts for common terminal sizes:
    - [ ] 80x24 (minimum VT100 size)
    - [ ] 120x40 (medium terminal)
    - [ ] 160x50 (large terminal)
    - [ ] 200x60 (full screen)
  - [ ] Auto-select appropriate layout based on terminal size
  - [ ] Scale/adjust windows when terminal resized
  - [ ] Option to lock layout (prevent auto-scaling)
  - [ ] Document how to resize terminal to specific dimensions (platform-specific)

- [ ] Layout validation
  - [ ] Check layout dimensions fit within terminal on load
  - [ ] Warn if windows would be off-screen
  - [ ] Auto-adjust window positions/sizes if needed
  - [ ] Provide helpful error messages for invalid layouts

### Timestamp Support
- [ ] Add optional timestamps to text windows
  - [ ] Configurable timestamp format
  - [ ] Per-window timestamp enable/disable
  - [ ] Timestamp color customization
  - [ ] Option for 12/24 hour format

## Quality of Life Features

### Autocomplete System
- [ ] Command autocomplete
  - [ ] Complete from command history
  - [ ] Complete known commands
  - [ ] Complete room directions
  - [ ] Complete visible NPC/player names
  - [ ] Tab completion UI

- [ ] Context-aware autocomplete
  - [ ] Different completions based on context
  - [ ] Fuzzy matching
  - [ ] Learn from usage patterns

### Window Management
- [x] Window groups/tabs
  - [x] Tab similar windows together (tabbed text windows)
  - [x] Switch between tabs (click or .switchtab)
  - [x] Activity indicators on inactive tabs
  - [x] Configurable tab colors
  - [x] Tab reordering (.movetab)

- [ ] Window snapping
  - [ ] Snap to edges when moving
  - [ ] Snap to other windows
  - [ ] Configurable snap distance

- [ ] Window presets
  - [ ] Save/load window arrangements
  - [ ] Quick-switch between layouts
  - [ ] Per-character layouts
  - [ ] Export/import layouts

### Command History
- [ ] Persistent command history
  - [ ] Save across sessions
  - [ ] Search history (Ctrl+R style)
  - [ ] History size limits
  - [ ] Clear history command

### Terminal Title Updates
- [ ] Update terminal title with game state
  - [ ] Show current room
  - [ ] Show character name
  - [ ] Show health/mana percentages
  - [ ] Show active status effects

## Advanced Features (Future / Experimental Branch)

### Clickable Links & Context Menus
⚠️ **Note**: These features should be developed on a separate branch to avoid performance impact on main

- [ ] Link detection & clicking
  - [ ] Detect clickable elements (items, NPCs, players, directions)
  - [ ] Render with underline or different color
  - [ ] Handle click events
  - [ ] Execute appropriate command on click

- [ ] Context menus
  - [ ] Right-click on items → look/get/put actions
  - [ ] Right-click on NPCs → look/attack/talk actions
  - [ ] Right-click on players → whisper/look/follow actions
  - [ ] Right-click on direction → go that way

- [ ] Performance optimization
  - [ ] Lazy detection (only active window)
  - [ ] Configurable enable/disable
  - [ ] Cache parsed elements
  - [ ] Benchmark impact on render performance

### Rich Text Rendering
- [ ] Support for more text attributes
  - [ ] Bold, italic, dim
  - [ ] Strikethrough
  - [ ] Multiple underline styles
  - [ ] 24-bit color support (if terminal supports)

## Testing & Quality

### Test Coverage
- [ ] Unit tests for parser
  - [ ] Test each XML tag type
  - [ ] Test nested tags
  - [ ] Test malformed XML
  - [ ] Test edge cases

- [ ] Widget tests
  - [ ] Test rendering
  - [ ] Test updates
  - [ ] Test resize behavior
  - [ ] Test color handling

### Documentation
- [ ] Complete API documentation
- [ ] Widget configuration examples
- [ ] Macro/keybind examples
- [ ] Highlight configuration guide
- [ ] Troubleshooting guide
- [ ] Performance tuning guide

### Performance
- [ ] Profile rendering performance
- [ ] Optimize hot paths
- [ ] Reduce allocations
- [ ] Benchmark against ProfanityFE
- [ ] Memory usage optimization

## Platform Support

### Cross-Platform Testing
- [ ] Test on Windows (primary)
- [ ] Test on Linux
- [ ] Test on macOS
- [ ] Document platform-specific quirks
- [ ] CI/CD for all platforms

## Configuration

### Enhanced Configuration
- [ ] Configuration validation
- [ ] Configuration migration tool
- [ ] Configuration documentation
- [ ] Per-character configurations
- [ ] Profile system (combat, RP, travel, etc.)

---

## Priority Levels

**P0 - Critical (Do First)**
- Target/Player widgets
- Complete indicators
- Highlighting system

**P1 - High Priority**
- Experience window
- Macro support
- Keybind support
- Stun handler script
- Autocomplete

**P2 - Medium Priority**
- Timestamps
- Window management improvements
- Command history
- Terminal title updates
- Enhanced configuration

**P3 - Low Priority**
- Rich text rendering
- Platform testing
- Documentation improvements
- Performance optimization

**P4 - Experimental**
- Clickable links
- Context menus
- Advanced UI features

---

## Notes

- Features marked with ⚠️ require special consideration
- Items marked with "per character" need character-specific configuration storage
- All new widgets should follow the existing widget architecture
- Performance testing should be done after each major feature
- Keep backward compatibility with existing configurations
