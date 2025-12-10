# Command Input

The command input widget provides the text entry field for sending commands to the game.

## Overview

The command input:
- Accepts keyboard input for game commands
- Maintains command history
- Supports editing operations (cut, copy, paste)
- Can display a custom prompt

## Configuration

```toml
[[windows]]
name = "input"
type = "command_input"

# Position and size
row = 38
col = 0
width = 120
height = 2

# Input-specific options
prompt = "> "             # Input prompt
history = true            # Enable command history
history_size = 1000       # Commands to remember

# Visual options
show_border = true
border_style = "rounded"
background_color = "#0A0A0A"
text_color = "#FFFFFF"
cursor_color = "#FFFFFF"
```

## Properties

### prompt

The prompt displayed before input:

```toml
prompt = "> "         # Default
prompt = ">> "        # Double arrow
prompt = "[cmd] "     # Custom label
prompt = ""           # No prompt
```

### history

Enable command history:

```toml
history = true        # Enable (default)
history = false       # Disable
```

### history_size

Number of commands to remember:

```toml
history_size = 1000   # Default
history_size = 100    # Smaller history
history_size = 5000   # Large history
```

## Input Operations

### Basic Input

| Key | Action |
|-----|--------|
| `Enter` | Send command |
| `Escape` | Clear input |
| Any text | Insert at cursor |

### Cursor Movement

| Key | Action |
|-----|--------|
| `Left` | Move cursor left |
| `Right` | Move cursor right |
| `Home` | Move to start |
| `End` | Move to end |
| `Ctrl+Left` | Previous word |
| `Ctrl+Right` | Next word |

### Editing

| Key | Action |
|-----|--------|
| `Backspace` | Delete before cursor |
| `Delete` | Delete at cursor |
| `Ctrl+Backspace` | Delete word before |
| `Ctrl+Delete` | Delete word after |
| `Ctrl+U` | Clear line |
| `Ctrl+K` | Delete to end |

### Clipboard

| Key | Action |
|-----|--------|
| `Ctrl+C` | Copy selection |
| `Ctrl+X` | Cut selection |
| `Ctrl+V` | Paste |
| `Ctrl+A` | Select all |

### History

| Key | Action |
|-----|--------|
| `Up` | Previous command |
| `Down` | Next command |
| `Ctrl+R` | Search history (if supported) |

## Display

### Single Line

```
> look
```

Standard single-line input.

### Multi-Line

```toml
[[windows]]
name = "input"
type = "command_input"
height = 3    # Multiple lines visible
```

Shows more context:

```
┌─ Input ────────────────────────────┐
│ > look                             │
│                                    │
└────────────────────────────────────┘
```

### No Border

```toml
[[windows]]
name = "input"
type = "command_input"
show_border = false
show_title = false
height = 1
```

Minimal footprint:

```
> look█
```

## Examples

### Standard Input

```toml
[[windows]]
name = "input"
type = "command_input"
row = 38
col = 0
width = 120
height = 2
prompt = "> "
history = true
show_border = true
border_style = "rounded"
title = "Command"
```

### Full-Width Minimal

```toml
[[windows]]
name = "input"
type = "command_input"
row = 39
col = 0
width = "100%"
height = 1
show_border = false
background_color = "#0A0A0A"
```

### Centered Input

```toml
[[windows]]
name = "input"
type = "command_input"
row = 38
col = 20
width = 80
height = 2
prompt = ">> "
border_style = "double"
```

## Command Processing

### Command Flow

1. User types command
2. User presses Enter
3. Command added to history
4. Command sent to game server
5. Input cleared for next command

### Special Commands

VellumFE intercepts some commands:

| Command | Action |
|---------|--------|
| `;command` | Client command (not sent to game) |
| `/quit` | Exit VellumFE |
| `/reload` | Reload configuration |

## History Features

### Navigation

- `Up` cycles through older commands
- `Down` cycles through newer commands
- Current input is preserved when browsing

### Persistence

Command history is saved between sessions in:
```
~/.vellum-fe/history
```

### Duplicate Handling

Consecutive duplicate commands are not added to history.

## Focus Behavior

The command input:
- Automatically receives focus on startup
- Maintains focus during normal gameplay
- Returns focus when popups close
- Can be focused with `Ctrl+I` or clicking

## Troubleshooting

### Commands not sending

1. Check input has focus (cursor visible)
2. Verify connection is active
3. Check for keybind conflicts with Enter

### History not working

1. Verify `history = true`
2. Check `history_size` > 0
3. Check file permissions on history file

### Cursor not visible

1. Check `cursor_color` is visible
2. Verify widget has focus
3. Check width is sufficient

## See Also

- [Keybinds Configuration](../configuration/keybinds-toml.md) - Input shortcuts
- [Quick Tour](../getting-started/quick-tour.md) - Basic usage
- [Automation](../automation/README.md) - Command automation
