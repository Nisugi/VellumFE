# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**profanity-rs** is a modern, Rust-based terminal client for GemStone IV, built with Ratatui. It connects to Lich (Ruby scripting engine) via detached mode and provides a TUI with dynamic window management, mouse support, and XML stream parsing.

## Build and Development Commands

```bash
# Build for development
cargo build

# Run the application
cargo run

# Build for release
cargo build --release
# Binary located at: target/release/profanity-rs

# Enable debug logs
RUST_LOG=debug cargo run
# Logs written to ~/.profanity-rs/debug.log
```

## Running the Application

**Prerequisites:**
1. Start Lich in detached mode first (wait 5-10 seconds before launching profanity-rs)
2. Default connection: `localhost:8000`

**Windows (PowerShell):**
```powershell
C:\Ruby4Lich5\3.4.5\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

**Linux/Mac:**
```bash
ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

## Architecture

### High-Level Flow

1. **main.rs** → Initializes logging to `~/.profanity-rs/debug.log`, loads config, creates and runs App
2. **app.rs** → Main event loop handling terminal events, server messages, and UI rendering
3. **network.rs** → TCP connection to Lich server (async via tokio)
4. **parser.rs** → Parses GemStone IV XML protocol into structured elements
5. **ui/** → Window management, text rendering, progress bars, countdown timers

### Key Architecture Patterns

**Window Management:**
- Windows use **absolute positioning** (row, col, rows, cols) - each window is independent
- No grid layout - windows can overlap, have gaps, be moved/resized freely
- WindowManager maintains windows and stream routing mappings
- Three widget types: `text`, `progress`, `countdown`

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
- Terminal events polled with 100ms timeout
- Server messages processed via mpsc channel (non-blocking `try_recv`)
- UI redrawn every frame with window layouts recalculated
- Mouse drag operations (resize/move) track delta from last position

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

**Layout Persistence:**
- Window configs stored in `~/.profanity-rs/config.toml` (full config)
- Layouts stored in `~/.profanity-rs/layouts/<name>.toml` (just windows array)
- Autosave layout created on exit, loaded on startup if exists

**Mouse Operations:**
- Click title bar (top border, excluding corners) to move window
- Click edges/corners to resize (corners resize from that corner, edges resize one dimension)
- Title bar detection excludes corners (leaves 1 cell margin on each side)
- Resize/move use incremental deltas, not absolute positions

## Module Structure

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
- `Config` struct with connection, UI, presets, highlights, keybinds
- Window template definitions for all built-in window types
- Layout save/load functionality
- Default configurations

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

**Important XML Elements:**
- `<pushStream id='...'/>` / `<popStream/>` - Stream routing
- `<progressBar id='...' value='...' text='...'/>` - Vitals updates
- `<roundTime value='...'/>` / `<castTime value='...'/>` - Timers
- `<preset id='...'> ... </preset>` - Styled text sections
- `<prompt time='...'>...</prompt>` - Game prompts (colored per character)

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

## Configuration

**Config location:** `~/.profanity-rs/config.toml`
**Layouts location:** `~/.profanity-rs/layouts/<name>.toml`

### Important Config Sections

```toml
[connection]
host = "127.0.0.1"
port = 8000

[ui]
command_echo_color = "#ffffff"
mouse_mode_toggle_key = "F11"
countdown_icon = "\u{f0c8}"  # Nerd Font icon for countdown blocks (default)

[[ui.prompt_colors]]
character = "R"  # Roundtime indicator
color = "#ff0000"

[[presets]]
id = "speech"
fg = "#53a684"

[[ui.windows]]
name = "main"
widget_type = "text"  # or "progress" or "countdown"
streams = ["main"]
row = 0
col = 0
rows = 30
cols = 120
buffer_size = 10000
show_border = true
border_style = "single"  # or "double", "rounded", "thick", "none"
title = "Main"
```

## Dot Commands (Local, Not Sent to Game)

- `.quit` - Exit application
- `.createwindow <template>` - Create window from template
- `.customwindow <name> <stream1,stream2,...>` - Create custom window
- `.deletewindow <name>` - Delete window
- `.windows` / `.listwindows` - List active windows
- `.templates` - List available templates
- `.rename <window> <new title>` - Change window title
- `.border <window> <style> [color]` - Change border style/color
- `.setprogress <window> <current> <max>` - Update progress bar
- `.setbarcolor <window> <color> [bg_color]` - Change progress bar colors
- `.setcountdown <window> <seconds>` - Set countdown timer
- `.savelayout [name]` - Save current layout (default: "default")
- `.loadlayout [name]` - Load saved layout
- `.layouts` - List saved layouts

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

## Testing Tips

- Use `.setprogress health 50 100` to manually test progress bars
- Use `.setcountdown roundtime 5` to test countdown timers
- Check `~/.profanity-rs/debug.log` for tracing output
- Terminal size changes require layout recalculation (handled automatically)
- Mouse operations log to debug when RUST_LOG=debug

## Known Limitations

- Windows can overlap (intentional - absolute positioning)
- No window Z-ordering (render order = definition order in config)
- Mouse support depends on terminal emulator capabilities
- Very long lines (>2000 chars) are truncated when read from files
