# profanity-rs Development Progress

## Recent Work Sessions

### Session: Progress Bars and Countdown Timers (2025-10-09)

#### Completed Features

##### 1. Progress Bar Enhancements
- **Auto-updating progress bars** - All vitals now update automatically from game XML
  - Health, Mana, Stamina, Spirit, Mind State, Stance, Blood Points, Encumbrance
  - Parser extracts current/max values from `<progressBar>` text field (e.g., "mana 407/407")
  - Fixed issue where percentage (0-100) was used instead of actual current value
  - Blood Points uses `<label>` tag instead of `<progressBar>`

- **ProfanityFE-style color matching**
  - Updated default colors to match ProfanityFE template:
    - Health: `#6e0202` / `#000000`
    - Mana: `#08086d` / `#000000`
    - Stamina: `#bd7b00` / `#000000`
    - Spirit: `#6e727c` / `#000000`
    - Mind State: `#008b8b` / `#000000`
    - Stance: `#000080` / `#000000`
    - Blood Points: `#4d0085` / `#000000`

- **Dynamic encumbrance coloring**
  - Encumbrance bar changes color based on load level:
    - 1-20: Green (`#006400`)
    - 21-40: Yellow (`#a29900`)
    - 41-60: Brown (`#8b4513`)
    - 61-100: Red (`#ff0000`)
  - Color updates automatically when encumbrance changes

- **Progress bar rendering**
  - Uses ProfanityFE-style background coloring on text instead of block characters
  - Shows current/max values in the bar itself
  - Properly fills based on percentage

##### 2. Countdown Timer Widget (New)
- **Created new Countdown widget** (`src/ui/countdown.rs`)
  - ProfanityFE-style character-based fill: N seconds = N characters filled from left
  - Centered countdown number display
  - Uses Unix timestamps for end times
  - Real-time countdown with 100ms poll timeout

- **Countdown templates** for roundtime, casttime, and stun:
  - Roundtime: Red (`#ff0000` / `#000000`)
  - Casttime: Blue (`#0000ff` / `#000000`)
  - Stun: Yellow (`#ffff00` / `#000000`)

- **Auto-updating from game XML**
  - Roundtime: Parses `<roundTime value='timestamp'/>` tags
  - Casttime: Parses `<castTime value='timestamp'/>` tags
  - Updates end time automatically when tags received

- **Manual countdown control**
  - `.setcountdown <window> <seconds>` command for testing
  - Can be set via Lich scripts for custom countdown tracking

##### 3. XML Parser Improvements
- **Fixed progressbar value extraction** (`src/parser.rs`)
  - Previously: Used `value` attribute (percentage 0-100) as current value
  - Now: Extracts actual current/max from text field
  - Example: `<progressBar id='mana' value='100' text='mana 407/407'/>` correctly shows 407/407 instead of 100/407

- **Added roundTime/castTime handlers**
  - Parse Unix timestamp from value attribute
  - Create ParsedElement events for app to handle

- **Added label handler**
  - Used for blood points and other non-progressbar vitals

##### 4. Window Manager Updates
- **Added Countdown to Widget enum** (`src/ui/window_manager.rs`)
  - Widget::Countdown variant
  - Countdown widget creation in WindowManager::new
  - set_countdown() method for Widget

- **Template support for countdown widgets**
  - Can create countdown windows with `.createwindow roundtime`
  - Default layout includes roundtime and casttime windows

##### 5. Configuration Updates
- **Updated config.rs** with all new window templates
  - Progress bar templates with correct colors
  - Countdown timer templates
  - All defaults match ProfanityFE

- **Updated default.toml layout**
  - Includes all progress bars at row 64
  - Includes countdown timers at row 61
  - Proper positioning for full status bar display

#### Files Modified

- `src/config.rs` - Added countdown templates, updated progress bar colors
- `src/ui/countdown.rs` - **NEW FILE** - Countdown widget implementation
- `src/ui/mod.rs` - Added countdown exports
- `src/ui/window_manager.rs` - Added Countdown widget support
- `src/ui/progress_bar.rs` - ProfanityFE-style rendering
- `src/parser.rs` - Fixed progressbar parsing, added roundtime/casttime/label handlers
- `src/app.rs` - Added handlers for progress bars, countdowns, and new commands
- `layouts/default.toml` - Added countdown windows to layout
- `README.md` - Updated documentation for new features

#### Known Issues

##### Countdown Display Not Showing (IN PROGRESS)
- **Symptoms**:
  - Countdown values are being set (confirmed in debug log: "Updated roundtime to end at...")
  - No visual countdown display appears on screen
  - No render debug output in console

- **Investigation**:
  - Countdown windows exist (`.windows` shows them)
  - Countdown windows are in default.toml layout (row 61)
  - Auto-update from XML is working (debug confirms)
  - Widget render method should be called but no eprintln output visible

- **Debugging Added**:
  - Added debug output in WindowManager countdown creation
  - Added debug output in Countdown::render() method
  - Added debug output in Countdown::set_end_time() method
  - Need to check console (not debug.log) for eprintln messages

- **Possible Causes**:
  1. Countdown windows off-screen (row 61 might be below terminal height)
  2. Render method not being called for some reason
  3. Terminal size mismatch
  4. Layout not loading countdown windows properly

