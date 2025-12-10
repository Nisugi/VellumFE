# Display Issues

Solving color, rendering, and visual problems.

## Color Problems

### Colors Don't Appear

**Symptom**: Everything is monochrome

**Causes**:
1. Terminal doesn't support colors
2. `TERM` variable incorrect
3. Color disabled in config

**Solutions**:

1. **Check terminal support**:
   ```bash
   # Check TERM
   echo $TERM
   # Should be xterm-256color, screen-256color, etc.

   # Check color count
   tput colors
   # Should be 256 or higher
   ```

2. **Set correct TERM**:
   ```bash
   export TERM=xterm-256color
   ```

3. **Enable colors in config**:
   ```toml
   [display]
   colors = true
   color_mode = "truecolor"  # or "256" or "16"
   ```

### Wrong Colors

**Symptom**: Colors look different than expected

**Causes**:
1. Terminal color scheme conflicts
2. True color vs 256-color mismatch
3. Theme file issues

**Solutions**:

1. **Match color modes**:
   ```toml
   # For terminals with true color (24-bit)
   [display]
   color_mode = "truecolor"

   # For older terminals
   [display]
   color_mode = "256"
   ```

2. **Check terminal palette**:
   - Terminal color scheme affects named colors
   - Try built-in "Dark" or "Light" preset
   - Use hex colors for consistency:
     ```toml
     [theme]
     background = "#1a1a1a"  # Explicit hex
     ```

3. **Test colors**:
   ```bash
   # Print color test
   for i in {0..255}; do
     printf "\e[48;5;${i}m %3d \e[0m" $i
     [ $((($i + 1) % 16)) -eq 0 ] && echo
   done
   ```

### Washed Out Colors

**Symptom**: Colors look pale or desaturated

**Causes**:
1. Terminal transparency
2. Bold-as-bright setting
3. Theme choices

**Solutions**:

1. **Disable terminal transparency** for accurate colors

2. **Adjust terminal settings**:
   - Disable "Bold as bright" if colors seem too bright
   - Enable "Bold as bright" if bold text is invisible

3. **Increase color saturation in theme**:
   ```toml
   [theme]
   # Use more saturated colors
   health = "#00ff00"    # Bright green
   mana = "#0080ff"      # Bright blue
   ```

### High Contrast / Low Visibility

**Symptom**: Some text hard to read

**Solutions**:
```toml
[theme]
# Increase contrast
background = "#000000"
text = "#ffffff"
text_dim = "#b0b0b0"  # Brighter dim text

# Use accessible color combinations
# Background: #000000, Text: #ffffff (21:1 contrast)
# Background: #1a1a1a, Text: #e0e0e0 (13:1 contrast)
```

## Character Rendering

### Missing Characters (Boxes)

**Symptom**: Characters display as □, ?, or empty boxes

**Cause**: Font missing required glyphs

**Solutions**:

1. **Install complete font**:
   - JetBrains Mono
   - Fira Code
   - Cascadia Code
   - Nerd Font variants (include many symbols)

2. **Set fallback font** in terminal settings

3. **Disable Unicode**:
   ```toml
   [display]
   unicode = false
   ascii_borders = true
   ```

### Wrong Box Drawing Characters

**Symptom**: Borders look wrong or misaligned

**Causes**:
1. Font substitution
2. Line height issues
3. Unicode normalization

**Solutions**:

1. **Use monospace font** designed for terminals

2. **Adjust line spacing**:
   ```yaml
   # Alacritty example
   font:
     offset:
       y: 0  # May need adjustment
   ```

3. **Use ASCII borders**:
   ```toml
   [display]
   border_style = "ascii"
   # Uses +--+ instead of ╔══╗
   ```

### Emoji Display Issues

**Symptom**: Emoji not rendering or wrong width

**Solutions**:

1. **Ensure emoji font installed**:
   - Noto Color Emoji (Linux)
   - Apple Color Emoji (macOS)
   - Segoe UI Emoji (Windows)

2. **Configure terminal**:
   ```yaml
   # Alacritty
   font:
     builtin_box_drawing: true
   ```

3. **Avoid emoji in critical areas**:
   ```toml
   [display]
   emoji_support = false  # Use text instead
   ```

## Layout Problems

### Widget Overlap

**Symptom**: Widgets draw over each other

**Diagnosis**:
```bash
# Start with layout debug
vellum-fe --debug-layout
```

**Solutions**:

1. **Check coordinates**:
   ```toml
   [[widgets]]
   type = "text"
   name = "main"
   x = 0
   y = 0
   width = 70
   height = 85  # Ends at y=85

   [[widgets]]
   type = "progress"
   name = "health"
   x = 0
   y = 86       # Start after main ends
   ```

2. **Use percentage positioning**:
   ```toml
   [[widgets]]
   type = "text"
   x = "0%"
   y = "0%"
   width = "70%"
   height = "85%"
   ```

### Widgets Outside Screen

**Symptom**: Widgets cut off or invisible

