# Browser & Editor System

VellumFE provides interactive popup interfaces for browsing and editing configuration without leaving the application.

## Overview

The browser/editor system includes:

- **Browsers** - Read-only lists with selection (highlights, keybinds, colors)
- **Editors** - Forms for creating/modifying configuration
- **Window Editor** - Complex form for widget configuration

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        User Interface                           │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────────┐ │
│  │   Browser    │  │    Editor    │  │    WindowEditor       │ │
│  │  (List View) │  │  (Form View) │  │  (Complex Form)       │ │
│  └──────────────┘  └──────────────┘  └───────────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│                        Widget Traits                            │
│  ┌───────────┐ ┌───────────┐ ┌──────────┐ ┌────────────────┐   │
│  │ Navigable │ │ Selectable│ │ Saveable │ │ TextEditable   │   │
│  └───────────┘ └───────────┘ └──────────┘ └────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Widget Traits

All browsers and editors implement behavior traits.

### Navigable

For list navigation:

```rust
pub trait Navigable {
    fn navigate_up(&mut self);      // Move selection up
    fn navigate_down(&mut self);    // Move selection down
    fn page_up(&mut self);          // Move up ~10 items
    fn page_down(&mut self);        // Move down ~10 items
    fn home(&mut self) {}           // Move to first (optional)
    fn end(&mut self) {}            // Move to last (optional)
}
```

### Selectable

For selecting items:

```rust
pub trait Selectable {
    fn get_selected(&self) -> Option<String>;
    fn delete_selected(&mut self) -> Option<String>;
}
```

### TextEditable

For text input fields:

```rust
pub trait TextEditable {
    fn get_focused_field(&self) -> Option<&TextArea<'static>>;
    fn get_focused_field_mut(&mut self) -> Option<&mut TextArea<'static>>;
    fn select_all(&mut self);           // Ctrl+A
    fn copy_to_clipboard(&self);        // Ctrl+C
    fn cut_to_clipboard(&mut self);     // Ctrl+X
    fn paste_from_clipboard(&mut self); // Ctrl+V
}
```

### FieldNavigable

For form field navigation:

```rust
pub trait FieldNavigable {
    fn next_field(&mut self);       // Tab
    fn previous_field(&mut self);   // Shift+Tab
    fn field_count(&self) -> usize;
    fn current_field(&self) -> usize;
}
```

### Saveable

For forms that persist data:

```rust
pub trait Saveable {
    type SaveResult;
    fn try_save(&mut self) -> Option<Self::SaveResult>;
    fn is_modified(&self) -> bool;
}
```

### Toggleable / Cyclable

For boolean and enum fields:

```rust
pub trait Toggleable {
    fn toggle_focused(&mut self) -> Option<bool>;
}

pub trait Cyclable {
    fn cycle_forward(&mut self);   // Space/Down
    fn cycle_backward(&mut self);  // Up
}
```

## Browser Types

### HighlightBrowser

Browse configured text highlights.

**Command**: `.highlights`

**Features**:
- Sorted by category, then name
- Color preview with sample text
- Category filtering
- Shows squelch status
- Shows sound indicator
- Shows redirect info

```
┌─ Highlights ────────────────────────────────────────┐
│                                                     │
│ ═══ Combat ═══                                      │
│ > critical_hit    "critical hit"   [█████]          │
│   damage_taken    "strikes you"    [█████]          │
│   damage_dealt    "You hit"        [█████]          │
│                                                     │
│ ═══ Chat ═══                                        │
│   speech          "^\\w+ says"     [█████]          │
│   whisper         "whispers"       [█████] ♪        │
│                                                     │
│ [Enter] Edit  [Delete] Remove  [Escape] Close       │
└─────────────────────────────────────────────────────┘
```

### KeybindBrowser

Browse configured keybinds.

**Command**: `.keybinds`

**Features**:
- Grouped by type (Actions, Macros)
- Shows key combo and action
- Columnar layout

