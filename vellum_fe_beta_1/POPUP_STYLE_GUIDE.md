# Popup Style Guide

This document defines the standardized styling and behavior for all popup widgets (editors, browsers, forms) in VellumFE.

## Design Principles

All popup widgets should follow consistent sizing, positioning, styling, and interaction patterns to provide a unified user experience.

## Standard Sizes

### Large Popups (70x20)
Used for most editors and browsers that need comfortable viewing space:
- **Width**: 70 columns
- **Height**: 20 rows
- **Use cases**:
  - Settings Editor
  - Window Editor
  - Highlight Browser
  - Keybind Browser
  - Color Palette Browser
  - Spell Color Browser

### Small Popups (52xN)
Used for compact forms with minimal fields:
- **Width**: 52 columns (fixed)
- **Height**: Variable based on content needs
  - Color Form: 9 rows
  - Spell Color Form: 9 rows
  - Add more as needed
- **Use cases**: Simple forms with few fields

### Exception: Large Forms
Some forms need more space for complex data entry:
- **Highlight Form**: 62x40 (many fields, checkboxes, help text)
- **Keybind Form**: 80x25 (dropdown with many actions)

## Positioning

### Centered on First Render
All popups spawn centered in the terminal on first display:

```rust
// Calculate centered position
let popup_width = 70;
let popup_height = 20;

if self.popup_x == 0 && self.popup_y == 0 {
    self.popup_x = (area.width.saturating_sub(popup_width)) / 2;
    self.popup_y = (area.height.saturating_sub(popup_height)) / 2;
}
```

### No Fixed Positions
- Do NOT use fixed positions like `(5, 1)` or `(10, 2)`
- Always calculate center based on terminal size
- User can drag to preferred position after opening

## Border Styling

### Required Settings
- **Show Border**: Yes (always `true`)
- **Border Type**: `BorderType::Plain` (single line)
- **Border Sides**: All sides (`Borders::ALL`)
- **Border Color**: Cyan (`Color::Cyan`)

```rust
let block = Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Plain)
    .border_style(Style::default().fg(Color::Cyan))
    .title(title);
```

## Background

### Clear Region with Black Background
Use the Ratatui pattern for clearing popup regions:

```rust
// Fill background with black
let bg_color = Color::Black;
for y in popup_area.y..popup_area.y + popup_area.height {
    for x in popup_area.x..popup_area.x + popup_area.width {
        if x < buf.area.width && y < buf.area.height {
            buf[(x, y)].set_char(' ').set_bg(bg_color);
        }
    }
}
```

