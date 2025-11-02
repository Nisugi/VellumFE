# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**VellumFE** is a modern, high-performance terminal frontend for GemStone IV, built with Ratatui. It connects to Lich (Ruby scripting engine) via detached mode and provides a blazing-fast TUI with dynamic window management, custom highlights with Aho-Corasick optimization, Wrayth-style clickable links with context menus, mouse support, and full XML stream parsing.

## Build and Development Commands

```bash
# Build for development
cargo build

# Run the application
cargo run

# Build for release
cargo build --release
# Binary located at: target/release/vellum-fe

# Enable debug logs
RUST_LOG=debug cargo run
# Logs written to ~/.vellum-fe/debug.log

# Run with character-specific config
cargo run -- --character Zoleta --port 8000
# Logs written to ~/.vellum-fe/debug_Zoleta.log
```

## Running the Application

**Prerequisites:**
1. Start Lich in detached mode first (wait 5-10 seconds before launching VellumFE)
2. Default connection: `localhost:8000`

**Windows (PowerShell):**
```powershell
# Note: Replace 3.4.x with your actual Ruby version (e.g., 3.4.2, 3.4.5, etc.)
C:\Ruby4Lich5\3.4.x\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

**Linux/Mac:**
```bash
ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

## Architecture

### High-Level Flow