```
┌─ Keybinds ──────────────────────────────────────────┐
│                                                     │
│ ═══ Actions ═══                                     │
│ > Ctrl+L         layout_editor                      │
│   Ctrl+H         highlight_browser                  │
│   F1             help                               │
│                                                     │
│ ═══ Macros ═══                                      │
│   Ctrl+1         "attack target"                    │
│   Ctrl+2         "stance defensive"                 │
│                                                     │
│ [Enter] Edit  [Delete] Remove  [Escape] Close       │
└─────────────────────────────────────────────────────┘
```

### ColorPaletteBrowser

Browse named color palette.

**Command**: `.colors`

**Features**:
- Grouped by color category
- Visual color swatches
- Hex code display

```
┌─ Color Palette ─────────────────────────────────────┐
│                                                     │
│ ═══ Red ═══                                         │
│ > ██ red         #FF0000                            │
│   ██ crimson     #DC143C                            │
│   ██ darkred     #8B0000                            │
│                                                     │
│ ═══ Green ═══                                       │
│   ██ green       #00FF00                            │
│   ██ forestgreen #228B22                            │
│                                                     │
│ [Escape] Close                                      │
└─────────────────────────────────────────────────────┘
```

### SpellColorBrowser

Browse spell-specific colors.

**Command**: `.spellcolors`

**Features**:
- Spell number to color mapping
- Color preview bars
- Add/edit/delete support

### ThemeBrowser

Browse available themes.

**Command**: `.themes`

**Features**:
- List saved theme profiles
- Preview theme colors
- Apply/save themes

## Window Editor

The WindowEditor provides comprehensive widget configuration.

**Command**: `.window <widget_name>`

### Field Categories

**Geometry**:
- `row`, `col` - Position
- `rows`, `cols` - Size
- `min_rows`, `min_cols` - Minimum size
- `max_rows`, `max_cols` - Maximum size

**Appearance**:
- `title` - Window title
- `show_title` - Toggle title display
- `title_position` - Title placement
- `bg_color` - Background color
- `text_color` - Text color
- `transparent_bg` - Use terminal background

**Borders**:
- `show_border` - Enable borders
- `border_style` - Style (plain, rounded, double)
- `border_color` - Border color
- `border_top/bottom/left/right` - Individual sides

**Content**:
- `streams` - Stream IDs for text widgets
- `buffer_size` - Max line count
- `wordwrap` - Enable word wrapping
- `timestamps` - Show timestamps
- `content_align` - Text alignment

### Widget-Specific Fields

**Tabbed Text**:
- `tab_bar_position` - Top or bottom
- `tab_active_color` - Active tab color
- `tab_inactive_color` - Inactive tab color

**Compass**:
- `compass_active_color` - Available direction color
- `compass_inactive_color` - Unavailable direction color

**Progress Bars**:
- `progress_label` - Bar label
- `progress_color` - Bar fill color

**Dashboard**:
- `dashboard_layout` - Horizontal or vertical
- `dashboard_spacing` - Item spacing

## Common Browser Features

### Popup Dragging

All browsers support dragging by the title bar:

```rust
pub fn handle_mouse(
    &mut self,
    mouse_col: u16,
    mouse_row: u16,
    mouse_down: bool,
    area: Rect,
) -> bool {
    let on_title_bar = mouse_row == self.popup_y
        && mouse_col > self.popup_x
        && mouse_col < self.popup_x + popup_width - 1;

    if mouse_down && on_title_bar && !self.is_dragging {
        self.is_dragging = true;
        self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
        self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
        return true;
    }
    // ...
}
```

### Scroll with Category Headers

Browsers with categories handle scrolling with sticky headers for context.

## Menu Keybinds

Common keybinds for browsers and editors:

