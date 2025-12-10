# Project Structure

A guided tour of the VellumFE codebase.

## Directory Layout

```
vellum-fe/
├── Cargo.toml           # Project manifest
├── Cargo.lock           # Dependency lock file
├── CLAUDE.md            # Development notes
├── README.md            # Project readme
│
├── src/
│   ├── main.rs          # CLI entry point (clap)
│   ├── config.rs        # Configuration loading (TOML files)
│   ├── parser.rs        # Stormfront XML protocol parser
│   ├── network.rs       # TCP/TLS connections (Lich proxy or direct eAccess)
│   ├── performance.rs   # Performance metrics tracking
│   │
│   ├── core/            # Business logic layer (NO frontend imports)
│   │   ├── mod.rs
│   │   ├── app_core/    # Main application state
│   │   │   ├── mod.rs
│   │   │   ├── state.rs     # AppCore - central state manager
│   │   │   ├── layout.rs    # Window layout management
│   │   │   ├── keybinds.rs  # Keybind configuration
│   │   │   └── commands.rs  # Dot-command processing
│   │   ├── messages.rs      # Message processing pipeline
│   │   ├── input_router.rs  # Input routing logic
│   │   └── menu_actions.rs  # Core menu action handlers
│   │
│   ├── data/            # Pure data structures (NO frontend imports)
│   │   ├── mod.rs
│   │   ├── widget.rs    # Widget data types (TextSpan, ActiveEffect, etc.)
│   │   ├── ui_state.rs  # UI state (InputMode, PopupMenu, etc.)
│   │   └── window.rs    # Window state structures
│   │
│   └── frontend/        # User interface layer
│       ├── mod.rs       # Frontend trait definition
│       ├── common/      # Shared types (KeyCode, MouseEvent, color)
│       └── tui/         # Ratatui terminal UI (~60 modules)
│           ├── mod.rs           # TuiFrontend struct
│           ├── runtime.rs       # Main event loop
│           ├── frontend_impl.rs # Frontend trait implementation
│           │
│           │ # === Input Handling ===
│           ├── input.rs         # Keyboard/mouse event handling
│           ├── input_handlers.rs    # Extracted input handler methods
│           ├── crossterm_bridge.rs  # Input event translation
│           │
│           │ # === Widget Types ===
│           ├── text_window.rs
│           ├── tabbed_text_window.rs
│           ├── command_input.rs
│           ├── progress_bar.rs
│           ├── countdown.rs
│           ├── compass.rs
│           ├── hand.rs
│           ├── indicator.rs
│           ├── injury_doll.rs
│           ├── active_effects.rs
│           ├── room_window.rs
│           ├── inventory_window.rs
│           ├── spells_window.rs
│           ├── dashboard.rs
│           ├── targets.rs
│           ├── players.rs
│           ├── spacer.rs
│           ├── performance_stats.rs
│           │
│           │ # === Browsers & Editors ===
│           ├── window_editor.rs
│           ├── highlight_browser.rs
│           ├── highlight_form.rs
│           ├── keybind_browser.rs
│           ├── keybind_form.rs
│           ├── color_palette_browser.rs
│           ├── color_form.rs
│           ├── color_picker.rs
│           ├── uicolors_browser.rs
│           ├── spell_color_browser.rs
│           ├── spell_color_form.rs
│           ├── theme_browser.rs
│           ├── theme_editor.rs
│           ├── settings_editor.rs
│           ├── indicator_template_editor.rs
│           │
│           │ # === Infrastructure ===
│           ├── sync.rs          # Data synchronization
│           ├── widget_manager.rs    # Widget cache management
│           ├── widget_traits.rs     # Navigable, Selectable, etc.
│           ├── colors.rs        # Color parsing/resolution
│           ├── theme_cache.rs   # Theme caching
│           ├── menu_actions.rs  # TUI menu handlers
│           ├── menu_builders.rs # Menu construction
│           ├── popup_menu.rs    # Popup menu rendering
│           ├── search.rs        # Text search functionality
│           ├── scrollable_container.rs
│           ├── title_position.rs
│           └── resize.rs        # Resize debouncing
│
├── defaults/            # Embedded default configurations
│   ├── config.toml
│   ├── colors.toml
│   ├── highlights.toml
│   ├── keybinds.toml
│   └── layouts/
│       ├── layout.toml
│       ├── sidebar.toml
│       └── none.toml
│
├── book/                # mdbook documentation
│   ├── book.toml
│   └── src/
│       └── *.md
│
└── tests/               # Integration tests
    └── *.rs
```

## Module Organization

### Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    src/frontend/tui/                        │
│                                                             │
│    Terminal rendering, input handling, visual themes        │
│    Knows about: core, data                                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                        src/core/                            │
│                                                             │
│    Application state, business logic, event processing      │
│    Knows about: data                                        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                        src/data/                            │
│                                                             │
│    Data models, parsing, serialization                      │
│    Knows about: nothing (foundation layer)                  │
└─────────────────────────────────────────────────────────────┘
```

**Import rule**: Upper layers can import lower layers, not vice versa.

## Key Files

### Entry Point: `src/main.rs`

```rust
// Typical structure
fn main() {
    // Parse command line arguments
    let args = Args::parse();

    // Load configuration
    let config = Config::load(&args)?;

    // Initialize application
    let app = App::new(config)?;

    // Run main loop
    app.run()?;
}
```

Responsibilities:
- CLI argument parsing
- Configuration loading
- Application initialization
- Main event loop

### Configuration: `src/config.rs`

```rust
// Configuration structures
pub struct Config {
    pub connection: ConnectionConfig,
    pub layout: LayoutConfig,
    pub colors: ColorConfig,
    pub keybinds: KeybindConfig,
    // ...
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> { ... }
    pub fn save(&self, path: &Path) -> Result<()> { ... }
}
```

Responsibilities:
- TOML parsing
- Default values
- Validation
- Serialization

### Parser: `src/parser.rs`

```rust
// XML protocol parsing
pub enum ParsedElement {
    Text(String),
    RoomName(String),
    RoomDesc(String),
    Prompt(PromptData),
    Vitals(VitalsData),
    // Many more variants...
}