Reference: [Ratatui Overwrite Regions Recipe](https://ratatui.rs/recipes/render/overwrite-regions/#_top)

## Title Formatting

### User-Defined Titles
- Titles are set by the popup implementation (e.g., " Settings Editor ", " Highlight Browser ")
- Always include leading/trailing spaces for padding: `" My Title "`
- **Position titles on the LEFT side of the top border** (not centered)
- No enforced capitalization or formatting - respect the implementation's choice

```rust
let title = " Settings Editor ";
// Title will be left-aligned in the top border
for (i, ch) in title.chars().enumerate() {
    buf.get_mut(x + 1 + i as u16, y)
        .set_char(ch)
        .set_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
}
```

## Form Field Styling

### Text Input Fields

#### Single-Row Layout
All input fields are rendered on a single row:
```
Name: my_window_name
Pattern: ^You see .*\.$
Category: combat
FG Color: #ff0000   [██]
BG Color: -         [  ]
```

- **NO borders** around textareas
- **NO multi-row spans** - everything fits on one line
- Label and input on the same row
- Labels in cyan (or gold when focused)
- Input area has dark maroon background

#### Field Labels
- **Position**: Same row as input, left side
- **Color**: Cyan (`Color::Cyan`) - matches border color
- **Focused Color**: Gold (`Color::Rgb(255, 215, 0)`)
- **Format**: Label text followed by colon and space
- **Example**: `"Name: "`, `"Pattern: "`, `"Category: "`

#### Input Areas (Textareas)
- **Position**: Same row as label, immediately after label
- **Background Color**: Dark maroon (`Color::Rgb(64, 0, 0)`)
- **Text Color**: White (default)
- **Borders**: NONE - textareas have no borders
- **Focused Field**: Label turns gold, input area stays dark maroon
- **Embedded Hints**: Gray text shown in empty fields
  - Color: `Color::DarkGray`
  - Example: `"Enter regex pattern..."`, `"e.g., monster|creature"`
  - Disappears when user types

#### Rendering Pattern
```rust
// Single row: "Label: [input area with dark maroon bg]"
let y = field_row;
let x = field_x;

// Field label - cyan normally, gold when focused
let label_color = if focused {
    Color::Rgb(255, 215, 0)  // Gold
} else {
    Color::Cyan
};

buf.set_string(x, y, "Name: ", Style::default().fg(label_color));

// Input area starts right after label (no border)
let input_x = x + label_width;
let input_width = 40; // Example width
let input_bg = Color::Rgb(64, 0, 0);

// Fill input area with dark maroon background
for i in 0..input_width {
    buf[(input_x + i, y)].set_bg(input_bg);
}

// Render text or hint
if field_value.is_empty() {
    buf.set_string(input_x, y, hint_text, Style::default().fg(Color::DarkGray).bg(input_bg));
} else {
    buf.set_string(input_x, y, &field_value, Style::default().fg(Color::White).bg(input_bg));
}
```

### Color Input Fields

#### Standard Format
- **Width**: 10 columns for color input
- **Spacing**: 1 column space after input
- **Preview**: 2 columns for color preview box
- **Total**: 13 columns (10 + 1 + 2)

#### Accepted Values
- Hex codes: `#RRGGBB` (e.g., `#ff0000`, `#00FF00`)
- Color palette names: Any color name configured in color palette
  - Examples: `red`, `forest_green`, `sky_blue`
  - Names are validated against user's palette configuration

#### Preview Box
- Shows the actual color as background
- 2 columns wide, 1 row tall
- Filled with spaces to show color
- Border: Optional, use `[]` characters around preview

```rust
// Color input field layout
let input_width = 10;
let spacing = 1;
let preview_width = 2;

// Render input area (10 cols)
let input_area = Rect {
    x: field_x,
    y: field_y,
    width: input_width,
    height: 1,
};

// Render preview (2 cols, offset by input + spacing)
let preview_x = field_x + input_width + spacing;
if let Some(color) = parse_color(&field_value) {
    buf[(preview_x, field_y)].set_bg(color).set_char(' ');
    buf[(preview_x + 1, field_y)].set_bg(color).set_char(' ');
}
```

### Checkboxes
- **Unchecked**: `[ ]`
- **Checked**: `[✓]`
- **Label Color**: Cyan (normal) / Gold (focused)
- **Toggle**: Space or Enter when focused

### Radio Buttons
- **Unselected**: `( )`
- **Selected**: `(•)`
- **Label Color**: Cyan (normal) / Gold (focused)
- **Select**: Space or Enter when focused

### Dropdowns
- **Closed State**: Show current value with `▼` indicator
- **Open State**: Show scrollable list of options
- **Selected Option**: Highlighted background
- **Navigation**: Arrow keys to move, Enter to select

## Navigation Patterns

### Standard Navigation Keys
- **Tab**: Move to next field
- **Shift+Tab**: Move to previous field
- **Up Arrow**: Move to previous field (same as Shift+Tab)
- **Down Arrow**: Move to next field (same as Tab)
- **Enter**: Activate/toggle current field
- **Space**: Activate/toggle current field (same as Enter)

### Standard Action Keys
- **Ctrl+S**: Save (no "Save" button to select)
- **Escape**: Cancel/Close (no "Cancel" button to select)
- **Delete** or **Ctrl+D**: Delete current item (no "Delete" button to select)

### Special Keys
Document special-case keys in footer only, such as:
- `/` to filter/search
- `?` for help
- Custom widget-specific shortcuts

## Footer Formatting

### Purpose
Display special-case shortcuts and context-specific help.

### What NOT to Include
Standard shortcuts that work everywhere:
- Tab/Shift+Tab navigation
- Ctrl+S to save
- Escape to cancel
- Delete/Ctrl+D to delete

### What to Include
Special functionality specific to this popup:
- `/` to filter entries
- `?` to show help
- `F2` to rename
- Widget-specific shortcuts

### Styling
- **Position**: Bottom row of popup (inside border)
- **Color**: `Color::DarkGray` or `Color::Gray`
- **Format**: Concise key hints separated by spaces
- **Example**: `"/ to filter  ? for help"`

```rust
// Footer render example
let footer_y = popup_area.y + popup_area.height - 2; // Inside bottom border
let footer_text = "/ to filter";
buf.set_string(
    popup_area.x + 2,
    footer_y,
    footer_text,
    Style::default().fg(Color::DarkGray)
);
```

## Mouse Interaction

### Drag to Move
All popups support dragging by the title bar:

1. **Title Bar Detection**: Top border row, excluding corners (leave 1 cell margin)
2. **Drag Tracking**: Store `drag_offset_x` and `drag_offset_y` on mouse down
3. **Drag Movement**: Update position based on mouse delta during drag
4. **Boundary Checking**: Ensure popup stays within terminal bounds

```rust
// Title bar click detection
let on_title_bar = mouse_row == self.popup_y
    && mouse_col > popup_area.x  // Exclude left corner
    && mouse_col < popup_area.x + popup_area.width.saturating_sub(1); // Exclude right corner

// Handle drag
if on_title_bar && !self.dragging {
    self.dragging = true;
    self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
    self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
}

if self.dragging {
    self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
    self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);

    // Keep within bounds
    self.popup_x = self.popup_x.min(area.width.saturating_sub(popup_width));
    self.popup_y = self.popup_y.min(area.height.saturating_sub(popup_height));
}
```

### Resize Support (Future)
- Not currently implemented for popups
- May be added in future versions
- Would follow same patterns as window resizing

## Required State Fields

Every popup widget should have these fields:

```rust
pub struct MyPopup {
    // Popup positioning
    popup_x: u16,
    popup_y: u16,

    // Drag state
    dragging: bool,
    drag_offset_x: u16,
    drag_offset_y: u16,

    // ... widget-specific fields ...
}
```

## Implementation Checklist

When creating or updating a popup widget:

- [ ] Size follows standard (70x20, 52xN, or documented exception)
- [ ] Centers on first render (checks if `popup_x == 0 && popup_y == 0`)
- [ ] Uses cyan border with `BorderType::Plain` and `Borders::ALL`
- [ ] Clears background with black color before rendering
- [ ] Title includes leading/trailing spaces
- [ ] Supports dragging via title bar
- [ ] Handles drag boundary checking
- [ ] Has required state fields (popup_x, popup_y, dragging, drag_offset_x, drag_offset_y)
- [ ] Mouse release stops dragging

## Example: Standard 70x20 Browser

```rust
pub struct MyBrowser {
    popup_x: u16,
    popup_y: u16,
    dragging: bool,
    drag_offset_x: u16,
    drag_offset_y: u16,
    scroll_offset: usize,
    selected_index: usize,
    items: Vec<MyItem>,
}

impl MyBrowser {
    pub fn new(items: Vec<MyItem>) -> Self {
        Self {
            popup_x: 0,
            popup_y: 0,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            scroll_offset: 0,
            selected_index: 0,
            items,
        }
    }

    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool) {
        const POPUP_WIDTH: u16 = 70;
        const POPUP_HEIGHT: u16 = 20;

        if !mouse_down {
            self.dragging = false;
            return;
        }

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: POPUP_WIDTH,
            height: POPUP_HEIGHT,
        };

        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > popup_area.x
            && mouse_col < popup_area.x + popup_area.width.saturating_sub(1);

        if on_title_bar && !self.dragging {
            self.dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
        }

        if self.dragging {
            self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
            self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        const POPUP_WIDTH: u16 = 70;
        const POPUP_HEIGHT: u16 = 20;

        // Center on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(POPUP_WIDTH)) / 2;
            self.popup_y = (area.height.saturating_sub(POPUP_HEIGHT)) / 2;
        }

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: POPUP_WIDTH,
            height: POPUP_HEIGHT,
        };

        // Clear background
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < buf.area.width && y < buf.area.height {
                    buf[(x, y)].set_char(' ').set_bg(Color::Black);
                }
            }
        }

        // Render border with title
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" My Browser ");

        block.render(popup_area, buf);

        // Render content inside border...
    }
}
```

## Current Compliance Status

### Fully Compliant Widgets ✓
- **SettingsEditor** (70x20, sticky category headers, Clear widget, left-aligned title, footer at row 18)
- **HighlightBrowser** (70x20, sticky category headers, Clear widget, left-aligned title, footer at row 18)
- **KeybindBrowser** (70x20, sticky section headers, Clear widget, left-aligned title, footer at row 18)
- **ColorPaletteBrowser** (70x20, sticky category headers, Clear widget, left-aligned title, footer at row 18)
- **UIColorsBrowser** (70x20, sticky category headers, Clear widget, left-aligned title, footer at row 18)
- **HighlightForm** (52x9, Clear widget, no buttons, Ctrl+S/Ctrl+D/Esc, transparent dropdowns)
- **KeybindForm** (52x9, Clear widget, no buttons, Ctrl+S/Ctrl+D/Esc, transparent dropdowns)
- **ColorForm** (52x9, Clear widget, no buttons, Ctrl+S/Ctrl+D/Esc)
- **SpellColorForm** (52x9, Clear widget, no buttons, Ctrl+S/Ctrl+D/Esc)

### Needs Updates
- **WindowEditor**: Uses fixed position (5, 1) - needs centering
- **UIColorsEditor**: Verify compliance with style guide

## Exceptions and Special Cases

### Variable Height Forms
Some forms need to calculate height based on content:
- Start with minimum height
- Add rows for each field/section
- Use 52xN for small forms, 62xN or 70xN for larger forms
- Still center on first render

### Modal Behavior
All popups are modal (block interaction with windows behind them):
- Use `InputMode` enum to track which popup is open
- Hide command input when popup is active
- ESC key closes popup and returns to Normal mode
- Only one popup open at a time

## Global UI Defaults

VellumFE uses a global defaults system for common widget properties. These are set in the config file under `[ui]`:

```toml
[ui]
border_color = "#00ffff"      # Cyan - default border color for all widgets
text_color = "#ffffff"        # White - default text color
border_style = "single"       # Default border style
background_color = "#000000"  # Black - default background
focused_border_color = "#ffff00"  # Yellow - for focused windows
selection_bg_color = "#444444"    # Selection highlight color
```

### Three-State Field System

Window and widget configurations use a three-state system for optional fields:

1. **Field Omitted (None)** - Uses global default from `ui.*` config
   ```toml
   [[windows]]
   name = "main"
   # border_color not specified → uses ui.border_color (#00ffff)
   ```

2. **Field Set to "-"** - Explicitly empty/transparent (no value)
   ```toml
   [[windows]]
   name = "main"
   border_color = "-"  # Explicitly no border color
   ```

3. **Field Set to Value** - Uses specific value
   ```toml
   [[windows]]
   name = "main"
   border_color = "#ff0000"  # Red border
   ```

### Why This Matters

**Problem Solved**: Previously, editing a window with an empty color field would load the default value (e.g., black) into the editor. If you saved without deleting it, the default would be written to the config file, changing the window's behavior.

**Solution**: The three-state system distinguishes between "not set" and "explicitly empty":
- Empty field in config = `None` = uses global default (won't save default to file)
- Field set to "-" = explicitly empty (saves "-" to file)
- Field set to color = uses that color (saves color to file)

This allows users to:
- Change global defaults and have all unset windows update automatically
- Explicitly set some windows to "no color" with "-"
- Override specific windows with custom values

## Future Enhancements

Potential improvements to consider:

- **Resize Support**: Allow users to resize popups via edges/corners
- **Remember Position**: Save popup positions per popup type in config
- **Animations**: Smooth open/close animations
- **Shadows**: Drop shadow effect for depth perception