| Key | Action | Context |
|-----|--------|---------|
| `Up/k` | Navigate up | All browsers |
| `Down/j` | Navigate down | All browsers |
| `PageUp` | Page up | All browsers |
| `PageDown` | Page down | All browsers |
| `Enter` | Select/confirm | Browsers → Editor |
| `Escape` | Close/cancel | All |
| `Tab` | Next field | Editors |
| `Shift+Tab` | Previous field | Editors |
| `Space` | Toggle/cycle | Toggleable/Cyclable |
| `Delete` | Delete selected | Selectable |
| `Ctrl+S` | Save | Saveable |

## Form Validation

Editors validate input before saving:

```rust
impl Saveable for HighlightForm {
    type SaveResult = HighlightPattern;

    fn try_save(&mut self) -> Option<Self::SaveResult> {
        // Validate required fields
        let name = self.name_field.lines()[0].trim();
        if name.is_empty() {
            self.error_message = Some("Name is required".to_string());
            return None;
        }

        // Validate pattern syntax
        let pattern = self.pattern_field.lines()[0].trim();
        if let Err(e) = regex::Regex::new(pattern) {
            self.error_message = Some(format!("Invalid regex: {}", e));
            return None;
        }

        // Build result
        Some(HighlightPattern {
            name: name.to_string(),
            pattern: pattern.to_string(),
            // ...
        })
    }
}
```

## Integration Points

### Commands

| Command | Browser/Editor | Function |
|---------|---------------|----------|
| `.highlights` | HighlightBrowser | Browse highlights |
| `.highlight <name>` | HighlightForm | Edit highlight |
| `.addhighlight` | HighlightForm | Add highlight |
| `.keybinds` | KeybindBrowser | Browse keybinds |
| `.keybind <key>` | KeybindForm | Edit keybind |
| `.addkeybind` | KeybindForm | Add keybind |
| `.colors` | ColorPaletteBrowser | Browse palette |
| `.spellcolors` | SpellColorBrowser | Browse spell colors |
| `.themes` | ThemeBrowser | Browse themes |
| `.window <name>` | WindowEditor | Edit widget |
| `.layout` | LayoutEditor | Edit layout |

### UI State

Browsers are tracked in `MenuState`:

```rust
pub enum MenuState {
    None,
    HighlightBrowser(HighlightBrowser),
    HighlightForm(HighlightForm),
    KeybindBrowser(KeybindBrowser),
    KeybindForm(KeybindForm),
    ColorPaletteBrowser(ColorPaletteBrowser),
    SpellColorBrowser(SpellColorBrowser),
    ThemeBrowser(ThemeBrowser),
    WindowEditor(WindowEditor),
    // ...
}
```

## Creating a New Browser

### 1. Define Entry Structure

```rust
#[derive(Clone)]
pub struct MyEntry {
    pub id: String,
    pub name: String,
    pub value: String,
}
```

### 2. Create Browser Struct

```rust
pub struct MyBrowser {
    entries: Vec<MyEntry>,
    selected_index: usize,
    scroll_offset: usize,

    // Popup dragging
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}
```

### 3. Implement Traits

```rust
impl Navigable for MyBrowser {
    fn navigate_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.adjust_scroll();
        }
    }
    // ...
}

impl Selectable for MyBrowser {
    fn get_selected(&self) -> Option<String> {
        self.entries.get(self.selected_index).map(|e| e.id.clone())
    }
    // ...
}
```

### 4. Implement Widget Rendering

```rust
impl Widget for &MyBrowser {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear popup area
        Clear.render(area, buf);

        // Draw border and title
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" My Browser ");
        block.render(area, buf);

        // Render entries with selection highlighting
        for (idx, entry) in self.visible_entries().enumerate() {
            let style = if idx == self.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            // ... render entry
        }
    }
}
```

### 5. Register Command

```rust
".mybrowser" => {
    let browser = MyBrowser::new(&config.my_data);
    app.ui_state.menu_state = MenuState::MyBrowser(browser);
}
```

## See Also

- [Commands](../reference/cli-options.md) - All browser commands
- [Keybinds Configuration](../configuration/keybinds-toml.md) - Browser keybinds
- [Highlights Configuration](../configuration/highlights-toml.md) - Highlight editing