- **Next Steps**:
  - Check console output for "Creating countdown widget" messages
  - Check console output for "COUNTDOWN RENDER CALLED" messages
  - Verify terminal height is at least 65+ rows to see row 61
  - May need to adjust layout positions for smaller terminals

#### Commands Added

- `.setbarcolor <window> <color> [bg_color]` - Change progress bar colors (hex format: #RRGGBB)
- `.setcountdown <window> <seconds>` - Manually set countdown timer for testing

#### Technical Details

##### Progress Bar Value Parsing
```rust
// Before (broken): Used percentage as current value
let value = Self::extract_attribute(tag, "value") // This is 0-100!
    .and_then(|v| v.parse::<u32>().ok())
    .unwrap_or(0);

// After (fixed): Extract from text field
let (value, max) = if let Some(slash_pos) = text.rfind('/') {
    let before_slash = &text[..slash_pos];
    let current = before_slash.split_whitespace()
        .rev()
        .find_map(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
        .unwrap_or(percentage);

    let after_slash = &text[slash_pos + 1..];
    let maximum = after_slash.split_whitespace()
        .find_map(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
        .unwrap_or(100);

    (current, maximum)
} else {
    (percentage, 100)
};
```

##### Countdown Timer Rendering
```rust
// ProfanityFE-style: Fill N characters where N = remaining seconds
let remaining = self.remaining_seconds().max(0) as u32;
let fill_chars = (remaining as u16).min(available_width as u16);

// Centered countdown number
let padding_left = (available_width.saturating_sub(value_text.len())) / 2;
let padding_right = available_width.saturating_sub(value_text.len() + padding_left);
let display_text = format!("{}{}{}", " ".repeat(padding_left), value_text, " ".repeat(padding_right));

// Render with colored background on filled portion
for (i, c) in display_text.chars().enumerate() {
    if (i as u16) < fill_chars {
        // Filled: white text on colored background
        buf.set_char(c);
        buf.set_fg(Color::White);
        buf.set_bg(bar_color);
    } else {
        // Empty: gray text on dark background
        buf.set_char(c);
        buf.set_fg(Color::DarkGray);
        buf.set_bg(bg_color);
    }
}
```

##### Encumbrance Dynamic Coloring
```rust
ParsedElement::ProgressBar { id, value, max, text } => {
    if let Some(window) = self.window_manager.get_window(&id) {
        if id == "encumlevel" {
            // Dynamic color based on value
            let color = if value <= 20 {
                "#006400" // Green: 1-20
            } else if value <= 40 {
                "#a29900" // Yellow: 21-40
            } else if value <= 60 {
                "#8b4513" // Brown: 41-60
            } else {
                "#ff0000" // Red: 61-100
            };
            window.set_bar_colors(Some(color.to_string()), Some("#000000".to_string()));
            window.set_progress_with_text(value, max, Some(text.clone()));
        }
    }
}
```

## Upcoming Work

### High Priority
1. **Fix countdown display issue** - Determine why countdowns aren't rendering
2. **Terminal size detection** - Adjust layouts for different terminal sizes
3. **Layout validation** - Warn if windows are positioned off-screen

### Medium Priority
1. **Stun detection** - Add text pattern matching for stun countdown
2. **Additional countdown types** - Disease, poison, other timed effects
3. **Progress bar animations** - Smooth transitions when values change
4. **More dynamic coloring** - Health (red at low HP), Mana (blue gradient), etc.

### Low Priority
1. **Custom countdown scripts** - API for Lich scripts to set countdowns
2. **Countdown alerts** - Flash/notify when countdown expires
3. **Progress bar styles** - Alternative rendering styles (vertical, minimal, etc.)
4. **Color themes** - Pre-defined color schemes for all widgets

## Testing Notes

### Manual Testing Checklist
- [x] Progress bars update from game data
- [x] Progress bars show correct current/max values
- [x] Encumbrance color changes with load level
- [x] Progress bar colors match ProfanityFE
- [x] Countdown values update from game XML
- [ ] Countdown bars display on screen
- [ ] Countdown numbers tick down in real-time
- [ ] Countdown bars fill/empty correctly
- [ ] `.setcountdown` command works
- [ ] `.setbarcolor` command works

### Confirmed Working
- Progress bar auto-updates from `<progressBar>` tags
- Roundtime/casttime updates from `<roundTime>`/`<castTime>` tags
- Value parsing from text field (e.g., "mana 407/407")
- Encumbrance dynamic coloring
- Blood points from `<label>` tag
- Progress bar color customization via `.setbarcolor`

### Known Bugs
- Countdown display not rendering (under investigation)

## Version History

### v0.2.0 (In Progress)
- Added progress bar auto-updates
- Added countdown timer widget
- Added dynamic encumbrance coloring
- Fixed progress bar value parsing
- Updated colors to match ProfanityFE
- Added `.setbarcolor` and `.setcountdown` commands

### v0.1.0
- Initial window management system
- Mouse support (click, drag, resize)
- Layout save/load
- Text windows with stream routing
- Basic progress bars
- XML parsing
- Lich connection
