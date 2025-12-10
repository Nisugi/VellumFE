# Adding Widgets

Guide to creating new widget types for VellumFE.

## Overview

Widgets are the visual building blocks of VellumFE. Each widget type:

- Displays specific data
- Has its own rendering logic
- Responds to state changes
- May handle user input

## Widget Architecture

### Widget Lifecycle

```
Configuration → Creation → State Sync → Render → Input (optional)
     │             │            │          │           │
     │             │            │          │           └─ Handle focus/keys
     │             │            │          └─ Draw to terminal frame
     │             │            └─ Check generation, update data
     │             └─ Instantiate from config
     └─ TOML defines type and properties
```

### Key Traits

Widgets may implement these traits from `widget_traits.rs`:

```rust
/// Navigation within a widget (scrolling, selection movement)
pub trait Navigable {
    fn navigate_up(&mut self);
    fn navigate_down(&mut self);
    fn page_up(&mut self);
    fn page_down(&mut self);
    fn home(&mut self) {}
    fn end(&mut self) {}
}

/// Item selection and deletion (for browsers/lists)
pub trait Selectable {
    fn get_selected(&self) -> Option<String>;
    fn delete_selected(&mut self) -> Option<String>;
}

/// Text editing capabilities (for forms with TextArea fields)
pub trait TextEditable {
    fn get_focused_field(&self) -> Option<&TextArea<'static>>;
    fn get_focused_field_mut(&mut self) -> Option<&mut TextArea<'static>>;
    fn select_all(&mut self);
    fn copy_to_clipboard(&self) -> Result<()>;
    fn cut_to_clipboard(&mut self) -> Result<()>;
    fn paste_from_clipboard(&mut self) -> Result<()>;
}

/// Toggle behavior for checkboxes/boolean fields
pub trait Toggleable {
    fn toggle_focused(&mut self) -> Option<bool>;
}
```

**Important**: VellumFE does **not** use a unified `Widget` trait. Instead:
- Rendering is pattern-matched by `WindowContent` enum variant in `frontend_impl.rs`
- Synchronization is handled by dedicated `sync_*` functions in `sync.rs`
- Widget caches are stored in `WidgetManager` by type

## Step-by-Step: New Widget

Let's create a "SpellTimer" widget that displays active spell durations.

### Step 1: Define the Widget Type

Add to `src/data/widget.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WidgetType {
    Text,
    Progress,
    Compass,
    // ... existing types ...
    SpellTimer,  // Add new type
}
```

### Step 2: Create Widget Configuration

```rust
// In src/data/widget.rs or dedicated file

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpellTimerConfig {
    pub name: String,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,

    // Widget-specific options
    pub show_duration: bool,
    pub group_by_circle: bool,
    pub max_display: usize,
}

impl Default for SpellTimerConfig {
    fn default() -> Self {
        Self {
            name: "spell_timer".into(),
            x: 0,
            y: 0,
            width: 20,
            height: 10,
            show_duration: true,
            group_by_circle: false,
            max_display: 10,
        }
    }
}
```

### Step 3: Create Widget Structure

Create `src/frontend/tui/spell_timer.rs`:

```rust
use crate::core::AppState;
use crate::data::SpellTimerConfig;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct SpellTimerWidget {
    config: SpellTimerConfig,
    spells: Vec<ActiveSpell>,
    last_generation: u64,
}

#[derive(Clone)]
struct ActiveSpell {
    name: String,
    duration: u32,
    circle: u8,
}

impl SpellTimerWidget {
    pub fn new(config: SpellTimerConfig) -> Self {
        Self {
            config,
            spells: Vec::new(),
            last_generation: 0,
        }
    }
}
```

### Step 4: Add Render Function

Create a public render function (VellumFE uses functions, not trait implementations):

```rust
// In src/frontend/tui/spell_timer.rs

pub fn render_spell_timer(
    widget: &SpellTimerWidget,
    area: Rect,
    buf: &mut Buffer,
    theme: &AppTheme,
) {
    // Create border
    let block = Block::default()
        .title("Spells")
        .borders(Borders::ALL);

    let inner = block.inner(area);
    block.render(area, buf);

    // Render spell list
    let items: Vec<ListItem> = widget.spells
        .iter()
        .take(widget.config.max_display)
        .map(|spell| {
            let text = if widget.config.show_duration {
                format!("{}: {}s", spell.name, spell.duration)
            } else {
                spell.name.clone()
            };
            ListItem::new(text)
        })
        .collect();

    let list = List::new(items);
    list.render(inner, buf);
}
```