1. **main.rs** → Parses command-line args, initializes character-specific logging, loads config with character override, creates and runs App
2. **app.rs** → Main event loop handling terminal events, server messages, and UI rendering
3. **network.rs** → TCP connection to Lich server (async via tokio)
4. **parser.rs** → Parses GemStone IV XML protocol into structured elements
5. **ui/** → Window management, text rendering, progress bars, countdown timers

### Key Architecture Patterns

**Window Management:**
- Windows use **absolute positioning** (row, col, rows, cols) - each window is independent
- No grid layout - windows can overlap, have gaps, be moved/resized freely
- WindowManager maintains windows and stream routing mappings
- Widget types: `text`, `progress`, `countdown`, `tabbed`, `indicator`, `compass`, `injury_doll`, `hands`, `dashboard`, `active_effects`, `map`

**Stream Routing:**
- Game output is divided into named streams (main, thoughts, speech, familiar, etc.)
- Parser emits `StreamPush`/`StreamPop` elements to switch active stream
- Windows subscribe to streams via `streams: Vec<String>` in their config
- Multiple streams can route to the same window (e.g., speech + whisper)

**XML Parsing State Machine:**
- Parser maintains stacks for nested tags: `color_stack`, `preset_stack`, `style_stack`, `bold_stack`
- Flushes text buffer when opening/closing color tags to ensure correct styling
- Handles paired tags (`<prompt>...</prompt>`) and self-closing tags (`<roundTime value='5'/>`)
- Decodes HTML entities (`&gt;`, `&lt;`, etc.)

**Event Loop:**
- Terminal events polled with configurable timeout (default 16ms for ~60 FPS)
- Poll timeout adjustable via `poll_timeout_ms` setting (lower = higher FPS, higher CPU usage)
- Server messages processed via mpsc channel (non-blocking `try_recv`)
- UI redrawn every frame with window layouts recalculated
- Mouse drag operations (resize/move) track delta from last position

**InputMode Pattern:**
- `InputMode` enum tracks current input mode (Normal, Search, HighlightForm, KeybindForm, SettingsEditor, HighlightBrowser, WindowEditor)
- Single source of truth for what mode the app is in
- All popup editors use InputMode to manage state (prevents multiple editors open at once)
- Command input hidden when popup editor is active (InputMode != Normal/Search)
- Keyboard and mouse events routed based on InputMode
- Esc key in any editor returns to Normal mode
- Uniform behavior: all popups draggable, black background, cyan border

### Critical Implementation Details

**Text Wrapping:**
- Text is added character-by-character to a line buffer in TextWindow
- When `finish_line()` is called, the buffer is wrapped to window width
- Styled text segments maintain their colors/bold through wrapping
- Each window tracks its own `inner_width` (updated during layout calculation)

**Progress Bars:**
- Auto-update from `<progressBar>` XML tags sent by game server
- Special handling: encumbrance changes color based on value (green→yellow→brown→red)
- Can display either current/max numbers OR custom text (e.g., "clear as a bell" for mind state)
- ProfanityFE-style background coloring (bar fills from left with background color)

**Countdown Timers:**
- Auto-update from `<roundTime>` and `<castTime>` XML tags (Unix timestamps)
- Character-based fill animation: fills N characters where N = remaining seconds
- Colors: roundtime=red, casttime=blue, stun=yellow
- Centered text shows remaining seconds

**Configuration System:**
- Embedded defaults bundled into binary using `include_str!()`
- Multi-character support with character-specific configs and layouts
- Config priority: `~/.vellum-fe/configs/<character>.toml` → `~/.vellum-fe/configs/default.toml` → embedded defaults
- Layout priority: `auto_<character>.toml` → `<character>.toml` → `default.toml`
- Character-specific debug logs: `debug_<character>.log`

**Layout Persistence:**
- Window configs stored in `~/.vellum-fe/configs/` directory
- Layouts stored in `~/.vellum-fe/layouts/<name>.toml` (just windows array)
- Autosave layout created on exit, loaded on startup if exists

**Mouse Operations:**
- Mouse support is enabled by default on application start
- Click title bar (top border, excluding corners) to move window
- Click edges/corners to resize (corners resize from that corner, edges resize one dimension)
- Click tabs in tabbed windows to switch between tabs
- Mouse scroll over windows to scroll up/down in text history
- Title bar detection excludes corners (leaves 1 cell margin on each side)
- Resize/move use incremental deltas, not absolute positions
- Text selection: Click and drag (no modifiers) in text windows to select text
  - Selected text automatically copied to clipboard on mouse release
  - Respects window boundaries (won't select across windows)
  - Escape or click anywhere to clear selection
  - Shift+Mouse enables native terminal selection (bypasses VellumFE selection)

**Clickable Links & Context Menus (Wrayth-style):**
- Game objects wrapped in `<a exist="..." noun="...">text</a>` tags become clickable links
- Left-click on any word in a link to open context menu with available actions
- Multi-word link prioritization: "raven feather" preferred over individual "raven"
- Recent links cache (last 100) for efficient lookup without position tracking
- Context menus populated from 588 command entries in `defaults/cmdlist1.xml`
- Hierarchical menu system supports 3 levels deep:
  - Main menu → Category submenu → Subcategory menu
  - Categories with `_` become submenus (e.g., `5_roleplay`)
  - Nested categories use hyphens (e.g., `5_roleplay-swear`)
  - Category 0 always appears at end
  - Category names displayed in lowercase
- Menu request/response protocol: `_menu #<exist_id> <counter>`
- Secondary noun support from `<mi noun="..."/>` tags for `%` placeholder substitution
- Menu text formatting:
  - `#` and `@` are removed/truncated from displayed text
  - `%` is substituted with secondary noun (e.g., held item) if available
  - If no secondary noun, `%` displays as-is for debugging
- Mouse and keyboard navigation:
  - Arrow keys to navigate, Enter to select
  - Esc or Left arrow to close submenu (keeps parent open)
  - Right arrow or Enter on submenu items to open nested menu
  - All three menu levels stay visible simultaneously
- Menus positioned at click location with automatic bounds checking
- Commands not in cmdlist1.xml won't appear (need investigation and manual addition)
- `_dialog` commands currently skipped (dialog box not yet implemented)
- Stream discard: If no window exists for a pushed stream, text is discarded until pop

## Module Structure

### src/main.rs
Entry point and initialization. Contains:
- Command-line argument parsing with `clap`
- Args struct: `--port` / `-p`, `--character` / `-c`, `--links`
- Character-specific debug log initialization
- Config loading with character override
- App creation and execution

**Command-Line Arguments:**
```bash
vellum-fe --port 8000 --character Zoleta --links true
```

### src/app.rs
Main application loop and state management. Contains:
- Event handling (keyboard, mouse)
- Server message processing (delegates to parser)
- Dot command handlers (`.createwindow`, `.savelayout`, etc.)
- Window focus management
- Resize/move state tracking

**Key State:**
- `window_manager` - Owns all windows and stream routing
- `parser` - XML parser with color/style state stacks
- `current_stream` - Active stream for incoming text
- `focused_window_index` - For keyboard scrolling (Tab to cycle)
- `resize_state` / `move_state` - Track active drag operations

### src/config.rs
Configuration management and window templates. Contains:
- `Config` struct with connection, UI, presets, highlights, keybinds, spell_colors
- Embedded defaults using `include_str!("../defaults/config.toml")` and `include_str!("../defaults/layout.toml")`
- `load_with_options(character, port)` - Character-specific config loading
- Multi-character support with separate configs/layouts per character
- Window template definitions for all built-in window types
- Layout save/load functionality with priority system
- Path helpers: `config_path()`, `configs_dir()`, `layouts_dir()`, `get_log_path()`

**Window Templates:**
Text windows: main, thoughts, speech, familiar, room, logons, deaths, arrivals, ambients, announcements, loot
Progress bars: health, mana, stamina, spirit, mindstate, encumbrance, stance, bloodpoints
Countdown timers: roundtime, casttime, stun

### src/network.rs
Async TCP connection to Lich server. Contains:
- `LichConnection::start()` - Spawns reader and writer tasks
- Sends `SET_FRONTEND_PID` on connect
- Emits `ServerMessage` enum (Connected, Disconnected, Text)

### src/parser.rs
GemStone IV XML protocol parser. Contains:
- `parse_line()` - Processes one line of XML, returns `Vec<ParsedElement>`
- Tag handlers for all XML elements
- Color/style stack management for nested tags
- Preset color mappings (loaded from config)

### src/selection.rs
Text selection state and coordinate tracking. Contains:
- `SelectionState` - Tracks selection start/end positions and active state
- `TextPosition` - Window-aware text coordinates (window_index, line, col)
- Selection boundary checking and normalization
- Window-relative coordinate conversion helpers

### src/cmdlist.rs
Command list parser for clickable link context menus. Contains:
- `CmdList` - Parses `defaults/cmdlist1.xml` with 588 command entries
- `CmdListEntry` - Command definition (coord, menu text, command, category)
- `get()` - Look up command by coordinate string (e.g., "2524,2061")
- `substitute_command()` - Replace placeholders in commands:
  - `#` → exist_id (unique object ID)
  - `@` → noun (object name)
  - `%` → secondary noun (e.g., held item)
- Command categories for hierarchical menus (e.g., "5_roleplay", "5_roleplay-swear")
- Loads at startup, shared across all menu requests

**Important XML Elements:**
- `<pushStream id='...'/>` / `<popStream/>` - Stream routing
- `<progressBar id='...' value='...' text='...'/>` - Vitals updates
- `<roundTime value='...'/>` / `<castTime value='...'/>` - Timers
- `<preset id='...'> ... </preset>` - Styled text sections
- `<prompt time='...'>...</prompt>` - Game prompts (colored per character)
- `<a exist='...' noun='...'>text</a>` - Clickable links for game objects
- `<menuResponse id='...'><mi coord='...' noun='...'/></menuResponse>` - Menu data from server

### src/map_data.rs
Map coordinate data loader and context manager. Contains:
- `MapData` - Container for all map contexts and room coordinates
- `MapContext` - Single map area (e.g., "Wehnimer's Landing - Town Square")
- `MapCoordinate` - (x, y, z) position for a room
- `load_default()` - Loads embedded `defaults/map_coordinates.json`
- `get_context_for_room()` - Determines which map context a room belongs to
- `get_coordinate()` - Gets (x, y, z) position for a room ID
- Supports separate coordinate spaces for outdoor areas vs building interiors

### src/ui/window_manager.rs
Manages multiple windows and their layouts. Contains:
- `calculate_layout()` - Converts window configs to Ratatui Rect positions
- `update_widths()` - Adjusts window inner widths for wrapping calculations
- Stream mapping (`stream_map: HashMap<String, String>`)
- Widget creation and management (TextWindow, ProgressBar, Countdown)

### src/ui/text_window.rs
Text display widget with wrapping and scrolling. Contains:
- `add_text()` - Adds styled text to current line buffer
- `finish_line()` - Wraps and commits current line to buffer
- `scroll_up()` / `scroll_down()` - Scrollback navigation
- Line wrapping that preserves styling across wrap points

### src/ui/progress_bar.rs
Progress bar widget for vitals. Contains:
- `set_progress()` - Update current/max values
- `set_progress_with_text()` - Update with custom display text
- `set_bar_colors()` - Change bar/background colors
- ProfanityFE-style background fill rendering

### src/ui/countdown.rs
Countdown timer widget. Contains:
- `set_countdown()` - Set end time (Unix timestamp)
- Character-based fill animation (fills N chars where N = seconds remaining)
- Automatic countdown via system time comparisons

### src/ui/map_widget.rs
Map widget for displaying local area navigation. Contains:
- `new()` - Create map widget with title
- `set_current_room()` - Update current room position and context
- Context-aware display (auto-switches between outdoor/indoor maps)
- Centered viewport (current room always at center, shows ±5 grid units)
- Visual indicators: `●` = current room, `○` = visited, `·` = unvisited
- Automatic connection lines between adjacent rooms
- Map coordinate data loaded from `defaults/map_coordinates.json`
- Supports multiple z-levels (up/down movement)

### src/ui/tabbed_text_window.rs
Tabbed text window widget with activity indicators. Contains:
- `new()` - Create empty tabbed window (tabs added later)
- `with_tabs()` - Create tabbed window with initial tabs
- `add_tab()` - Dynamically add new tab
- `remove_tab()` - Remove tab by name
- `switch_to_tab()` / `switch_to_tab_by_name()` - Change active tab
- `add_text_to_stream()` - Route text to correct tab, set unread flag if inactive
- `finish_line_for_stream()` - Finish line for specific tab's stream

### src/ui/popup_menu.rs
Context menu widget for clickable links. Contains:
- `PopupMenu` - Renders menu as overlay at specified position
- `MenuItem` - Menu entry with display text and command to execute
- `new()` - Create menu with items and position
- `select_next()` / `select_previous()` - Arrow key navigation
- `get_selected_command()` - Get command for current selection
- `check_click()` - Handle mouse clicks on menu items
- `get_items()` / `get_position()` / `get_selected_index()` - Accessors for state
- Rendering: Solid black background with bordered menu
- Supports nested menus via `__SUBMENU__<category>` command format
- Menu items show ">" indicator for submenus
- Multiple menu levels (popup_menu, submenu, nested_submenu) can be displayed simultaneously

**Key Features:**
- Each tab contains its own TextWindow instance
- Tabs route to specific game streams (speech, thoughts, whisper, etc.)
- Unread indicators: inactive tabs with new messages show configurable prefix and color
- Clicking a tab clears its unread status
- Customizable colors for active, inactive, and unread tabs

### src/ui/settings_editor.rs
Settings editor widget for all configuration values. Contains:
- `SettingsEditor` - Popup editor with category grouping and keyboard/mouse navigation
- `SettingItem` - Individual setting with category, key, display name, value, description, editable flag
- `SettingValue` - Enum for different value types (String, Number, Float, Boolean, Color)
- `with_items()` - Create editor with settings list
- `previous()` / `next()` / `page_up()` / `page_down()` - Navigation
- `start_edit()` - Enter edit mode (or no-op for booleans)
- `toggle_boolean()` - Toggle boolean values (updates internal state)
- `finish_edit()` - Complete edit and return new value
- `handle_mouse()` - Mouse drag support for moving popup
- `render()` - Draws popup with black background, cyan border, category headers
- Popup positioned at (10, 2) with 70x25 size, draggable by title bar
- Boolean values display as `[✓]` / `[ ]` checkboxes
- Preset colors use "fg bg" format (e.g., `#9BA2B2 #395573` or `#53a684 -`)
- Real-time validation for hex colors, port ranges, volume, poll timeout
- Changes save immediately to config file and refresh display

### src/ui/highlight_browser.rs
Highlight browser widget for viewing and managing highlights. Contains:
- `HighlightBrowser` - Popup browser with category grouping and color/sound indicators
- `HighlightEntry` - Display entry with name, pattern, category, colors, has_sound
- `new()` - Create browser from HashMap of highlights
- `previous()` / `next()` / `page_up()` / `page_down()` - Navigation
- `get_selected()` - Returns currently selected highlight name
- `handle_mouse()` - Mouse drag support for moving popup
- `render()` - Draws popup with category headers in yellow/bold
- Groups highlights by category with sorted display
- Shows color preview `[#RRGGBB]` for fg/bg colors
- Shows ♫ indicator for highlights with sounds
- Popup positioned at (10, 2), draggable by title bar

### src/ui/highlight_form.rs
Highlight form widget for creating/editing highlights. Contains:
- `HighlightFormWidget` - Popup form with text fields, checkboxes, buttons
- `FormMode` - Create or Edit(name) mode
- `FormResult` - Save, Delete, or Cancel result
- `new()` - Create form for new highlight
- `new_edit()` / `with_pattern()` - Create form for editing existing highlight
- `input()` - Handle keyboard input
- `handle_mouse()` - Mouse drag support for moving popup
- `render()` - Draws popup with black background, cyan border
- Fields: Name, Pattern (regex), Category, FG Color, BG Color, Bold, Color Entire Line, Fast Parse, Sound File, Volume
- Tab/Shift+Tab navigation between fields
- Color preview boxes for fg/bg colors
- Popup positioned at (10, 2) with 62x40 size, draggable by title bar

### src/ui/keybind_form.rs
Keybind form widget for creating/editing keybinds. Contains:
- `KeybindFormWidget` - Popup form with text fields, radio buttons, dropdown
- `FormMode` - Create or Edit mode
- `KeybindFormResult` - Save, Delete, or Cancel result
- `KeybindActionType` - Action (built-in) or Macro (text)
- `new()` - Create form for new keybind
- `new_edit()` - Create form for editing existing keybind
- `input()` - Handle keyboard input
- `handle_mouse()` - Mouse drag support for moving popup
- `render()` - Draws popup with black background, cyan border
- Fields: Key Combo, Action Type (radio buttons), Action/Macro field
- Action dropdown with 23 built-in actions
- Popup positioned at (10, 2) with 80x25 size, draggable by title bar

### src/ui/window_editor_v2.rs
Window editor widget for creating/editing windows. Contains:
- `WindowEditor` - Comprehensive popup editor for all window types
- `EditorMode` - SelectingWindow, Editing, or Dropdown mode
- `WindowEditorResult` - Save or Cancel result
- `open_for_window()` - Open in window selection mode
- `open_for_new_window()` - Open for creating new window
- `load_window()` - Load window for editing
- `handle_key()` - Handle keyboard input (uses ratatui KeyEvent)
- `handle_mouse()` - Mouse drag support for moving popup
- `render()` - Draws popup with widget-specific fields
- Dynamic field display based on widget_type
- Supports all widget types: text, progress, countdown, tabbed, indicator, compass, injury_doll, hands, dashboard, active_effects, inventory, room, map
- Popup uses `InputMode::WindowEditor` for state management

## Configuration

**Directory Structure:**
- `~/.vellum-fe/configs/default.toml` - Default configuration
- `~/.vellum-fe/configs/<character>.toml` - Character-specific configs
- `~/.vellum-fe/layouts/default.toml` - Default window layout
- `~/.vellum-fe/layouts/<character>.toml` - Character layouts
- `~/.vellum-fe/layouts/auto_<character>.toml` - Autosaved layouts (highest priority)
- `~/.vellum-fe/debug.log` - Debug log (or `debug_<character>.log` with `-c`)
- `defaults/config.toml` - Source defaults (embedded at compile time)
- `defaults/layout.toml` - Source layout defaults (embedded at compile time)

**Config Loading Priority:**
1. `~/.vellum-fe/configs/<character>.toml` (if `--character` specified)
2. `~/.vellum-fe/configs/default.toml`
3. Embedded defaults from `defaults/config.toml`

**Layout Loading Priority:**
1. `~/.vellum-fe/layouts/auto_<character>.toml`
2. `~/.vellum-fe/layouts/<character>.toml`
3. `~/.vellum-fe/layouts/default.toml`
4. Embedded defaults from `defaults/layout.toml`

### Important Config Sections

```toml
[connection]
host = "127.0.0.1"
port = 8000

[ui]
command_echo_color = "#ffffff"
countdown_icon = "\u{f0c8}"  # Nerd Font icon for countdown blocks (default)

[[ui.prompt_colors]]
character = "R"  # Roundtime indicator
color = "#ff0000"

[[presets]]
id = "speech"
fg = "#53a684"

[[ui.windows]]
name = "main"
widget_type = "text"  # or "progress", "countdown", "tabbed", "map", etc.
streams = ["main"]
row = 0
col = 0
rows = 30
cols = 120
buffer_size = 10000
show_border = true
border_style = "single"  # or "double", "rounded", "thick", "none"
title = "Main"

# Tabbed window example
[[ui.windows]]
name = "chat"
widget_type = "tabbed"
streams = []  # Tabs handle their own streams
row = 0
col = 120
rows = 24
cols = 60
buffer_size = 5000
show_border = true
title = "Chat"
tab_bar_position = "top"  # or "bottom"
tab_active_color = "#ffff00"  # Yellow for active tab
tab_inactive_color = "#808080"  # Gray for inactive tabs
tab_unread_color = "#ffffff"  # White/bold for unread tabs
tab_unread_prefix = "* "  # Prefix shown on tabs with unread

[[ui.windows.tabs]]
name = "Speech"
stream = "speech"

[[ui.windows.tabs]]
name = "Thoughts"
stream = "thoughts"

[[ui.windows.tabs]]
name = "Whisper"
stream = "whisper"

# Map widget example
[[ui.windows]]
name = "map"
widget_type = "map"
row = 0
col = 80
rows = 25
cols = 40
show_border = true
title = "Local Map"
```

## Dot Commands (Local, Not Sent to Game)

### Window Management
- `.quit` - Exit application
- `.createwindow <template>` - Create window from template
- `.customwindow <name> <stream1,stream2,...>` - Create custom text window
- `.deletewindow <name>` - Delete window
- `.windows` / `.listwindows` - List active windows
- `.templates` - List available templates
- `.rename <window> <new title>` - Change window title
- `.border <window> <style> [color]` - Change border style/color

### Tabbed Windows
- `.createtabbed <name> <tab1:stream1,tab2:stream2,...>` - Create tabbed window
  - Example: `.createtabbed chat Speech:speech,Thoughts:thoughts,Whisper:whisper`
- `.addtab <window> <tab_name> <stream>` - Add tab to tabbed window
  - Example: `.addtab chat LNet logons`
- `.removetab <window> <tab_name>` - Remove tab from tabbed window
- `.switchtab <window> <tab_name|index>` - Switch to specific tab
  - Example: `.switchtab chat Speech` or `.switchtab chat 0`

### Widget-Specific Commands
- `.setprogress <window> <current> <max>` - Update progress bar
- `.setbarcolor <window> <color> [bg_color]` - Change progress bar colors
- `.setcountdown <window> <seconds>` - Set countdown timer

### Layout Management
- `.savelayout [name]` - Save current layout (default: "default")
- `.loadlayout [name]` - Load saved layout
- `.layouts` - List saved layouts

### Popup Editors
All popup editors (Settings, Highlights, Keybinds, Windows) follow the same pattern:
- Draggable via title bar (click and drag)
- Black background with cyan border
- Use `InputMode` enum to manage state (only one editor open at a time)
- Hide command input when open
- `Esc` closes the editor

**Settings Editor:**
- `.settings` / `.config` - Open settings editor
  - Navigate: `↑/↓` arrows, `PgUp/PgDn` for pages
  - Edit/Toggle: `Enter` or `Space`
  - Categories: Connection, UI, Sound, Presets, Spells, Prompts
  - All changes save immediately to config file
  - Boolean settings show as `[✓]` / `[ ]` and toggle on Enter/Space
  - Preset colors use format: `#RRGGBB #RRGGBB` (fg bg), use `-` for no color
  - Validation: Hex colors, port ranges (1-65535), volume (0.0-1.0), poll timeout (1-1000ms)

**Highlight Browser:**
- `.highlights` / `.listhl` - Open highlight browser
  - Navigate: `↑/↓` arrows, `PgUp/PgDn` for pages
  - Edit: `Enter` on selected highlight
  - Delete: `Delete` key on selected highlight
  - Groups highlights by category with yellow headers
  - Shows color preview `[#RRGGBB]` and sound indicator ♫
  - Sorts by category then name

**Highlight Form:**
- `.addhl` - Add new highlight
- Opens automatically when editing from browser
  - Fields: Name, Pattern (regex), Category, FG Color, BG Color, Bold, Color Entire Line, Fast Parse, Sound File, Volume
  - Tab/Shift+Tab to navigate fields
  - Category field enables grouping in browser

**Keybind Form:**
- `.addkeybind` - Add new keybind
  - Fields: Key Combo, Action Type (Action/Macro), Action/Macro Text
  - Tab/Shift+Tab to navigate fields
  - Action dropdown for built-in actions

**Window Editor:**
- `.editwindow [name]` - Edit existing window
- `.addwindow` / `.newwindow` - Create new window
- `.editinput` - Edit command input box
  - Widget-specific fields displayed dynamically
  - Comprehensive window configuration

## Common Development Patterns

### Adding a New Window Template

1. Add template in `config.rs::get_window_template()`
2. Add template name to `available_window_templates()`
3. Specify widget_type ("text", "progress", "countdown")
4. Set default position/size, streams, colors

### Adding a New XML Tag Handler

1. Add variant to `ParsedElement` enum in `parser.rs`
2. Add handler method in `XmlParser` (e.g., `handle_my_tag()`)
3. Call handler from `process_tag()`
4. Add handling in `app.rs::handle_server_message()` match block

### Adding a New Dot Command

1. Add case to match in `app.rs::handle_dot_command()`
2. Parse arguments from `parts: Vec<&str>`
3. Modify config or window_manager state
4. Call `update_window_manager_config()` if changing windows
5. Call `add_system_message()` to provide feedback

## Recent Feature Additions (2025)

### Command Input Horizontal Scrolling
- Command input now scrolls horizontally for long commands
- Cursor stays visible when typing past window width
- Smooth scrolling keeps cursor at ~30% from left edge when in middle of text
- Implemented in `src/ui/command_input.rs`

### Configurable Compass Colors
- Compass widget supports customizable colors for active/inactive exits
- Config fields: `compass_active_color` (default: #00ff00), `compass_inactive_color` (default: #333333)
- Configurable via layout TOML or window editor UI
- Added to WindowDef, WindowConfig, and window editor (fields 24, 25)

### Text Color for Hands and Progress Bars
- Added `text_color` field to WindowDef for customizing text color on widgets
- Applies to Hand, Hands, and ProgressBar widgets
- Configurable via layout TOML or window editor UI (field 23)
- Default: white

### Window Editor Improvements
- Dynamic tab order that skips hidden fields based on widget type
- Compass colors (active/inactive) added for compass widgets
- Text color field added for hands and progress widgets
- Bar color field for countdown and progress widgets
- All conditionally rendered fields now in correct tab order

### Bloodpoints Support
- Added handler for `<dialogData id='BetrayerPanel'>` XML tags
- Updates lblBPs/bloodpoints/blood windows with blood points value
- Parses "Blood Points: XX" format from label values

### Border Fixes
- Fixed text windows to respect `show_border` flag for width calculations
- Text no longer shifts left by one column when border is removed
- Border padding only applied when borders are actually shown

### Content Alignment
- Text windows now support `content_align` setting
- Only centers content when text is shorter than window height
- Once content fills window, reverts to normal top-aligned scrolling
- Prevents jarring "filling from center" effect

## Testing Tips

- Use `.setprogress health 50 100` to manually test progress bars
- Use `.setcountdown roundtime 5` to test countdown timers
- Check `~/.vellum-fe/debug.log` for tracing output
- Terminal size changes require layout recalculation (handled automatically)
- Mouse operations log to debug when RUST_LOG=debug
- Test compass colors: Edit compass window, set `compass_active_color` and `compass_inactive_color`
- Test command input scrolling: Type 120+ characters in command input

## Future Features

### VellumFE Companion Server (Planned)
- WebSocket server to allow mobile/remote access
- Broadcast game events (vitals, text, room updates) to companion apps
- Receive commands from companion devices
- See `COMPANION_APP.md` for full specification
- Target: Allow playing GemStone IV from mobile devices via VellumFE proxy

## Known Limitations

- Windows can overlap (intentional - absolute positioning)
- No window Z-ordering (render order = definition order in config)
- Mouse support depends on terminal emulator capabilities
- Very long lines (>2000 chars) are truncated when read from files
- Command input horizontal scrolling uses simple algorithm (not perfect edge cases)