**Causes**:
1. Fixed positions larger than terminal
2. Percentage math errors

**Solutions**:

1. **Use percentages for flexible layouts**:
   ```toml
   [[widgets]]
   width = "25%"   # Always fits
   ```

2. **Handle resize**:
   ```toml
   [layout]
   resize_behavior = "scale"  # Rescale widgets
   ```

3. **Check terminal size**:
   ```bash
   echo "Columns: $(tput cols) Rows: $(tput lines)"
   ```

### Wrong Z-Order

**Symptom**: Popup appears behind other widgets

**Solution**:
```toml
[[widgets]]
type = "text"
name = "popup"
z_index = 100  # Higher = on top
```

## Border Issues

### Borders Not Aligned

**Symptom**: Border corners don't meet, gaps appear

**Causes**:
1. Font character width inconsistency
2. Terminal cell size
3. Unicode width calculation

**Solutions**:

1. **Use consistent font**:
   - Pure monospace font
   - Same font for all text

2. **ASCII fallback**:
   ```toml
   [display]
   border_style = "ascii"
   ```

3. **Adjust border characters**:
   ```toml
   [theme.borders]
   horizontal = "─"
   vertical = "│"
   top_left = "┌"
   top_right = "┐"
   bottom_left = "└"
   bottom_right = "┘"
   ```

### Double-Width Character Issues

**Symptom**: CJK characters or emoji break alignment

**Solution**:
```toml
[display]
wide_character_support = true
wcwidth_version = "unicode_15"
```

## Refresh Issues

### Partial Redraws

**Symptom**: Parts of screen don't update

**Causes**:
1. Optimization too aggressive
2. Terminal damage tracking
3. SSH/tmux issues

**Solutions**:

1. **Force full refresh**:
   ```
   # In VellumFE
   .refresh
   ```
   Or keybind:
   ```toml
   [keybinds."ctrl+l"]
   action = "refresh_screen"
   ```

2. **Disable lazy rendering**:
   ```toml
   [performance]
   lazy_render = false
   ```

3. **tmux settings**:
   ```bash
   # In .tmux.conf
   set -g default-terminal "screen-256color"
   set -ag terminal-overrides ",xterm-256color:RGB"
   ```

### Screen Flicker

**Symptom**: Screen flashes during updates

**Solutions**:

1. **Enable double buffering**:
   ```toml
   [display]
   double_buffer = true
   ```

2. **Batch updates**:
   ```toml
   [performance]
   batch_updates = true
   ```

3. **Check terminal**:
   - Some terminals handle rapid updates poorly
   - Try Alacritty, Kitty, or WezTerm

### Ghost Characters

**Symptom**: Old characters remain after widget moves

**Solution**:
```toml
[display]
clear_on_move = true
full_redraw_on_resize = true
```

## Terminal-Specific Issues

### SSH Display Problems

**Symptom**: Display corrupted over SSH

**Solutions**:

1. **Set TERM correctly**:
   ```bash
   ssh -t user@host "TERM=xterm-256color vellum-fe"
   ```

2. **Enable compression**:
   ```bash
   ssh -C user@host
   ```

3. **Reduce color depth**:
   ```toml
   [display]
   color_mode = "256"  # More compatible
   ```

### tmux Display Problems

**Symptom**: Colors or characters wrong in tmux

**Solutions**:

1. **Configure tmux**:
   ```bash
   # .tmux.conf
   set -g default-terminal "tmux-256color"
   set -as terminal-features ",xterm-256color:RGB"
   ```

2. **Start VellumFE correctly**:
   ```bash
   TERM=tmux-256color vellum-fe
   ```

### Screen Display Problems

**Symptom**: Issues in GNU Screen

**Solutions**:
```bash
# .screenrc
term screen-256color
defutf8 on
```

## Accessibility

### High Contrast Mode

```toml
[theme]
name = "high_contrast"
background = "#000000"
text = "#ffffff"
border = "#ffffff"
border_focused = "#ffff00"

health = "#00ff00"
health_low = "#ffff00"
health_critical = "#ff0000"
```

### Large Text

Configure in terminal settings, not VellumFE:
- Increase font size to 14pt+
- VellumFE will adapt to available space

### Screen Reader Hints

```toml
[accessibility]
screen_reader_hints = true
announce_focus_changes = true
```

## Diagnostic Commands

### Test Display Capabilities

```bash
# Color test
printf "\e[38;2;255;0;0mTrue Color Red\e[0m\n"

# Unicode test
echo "╔═══════╗"
echo "║ Test  ║"
echo "╚═══════╝"

# Box drawing test
echo "┌─┬─┐"
echo "├─┼─┤"
echo "└─┴─┘"
```

### Debug Mode

```bash
# Start with display debugging
vellum-fe --debug-display

# Log display operations
TF_LOG_DISPLAY=1 vellum-fe
```

## See Also

- [Colors Configuration](../configuration/colors-toml.md)
- [Themes](../customization/creating-themes.md)
- [Platform Issues](./platform-issues.md)

