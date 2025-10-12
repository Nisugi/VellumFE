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

- [x] Keybind Management UI
  - [x] `.addkeybind` / `.addkey` - Create new keybind with form
  - [x] `.editkeybind <key>` / `.editkey` - Edit existing keybind
  - [x] `.deletekeybind <key>` / `.delkey` - Delete keybind
  - [x] `.listkeybinds` / `.listkeys` / `.keybinds` - List all keybinds
  - [x] Interactive form with key validation
  - [x] Action dropdown (24 built-in actions)
  - [x] Macro text input for custom commands
  - [x] Auto-save to config and hot-reload keybind_map

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
- [x] Create in-app highlight management system
  - [x] `.addhighlight` command - Create new highlight with interactive prompts
  - [x] `.edithighlight <name>` command - Edit existing highlight
  - [x] `.removehighlight <name>` command - Delete highlight
  - [x] `.listhighlights` command - Show all configured highlights
  - [x] `.testhighlight <name>` command - Test pattern against recent text
  - [x] Hot reload highlights without restarting VellumFE
  - [x] Auto-save highlights to config file
  - [x] Validation for regex patterns (catch errors before saving)
  - [x] Visual preview of highlight colors
  - [x] Support for creating sound-enabled highlights

### Text Selection
- [x] Add arboard dependency for clipboard integration
- [x] Implement VellumFE-aware text selection
  - [x] Add selection state tracking (start pos, end pos, active window)
  - [x] Mouse-based selection trigger (no modifier + drag for selection, Shift+Mouse for native terminal)
  - [x] Respect window boundaries (don't select across windows)
  - [x] Select text within text windows only
  - [x] Copy to clipboard on mouse release
  - [x] Support multi-line selection within window
  - [x] Handle wrapped lines correctly
  - [x] Clear selection on click or Escape key
  - [x] Config option to enable/disable custom selection
- [ ] Visual selection highlighting (deferred - not needed currently, can revisit if requested)

### Wrayth-Style Drag-and-Drop & Context Menus
✅ **Status**: COMPLETE! All phases implemented and tested.
⚠️ **Note**: Emulate Wrayth's clickable link system for game objects
⚠️ **Note**: Must coordinate with text selection feature (different modifier keys)
⚠️ **Design Decision**: Default to text selection (safer), modifier key for drag-drop to prevent accidental expensive item drops

- [x] Parse and load cmdlist1.xml
  - [x] Add quick-xml parser for `<cli coord="..." menu="..." command="..." menu_cat="..."/>` entries
  - [x] Embed default cmdlist1.xml in binary using `include_bytes!("../defaults/cmdlist1.xml")`
  - [x] Extract embedded file to `~/.vellum-fe/cmdlist1.xml` on first run (if missing)
  - [x] Always load from `~/.vellum-fe/cmdlist1.xml` (users can update when Simutronics updates)
  - [x] Re-extract from embedded if file missing/corrupted (self-healing)
  - [x] Build lookup table: coord → (menu_text, command_template, category)
  - [x] Handle @ and # placeholders (@ = display name, # = exist_id in command)
  - [x] Support % placeholder for secondary items (e.g., "pour % on @")
  - [x] Cache parsed data in memory for fast lookups

- [x] Link detection and tracking
  - [x] Parse `<a exist="ID" noun="...">text</a>` tags from game XML
  - [x] Track exist_id and noun for each link in parsed text
  - [x] Store link positions (window, line, column range) for click detection
  - [x] Render links with underline or different color (configurable)
  - [x] Update link positions when window scrolls or resizes
  - [x] Clear link cache when window content changes

- [x] Left-click context menu (NO right-click!)
  - [x] **Distinguish click from drag**: Movement threshold (2 pixels)
  - [x] Detect left-click on link (mouse down + up at same position = CLICK)
  - [x] If mouse moves beyond threshold: DRAG mode (not click)
  - [x] Generate request counter (correlation ID) for menu request
  - [x] Send `_menu #exist_id counter` to game server on click
  - [x] Parse menu response: `<menu id="counter" path="" cat_list="..."><mi coord="..."/><mi coord="..."/>...`
  - [x] Verify response `id` attribute matches our `counter` (request correlation)
  - [x] Extract all `<mi coord="..."/>` tags from response
  - [x] Look up each coord in cmdlist1.xml to get menu entries (menu, command, menu_cat)
  - [x] Skip coords not found in cmdlist (game adds commands faster than cmdlist updates)
  - [x] **Filter out dialog commands** (commands starting with `_dialog`)
    - [x] Skip `_dialog` commands (speak to, sing to, recite to, submit bug report)
    - [ ] Later phase: Implement dialog widget for `_dialog` commands
  - [x] **Substitute placeholders correctly**:
    - [x] `@` = noun (display text: "look @" → "look pendant")
    - [x] `#` = "#exist_id" **WITH # symbol** (command: "look #" → "look #73772244")
  - [x] **Group by category** and build menu structure:
    - [x] Parse `menu_cat` for base category and subcategory (e.g., "5_roleplay-swear" → base=5_roleplay, sub=swear)
    - [x] Sort categories by number (0-13, top to bottom)
    - [x] Categories with ≤4 items: Add all directly to main menu
    - [x] Categories with 5+ items: Create submenu trigger with ">" (e.g., "roleplay >")
    - [x] Extract category display name from suffix (e.g., "5_roleplay" → "roleplay")
  - [x] Render context menu as popup widget at mouse position
  - [x] **Menu items are clickable links** (reuse link rendering!)
  - [x] Track bounds for each menu item and submenu trigger
  - [x] **Handle submenu clicks**: Open submenu popup at appropriate position
  - [x] **Handle nested submenus**: Subcategories with `-` create nested popups (3 levels deep)
  - [x] Send selected command on menu item click
  - [x] Close menu on final selection, click outside, or Escape key
  - [x] Keyboard navigation (Arrow keys, Enter, Escape)

- [x] Drag-and-drop functionality (**REQUIRES Ctrl key for safety!**)
  - [x] Check if Ctrl key is held on mouse down on link
  - [x] If no Ctrl: regular click opens context menu immediately
  - [x] If Ctrl held: track mouse down on link for drag-drop
  - [x] Detect drop target on mouse release
    - [x] Drop on another link: send `put my X in my Y`
    - [x] Drop in empty space: send `drop my X`
  - [x] Cancel drag on Escape key (or no significant movement)
  - [ ] Visual feedback during drag (highlight source link, show dragging cursor) - future enhancement
  - [ ] Handle text scrolling during drag operation - future enhancement
  - [ ] Auto-scroll window if mouse near top/bottom edge - future enhancement

- [x] Interaction with text selection (SIMPLIFIED STRATEGY!)
  - [x] **No modifier + click on link** = Context menu
  - [x] **No modifier + drag (not on link)** = Text selection (VellumFE-aware)
  - [x] **Ctrl + drag on link** = Drag-and-drop (requires Ctrl for safety!)
  - [x] **Shift + drag** = Native terminal selection (VellumFE ignores, passthrough)
  - [x] Check if Ctrl held on mouse down on link
  - [ ] Visual indicator when Ctrl held over link (cursor change, highlight) - future enhancement

- [x] Configuration options
  - [x] `drag_modifier_key` - Modifier key for drag-drop ("ctrl", "alt", "shift", "none")
  - [x] Note: cmdlist1.xml always loaded from `~/.vellum-fe/cmdlist1.xml` (no path config needed)
  - [x] `link_color` - Color for clickable links
  - [x] `link_underline` - Underline links (true/false)
  - [x] `selection_enabled` - Enable/disable VellumFE text selection (default: true)
  - [x] `selection_respect_window_boundaries` - Limit selection to single window (default: true)

- [x] Performance considerations
  - [x] Lazy link detection (only recent links cached)
  - [x] Limit link cache size (100 recent links)
  - [x] Smart word-at-position detection with multi-word priority

- [x] Testing and edge cases
  - [x] Handle malformed exist_id values
  - [x] Handle missing cmdlist1.xml gracefully (self-healing)
  - [x] Handle network lag during context menu operations
  - [x] Test with multiple items with same noun (multi-word priority)
  - [x] Test drag-drop across different window types
  - [x] Test with very long link text (wrapping)

### Terminal Size Management & Responsive Layouts
- [x] Terminal size detection and management
  - [x] Detect terminal dimensions on startup
  - [x] Handle terminal resize events gracefully (don't crash)
  - [x] Show error/warning if terminal too small for layout
  - [x] Minimum terminal size requirements (80x24)
  - [x] Clamp window dimensions to fit within terminal bounds

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
- [x] Command autocomplete
  - [x] Complete dot commands (`.createwindow`, `.addhl`, etc.)
  - [x] Complete window names
  - [x] Complete template names
  - [x] Tab completion UI (press Tab to cycle through completions)
  - [ ] Complete from command history
  - [ ] Complete room directions
  - [ ] Complete visible NPC/player names

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
- [x] Persistent command history
  - [x] Save across sessions (`~/.vellum-fe/history/`)
  - [x] Character-specific history files
  - [x] Auto-load on startup
  - [x] Auto-save on exit
  - [x] History size limits (configurable max_history)
  - [x] Up/Down arrow navigation through history
  - [ ] Search history (Ctrl+R style)
  - [ ] Clear history command

### Terminal Title Updates
- [ ] Update terminal title with game state
  - [ ] Show current room
  - [ ] Show character name
  - [ ] Show health/mana percentages
  - [ ] Show active status effects

## Advanced Features (Future / Experimental Branch)

### ⚠️ Old Clickable Links Section (REPLACED - See "Wrayth-Style Drag-and-Drop" above)
⚠️ **Note**: This section is superseded by the comprehensive Wrayth-style implementation above
⚠️ **Note**: Retained for historical reference only - DO NOT IMPLEMENT THIS VERSION

<details>
<summary>Old clickable links design (click to expand)</summary>

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

</details>

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
- ✅ Target/Player widgets (DONE)
- ✅ Complete indicators (DONE)
- ✅ Highlighting system (DONE)

**P1 - High Priority**
- ✅ Terminal size management (DONE - prevents crashes)
- ✅ Highlight management UI (DONE)
- ✅ Text selection (DONE - window-aware, clipboard copy)
- ✅ Keybind support (DONE)
- ✅ Command history persistence (DONE)
- ✅ Tab completion (DONE - dot commands, windows, templates)
- Macro support (defer to Lich - keybinds sufficient)
- Stun handler script

**P2 - Medium Priority**
- ✅ Wrayth-style drag-and-drop (COMPLETE!)
  - ✅ Phase 1: Link detection and metadata storage (DONE)
  - ✅ Phase 2: cmdlist1.xml parsing (DONE - 588 entries loaded)
  - ✅ Phase 3: Menu request/response flow (DONE)
  - ✅ Phase 4: Mouse click detection on links + popup menu rendering (DONE)
  - ✅ Phase 5: Drag and drop with Ctrl modifier key (DONE)
  - Features: Hierarchical context menus, keyboard navigation, drag-to-container, configurable modifier key
- Timestamps
- Window management improvements
- Terminal title updates
- Enhanced configuration

**P3 - Low Priority**
- Rich text rendering
- Platform testing
- Documentation improvements
- Performance optimization

**P4 - Experimental**
- Advanced UI features
- Visual effects
- Multi-select drag-drop

---

## Notes

- Features marked with ⚠️ require special consideration
- Items marked with "per character" need character-specific configuration storage
- All new widgets should follow the existing widget architecture
- Performance testing should be done after each major feature
- Keep backward compatibility with existing configurations
