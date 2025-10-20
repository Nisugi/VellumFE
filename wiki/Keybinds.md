# Keybinds Guide

VellumFE's keybind system allows you to map keyboard shortcuts to actions or text macros. This guide covers creating, managing, and using keybinds.

## What are Keybinds?

Keybinds are keyboard shortcuts that trigger either:

- **Actions** - Built-in VellumFE operations (scroll, focus, quit, etc.)
- **Macros** - Text sent to the game (commands, aliases, etc.)

## Managing Keybinds

### Creating Keybinds

```bash
.addkeybind
```

Opens the keybind form with three main fields:

1. **Key Combo** - Keyboard shortcut (e.g., `F1`, `Ctrl+K`)
2. **Action Type** - Action or Macro (radio buttons)
3. **Action/Macro** - What to execute

**Navigation:**
- Tab/Shift+Tab - Navigate fields
- Enter - Save keybind
- Esc - Cancel

### Keybind Form Fields

**Key Combo** (required)
- The keyboard shortcut
- Examples:
  - `F1`, `F2`, ..., `F12` (function keys)
  - `Ctrl+K` (Ctrl + letter)
  - `Alt+S` (Alt + letter)
  - `Ctrl+Shift+L` (modifier combinations)
  - `Esc` (special keys)

**Supported modifiers:**
- `Ctrl` - Control key
- `Alt` - Alt key
- `Shift` - Shift key
- Combinations: `Ctrl+Shift`, `Ctrl+Alt`, `Alt+Shift`, `Ctrl+Alt+Shift`

**Action Type** (required)
- **Action** - Execute built-in VellumFE operation
- **Macro** - Send text to the game

**Action/Macro Field** (required)
- If Action type:
  - Dropdown with built-in actions
  - 23 available actions (see below)
- If Macro type:
  - Text input for command
  - Sent exactly as typed to the game

## Built-in Actions

VellumFE provides 23 built-in actions:

### Scrolling
- **ScrollUp** - Scroll focused window up
- **ScrollDown** - Scroll focused window down
- **PageUp** - Scroll focused window page up
- **PageDown** - Scroll focused window page down

### Window Focus
- **FocusNextWindow** - Cycle to next window (like Tab)
- **FocusPreviousWindow** - Cycle to previous window (like Shift+Tab)

### Text Selection
- **ClearSelection** - Clear current text selection
- **CopySelectedText** - Copy selected text to clipboard

### Display
- **ToggleTimestamps** - Show/hide timestamps on messages
- **ToggleBorders** - Show/hide window borders
- **IncreaseFontSize** - Increase terminal font size (if supported)
- **DecreaseFontSize** - Decrease terminal font size (if supported)

### Layout
- **SaveLayout** - Save current layout as default
- **LoadLayout** - Reload default layout

### Application
- **Quit** - Exit VellumFE

## Keybind Examples

### Function Key Macros

**Quick combat stances:**
```
F1 → stance offensive
F2 → stance defensive
F3 → stance guarded
F4 → stance advance
```

**Travel shortcuts:**
```
F5 → out
F6 → go path
F7 → climb tree
F8 → swim river
```

**Spell casting:**
```
F9 → cast 509
F10 → cast 506
F11 → incant 1711
F12 → prepare 1711
```

### Ctrl Key Combinations

**Window navigation:**
```
Ctrl+N → FocusNextWindow
Ctrl+P → FocusPreviousWindow
```

**Scrolling:**
```
Ctrl+Up → PageUp
Ctrl+Down → PageDown
```

**Layout management:**
```
Ctrl+S → SaveLayout
Ctrl+L → LoadLayout
```

### Alt Key Combinations

**Common commands:**
```
Alt+L → look
Alt+I → inventory
Alt+S → stance
Alt+H → health
```

**Social commands:**
```
Alt+W → wave
Alt+B → bow
Alt+N → nod
```

### Shift Combinations

**Quick loot:**
```
Shift+F1 → get box
Shift+F2 → open box
Shift+F3 → search
Shift+F4 → loot all
```

### Multi-Line Macros

**Complex sequences:**
```
Ctrl+A → stance defensive;health;mana
Ctrl+B → open my backpack;look in my backpack
```

**Note:** Multi-command macros with `;` are sent as single line to game. The game's command parser handles splitting.

## Editing Keybinds

Currently, keybinds can only be edited manually in the config file:

1. Exit VellumFE
2. Open `~/.vellum-fe/configs/<character>.toml`
3. Find `[[keybinds]]` section
4. Edit keybind entries
5. Save and relaunch

**Keybind format:**
```toml
[[keybinds]]
key = "F1"
action_type = "macro"
action = "stance offensive"

[[keybinds]]
key = "Ctrl+N"
action_type = "action"
action = "FocusNextWindow"
```

## Deleting Keybinds

To delete a keybind:

1. Exit VellumFE
2. Open `~/.vellum-fe/configs/<character>.toml`
3. Remove the `[[keybinds]]` entry
4. Save and relaunch

**Future:** Keybind browser/editor planned for easier management.

## Key Combo Syntax

### Basic Keys

**Letters:**
```
A, B, C, ..., Z
a, b, c, ..., z
```

**Numbers:**
```
0, 1, 2, ..., 9
```

**Function Keys:**
```
F1, F2, F3, ..., F12
```

**Special Keys:**
```
Esc
Enter
Space
Tab
Backspace
Delete
Insert
Home
End
PageUp
PageDown
```

**Arrow Keys:**
```
Up
Down
Left
Right
```

### Modifiers

**Single Modifier:**
```
Ctrl+A
Alt+S
Shift+F
```

