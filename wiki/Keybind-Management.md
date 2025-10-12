# Keybind Management

VellumFE includes a flexible keybind system that allows you to map keyboard shortcuts to built-in actions or custom macros. Keybinds can execute navigation commands, window operations, text sending macros, and more.

## Table of Contents

- [Overview](#overview)
- [Opening the Keybind Form](#opening-the-keybind-form)
- [Form Fields](#form-fields)
- [Navigation](#navigation)
- [Saving and Managing Keybinds](#saving-and-managing-keybinds)
- [Dot Commands](#dot-commands)
- [Configuration File Format](#configuration-file-format)
- [Available Actions](#available-actions)
- [Examples](#examples)
- [Tips and Best Practices](#tips-and-best-practices)

## Overview

Keybinds in VellumFE allow you to:
- Map **keyboard shortcuts** to built-in actions
- Create **custom macros** that send text to the game
- Use **modifier keys** (Ctrl, Alt, Shift) for complex combinations
- Support **function keys** (F1-F12) and **numpad keys**
- Execute **window operations**, **navigation**, and **text manipulation** commands

All keybinds are stored in your character-specific config file and can be managed through an interactive TUI form or via dot commands.

## Opening the Keybind Form

There are two ways to open the keybind management form:

### Create a New Keybind
Type `.addkeybind` or `.addkey` in the command input:

```
.addkeybind
```

This opens an empty form where you can define a new keybind.

### Edit an Existing Keybind
Type `.editkeybind <key>` or `.editkey <key>` where `<key>` is the key combination:

```
.editkeybind ctrl+e
```

This opens the form pre-filled with the existing keybind's settings.

## Form Fields

The keybind form contains the following fields:

### Key Combination
**Required field**. The keyboard shortcut to trigger this keybind.

- Format: `key` or `modifier+key` or `modifier+modifier+key`
- Modifiers: `ctrl`, `alt`, `shift`
- Keys: letters, numbers, function keys (f1-f12), numpad keys, special keys
- Case-insensitive: `Ctrl+E` and `ctrl+e` are equivalent
- Real-time validation shows errors immediately
- Examples:
  - `f5` - Function key 5
  - `ctrl+e` - Control + E
  - `alt+shift+a` - Alt + Shift + A
  - `ctrl+f1` - Control + Function key 1

**Valid Key Names:**
- Letters: `a` through `z`
- Numbers: `0` through `9`
- Function keys: `f1` through `f12`
- Numpad: `kp0` through `kp9`, `kpenter`, `kpplus`, `kpminus`, etc.
- Special keys: `enter`, `tab`, `space`, `backspace`, `delete`, `home`, `end`, `pageup`, `pagedown`, `left`, `right`, `up`, `down`, `esc`

### Type: Action or Macro
**Required field**. Radio button selection determining the keybind type:

#### Action
Executes a built-in command. When selected, shows a dropdown list of 24 available actions.

- Navigate with **Up/Down** arrow keys
- Press **Enter** to select
- See [Available Actions](#available-actions) for full list

#### Macro
Sends custom text to the game. When selected, shows a text input field.

- Enter any text to send when the key is pressed
- Use `\r` to represent pressing Enter
- Example: `north\r` sends "north" and presses Enter
- Example: `say Hello!\r` sends "say Hello!" and presses Enter

### Action Dropdown (when Type = Action)
Shows all 24 built-in actions. Use arrow keys to navigate and Enter to select.

See [Available Actions](#available-actions) for detailed descriptions.

### Macro Text (when Type = Macro)
Text to send to the game when the key is pressed.

- Multi-line text is supported
- Use `\r` to simulate pressing Enter
- Text is sent exactly as typed (no processing)

## Navigation

### Keyboard Navigation
- **Tab** - Move to next field/button (wraps to beginning)
- **Shift+Tab** - Move to previous field/button (limited support)
- **Space** - Toggle between Action/Macro types (when focused on Type field)
- **Enter** - Activate button (Save/Cancel/Delete) or select dropdown item
- **Esc** - Close form without saving
- **Arrow keys** - Navigate dropdown (when focused on action list) or move cursor in text fields
- **Up/Down** - Scroll through action dropdown
- **Home/End** - Jump to start/end of text field
- **Backspace/Delete** - Edit text in fields

### Visual Indicators
- **Focused text fields**: Yellow border
- **Unfocused text fields**: Dark gray border
- **Selected radio button**: Yellow bullet with bold text
- **Unselected radio button**: Gray bullet
- **Focused buttons**: Inverted colors (e.g., black text on green background)
- **Invalid key combo**: Red error message below key combination field
- **Dropdown selection**: White background highlight

## Saving and Managing Keybinds

### Save Button
Press **Enter** when focused on the Save button (or Tab until it's highlighted and press Enter).

- Validates all fields before saving
- Shows error if key combination is empty or invalid
- Shows error if action/macro value is empty
- Saves to character-specific config file
- Automatically reloads keybinds for immediate use
- Shows confirmation message: "Keybind 'key' saved"

### Cancel Button
Press **Enter** when focused on the Cancel button (or press **Esc** anywhere).

- Closes form without saving
- Discards all changes

### Delete Button
Press **Enter** when focused on the Delete button (only shown in Edit mode).

- Removes the keybind from config
- Shows confirmation message: "Keybind 'key' deleted"
- Cannot be undone (except by manually re-creating the keybind)

## Dot Commands

VellumFE provides several dot commands for keybind management:

### Create New Keybind
```
.addkeybind
.addkey
```
Opens the keybind form in Create mode.

### Edit Existing Keybind
```
.editkeybind <key>
.editkey <key>
```
Opens the keybind form in Edit mode with the specified keybind loaded.

**Example:**
```
.editkey ctrl+e
```

### Delete Keybind
```
.deletekeybind <key>
.delkey <key>
```
Immediately deletes the specified keybind (no confirmation prompt).

**Example:**
```
.delkey f5
```

### List All Keybinds
```
.listkeybinds
.listkeys
.keybinds
```
Shows a count and comma-separated list of all configured keybinds.

**Example output:**
```
8 keybinds: alt+1, alt+2, ctrl+e, ctrl+f, f1, f5, shift+f1, shift+up
```

## Configuration File Format

Keybinds are stored in your character-specific config file at:
```
~/.vellum-fe/configs/<character>.toml
```

### Example Configuration
```toml
[keybinds]

# Built-in action keybinds
"f5" = { action = "scroll_to_bottom" }
"ctrl+e" = { action = "cursor_end" }
"ctrl+a" = { action = "cursor_home" }
"shift+up" = { action = "scroll_up" }
"shift+down" = { action = "scroll_down" }

# Macro keybinds
"alt+1" = { macro_text = "north\r" }
"alt+2" = { macro_text = "south\r" }
"f1" = { macro_text = "cast 1111\r" }
"ctrl+f" = { macro_text = "forage\r" }
```

### Field Reference
- **Action type**: `{ action = "action_name" }`
  - `action` (string, required) - Name of built-in action

- **Macro type**: `{ macro_text = "text" }`
  - `macro_text` (string, required) - Text to send to game

## Available Actions

VellumFE provides 24 built-in actions that can be bound to keys:

### Command Input Actions
- **send_command** - Send the current command input to the game
- **clear_command** - Clear the command input field
- **cursor_left** - Move cursor left one character
- **cursor_right** - Move cursor right one character
- **cursor_word_left** - Move cursor left one word
- **cursor_word_right** - Move cursor right one word
- **cursor_home** - Move cursor to start of input
- **cursor_end** - Move cursor to end of input
- **delete_char** - Delete character under cursor
- **delete_word** - Delete word at cursor
- **backspace_char** - Delete character before cursor
- **backspace_word** - Delete word before cursor
- **recall_prev_command** - Recall previous command from history (up arrow)
- **recall_next_command** - Recall next command from history (down arrow)

### Window Actions
- **next_window** - Focus next window
- **prev_window** - Focus previous window
- **scroll_up** - Scroll focused window up
- **scroll_down** - Scroll focused window down
- **scroll_to_top** - Jump to top of focused window
- **scroll_to_bottom** - Jump to bottom of focused window

### UI Actions
- **toggle_mouse** - Enable/disable mouse support
- **exit** - Quit VellumFE

### Search Actions
- **start_search** - Open search mode in focused window
- **clear_search** - Clear current search and exit search mode

## Examples

### Example 1: Direction Macros
Bind Alt+numpad keys to send movement commands:

**Key:** `alt+kp8`
**Type:** Macro
**Macro Text:** `north\r`

**Key:** `alt+kp2`
**Type:** Macro
**Macro Text:** `south\r`

**Key:** `alt+kp4`
**Type:** Macro
**Macro Text:** `west\r`

**Key:** `alt+kp6`
**Type:** Macro
**Macro Text:** `east\r`

### Example 2: Spell Casting
Bind function keys to cast spells:

**Key:** `f1`
**Type:** Macro
**Macro Text:** `cast 1111\r`

**Key:** `f2`
**Type:** Macro
**Macro Text:** `cast 509\r`

**Key:** `shift+f1`
**Type:** Macro
**Macro Text:** `prep 1111\r`

### Example 3: Window Navigation
Use Ctrl+numbers to switch between windows:

**Key:** `ctrl+1`
**Type:** Action
**Action:** `next_window`

**Key:** `ctrl+2`
**Type:** Action
**Action:** `prev_window`

### Example 4: Quick Scrolling
Bind page up/down for easier scrolling:

**Key:** `pageup`
**Type:** Action
**Action:** `scroll_up`

**Key:** `pagedown`
**Type:** Action
**Action:** `scroll_down`

**Key:** `home`
**Type:** Action
**Action:** `scroll_to_top`

**Key:** `end`
**Type:** Action
**Action:** `scroll_to_bottom`

### Example 5: Command Line Editing
Emacs-style keybinds for command input:

**Key:** `ctrl+a`
**Type:** Action
**Action:** `cursor_home`

**Key:** `ctrl+e`
**Type:** Action
**Action:** `cursor_end`

**Key:** `ctrl+k`
**Type:** Action
**Action:** `delete_word`

**Key:** `ctrl+u`
**Type:** Action
**Action:** `clear_command`

### Example 6: Common Game Commands
Quick access to frequently used commands:

**Key:** `f5`
**Type:** Macro
**Macro Text:** `look\r`

**Key:** `f6`
**Type:** Macro
**Macro Text:** `inv\r`

**Key:** `f7`
**Type:** Macro
**Macro Text:** `exp\r`

**Key:** `f8`
**Type:** Macro
**Macro Text:** `time\r`

## Tips and Best Practices

### Key Combination Tips
1. **Avoid conflicts** - Don't override system shortcuts (Ctrl+C, Ctrl+Z, etc.)
2. **Use modifiers** - Modifier keys (Ctrl, Alt, Shift) provide more options
3. **Function keys** - F1-F12 are ideal for macros (unmodified or with Ctrl/Alt/Shift)
4. **Numpad convenience** - Use numpad keys for directional movement
5. **Logical grouping** - Use similar keys for related commands

### Macro Tips
1. **Always include `\r`** - Add `\r` at the end to auto-submit commands
2. **Test macros** - Verify text is sent correctly before relying on keybind
3. **Multi-step macros** - Separate commands with `\r` for sequences: `north\rlook\r`
4. **Variable text** - Macros send literal text (use Lich for dynamic macros)

### Action Tips
1. **Window management** - Bind `next_window`/`prev_window` for easy navigation
2. **Scrolling** - Bind `scroll_up`/`scroll_down` for hands-free reading
3. **Command recall** - Use `recall_prev_command` for history navigation
4. **Quick bottom** - Bind `scroll_to_bottom` (F5 recommended) to return to live view

### Organization Tips
1. **Document your keybinds** - Add comments in config file
2. **Consistent scheme** - Use consistent modifier patterns (e.g., Alt for movement)
3. **Leave room to grow** - Don't bind every key immediately
4. **Back up your config** - Keybinds are stored in `~/.vellum-fe/configs/`

### Performance Tips
1. **Keybinds are fast** - No performance concerns with many keybinds
2. **Hot-reload** - Changes apply immediately after saving
3. **No limit** - Create as many keybinds as needed

## Troubleshooting

### Key doesn't trigger
- Check key combination format: `ctrl+e` (lowercase, plus signs)
- Verify key name is valid (see Valid Key Names above)
- Check for conflicts with terminal emulator shortcuts
- Try different modifier combination

### Macro doesn't send text
- Verify `\r` is included at end if you want to press Enter
- Check macro text has no typos
- Test by typing the text manually first

### Action does nothing
- Verify action name is spelled correctly
- Check that action applies (e.g., scroll actions require focused window)
- Confirm keybind was saved and reloaded

### Form won't save
- Check key combination field is not empty
- Check key combination is valid format
- Check action or macro text is not empty
- Check for error messages below fields

### Can't edit existing keybind
- Verify key combination matches exactly (case-insensitive)
- Use `.listkeybinds` to see exact key names
- Try deleting and recreating if edit fails

## Related Documentation

- [Window Management](https://github.com/Nisugi/VellumFE/wiki/Window-Management) - Window navigation and operations

## See Also

- [Lich Scripting](https://github.com/elanthia-online/lich-5) - Complex macro system via Lich
- [Crossterm Key Events](https://docs.rs/crossterm/latest/crossterm/event/enum.KeyCode.html) - Full key code reference
