# Development Guide

This guide is for developers who want to contribute to profanity-rs or understand its internals.

## Table of Contents

- [Project Overview](#project-overview)
- [Project Structure](#project-structure)
- [Architecture](#architecture)
- [Key Modules](#key-modules)
- [Development Workflow](#development-workflow)
- [Adding Features](#adding-features)
- [Testing](#testing)
- [Debugging](#debugging)
- [Contributing](#contributing)

---

## Project Overview

**profanity-rs** is a modern, Rust-based terminal client for GemStone IV, built with [Ratatui](https://github.com/ratatui-org/ratatui). It connects to the [Lich scripting engine](https://github.com/elanthia-online/lich-5) via detached mode and provides a terminal user interface (TUI) with dynamic window management, mouse support, and XML stream parsing.

### Key Technologies

- **Language:** Rust 1.70+
- **TUI Framework:** Ratatui
- **Async Runtime:** Tokio
- **Terminal Backend:** Crossterm
- **Configuration:** TOML (via serde)

### Design Goals

1. **Performance:** Fast rendering, minimal CPU/memory usage
2. **Flexibility:** Fully customizable window layouts
3. **Reliability:** Robust XML parsing, graceful error handling
4. **Usability:** Mouse support, intuitive commands
5. **Extensibility:** Easy to add new widgets and features

---

## Project Structure

```
profanity-rs/
├── src/
│   ├── main.rs                 # Entry point, logging setup
│   ├── app.rs                  # Main application loop and state
│   ├── config.rs               # Configuration management
│   ├── network.rs              # TCP connection to Lich
│   ├── parser.rs               # XML protocol parser
│   ├── performance.rs          # Performance monitoring
│   └── ui/
│       ├── mod.rs              # UI module exports
│       ├── window_manager.rs   # Window management and layout
│       ├── text_window.rs      # Text display widget
│       ├── progress_bar.rs     # Progress bar widget
│       ├── countdown.rs        # Countdown timer widget
│       ├── compass.rs          # Compass widget
│       ├── injury_doll.rs      # Injury tracking widget
│       ├── indicator.rs        # Status indicator widget
│       ├── dashboard.rs        # Dashboard of indicators
│       ├── active_effects.rs   # Active spells/effects widget
│       ├── hand.rs             # Single hand display
│       ├── hands.rs            # Both hands display
│       ├── command_input.rs    # Command input widget
│       ├── performance_stats.rs # Performance stats display
│       └── scrollable_container.rs # Scrollable container widget
├── Cargo.toml                  # Rust dependencies
├── config.toml                 # Example configuration
├── README.md                   # Project overview
├── CLAUDE.md                   # Claude AI development guide
└── TODO.md                     # Feature roadmap
```

### File Responsibilities

| File | Lines | Responsibility |
|------|-------|----------------|
| `main.rs` | ~100 | Initialize logging, load config, start app |
| `app.rs` | ~1500 | Event loop, input handling, dot commands |
| `config.rs` | ~800 | Config file I/O, window templates |
| `network.rs` | ~150 | Async TCP client for Lich connection |
| `parser.rs` | ~600 | XML parsing state machine |
| `window_manager.rs` | ~800 | Window lifecycle, stream routing |
| `text_window.rs` | ~400 | Text buffer, wrapping, scrolling |
| `progress_bar.rs` | ~200 | Progress bar rendering |
| `countdown.rs` | ~150 | Countdown timer logic and rendering |

---

## Architecture

### High-Level Flow

```
┌──────────┐
│ main.rs  │  Initialize logging, load config
└────┬─────┘
     │
     ▼
┌──────────┐
│  app.rs  │  Create App, start event loop
└────┬─────┘
     │
     ├─────────────────────────────────────┐
     ▼                                     ▼
┌─────────────┐                   ┌──────────────┐
│ network.rs  │                   │  Terminal    │
│ (Tokio TCP) │                   │   Events     │
└──────┬──────┘                   └──────┬───────┘
       │                                 │
       │ ServerMessage                   │ Event
       │ via mpsc                        │ (Key/Mouse)
       ▼                                 ▼
┌──────────────────────────────────────────────┐
│              App Event Loop                  │
│                                              │
│  ┌─────────────┐      ┌──────────────┐      │
│  │  parser.rs  │─────▶│ window_mgr   │      │
│  │ (XML parse) │      │ (routing)    │      │
│  └─────────────┘      └──────────────┘      │
│                                              │
│  ┌──────────────────────────────────┐       │
│  │   Render UI (Ratatui)            │       │
│  │   - Text windows                 │       │
│  │   - Progress bars                │       │
│  │   - Countdown timers             │       │
│  │   - etc.                         │       │
│  └──────────────────────────────────┘       │
└──────────────────────────────────────────────┘
```

### Event Loop (app.rs)

```rust
loop {
    // 1. Poll terminal events (keyboard, mouse) with 100ms timeout
    if event::poll(Duration::from_millis(100))? {
        match event::read()? {
            Event::Key(key) => /* handle keyboard */,
            Event::Mouse(mouse) => /* handle mouse */,
            Event::Resize(w, h) => /* handle resize */,
        }
    }

    // 2. Process server messages (non-blocking)
    while let Ok(msg) = rx.try_recv() {
        match msg {
            ServerMessage::Text(line) => {
                // Parse XML
                let elements = parser.parse_line(&line);

                // Route to windows
                for element in elements {
                    window_manager.process_element(element);
                }
            }
            // ...
        }
    }

    // 3. Render UI
    terminal.draw(|f| {
        window_manager.render(f);
        command_input.render(f);
    })?;
}
```

### Window Management

Windows use **absolute positioning** (row, col, rows, cols):

```toml
[[ui.windows]]
name = "main"
row = 0     # Top row (0-indexed)
col = 0     # Left column (0-indexed)
rows = 30   # Height in rows
cols = 100  # Width in columns
```

- No grid layout - windows can overlap or have gaps
- Each window is independent
- WindowManager converts configs to Ratatui `Rect` positions
- Layout recalculated every frame (cheap)

### Stream Routing

```
Game Output ─┐
             │
             ▼
      ┌─────────────┐
      │  Parser     │
      │             │
      │ <pushStream │ ──┐
      │  id='X'/>   │   │ Switch to stream X
      │             │   │
      │ Text...     │ ──┼─▶ Current stream: X
      │             │   │
      │ <popStream/>│ ──┘ Pop back to previous stream
      └─────────────┘
             │
             ▼
      ┌─────────────────┐
      │ WindowManager   │
      │ stream_map:     │
      │   "main" -> "main_window"
      │   "thoughts" -> "thought_window"
      │   "loot" -> "loot_window"
      └─────────────────┘
             │
             ▼
       Route text to appropriate window
```

**Key insights:**
- Parser maintains stream stack
- WindowManager maintains stream → window mapping
- Multiple windows can subscribe to same stream (last one wins)
- Multiple streams can route to same window

### XML Parsing State Machine

The parser maintains stacks for nested XML tags:

```rust
pub struct XmlParser {
    color_stack: Vec<Option<String>>,      // Current color
    preset_stack: Vec<String>,             // Current preset ID
    style_stack: Vec<String>,              // Current style
    bold_stack: Vec<bool>,                 // Bold state
    text_buffer: String,                   // Accumulating text
    current_preset_color: Option<String>,  // Color from preset
    current_color: Option<String>,         // Explicit color
    // ...
}
```

**Parsing rules:**
1. Flush text buffer when opening/closing color tags
2. Maintain nested tag state on stacks
3. Decode HTML entities (`&gt;` → `>`)
4. Handle self-closing tags (`<roundTime value='5'/>`)
5. Emit structured `ParsedElement` events

---

## Key Modules

### src/main.rs

**Responsibilities:**
- Initialize tracing (debug logs to `~/.profanity-rs/debug.log`)
- Load configuration from `~/.profanity-rs/config.toml`
- Create and run `App`
- Save autosave layout on exit

**Key functions:**
- `main()` - Entry point

---

### src/app.rs

**Responsibilities:**
- Main event loop
- Terminal event handling (keyboard, mouse, resize)
- Server message processing
- Dot command parsing and execution
- Window focus management
- Mouse drag operations (resize, move)

**Key state:**
```rust
pub struct App {
    config: Config,
    window_manager: WindowManager,
    parser: XmlParser,
    current_stream: String,
    focused_window_index: usize,
    mouse_mode_enabled: bool,
    resize_state: Option<ResizeState>,
    move_state: Option<MoveState>,
    // ...
}
```

**Key functions:**
- `run()` - Main event loop
- `handle_key_event()` - Process keyboard input
- `handle_mouse_event()` - Process mouse input
- `handle_dot_command()` - Execute dot commands
- `handle_server_message()` - Process game server messages

---

### src/config.rs

**Responsibilities:**
- Load/save configuration files
- Define window templates
- Manage layouts
- Provide default configurations

**Key types:**
```rust
pub struct Config {
    pub connection: ConnectionConfig,
    pub ui: UiConfig,
    pub presets: Vec<PresetDef>,
    pub highlights: Vec<HighlightDef>,
    pub keybinds: Vec<KeybindDef>,
}

pub struct WindowDef {
    pub name: String,
    pub widget_type: String,
    pub streams: Vec<String>,
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
    // ...
}
```

**Key functions:**
- `load()` - Load config from file
- `save()` - Save config to file
- `get_window_template()` - Get built-in template by name
- `save_layout()` / `load_layout()` - Layout persistence

---

### src/network.rs

**Responsibilities:**
- Async TCP connection to Lich server
- Send commands to game
- Receive game output
- Connection state management

**Key types:**
```rust
pub enum ServerMessage {
    Connected,
    Disconnected,
    Text(String),
}

pub struct LichConnection;
```

**Key functions:**
- `start()` - Spawn reader/writer tasks
- Reader task: Reads lines from TCP socket, sends `ServerMessage::Text`
- Writer task: Receives commands via channel, writes to TCP socket

---

### src/parser.rs

**Responsibilities:**
- Parse GemStone IV XML protocol
- Maintain color/style state stacks
- Emit structured parse events

**Key types:**
```rust
pub enum ParsedElement {
    Text(StyledText),
    StreamPush(String),
    StreamPop,
    ProgressBar { id: String, current: u32, max: u32, text: Option<String> },
    RoundTime(u64),
    CastTime(u64),
    Prompt,
    // ...
}

pub struct XmlParser {
    // State stacks
    color_stack: Vec<Option<String>>,
    preset_stack: Vec<String>,
    // ...
}
```

**Key functions:**
- `parse_line()` - Parse one line of XML, return `Vec<ParsedElement>`
- `process_tag()` - Route tag to appropriate handler
- `handle_color_tag()` - Process `<d>` color tags
- `handle_preset_tag()` - Process `<preset>` tags
- `handle_progress_bar()` - Process `<progressBar>` tags
- `decode_entities()` - Decode HTML entities

**Important XML tags:**
- `<pushStream id='X'/>` - Switch to stream X
- `<popStream/>` - Return to previous stream
- `<d cmd='...'><preset id='Y'>Text</preset></d>` - Styled text
- `<progressBar id='health' value='150' text='max:350'/>` - Progress update
- `<roundTime value='1234567890'/>` - Roundtime timer (Unix timestamp)
- `<prompt>...</prompt>` - Game prompt

---

### src/ui/window_manager.rs

**Responsibilities:**
- Manage window lifecycle
- Calculate window layouts
- Route streams to windows
- Widget creation and rendering

**Key types:**
```rust
pub struct WindowManager {
    pub widgets: Vec<Box<dyn Widget>>,
    pub stream_map: HashMap<String, String>, // stream -> window name
    configs: Vec<WindowConfig>,
    // ...
}
```

**Key functions:**
- `new()` - Create from configs
- `calculate_layout()` - Convert configs to Ratatui `Rect`s
- `get_window()` - Get widget by name
- `add_text_to_stream()` - Route text to window by stream
- `render()` - Render all windows

---

### src/ui/text_window.rs

**Responsibilities:**
- Display styled text
- Text wrapping
- Scrollback buffer
- Scrolling

**Key types:**
```rust
pub struct TextWindow {
    lines: Vec<Vec<StyledText>>,  // Rendered lines
    line_buffer: Vec<StyledText>,  // Current line being built
    scroll_offset: usize,
    buffer_size: usize,
    inner_width: usize,
    // ...
}
```

**Key functions:**
- `add_text()` - Add styled text to current line
- `finish_line()` - Wrap and commit current line to buffer
- `scroll_up()` / `scroll_down()` - Navigate scrollback
- `wrap_line()` - Wrap long lines to window width

**Text wrapping algorithm:**
1. Accumulate styled text segments in `line_buffer`
2. On `finish_line()`, concatenate all segments
3. Split by `inner_width` (window width minus border)
4. Preserve styling across wrap points
5. Add wrapped lines to `lines` buffer
6. Trim old lines if buffer exceeds `buffer_size`

---

### src/ui/progress_bar.rs

**Responsibilities:**
- Render progress bars
- Auto-update from `<progressBar>` tags
- Custom text display

**Key types:**
```rust
pub struct ProgressBar {
    current: u32,
    max: u32,
    text: Option<String>,
    bar_color: Color,
    bar_background_color: Color,
    // ...
}
```

**Rendering:**
- ProfanityFE-style: background color fills from left
- Text shows either `current/max` or custom text
- Special handling: encumbrance changes color by value

---

### src/ui/countdown.rs

**Responsibilities:**
- Render countdown timers
- Auto-update from `<roundTime>` / `<castTime>` tags

**Key types:**
```rust
pub struct Countdown {
    end_time: u64,  // Unix timestamp
    color: Color,
    // ...
}
```

**Rendering:**
- Character-based fill: fills N chars where N = remaining seconds
- Centered text shows remaining seconds
- Auto-counts down using system time

---

## Development Workflow

### Setup

```bash
# Clone repository
git clone https://github.com/yourusername/profanity-rs.git
cd profanity-rs

# Build
cargo build

# Run (development)
cargo run

# Run with debug logs
RUST_LOG=debug cargo run
# Logs: ~/.profanity-rs/debug.log

# Build release
cargo build --release
# Binary: target/release/profanity-rs
```

### Code Style

- Follow Rust standard style (use `rustfmt`)
- Use `clippy` for linting: `cargo clippy`
- Document public APIs with doc comments
- Keep functions focused and small
- Prefer explicit over implicit

**Format code:**
```bash
cargo fmt
```

**Lint code:**
```bash
cargo clippy
```

### Git Workflow

1. Create a feature branch: `git checkout -b feature/my-feature`
2. Make changes
3. Commit with descriptive messages
4. Push to GitHub: `git push origin feature/my-feature`
5. Create pull request

**Commit message format:**
```
Add feature: Brief description

Longer description if needed.
Explain why, not what (code shows what).

Fixes #123
```

---

## Adding Features

### Adding a New Window Template

**Example: Add a "skills" window**

1. **Add template in `config.rs`:**

```rust
pub fn get_window_template(name: &str) -> Option<WindowDef> {
    match name {
        // ... existing templates ...
        "skills" => Some(WindowDef {
            name: "skills".to_string(),
            widget_type: "text".to_string(),
            streams: vec!["skills".to_string()],
            row: 0,
            col: 120,
            rows: 20,
            cols: 40,
            buffer_size: 1000,
            show_border: true,
            border_style: Some("rounded".to_string()),
            title: Some("Skills".to_string()),
            // ... other fields ...
        }),
        _ => None,
    }
}
```

2. **Add to template list:**

```rust
pub fn available_window_templates() -> Vec<String> {
    vec![
        // ... existing templates ...
        "skills".to_string(),
    ]
}
```

3. **Test:**
```
.createwindow skills
```

---

### Adding a New XML Tag Handler

**Example: Handle `<skill>` tags**

1. **Add variant to `ParsedElement` enum in `parser.rs`:**

```rust
pub enum ParsedElement {
    // ... existing variants ...
    Skill { name: String, ranks: u32, percent: u32 },
}
```

2. **Add handler method:**

```rust
impl XmlParser {
    fn handle_skill_tag(&mut self, tag: &str) -> Vec<ParsedElement> {
        // Parse attributes
        let name = extract_attr(tag, "name");
        let ranks = extract_attr(tag, "ranks").parse().unwrap_or(0);
        let percent = extract_attr(tag, "percent").parse().unwrap_or(0);

        vec![ParsedElement::Skill { name, ranks, percent }]
    }
}
```

3. **Call from `process_tag()`:**

```rust
fn process_tag(&mut self, tag: &str) -> Vec<ParsedElement> {
    if tag.starts_with("<skill ") {
        return self.handle_skill_tag(tag);
    }
    // ... existing handlers ...
}
```

4. **Handle in `app.rs`:**

```rust
fn handle_server_message(&mut self, msg: ServerMessage) {
    // ... existing code ...
    for element in elements {
        match element {
            ParsedElement::Skill { name, ranks, percent } => {
                // Update skills widget
                if let Some(window) = self.window_manager.get_window("skills") {
                    window.add_skill(name, ranks, percent);
                }
            }
            // ... existing handlers ...
        }
    }
}
```

---

### Adding a New Dot Command

**Example: Add `.clearwindow` command**

1. **Add case in `app.rs::handle_dot_command()`:**

```rust
fn handle_dot_command(&mut self, command: &str) {
    let parts: Vec<&str> = command[1..].split_whitespace().collect();

    match parts[0] {
        // ... existing commands ...
        "clearwindow" | "clear" => {
            if parts.len() < 2 {
                self.add_system_message("Usage: .clearwindow <window_name>");
                return;
            }

            let window_name = parts[1];
            if let Some(window) = self.window_manager.get_window(window_name) {
                window.clear();
                self.add_system_message(&format!("Cleared window '{}'", window_name));
            } else {
                self.add_system_message(&format!("Window '{}' not found", window_name));
            }
        }
        _ => {
            self.add_system_message(&format!("Unknown command: {}", command));
        }
    }
}
```

2. **Test:**
```
.clearwindow main
```

---

### Adding a New Widget Type

**Example: Add a "map" widget**

1. **Create `src/ui/map.rs`:**

```rust
use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget as RatatuiWidget};
use crate::ui::{Widget, StyledText};

pub struct Map {
    // ... fields ...
}

impl Map {
    pub fn new() -> Self {
        Self {
            // ... initialization ...
        }
    }

    pub fn set_room(&mut self, room_id: u32) {
        // ... update map ...
    }
}

impl Widget for Map {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        // ... render map ...
    }

    // ... implement other Widget methods ...
}
```

2. **Add to `ui/mod.rs`:**

```rust
mod map;
pub use map::Map;
```

3. **Add to `WindowManager::create_widget()`:**

```rust
fn create_widget(&self, config: &WindowConfig) -> Box<dyn Widget> {
    match config.widget_type.as_str() {
        // ... existing types ...
        "map" => Box::new(Map::new()),
        _ => Box::new(TextWindow::new(/* ... */)),
    }
}
```

4. **Add template in `config.rs`** (as shown earlier)

5. **Handle updates in `app.rs`** (parse XML, update widget)

---

## Testing

### Manual Testing

Use debug commands to test widgets:

```
.randomprogress      # Test progress bars
.randomcountdowns    # Test countdown timers
.randomcompass       # Test compass
.randominjuries      # Test injury doll
.indicatoron         # Test indicators
```

### Unit Tests

**Example: Test parser**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_tag() {
        let mut parser = XmlParser::new(vec![], HashMap::new());
        let elements = parser.parse_line("<d cmd='look'><preset id='speech'>Hello</preset></d>");

        assert_eq!(elements.len(), 1);
        match &elements[0] {
            ParsedElement::Text(text) => {
                assert_eq!(text.content, "Hello");
                // ... check color ...
            }
            _ => panic!("Expected Text element"),
        }
    }
}
```

Run tests:
```bash
cargo test
```

### Integration Testing

1. Start Lich in detached mode
2. Launch profanity-rs
3. Perform in-game actions
4. Verify UI updates correctly
5. Check debug logs for errors

---

## Debugging

### Enable Debug Logs

```bash
RUST_LOG=debug cargo run
```

Logs go to: `~/.profanity-rs/debug.log`

### View Logs in Real-Time

**Linux/Mac:**
```bash
tail -f ~/.profanity-rs/debug.log
```

**Windows (PowerShell):**
```powershell
Get-Content ~/.profanity-rs/debug.log -Tail 50 -Wait
```

### Add Debug Logging

```rust
use tracing::{debug, info, warn, error};

debug!("Processing tag: {}", tag);
info!("Window created: {}", name);
warn!("Unknown stream: {}", stream);
error!("Failed to parse: {}", err);
```

### Common Debug Tasks

**Check stream routing:**
```rust
debug!("Current stream: {}", self.current_stream);
debug!("Stream map: {:?}", self.window_manager.stream_map);
```

**Check XML parsing:**
```rust
debug!("Parsed elements: {:?}", elements);
```

**Check window state:**
```rust
debug!("Window count: {}", self.window_manager.widgets.len());
debug!("Focused window: {}", self.focused_window_index);
```

### Use Debugger

**VS Code + rust-analyzer:**

1. Install CodeLLDB extension
2. Add launch configuration:
```json
{
    "version": "0.2.0",
    "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug profanity-rs",
            "cargo": {
                "args": ["build", "--bin=profanity-rs"]
            },
            "args": [],
            "cwd": "${workspaceFolder}"
        }
    ]
}
```
3. Set breakpoints
4. Press F5 to debug

---

## Contributing

### Before You Start

1. Check existing issues: https://github.com/yourusername/profanity-rs/issues
2. Discuss major changes in an issue first
3. Read this development guide
4. Set up development environment

### Pull Request Process

1. **Fork the repository**
2. **Create a feature branch:** `git checkout -b feature/my-feature`
3. **Make your changes**
4. **Add tests** if applicable
5. **Run tests:** `cargo test`
6. **Format code:** `cargo fmt`
7. **Lint code:** `cargo clippy`
8. **Commit changes** with clear messages
9. **Push to your fork:** `git push origin feature/my-feature`
10. **Create pull request** on GitHub

### PR Checklist

- [ ] Code compiles without warnings
- [ ] Tests pass
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Documentation updated if needed
- [ ] CHANGELOG updated (if applicable)
- [ ] Commit messages are clear
- [ ] PR description explains changes

### Code Review

- Be patient - reviews may take a few days
- Respond to feedback constructively
- Make requested changes
- Keep discussion focused on the code

### After Merge

- Delete your feature branch
- Pull latest main: `git checkout main && git pull`
- Celebrate!

---

## Useful Resources

### Rust

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Rust Standard Library](https://doc.rust-lang.org/std/)

### Ratatui

- [Ratatui Documentation](https://ratatui.rs/)
- [Ratatui Examples](https://github.com/ratatui-org/ratatui/tree/main/examples)
- [Ratatui Book](https://ratatui.rs/tutorial/)

### GemStone IV

- [GemStone IV](https://www.play.net/gs4/)
- [Lich Scripting Engine](https://github.com/elanthia-online/lich-5)
- [GemStone IV XML Protocol](https://gswiki.play.net/XML_protocol)

---

[← Previous: Troubleshooting](Troubleshooting.md) | [Next: Feature Roadmap →](Feature-Roadmap.md)
