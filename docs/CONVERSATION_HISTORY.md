# Conversation History - profanity-rs Development

This file contains a summary of the development conversation for continuing work on profanity-rs.

## Current Session Summary

### Major Features Implemented

1. **Window Move/Drag** - Click and drag title bar to reposition windows
2. **Dynamic Window Creation/Deletion** - Commands to create and delete windows at runtime
3. **Border Styling** - Command to change window border styles and colors
4. **README Documentation** - Comprehensive documentation for all features

### Recent Changes

#### Window Moving (Completed)
- Added `MoveState` struct to track active move operations
- Implemented `check_title_bar()` to detect clicks on title bar (middle of top border)
- Updated `check_resize_border()` to only resize from corners and edges, not title bar middle
- Added `move_window()` method to update window row/col position
- Mouse drag on title bar now moves window instead of resizing

**Key Implementation Details:**
- Title bar is the top border excluding the first and last character (corners)
- Top corners resize vertically, middle of top border moves window
- Bottom border, left/right edges resize as expected
- Independent window positioning with gaps allowed

#### Dynamic Window Creation/Deletion (Completed)

**Config Changes ([config.rs](C:\Gemstone\profanity-next\src\config.rs)):**
- Added `get_window_template(name)` - Returns WindowDef for predefined templates
- Added `available_window_templates()` - Lists all available templates
- Templates include: main, thoughts, speech, familiar, room, logons, deaths, arrivals, ambients, announcements, loot

**App Commands ([app.rs](C:\Gemstone\profanity-next\src\app.rs)):**
- `.createwindow <name>` or `.createwin <name>` - Create window from template
- `.deletewindow <name>` or `.deletewin <name>` - Delete window
- `.windows` or `.listwindows` - List all active windows

**WindowManager Updates ([window_manager.rs](C:\Gemstone\profanity-next\src\ui\window_manager.rs)):**
- Enhanced `update_config()` to create new TextWindow instances
- Automatically maps streams to windows when created
- Cleans up windows and stream mappings when deleted
- Updates border config on existing windows

#### Border Styling Command (Completed)

**App Command ([app.rs](C:\Gemstone\profanity-next\src\app.rs)):**
- `.border <window> <style> [color]` - Change window border
- Styles: single, double, rounded, thick, none
- Example: `.border main rounded #00ff00`

**TextWindow Update ([text_window.rs](C:\Gemstone\profanity-next\src\ui\text_window.rs)):**
- Added `set_border_config()` method to update borders on existing windows

**WindowManager Update ([window_manager.rs](C:\Gemstone\profanity-next\src\ui\window_manager.rs)):**
- `update_config()` now applies border config changes to existing windows

#### README Documentation (Completed)

Created comprehensive [README.md](C:\Gemstone\profanity-next\README.md) with:
- Installation instructions
- Quick start guide
- Window management documentation (create, delete, move, resize, borders)
- Layout management (save, load, autosave)
- Stream routing table
- Commands reference
- Troubleshooting guide
- Development setup

### Command Reference

#### Window Commands
```
.createwindow <name>      # Create window from template
.createwin <name>         # Short form
.deletewindow <name>      # Delete window
.deletewin <name>         # Short form
.windows                  # List all windows
.listwindows              # Alternative
.border <win> <style> [color]  # Change border
```

#### Layout Commands
```
.savelayout [name]        # Save layout (default: "default")
.loadlayout [name]        # Load layout (default: "default")
.layouts                  # List saved layouts
```

#### Application Commands
```
.quit                     # Exit application
```

### Available Window Templates

1. **main** - Main game output (30x120, streams: main)
2. **thoughts** - Character thoughts (10x40, streams: thoughts)
3. **speech** - Speech and whispers (10x40, streams: speech, whisper)
4. **familiar** - Familiar messages (10x40, streams: familiar)
5. **room** - Room descriptions (10x40, streams: room)
6. **logons** - Login/logout (10x40, streams: logons)
7. **deaths** - Death messages (10x40, streams: deaths)
8. **arrivals** - Character movements (10x40, streams: arrivals)
9. **ambients** - Ambient messages (10x40, streams: ambients)
10. **announcements** - Game announcements (10x40, streams: announcements)
11. **loot** - Loot messages (10x40, streams: loot)

### Mouse Interaction Summary

**Always-on Features:**
- **Click** - Focus window
- **Scroll wheel** - Scroll window under cursor
- **Drag title bar** (middle of top border) - Move window
- **Drag top corners** - Resize vertically from top
- **Drag bottom border** - Resize vertically from bottom
- **Drag left/right edges** - Resize horizontally
- **Shift+drag** - Select text for copying