### Step 5: Add Sync Function

Create a sync function in `sync.rs`:

```rust
// In src/frontend/tui/sync.rs

pub fn sync_spell_timer_widgets(
    ui_state: &UiState,
    layout: &Layout,
    widget_manager: &mut WidgetManager,
    theme: &AppTheme,
) {
    for window_def in layout.windows.iter() {
        if window_def.widget_type != WidgetType::SpellTimer {
            continue;
        }

        let name = &window_def.name;

        // Get or create widget in cache
        let widget = widget_manager.spell_timer_widgets
            .entry(name.clone())
            .or_insert_with(|| SpellTimerWidget::new(name.clone()));

        // Get data from state
        if let Some(window_state) = ui_state.windows.get(name) {
            // Update widget from state...
        }
    }
}
```

### Step 6: Register in WidgetManager

Add cache to `widget_manager.rs`:

```rust
pub struct WidgetManager {
    // ... existing caches ...
    pub spell_timer_widgets: HashMap<String, SpellTimerWidget>,
}
```

### Step 7: Add to Module

In `src/frontend/tui/mod.rs`:

```rust
mod spell_timer;
pub use spell_timer::SpellTimerWidget;
```

### Step 8: Document Configuration

Update TOML documentation:

```toml
# Example spell timer configuration
[[widgets]]
type = "spell_timer"
name = "spells"
x = 80
y = 0
width = 20
height = 15
show_duration = true
group_by_circle = true
max_display = 10
```

## Widget Best Practices

### State Management

```rust
// GOOD: Check generation before updating
fn sync(&mut self, state: &AppState) -> bool {
    if state.generation() == self.last_generation {
        return false;
    }
    // ... update ...
    self.last_generation = state.generation();
    true
}

// BAD: Always update (wasteful)
fn sync(&mut self, state: &AppState) -> bool {
    // ... update ...
    true  // Always redraws
}
```

### Efficient Rendering

```rust
// GOOD: Cache computed values
struct MyWidget {
    cached_lines: Vec<Line<'static>>,
    // ...
}

fn sync(&mut self, state: &AppState) -> bool {
    // Rebuild cache only when data changes
    self.cached_lines = self.compute_lines(state);
    true
}

fn render(&self, frame: &mut Frame, area: Rect) {
    // Use cached data
    let paragraph = Paragraph::new(self.cached_lines.clone());
    frame.render_widget(paragraph, area);
}
```

### Configuration Validation

```rust
impl SpellTimerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 {
            return Err("Widget must have non-zero dimensions".into());
        }
        if self.max_display == 0 {
            return Err("max_display must be at least 1".into());
        }
        Ok(())
    }
}
```

### Error Handling

```rust
fn render(&self, frame: &mut Frame, area: Rect) {
    // Handle edge cases gracefully
    if area.width < 3 || area.height < 3 {
        // Area too small to render
        return;
    }

    if self.spells.is_empty() {
        // Show placeholder
        let text = Paragraph::new("No active spells");
        frame.render_widget(text, area);
        return;
    }

    // Normal rendering...
}
```

## Testing Widgets

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_creation() {
        let config = SpellTimerConfig::default();
        let widget = SpellTimerWidget::new(config);
        assert_eq!(widget.name(), "spell_timer");
    }

    #[test]
    fn test_sync_updates_generation() {
        let mut widget = SpellTimerWidget::new(Default::default());
        let mut state = AppState::new();

        state.set_generation(1);
        assert!(widget.sync(&state));
        assert_eq!(widget.last_generation(), 1);

        // Same generation shouldn't trigger update
        assert!(!widget.sync(&state));
    }
}
```

### Visual Testing

Create a test layout that isolates your widget:

```toml
# test_layout.toml
[[widgets]]
type = "spell_timer"
name = "test"
x = 0
y = 0
width = 100
height = 100
```

## See Also

- [Widget Reference](../widgets/README.md) - Existing widgets
- [Project Structure](./project-structure.md) - Code organization
- [Architecture](../architecture/widget-sync.md) - Sync system