**Multiple Modifiers:**
```
Ctrl+Shift+A
Ctrl+Alt+S
Alt+Shift+F
Ctrl+Alt+Shift+K
```

**Case Sensitivity:**
- Modifiers are case-insensitive: `Ctrl+A` = `ctrl+a` = `CTRL+A`
- Base keys are case-insensitive for letters: `A` = `a`

### Reserved Keys

Some keys are reserved and cannot be rebound:
- **Enter** (in command input) - Send command
- **Esc** (when popup open) - Close popup
- **Tab** (base, no modifiers) - Cycle window focus

You can bind modified versions:
- `Ctrl+Enter`, `Alt+Enter` ✓
- `Shift+Tab`, `Ctrl+Tab` ✓

## Default Keybinds

VellumFE has minimal default keybinds:

- **Tab** - Cycle window focus forward
- **Shift+Tab** - Cycle window focus backward
- **PgUp** - Scroll focused window up
- **PgDn** - Scroll focused window down
- **Esc** - Clear selection or close popup
- **Ctrl+C** - Copy selected text

These cannot currently be rebound (except Ctrl+C via explicit keybind).

## Keybind Best Practices

### Organization

**Group by purpose:**
```
F1-F4: Combat
F5-F8: Travel
F9-F12: Magic
```

**Consistent patterns:**
```
Ctrl+Key: Actions
Alt+Key: Commands
Shift+Key: Loot/Inventory
```

### Avoid Conflicts

**Terminal conflicts:**
- Many terminals reserve `Ctrl+C` (interrupt)
- Some reserve `Ctrl+Z` (suspend)
- `Ctrl+S`/`Ctrl+Q` (flow control in some terminals)

**Application conflicts:**
- Don't override critical VellumFE keys
- Test keybinds after creating

### Muscle Memory

**Common placements:**
```
F1-F4: Left hand reach
F5-F8: Comfortable reach
F9-F12: Stretch reach (less frequent)
```

**Home row bias:**
```
Ctrl+A, S, D, F: Easy left hand
Ctrl+H, J, K, L: Easy right hand
```

### Character-Specific Keybinds

Use `--character` for profession-specific keybinds:

**Warrior:**
```
F1 → berserk
F2 → warcry
F3 → weapon bonding
```

**Wizard:**
```
F1 → prep 901
F2 → prep 910
F3 → prep 920
```

**Cleric:**
```
F1 → pray
F2 → cast 301
F3 → cast 303
```

## Advanced Keybind Techniques

### Chained Commands

Use game's command separator (`;`):
```
Macro: stance defensive;health;mana
```

Sends: `stance defensive;health;mana` (game parses as 3 commands)

### Conditional Macros

Some games support conditional syntax:
```
Macro: {if stance=offensive then stance defensive else stance offensive}
```

Check your game's scripting documentation.

### Keybind Variables

Not currently supported, but planned:
```
Macro: cast {target}
```

Would require variable substitution system.

## Troubleshooting

### Keybind Not Working

**Check key combo:**
1. Verify syntax: `Ctrl+A`, not `Ctrl-A` or `Ctrl A`
2. Check case: `Ctrl+a` works, `CTRL+A` works
3. Test in keybind form (validates on save)

**Check for conflicts:**
1. Terminal may intercept key
2. Another keybind may override
3. Try different key combination

**Check action type:**
1. Action must exactly match built-in action name
2. Macro sends text as-is to game
3. Verify action_type in config file

### Key Not Registering

**Terminal not sending key:**
- Some terminals don't support all key combos
- Try different terminal emulator
- Check terminal key mapping settings

**Modifier not working:**
- Verify modifier key is pressed first
- Some keyboards have modifier issues
- Try different modifier (Alt instead of Ctrl)

### Macro Not Executing

**Macro not sent:**
- Check macro text in config file
- Verify quotes are correct in TOML
- Test macro by typing manually

**Game not recognizing command:**
- Macro sent exactly as written
- Check game's command syntax
- Test command in game first

### Action Not Executing

**Action name wrong:**
- Must exactly match built-in action
- Check spelling and case
- Refer to built-in actions list above

**Action not implemented:**
- Some actions may not be fully implemented
- Check debug log for errors
- Report issue if action doesn't work

## Configuration File Format

Keybinds are stored in `~/.vellum-fe/configs/<character>.toml`:

```toml
[[keybinds]]
key = "F1"                       # Key combination
action_type = "macro"            # "macro" or "action"
action = "stance offensive"      # Macro text or action name

[[keybinds]]
key = "Ctrl+N"
action_type = "action"
action = "FocusNextWindow"

[[keybinds]]
key = "Alt+L"
action_type = "macro"
action = "look"
```

**Multiple keybinds:**
```toml
[[keybinds]]
key = "F1"
action_type = "macro"
action = "stance offensive"

[[keybinds]]
key = "F2"
action_type = "macro"
action = "stance defensive"

[[keybinds]]
key = "F3"
action_type = "macro"
action = "stance guarded"
```

## Future Features

Planned keybind enhancements:

- **Keybind browser** - GUI for viewing/editing keybinds
- **Keybind groups** - Organize keybinds by category
- **Variable substitution** - `{target}`, `{spell}`, etc.
- **Conditional keybinds** - Context-aware behavior
- **Keybind export/import** - Share keybind sets
- **More actions** - Additional built-in actions

## See Also

- [Commands Reference](Commands.md) - Dot commands
- [Configuration](Configuration.md) - Config file format
- [Highlights](Highlights.md) - Text highlighting system
- [Getting Started](Getting-Started.md) - Basic controls
