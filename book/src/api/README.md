# API Documentation

Programmatic interface documentation for VellumFE.

## Rust API Documentation

VellumFE's internal API is documented using Rust's built-in documentation system.

### Viewing Rustdoc

Generate and view the API documentation:

```bash
# Generate documentation
cargo doc --no-deps

# Open in browser
cargo doc --no-deps --open

# Include private items
cargo doc --no-deps --document-private-items
```

### Online Documentation

When available, API documentation is hosted at:

- **Stable**: [docs.rs/vellum-fe](https://docs.rs/vellum-fe) (when published)
- **Development**: Generated from source

## Module Overview

VellumFE is organized into three main layers:

### Data Layer (`src/data/`)

Core data structures and game state.

```rust
// Key types
pub struct GameState { ... }
pub struct ParsedElement { ... }
pub enum StreamId { ... }
pub struct Vitals { ... }
```

**Key modules**:
- `data::game_state` - Central state management
- `data::parsed` - Parser output types
- `data::vitals` - Health/mana/stamina
- `data::room` - Room information

### Core Layer (`src/core/`)

Business logic and widget management.

```rust
// Key types
pub struct WidgetManager { ... }
pub struct AppState { ... }
pub trait Widget { ... }
```

**Key modules**:
- `core::widget_manager` - Widget lifecycle
- `core::app_state` - Application state
- `core::sync` - State synchronization
- `core::layout` - Layout management

### Frontend Layer (`src/frontend/`)

User interface and rendering.

```rust
// Key types
pub struct TuiApp { ... }
pub struct InputHandler { ... }
```

**Key modules**:
- `frontend::tui` - Terminal UI
- `frontend::tui::input` - Input handling
- `frontend::tui::render` - Rendering

## Key Traits

### Widget Trait

All widgets implement the core widget trait:

```rust
pub trait Widget {
    /// Widget name/identifier
    fn name(&self) -> &str;

    /// Widget type
    fn widget_type(&self) -> WidgetType;

    /// Render the widget to a frame region
    fn render(&self, frame: &mut Frame, area: Rect);

    /// Handle input events
    fn handle_input(&mut self, event: KeyEvent) -> Option<WidgetAction>;

    /// Update from game state
    fn update(&mut self, state: &GameState) -> bool;
}
```

### Browser Trait

Popup windows implement:

```rust
pub trait Browser {
    /// Browser title
    fn title(&self) -> &str;

    /// Render browser content
    fn render(&self, frame: &mut Frame, area: Rect);

    /// Handle input
    fn handle_input(&mut self, event: KeyEvent) -> BrowserAction;

    /// Initialize with game state
    fn init(&mut self, state: &GameState);
}
```

## Configuration API

### Loading Configuration

```rust
use vellum_fe::config::{Config, Layout, Theme};

// Load from default location
let config = Config::load()?;

// Load from specific path
let config = Config::from_file("path/to/config.toml")?;

// Access settings
let port = config.connection.port;
let render_rate = config.performance.render_rate;
```

### Layout Configuration

```rust
use vellum_fe::config::Layout;

let layout = Layout::load("layout.toml")?;

for widget_config in layout.widgets {
    println!("Widget: {} at ({}, {})",
             widget_config.name,
             widget_config.x,
             widget_config.y);
}
```

## Parser API

### Parsing Game Data

```rust
use vellum_fe::parser::Parser;

let mut parser = Parser::new();

// Feed data from game
let elements = parser.parse(raw_data)?;

// Process parsed elements
for element in elements {
    match element {
        ParsedElement::Text(text) => { /* ... */ }
        ParsedElement::Prompt(time) => { /* ... */ }
        ParsedElement::Vitals(vitals) => { /* ... */ }
        // ... other variants
    }
}
```

### ParsedElement Variants

```rust
pub enum ParsedElement {
    Text(String),
    Prompt(u64),
    StreamPush(StreamId),
    StreamPop,
    StreamClear(StreamId),
    Vitals(VitalUpdate),
    Roundtime(u64),
    Casttime(u64),
    Compass(Vec<Direction>),
    Indicator(String, bool),
    Component(String, String),
    // ... and more
}
```

## Network API

### Lich Connection

```rust
use vellum_fe::network::LichConnection;

let conn = LichConnection::new("127.0.0.1", 8000)?;
conn.connect()?;

// Send command
conn.send("look")?;

// Receive data
let data = conn.receive()?;
```

### Direct Connection

```rust
use vellum_fe::network::DirectConnection;

let conn = DirectConnection::new(
    "account",
    "password",
    "prime",
    "character_name"
)?;

conn.authenticate()?;
conn.connect()?;
```

## Event System

### Input Events

```rust
use vellum_fe::events::{Event, KeyEvent};

loop {
    match event_receiver.recv()? {
        Event::Key(key_event) => {
            // Handle keyboard input
        }
        Event::Mouse(mouse_event) => {
            // Handle mouse input
        }
        Event::Resize(w, h) => {
            // Handle terminal resize
        }
        Event::GameData(data) => {
            // Handle incoming game data
        }
    }
}
```

### Widget Actions

```rust
pub enum WidgetAction {
    None,
    SendCommand(String),
    OpenBrowser(BrowserType),
    CloseBrowser,
    FocusWidget(String),
    Scroll(ScrollDirection),
    Quit,
}
```

## Building Extensions

### Adding a Widget Type

```rust
use vellum_fe::widgets::{Widget, WidgetType};

pub struct CustomWidget {
    name: String,
    // Custom fields
}

impl Widget for CustomWidget {
    fn name(&self) -> &str {
        &self.name
    }

    fn widget_type(&self) -> WidgetType {
        WidgetType::Custom
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Render implementation
    }

    fn handle_input(&mut self, event: KeyEvent) -> Option<WidgetAction> {
        // Input handling
        None
    }

    fn update(&mut self, state: &GameState) -> bool {
        // Update from state, return true if changed
        false
    }
}
```

### Adding a Parser Extension

```rust
use vellum_fe::parser::{Parser, ParsedElement};

impl Parser {
    pub fn handle_custom_tag(&mut self, tag: &str, attrs: &[Attribute])
        -> Option<ParsedElement>
    {
        if tag == "customtag" {
            // Parse custom tag
            Some(ParsedElement::Custom(/* ... */))
        } else {
            None
        }
    }
}
```

## Error Handling

VellumFE uses the `anyhow` crate for error handling:

```rust
use anyhow::{Result, Context};

fn load_config() -> Result<Config> {
    let content = std::fs::read_to_string("config.toml")
        .context("Failed to read config file")?;

    let config: Config = toml::from_str(&content)
        .context("Failed to parse config")?;

    Ok(config)
}
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_basic() {
        let mut parser = Parser::new();
        let result = parser.parse(b"<prompt>&gt;</prompt>").unwrap();
        assert!(matches!(result[0], ParsedElement::Prompt(_)));
    }
}
```

### Integration Tests

```rust
// tests/integration_test.rs
use vellum_fe::*;

#[test]
fn test_full_workflow() {
    // Setup
    let config = Config::default();
    let mut app = App::new(config);

    // Simulate game data
    app.process_data(test_data());

    // Verify state
    assert!(app.game_state().vitals().health > 0);
}
```

## Version Compatibility

API stability follows semantic versioning:

- **0.x.y**: API may change between minor versions
- **1.x.y**: Breaking changes only in major versions

## See Also

- [Project Structure](../development/project-structure.md) - Code organization
- [Adding Widgets](../development/adding-widgets.md) - Widget development
- [Parser Extensions](../development/parser-extensions.md) - Parser development
- [Contributing](../development/contributing.md) - Contribution guide