pub struct Parser {
    // Parser state
}

impl Parser {
    pub fn parse(&mut self, input: &str) -> Vec<ParsedElement> { ... }
}
```

Responsibilities:
- XML tag recognition
- State machine parsing
- Element extraction
- Stream identification

### Network: `src/network.rs`

```rust
// Connection handling
pub enum ConnectionMode {
    Lich { host: String, port: u16 },
    Direct { account: String, ... },
}

pub struct Connection {
    mode: ConnectionMode,
    stream: TcpStream,
}

impl Connection {
    pub fn connect(mode: ConnectionMode) -> Result<Self> { ... }
    pub fn send(&mut self, data: &str) -> Result<()> { ... }
    pub fn receive(&mut self) -> Result<String> { ... }
}
```

Responsibilities:
- Connection establishment
- TLS handling (direct mode)
- Data transmission
- Reconnection logic

### Widget Data: `src/data/widget.rs`

```rust
// Widget type definitions
pub enum WidgetType {
    Text,
    TabbedText,
    Progress,
    Compass,
    // ...
}

pub struct WidgetConfig {
    pub widget_type: WidgetType,
    pub name: String,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    // Type-specific config...
}
```

Responsibilities:
- Widget type enumeration
- Configuration structures
- Layout data
- Serialization traits

### Application State: `src/core/app_core/state.rs`

```rust
// Application state management
pub struct AppState {
    pub vitals: Vitals,
    pub room: RoomInfo,
    pub inventory: Inventory,
    pub indicators: Indicators,
    // ...
    generation: u64,  // Change tracking
}

impl AppState {
    pub fn update(&mut self, element: ParsedElement) {
        // Update state based on parsed element
        self.generation += 1;
    }
}
```

Responsibilities:
- Game state tracking
- Update processing
- Change notification
- Generation counting

### Layout: `src/core/app_core/layout.rs`

```rust
// Layout management
pub struct Layout {
    widgets: Vec<Widget>,
    focus_index: usize,
}

impl Layout {
    pub fn load(config: &LayoutConfig) -> Result<Self> { ... }
    pub fn render(&self, frame: &mut Frame) { ... }
    pub fn handle_input(&mut self, key: KeyEvent) { ... }
}
```

Responsibilities:
- Widget arrangement
- Focus management
- Layout loading
- Coordinate calculation

### TUI Module: `src/frontend/tui/mod.rs`

```rust
// Terminal UI main module
pub struct Tui {
    terminal: Terminal<Backend>,
    layout: Layout,
    state: AppState,
}

impl Tui {
    pub fn new() -> Result<Self> { ... }
    pub fn run(&mut self) -> Result<()> { ... }
    pub fn render(&mut self) -> Result<()> { ... }
    pub fn handle_event(&mut self, event: Event) -> Result<()> { ... }
}
```

Responsibilities:
- Terminal setup/teardown
- Event loop
- Rendering coordination
- Input routing

## Data Flow

### Incoming Data

```
Network → Parser → State Update → Widget Sync → Render
   │         │           │              │           │
   │         │           │              │           └─ TUI draws frame
   │         │           │              └─ Widgets check generation
   │         │           └─ AppState updates, bumps generation
   │         └─ Raw XML → ParsedElements
   └─ TCP/TLS receives bytes
```

### User Input

```
Terminal Event → TUI → Handler → Action → Effect
      │           │        │        │         │
      │           │        │        │         └─ State change or command send
      │           │        │        └─ Keybind lookup / macro expansion
      │           │        └─ Input type routing (key/mouse/resize)
      │           └─ Event loop captures
      └─ User presses key
```

## Testing Structure

```
tests/
├── integration_test.rs   # Full application tests
├── parser_tests.rs       # Parser-specific tests
└── widget_tests.rs       # Widget rendering tests

src/
├── parser.rs
│   └── #[cfg(test)] mod tests { ... }  # Unit tests
└── config.rs
    └── #[cfg(test)] mod tests { ... }  # Unit tests
```

## Configuration Files

User configuration (runtime):

```
~/.vellum-fe/
├── config.toml
├── layout.toml
├── colors.toml
├── highlights.toml
├── keybinds.toml
├── triggers.toml
└── simu.pem
```

## Documentation

```
book/                    # mdbook documentation (this!)
├── book.toml
└── src/
    ├── SUMMARY.md
    └── *.md

docs/                    # Additional docs
└── *.md
```

## Build Artifacts

```
target/
├── debug/               # Debug build
│   └── vellum-fe
├── release/             # Release build
│   └── vellum-fe
└── doc/                 # Generated documentation
    └── vellum_fe/
```

## See Also

- [Architecture](../architecture/README.md) - System design
- [Adding Widgets](./adding-widgets.md) - Extend widget system
- [Parser Extensions](./parser-extensions.md) - Extend parser