### Technical Architecture

**Absolute Positioning:**
- Windows use absolute terminal cell coordinates (row, col, rows, cols)
- No proportional grid - windows are independent
- Gaps between windows are allowed
- Overlapping windows are allowed

**Stream Routing:**
- Game streams automatically route to appropriate windows
- Multiple streams can route to same window
- Stream map updated when windows created/deleted

**Layout Persistence:**
- Layouts saved to `~/.profanity-rs/layouts/<name>.toml`
- Autosave on exit to "autosave.toml"
- Autoload "autosave" on startup

### File Changes This Session

1. **src/app.rs**
   - Fixed `check_resize_border()` - only resize from corners/edges, not title bar middle
   - Added `check_title_bar()` - detect title bar clicks
   - Added `move_window()` - update window position
   - Updated mouse event handler for move operations
   - Added `.createwindow`, `.deletewindow`, `.windows`, `.border` commands

2. **src/config.rs**
   - Added `get_window_template()` - returns WindowDef for templates
   - Added `available_window_templates()` - lists available templates
   - Defined 11 window templates with appropriate stream mappings

3. **src/ui/window_manager.rs**
   - Enhanced `update_config()` to create new windows dynamically
   - Added border config updates for existing windows
   - Added window deletion and stream cleanup

4. **src/ui/text_window.rs**
   - Added `set_border_config()` - update borders on existing windows

5. **README.md**
   - Complete rewrite with comprehensive documentation
   - Window management guide
   - Command reference
   - Troubleshooting section

### Known Issues

None currently - all features working as intended.

### Next Steps / Future Enhancements

1. **Widget Types** - Implement other widget types from config:
   - Indicators (status displays with color changes)
   - Progress bars (vitals, spell tracking)
   - Countdown timers (roundtime)
   - Injury doll (color-based or image-based)

2. **Highlight Patterns** - Implement regex-based text highlighting

3. **Keybinds** - Implement custom keybind system

4. **Color Themes** - Add theme support for easy color scheme switching

5. **Status Bar** - Add bottom status bar with indicators

6. **Tab Completion** - Add command/name autocomplete

7. **Window Stacking** - Z-order management for overlapping windows

8. **Snap to Grid** - Optional grid snapping for layout alignment

9. **Window Minimize** - Collapse windows to title bar only

10. **Split Panes** - Optional pane splitting within windows

## Previous Session Context

This session continued from a previous conversation that ended with:
- Grid-based layout implementation (replaced with absolute positioning)
- Mouse support implementation (always-on with Shift+drag for selection)
- Drag-to-resize implementation (all four edges/corners)
- Layout save/load system (autosave + named layouts)
- UTF-8 character boundary fixes in command input

## Development Patterns

### Adding New Commands

1. Add command handler to `handle_dot_command()` in `src/app.rs`
2. Parse command arguments from `parts` vector
3. Validate inputs and show usage if invalid
4. Perform operation on `self.config` or other state
5. Call `update_window_manager_config()` if windows changed
6. Show feedback with `add_system_message()`

### Adding New Window Templates

1. Add template to `get_window_template()` in `src/config.rs`
2. Define WindowDef with appropriate streams
3. Add to `available_window_templates()` list
4. Template automatically available via `.createwindow`

### Mouse Event Handling

1. Check in order: resize borders, title bar, window click
2. Use state structs (ResizeState, MoveState) to track operations
3. Update start positions on each drag event for smooth movement
4. Clear states on mouse up

## Build and Run

```bash
# Build
cargo build --release

# Run with debug logging
RUST_LOG=debug cargo run

# Debug logs location
~/.profanity-rs/debug.log
```

## Configuration Files

- **Main config**: `~/.profanity-rs/config.toml`
- **Layouts**: `~/.profanity-rs/layouts/<name>.toml`
- **Debug logs**: `~/.profanity-rs/debug.log`

## Testing Checklist

When testing new features:
- [ ] Create multiple windows
- [ ] Move windows by dragging title bar
- [ ] Resize from all edges and corners
- [ ] Change border styles with `.border`
- [ ] Delete windows
- [ ] Save and load layouts
- [ ] Check autosave on exit
- [ ] Verify stream routing works
- [ ] Test mouse scroll and focus
- [ ] Test text selection with Shift+drag

## Useful Debug Commands

```
.windows                    # List all windows
.createwindow loot         # Test window creation
.border loot rounded #ff0000  # Test border styling
.savelayout test           # Save current setup
.loadlayout test           # Restore saved setup
.deletewindow loot         # Test deletion
```
