# Highlight Management UI Design

## Overview
A TUI form for managing highlight patterns in VellumFE. Uses `tui-textarea` for text inputs and custom widgets for checkboxes and color previews.

## UI Layout

```
╔═══════════════════════════════════════════════════════════════╗
║                    Manage Highlight                           ║
╠═══════════════════════════════════════════════════════════════╣
║                                                               ║
║  Name:         [swing_highlight_____________]                 ║
║                                                               ║
║  Pattern:      [You swing.*_________________]                 ║
║                (Regex - matches text in game output)          ║
║                                                               ║
║  Foreground:   [#ff0000_____] [████] (Red)                    ║
║                                                               ║
║  Background:   [____________] [    ] (None)                   ║
║                                                               ║
║  [X] Bold                                                     ║
║  [ ] Color entire line                                        ║
║  [ ] Fast parse (Aho-Corasick for literals)                   ║
║                                                               ║
║  Sound:        [sword_swing.wav_________] [Test]              ║
║  Volume:       [0.8_____] (0.0 - 1.0)                         ║
║                                                               ║
║  [Save]  [Cancel]  [Delete]  [Test Pattern]                  ║
║                                                               ║
║  Status: Ready                                                ║
╚═══════════════════════════════════════════════════════════════╝
```

## Widget Structure

### HighlightFormWidget
```rust
pub struct HighlightFormWidget {
    name: TextArea<'static>,           // tui-textarea for name
    pattern: TextArea<'static>,        // tui-textarea for regex pattern
    fg_color: TextArea<'static>,       // tui-textarea for foreground color
    bg_color: TextArea<'static>,       // tui-textarea for background color
    sound: TextArea<'static>,          // tui-textarea for sound filename
    sound_volume: TextArea<'static>,   // tui-textarea for volume

    bold: bool,                        // Checkbox state
    color_entire_line: bool,           // Checkbox state
    fast_parse: bool,                  // Checkbox state

    focused_field: usize,              // 0-8 for different fields
    status_message: String,            // Status bar message
    pattern_error: Option<String>,     // Regex validation error

    mode: FormMode,                    // Create or Edit
    original_name: Option<String>,     // For Edit mode
}

pub enum FormMode {
    Create,
    Edit(String),  // Contains original highlight name
}
```

### Field Navigation
- **Tab**: Move to next field
- **Shift+Tab**: Move to previous field
- **Space**: Toggle checkbox (when focused on checkbox)
- **Enter**: Submit form (when on Save button) or insert newline (in text fields)
- **Esc**: Cancel/close form

### Field Order
0. Name
1. Pattern
2. Foreground color
3. Background color
4. Bold (checkbox)
5. Color entire line (checkbox)
6. Fast parse (checkbox)
7. Sound
8. Sound volume
9. Save button
10. Cancel button
11. Delete button (Edit mode only)
12. Test Pattern button

## Validation

### Pattern Field
- Real-time regex validation
- Show error message below field if invalid
- Highlight field border in red if invalid
- Cannot save with invalid regex

### Color Fields
- Validate hex color format (#RRGGBB)
- Show color preview box next to input
- Empty = no color (None)

### Sound Volume
- Must be 0.0 - 1.0
- Empty = use default from config

## Features

### Live Preview
- Color preview boxes update as you type
- Pattern validation happens on every keystroke
- Status bar shows current state

### Test Pattern Button
- Opens a small popup with text input
- Type test text, press Enter
- Shows whether pattern matches
- Shows what would be highlighted

### Sound Test Button
- Plays the sound file at specified volume
- Shows error if file doesn't exist

## Dot Commands

```
.addhighlight [name]
  - Opens highlight form in Create mode
  - If name provided, pre-fills it

.edithighlight <name>
  - Opens highlight form in Edit mode
  - Loads existing highlight for editing

.deletehighlight <name>
  - Deletes highlight (with confirmation)

.listhighlights
  - Shows all configured highlights

.testhighlight <name> <text>
  - Tests if text matches the pattern
```

## Implementation Phases

### Phase 1: Core Form Widget
- Create `HighlightFormWidget` struct
- Implement field navigation (Tab/Shift+Tab)
- Add `tui-textarea` for text inputs
- Basic rendering

### Phase 2: Checkboxes and Colors
- Custom checkbox rendering
- Color preview boxes
- Hex color validation

### Phase 3: Validation and Status
- Regex pattern validation
- Real-time error display
- Status bar messages

### Phase 4: Form Actions
- Save: Write to config file
- Cancel: Close form
- Delete: Remove from config
- Test Pattern: Popup with test input

### Phase 5: Dot Commands
- Integrate with app.rs
- Add command handlers
- List existing highlights

## File Structure

```
src/ui/highlight_form.rs       - Main form widget
src/ui/checkbox.rs              - Custom checkbox widget (if needed)
```

## Integration with App

### App State
```rust
pub enum InputMode {
    Normal,         // Normal command input
    Search,         // Search mode (Ctrl+F)
    HighlightForm,  // Editing highlight
}

pub struct App {
    // ... existing fields ...
    highlight_form: Option<HighlightFormWidget>,  // None when not shown
}
```

### Event Handling
- When in `InputMode::HighlightForm`:
  - Route all key events to form widget
  - Form returns `FormResult` on submit/cancel
  - App updates config and closes form

## Config Integration

### Loading Highlights
```rust
// Get all highlight names
let names: Vec<String> = config.highlights.keys().collect();

// Load specific highlight
let pattern = config.highlights.get("swing");
```

### Saving Highlights
```rust
// Add/update
config.highlights.insert(name, pattern);
config.save(None)?;

// Delete
config.highlights.remove(&name);
config.save(None)?;

// Reload window manager's highlights
window_manager.update_highlights(config.highlights.clone());
```

## Color Preview Implementation

```rust
fn render_color_preview(color_hex: &str, area: Rect, buf: &mut Buffer) {
    if let Ok(color) = parse_hex_color(color_hex) {
        let block = Block::default().bg(color);
        block.render(area, buf);
    } else {
        // Show empty box or error indicator
        let text = Paragraph::new("Invalid");
        text.render(area, buf);
    }
}
```

## Test Pattern Popup

```
╔═══════════════════════════════════════════════════╗
║               Test Pattern                        ║
╠═══════════════════════════════════════════════════╣
║                                                   ║
║  Pattern: You swing.*                             ║
║                                                   ║
║  Test Text:                                       ║
║  [You swing a greatsword at the kobold!_______]  ║
║                                                   ║
║  Result: ✓ Match found!                           ║
║  Matched: "You swing a greatsword at the kobold!"║
║                                                   ║
║  [Close]                                          ║
╚═══════════════════════════════════════════════════╝
```

## Notes

- Use `tui-textarea` for all text input fields (already researched, it's perfect for this)
- Custom checkboxes: just render `[X]` or `[ ]` and handle Space key
- Color previews: 4-cell wide colored blocks next to hex input
- Form should be centered on screen, ~60 cols wide
- Use popup rendering (Clear + Block over existing UI)
