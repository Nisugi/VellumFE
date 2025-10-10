# Profanitui TODO List

## High Priority Features

### Active Effects System (effectmon)
- [ ] Create ActiveEffects widget for displaying active spells/buffs/debuffs
  - [ ] Parse `<spell>` XML tags for active spells
  - [ ] Parse buff/debuff indicators from game
  - [ ] Display spell names with remaining duration
  - [ ] Color-code by type (buff=green, debuff=red, utility=blue)
  - [ ] Show cooldown timers for abilities
  - [ ] Support multiple display modes (list, grid, compact)

- [ ] Create MissingSpells widget
  - [ ] Track known spell list per character/profession
  - [ ] Highlight missing defensive spells
  - [ ] Highlight missing offensive spells
  - [ ] Configurable spell priority/grouping
  - [ ] Visual warnings for critical missing buffs

### Target System (targetlist)
- [ ] Create Targets widget - compact mode
  - [ ] Display format: `Targets [XX]` with count
  - [ ] Parse target data from game
  - [ ] Update count in real-time
  - [ ] Click to expand to full list

- [ ] Create Targets widget - expanded mode
  - [ ] Show header with count
  - [ ] List all targets beneath header
  - [ ] Color-code by threat level if available
  - [ ] Show target health bars if available
  - [ ] Support filtering/sorting

### Players Widget (targetlist)
- [ ] Create Players widget - compact mode
  - [ ] Display format: `Players [XX]` with count
  - [ ] Parse player arrival/departure
  - [ ] Update count in real-time

- [ ] Create Players widget - expanded mode
  - [ ] List all players in room
  - [ ] Show player names
  - [ ] Optionally show profession/level if available
  - [ ] Color-code by group/friend status
  - [ ] Support player notes/tags

### Experience Window (exp)
- [ ] Create Experience/Skills widget
  - [ ] Parse skill experience data
  - [ ] Display format: `SkillName: ranks percent% [mindstate/34]`
  - [ ] Color-code by skill category (armor, weapon, magic, survival, lore)
  - [ ] Color-code mindstate (0=white, 1-10=cyan, 11-20=green, 21-30=yellow, 31-34=red)
  - [ ] Sort skills by category or mindstate
  - [ ] Show pulsing indicator
 for actively learning skills
  - [ ] Track skill gains over time
  - [ ] Support skill filtering/grouping

### Percentage/Stats Window (perc)
- [ ] Create general stats display widget
  - [ ] Parse and display various percentage-based stats
  - [ ] Support custom formatting per stat type
  - [ ] Configurable refresh rate

## Parser Improvements

### XML Parser Enhancements
- [ ] Fix blank line handling
  - [ ] Preserve intentional blank lines (like from `mana` command output)
  - [ ] Distinguish between XML artifact blanks and content blanks
  - [ ] Add test cases for various blank line scenarios

- [ ] Improve `<pushStream>` / prompt handling
  - [ ] Track active stream destinations
  - [ ] Skip duplicate prompts when thoughts stream is active
  - [ ] Only show prompt in main if thoughts window doesn't exist
  - [ ] Test with various stream combinations

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
- [ ] Design keybind system
  - [ ] Map keys to commands
  - [ ] Support modifier keys (Ctrl, Alt, Shift)
  - [ ] Function key support
  - [ ] Configurable per character/global

- [ ] Implement keybind handler
  - [ ] Capture key events
  - [ ] Execute bound commands
  - [ ] Show keybind help/cheatsheet
  - [ ] Conflict detection

- [ ] Common keybind defaults
  - [ ] F1-F12 for common commands
  - [ ] Number pad for directional movement
  - [ ] Customizable combat hotkeys

## Highlighting & Text Features

### Advanced Highlighting System
- [ ] Implement regex-based highlighting
  - [ ] Load highlight patterns from config
  - [ ] Support foreground/background colors
  - [ ] Support underline/bold/italic
  - [ ] Priority system for overlapping highlights

- [ ] Create highlight configuration format (XML or TOML)
  - [ ] Pattern definitions
  - [ ] Color specifications
  - [ ] Enable/disable per highlight
  - [ ] Inherit from other highlight files

- [ ] Highlighting presets
  - [ ] Combat messages
  - [ ] Player names
  - [ ] Important items/loot
  - [ ] NPC names
  - [ ] Room directions
  - [ ] Spell casts

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
- [ ] Window snapping
  - [ ] Snap to edges when moving
  - [ ] Snap to other windows
  - [ ] Configurable snap distance

- [ ] Window groups/tabs
  - [ ] Tab similar windows together
  - [ ] Switch between tabs
  - [ ] Detach tabs to separate windows

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
- Active Effects widget (effectmon)
- Target/Player widgets
- Complete indicators
- Parser improvements (blank lines, prompts)
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
