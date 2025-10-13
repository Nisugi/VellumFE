# Settings Editor Design

## Overview

A comprehensive in-app configuration editor for all settings **except** highlights and windows (which have their own dedicated editors). This provides a user-friendly way to modify config.toml settings without manual file editing.

## Scope

### Included Settings:

**Connection Settings:**
- host (string)
- port (number)

**UI Settings:**
- buffer_size (number)
- show_timestamps (boolean)
- command_echo_color (color)
- countdown_icon (string)
- selection_enabled (boolean)
- selection_respect_window_boundaries (boolean)
- selection_bg_color (color)
- drag_modifier_key (enum: ctrl/alt/shift/none)

**Sound Settings:**
- enabled (boolean)
- volume (float 0.0-1.0)
- cooldown_ms (number)

**Preset Colors:**
- List of presets (whisper, links, speech, roomName, monsterbold, etc.)
- Edit fg/bg colors per preset

**Spell Colors:**
- List of spell color ranges
- Edit spell IDs and color per range

**Prompt Colors:**
- List of prompt character colors
- Edit character and color

**Event Patterns:**
- List/add/edit/delete event patterns
- Fields: pattern, event_type, action, duration, duration_capture, duration_multiplier, enabled

### Excluded (Have Dedicated Editors):
- Highlights (`.addhighlight`, `.edithighlight`)
- Keybinds (`.addkeybind`, `.editkeybind`)
- Windows (Window editor UI)

## UI Design

### Menu Structure

```
.settings [category]

Categories:
- connection    - Host, port
- ui            - Buffer size, colors, selection, etc.
- sound         - Sound enable, volume, cooldown
- presets       - Color presets (whisper, links, etc.)
- spells        - Spell color ranges
- prompts       - Prompt character colors
- events        - Event patterns (list/add/edit/delete)
- all           - Show all categories (default)
```

### Display Format

**Tabbed Interface** (similar to window editor):
- Tab per category
- Arrow keys to navigate between tabs
- Enter to edit selected setting
- Escape to close without saving
- Ctrl+S to save changes

**Alternative: Scrollable List**:
- Single scrollable list of all settings
- Group by category with headers
- More compact, easier to implement initially

## Implementation Plan

### Phase 1: Simple List Editor (Today)

Create a basic scrollable list showing all editable settings:

```
================== Settings ==================

[Connection]
  host: 127.0.0.1
  port: 8000

[UI]
  buffer_size: 1000
  show_timestamps: true
  command_echo_color: #ffffff
  countdown_icon:
  selection_enabled: true
  ...

[Sound]
  enabled: true
  volume: 0.7
  cooldown_ms: 500

[Presets]
  whisper: fg=#60b4bf
  links: fg=#477ab3
  speech: fg=#53a684
  ...

Arrow keys: Navigate | Enter: Edit | Esc: Close
==============================================
```

### Phase 2: Edit Individual Settings

When user presses Enter on a setting:
- Simple text input for strings/numbers
- Toggle for booleans (y/n or true/false)
- Color picker for colors (hex input with validation)
- Dropdown for enums

### Phase 3: Validation

- Port: 1-65535
- Volume: 0.0-1.0
- Colors: Valid hex (#RRGGBB)
- Boolean: true/false or yes/no

### Phase 4: Complex Editors

For lists (presets, spells, prompts, events):
- Show list of items
- Add/Edit/Delete operations
- Sub-editor forms

## Dot Commands

```
.settings           - Open settings editor (all categories)
.settings connection - Open settings editor (connection tab)
.settings ui        - Open settings editor (UI tab)
.settings sound     - Open settings editor (sound tab)
.settings presets   - Open settings editor (presets tab)
.settings spells    - Open settings editor (spell colors tab)
.settings prompts   - Open settings editor (prompt colors tab)
.settings events    - Open settings editor (event patterns tab)

.config             - Alias for .settings
```

## Data Structures

```rust
pub enum SettingValue {
    String(String),
    Number(i64),
    Float(f64),
    Boolean(bool),
    Color(String),      // Hex color
    Enum(String, Vec<String>), // (current, options)
}

pub struct SettingItem {
    pub category: String,
    pub key: String,
    pub display_name: String,
    pub value: SettingValue,
    pub description: Option<String>,
}

pub struct SettingsEditor {
    pub items: Vec<SettingItem>,
    pub selected_index: usize,
    pub editing_index: Option<usize>,
    pub edit_buffer: String,
    pub category_filter: Option<String>,
}
```

## File Structure

Create `src/ui/settings_editor.rs`:
- `SettingsEditor` struct
- Render method
- Navigation (up/down/pgup/pgdown)
- Edit mode handling
- Save to config

## Integration

### App State

Add to `src/app.rs`:

```rust
pub struct App {
    // ... existing fields ...
    settings_editor: Option<SettingsEditor>,
}
```

### Event Handling

```rust
if let Some(ref mut editor) = self.settings_editor {
    match event {
        KeyCode::Up => editor.previous(),
        KeyCode::Down => editor.next(),
        KeyCode::Enter => editor.start_edit(),
        KeyCode::Esc => {
            if editor.is_editing() {
                editor.cancel_edit();
            } else {
                self.settings_editor = None;
            }
        }
        KeyCode::Char('s') if modifiers.contains(KeyModifiers::CONTROL) => {
            editor.save(&mut self.config)?;
            self.settings_editor = None;
        }
        // ... handle edit mode input
    }
}
```

### Rendering

Overlay on top of main UI (like window editor):

```rust
if let Some(ref editor) = self.settings_editor {
    editor.render(area, buf);
}
```

## Example Usage

```
User types: .settings ui
-> Opens settings editor on UI tab
-> User navigates to "buffer_size: 1000"
-> Presses Enter
-> Edit field appears: [1000_]
-> User types "5000" and Enter
-> Value updated (not saved yet)
-> User presses Ctrl+S
-> Saves to config file
-> Settings editor closes
```

## Testing Scenarios

1. **Basic navigation**: Arrow keys scroll through settings
2. **Category filtering**: `.settings sound` shows only sound settings
3. **Edit string**: Change host to "localhost"
4. **Edit number**: Change port to 8001
5. **Edit boolean**: Toggle show_timestamps
6. **Edit color**: Change command_echo_color to #ff0000
7. **Validation**: Try invalid port (99999), should reject
8. **Save**: Ctrl+S writes to config file
9. **Cancel**: Esc without saving, verify no changes
10. **Reload**: Close and reopen, verify saved values persist

## Future Enhancements

- **Live Preview**: Changes apply immediately before saving
- **Reset to Default**: Button to reset individual settings
- **Search**: Filter settings by name
- **Help Text**: Show description for each setting
- **Undo/Redo**: Multi-level undo for editing session
- **Import/Export**: Save/load settings profiles
- **Character Profiles**: Quick switch between character-specific configs

## Notes

- Don't edit highlights/keybinds here (use dedicated editors)
- Don't edit windows here (use window editor)
- Focus on simple, frequently-changed settings first
- Complex lists (presets, spells) can be phase 2
- Validation is critical - don't allow invalid configs

---

**Status**: Design complete, ready for implementation
**Estimated Effort**: 3-4 hours for Phase 1 (basic editor)
**Dependencies**: None - builds on existing config infrastructure
