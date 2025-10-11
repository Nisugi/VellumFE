# Text Selection

VellumFE provides window-aware text selection with automatic clipboard integration, allowing you to easily copy text from any text window.

## Features

- **Mouse-based selection**: Click and drag to select text
- **Automatic clipboard copy**: Selected text is automatically copied when you release the mouse
- **Window boundaries**: Selection stays within a single window (won't select across windows)
- **Multi-line support**: Select text across multiple lines within the same window
- **Works with wrapped lines**: Correctly handles text that wraps across multiple display lines
- **Native terminal selection**: Shift+Mouse still works for native terminal selection

## How to Use

### Basic Selection

1. **Click and drag** (no modifier keys) in any text window to select text
2. **Release the mouse** - text is automatically copied to your clipboard
3. **Paste** the text anywhere with Ctrl+V (or Cmd+V on Mac)

### Clearing Selection

- **Click anywhere** to clear the selection
- **Press Escape** to clear the selection

### Native Terminal Selection

If you prefer to use your terminal emulator's native selection (which may support features like selecting across windows or other terminal content):

- **Hold Shift while dragging** - VellumFE will pass through to native terminal selection
- This bypasses VellumFE's selection system entirely

## Configuration

Text selection can be configured in your `~/.vellum-fe/configs/default.toml` (or character-specific config):

```toml
[ui]
# Enable or disable VellumFE text selection
selection_enabled = true

# Keep selection within single window boundaries
selection_respect_window_boundaries = true

# Background color for selected text (for future visual highlighting)
selection_bg_color = "#4a4a4a"
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `selection_enabled` | boolean | `true` | Enable or disable VellumFE's text selection system |
| `selection_respect_window_boundaries` | boolean | `true` | Prevent selection from spanning across multiple windows |
| `selection_bg_color` | string | `"#4a4a4a"` | Background color for selected text (reserved for future visual highlighting) |

## Current Limitations

- **No visual highlighting**: Selected text is not currently highlighted with a background color. This is a planned feature that can be added if requested.
- **Text windows only**: Selection only works in text windows, not in progress bars, countdown timers, or other widget types.

## Tips

- **Quick copy**: Click and drag to select, release to copy - no need to press Ctrl+C
- **Scrollback selection**: You can scroll back in history and select older text
- **Window isolation**: Selection automatically stops at window borders, making it easy to select text from a specific window without accidentally including text from adjacent windows
- **Debug logging**: If selection isn't working, enable debug logging with `RUST_LOG=debug` to see detailed mouse event information in `~/.vellum-fe/debug.log`

## Troubleshooting

### Selection doesn't seem to work

1. Make sure `selection_enabled = true` in your config
2. Verify you're clicking in a text window (not on borders, progress bars, etc.)
3. Try without holding Shift (Shift enables native terminal selection)
4. Enable debug logging to see what's happening:
   ```bash
   RUST_LOG=debug cargo run
   ```
   Then check `~/.vellum-fe/debug.log` for selection-related messages

### Can't select across windows

This is intentional! The `selection_respect_window_boundaries` setting (default: `true`) prevents selection from spanning multiple windows. This helps you select text from a specific window without accidentally including text from other windows.

If you need to select text that spans multiple windows, use native terminal selection (Shift+Mouse drag).

### Nothing pastes after selection

1. Make sure you released the mouse button (copy happens on mouse up)
2. Try selecting a larger area to ensure you're actually selecting text
3. Check debug logs to see if clipboard copy is being attempted
4. Test your system clipboard with another application to ensure it's working

## Examples

### Selecting a Combat Message
Click at the start of "You swing", drag to the end of the line, release. The text is now on your clipboard.

### Selecting Multiple Lines
Click at the start of a paragraph, drag down through multiple lines, release. All selected lines are copied with newlines preserved.

### Selecting from Scrollback
Use mouse scroll or Page Up to scroll back in history, then select and copy older text just like current text.

## Related Features

- **Search**: Use Ctrl+F to search within a window (see search documentation)
- **Highlights**: Custom patterns can be highlighted with colors (see Highlight Management documentation)
- **Mouse Mode**: Text selection works in the default mouse mode (when mouse_mode_enabled = false)
