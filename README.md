# profanity-rs

A modern, Rust-based terminal client for GemStone IV, built with [Ratatui](https://github.com/ratatui-org/ratatui). This is a complete rewrite of [ProfanityFE](https://github.com/elanthia-online/profanity) with enhanced features and performance.

## Features

- **Dynamic Window Management** - Create, delete, move, and resize windows on the fly
- **Mouse Support** - Click to focus, scroll to navigate, drag to move/resize
- **Text Selection** - Shift+drag to select and copy text
- **Stream Routing** - Game streams automatically route to appropriate windows
- **Layout Management** - Save and load custom window layouts
- **XML Parsing** - Full support for GemStone IV's XML protocol
- **Scrollback Buffer** - Navigate command and window history
- **Live Configuration** - Most settings can be changed without restarting

## Installation

### Prerequisites

- Rust toolchain (1.70+)
- Lich (for connecting to GemStone IV)

### Building from Source

```bash
git clone https://github.com/yourusername/profanity-rs.git
cd profanity-rs
cargo build --release
```

The binary will be at `target/release/profanity-rs`.

## Quick Start

### 1. Start Lich in Detached Mode

**Windows (PowerShell):**
```powershell
C:\Ruby4Lich5\3.4.5\bin\rubyw.exe C:\Ruby4Lich5\Lich5\lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

**Linux/Mac:**
```bash
ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
```

### 2. Launch profanity-rs

```bash
./profanity-rs
```

The client will connect to Lich on `localhost:8000` by default.

### Configuration

On first run, a default config is created at `~/.profanity-rs/config.toml`. You can edit this file to customize:
- Connection settings (host, port)
- Default windows and layouts
- Color schemes and highlights
- Keybinds

## Window Management

### Creating Windows

Use `.createwindow` (or `.createwin`) to create a new window from a template:

```
.createwindow loot
.createwin familiar
```

**Available window templates:**

*Text Windows:*
- `main` - Main game output
- `thoughts` - Character thoughts
- `speech` - Speech and whispers
- `familiar` - Familiar messages
- `room` - Room descriptions
- `logons` - Login/logout messages
- `deaths` - Death messages
- `arrivals` - Character arrivals/departures
- `ambients` - Ambient messages
- `announcements` - Game announcements
- `loot` - Loot messages

*Progress Bars:*
- `health` - Health/HP progress bar (auto-updates from game)
- `mana` - Mana/MP progress bar (auto-updates from game)
- `stamina` - Stamina progress bar (auto-updates from game)
- `spirit` - Spirit progress bar (auto-updates from game)
- `mindstate` - Mind state progress bar (auto-updates from game)
- `encumlevel` - Encumbrance progress bar (auto-updates, dynamic color based on load)
- `stance` - Stance progress bar (auto-updates from game)
- `bloodpoints` - Blood points progress bar (auto-updates from game)

*Countdown Timers:*
- `roundtime` - Roundtime countdown (auto-updates from game)
- `casttime` - Cast time countdown (auto-updates from game)
- `stun` - Stun countdown (set via script)

*Special Widgets:*
- `compass` - Visual compass showing available exits (auto-updates from game)
- `injuries` - Injury doll showing wound/scar locations (auto-updates from game)
- `hands` - Grouped display of left/right/spell hands (auto-updates from game)
- `lefthand` - Individual left hand display (auto-updates from game)
- `righthand` - Individual right hand display (auto-updates from game)
- `spellhand` - Individual prepared spell display (auto-updates from game)

*Status Indicators:*
- `poisoned` - Poison status indicator with icon (auto-updates from game)
- `diseased` - Disease status indicator with icon (auto-updates from game)
- `bleeding` - Bleeding status indicator with icon (auto-updates from game)
- `stunned` - Stunned status indicator with icon (auto-updates from game)
- `webbed` - Webbed status indicator with icon (auto-updates from game)
- `status_dashboard` - Dashboard container showing all status indicators at once

If the template doesn't exist, you'll see a list of available templates.

### Creating Custom Windows

Use `.customwindow` (or `.customwin`) to create a window with custom stream routing:

```
.customwindow combat combat,death
.customwin alerts warning,danger,critical
```

This creates a window with:
- Your specified name
- Custom stream routing (comma-separated, no spaces)
- Default size (10x40) and position (0,0)
- Single-line border
- Move, resize, and style it after creation

### Deleting Windows

Use `.deletewindow` (or `.deletewin`) to remove a window:

```
.deletewindow loot
.deletewin familiar
```

### Listing Windows

View all active windows:

```
.windows
.listwindows
```

### Moving Windows

**With Mouse:**
1. Click and hold on a window's title bar (top border, excluding corners)
2. Drag to move the window
3. Release to place

Windows use absolute positioning, so they can overlap or have gaps between them.

### Resizing Windows

**With Mouse:**
1. Click and hold on a window's edge or corner
   - **Corners**: Resize from that corner
   - **Top/Bottom edges**: Resize vertically
   - **Left/Right edges**: Resize horizontally
2. Drag to resize
3. Release when done

Each window is independent - resizing one doesn't affect others.

### Progress Bars

Progress bars are special widgets for displaying vitals (health, mana, stamina, spirit) or any numeric value with a visual bar. Progress bars automatically update from game XML data (minivitals).

**Creating progress bars:**
```
.createwindow health
.createwindow mana
.createwindow stamina
.createwindow spirit
.createwindow mindstate
.createwindow encumlevel
.createwindow stance
.createwindow bloodpoints
```

**Manual progress updates:**
```
.setprogress health 150 200
.setprogress mana 85 120
```

**Changing bar colors:**
```
.setbarcolor health #ff0000 #000000
.setbarcolor mana #0000ff #1a1a1a
```

Progress bars display:
- A colored bar showing the percentage filled (ProfanityFE-style background coloring)
- Current/max values as text
- Automatic updates from game data
- Special features:
  - **Encumbrance**: Dynamic color changing based on load (green→yellow→brown→red)
  - **Stance**: Shows stance name instead of percentage (defensive/guarded/neutral/forward/advance/offensive)
  - **Mindstate**: Shows descriptive text (e.g., "clear as a bell") instead of numbers
  - **All vitals**: Auto-update from `<progressBar>` XML tags

### Countdown Timers

Countdown timers show remaining time for roundtime, casttime, and stun effects. They use a ProfanityFE-style character-based fill that grows/shrinks with remaining seconds.

**Creating countdown timers:**
```
.createwindow roundtime
.createwindow casttime
.createwindow stun
```

**Manual countdown testing:**
```
.setcountdown roundtime 5
.setcountdown casttime 3
```

Countdown timers:
- Auto-update from `<roundTime>` and `<castTime>` XML tags
- Display remaining seconds centered
- Fill N characters from left where N = remaining seconds
- Colors: RT=red, Cast=blue, Stun=yellow
- Can be set manually via commands or Lich scripts

### Compass Widget

The compass widget shows available exits in a visual compass layout, automatically updating from game XML.

**Creating compass:**
```
.createwindow compass
```

The compass displays:
- Eight directional exits (N, NE, E, SE, S, SW, W, NW)
- OUT exit in the center
- Active exits shown in color
- Inactive/unavailable exits shown dimmed
- Auto-updates from `<compass>` XML tags

### Injury Doll Widget

The injury doll displays character wounds and scars as a visual representation, automatically updating from game XML.

**Creating injury doll:**
```
.createwindow injuries
```

The injury doll shows:
- Body part diagram (head, neck, chest, abdomen, back, limbs)
- Wound levels: `?` (rank 1), `!` (rank 2), `*` (rank 3)
- Scar indicators: `S` prefix
- Color-coded severity (yellow→red based on rank)
- Auto-updates from `<dialogData>` XML tags

### Hands Widget

Display what you're holding and what spell you have prepared.

**Creating hands widgets:**
```
.createwindow hands        # All three in one window
.createwindow lefthand     # Just left hand
.createwindow righthand    # Just right hand
.createwindow spellhand    # Just prepared spell
```

The hands widgets show:
- Left hand: `L: <item>`
- Right hand: `R: <item>`
- Prepared spell: `S: <spell>`
- Auto-updates from `<left>`, `<right>`, and `<spell>` XML tags
- Choose grouped (all 3) or individual displays

### Status Indicators

Status indicators show active conditions with Nerd Font icons. They use a 2-color scheme: black when inactive, colored when active.

**Creating individual indicators:**
```
.createwindow poisoned     # Green poison icon
.createwindow diseased     # Brownish-red disease icon
.createwindow bleeding     # Red blood drop icon
.createwindow stunned      # Yellow lightning bolt icon
.createwindow webbed       # Grey web icon
```

**Creating status dashboard:**
```
.createwindow status_dashboard
```

Status indicators:
- Auto-update from `<dialogData id='IconPOISONED'>` XML tags
- 2-color display: black (off) → color (active)
- Can be displayed individually or in a dashboard
- Dashboard hides inactive indicators by default

**Testing indicators:**
```
.indicatoron               # Force all indicators active
.indicatoroff              # Force all indicators inactive
```

### Dashboard Widget

Dashboards are container widgets that group multiple indicators together with configurable layouts.

**Important:** Dashboards can only be created via `config.toml` - they cannot be created with `.createwindow` because they require complex indicator configuration.

**Dashboard features:**
- Three layout modes: horizontal, vertical, or grid
- Configurable spacing between indicators
- Hide inactive indicators automatically
- Contains multiple indicators in one window

**To create a custom dashboard:**

1. Edit `~/.profanity-rs/config.toml`
2. Add a dashboard window definition:

```toml
[[ui.windows]]
name = "my_dashboard"
widget_type = "dashboard"
streams = []
row = 0
col = 80
rows = 3
cols = 15
buffer_size = 0
show_border = true
border_style = "single"
title = "Status"

# Dashboard-specific settings
dashboard_layout = "horizontal"  # or "vertical" or "grid_2x3"
dashboard_spacing = 1            # spaces between icons
dashboard_hide_inactive = true   # hide inactive indicators

# Define which indicators to include
[[ui.windows.dashboard_indicators]]
id = "poisoned"
icon = "\u{e231}"
colors = ["#000000", "#00ff00"]

[[ui.windows.dashboard_indicators]]
id = "diseased"
icon = "\u{e286}"
colors = ["#000000", "#8b4513"]

[[ui.windows.dashboard_indicators]]
id = "bleeding"
icon = "\u{f043}"
colors = ["#000000", "#ff0000"]
```

**Dashboard layout options:**
- `"horizontal"` - Icons in a row (left to right)
- `"vertical"` - Icons in a column (top to bottom)
- `"grid_RxC"` - Grid layout, e.g., `"grid_2x3"` for 2 rows × 3 columns

**Dashboard settings:**
- `dashboard_spacing` - Number of spaces between icons (default: 1)
- `dashboard_hide_inactive` - Hide indicators when inactive (default: true)
- `dashboard_indicators` - Array of indicator definitions with id, icon, and colors

The built-in `status_dashboard` template includes all 5 status indicators in a horizontal layout.

### Window Borders

Each window can have a border with different styles:

**Available border styles:**
- `single` - Single line border (─│┌┐└┘)
- `double` - Double line border (═║╔╗╚╝)
- `rounded` - Rounded corners (─│╭╮╰╯)
- `thick` - Thick border (━┃┏┓┗┛)
- `none` - No border

**Changing border style:**
```
.border <window> <style> [color]
```

Examples:
```
.border main rounded
.border speech double #00ff00
.border thoughts single
.border loot none
```

### Mouse Features

**Always available:**
- **Click** - Focus a window
- **Scroll wheel** - Scroll window under cursor
- **Drag title bar** - Move window
- **Drag edge/corner** - Resize window
- **Shift+drag** - Select text for copying

### Keyboard Shortcuts

- **Arrow keys** - Move cursor in command input
- **Up/Down** - Navigate command history
- **PageUp/PageDown** - Scroll focused window
- **Home/End** - Jump to start/end of command input
- **Ctrl+C** - Quit application

## Layout Management

### Saving Layouts

Save your current window arrangement:

```
.savelayout mysetup
.savelayout hunting
```

Layouts are saved to `~/.profanity-rs/layouts/<name>.toml`.

### Loading Layouts

Load a previously saved layout:

```
.loadlayout mysetup
.loadlayout hunting
```

### Listing Layouts

View all saved layouts:

```
.layouts
```

### Autosave

Your layout is automatically saved as "autosave" when you exit gracefully and restored when you launch the client.

**Important:** Autosave only works when you exit properly:
- Type `.quit` in the command input, or
- Press `Ctrl+C`

**Note:** Closing the terminal window with the X button will kill the process immediately and prevent autosave from running. Always use `.quit` or `Ctrl+C` to ensure your layout is saved.

## Stream Routing

Game output is divided into streams that route to specific windows:

| Stream | Default Window | Description |
|--------|----------------|-------------|
| `main` | main | General game output |
| `thoughts` | thoughts | Character thoughts |
| `speech` | speech | Speech messages |
| `whisper` | speech | Whisper messages |
| `familiar` | familiar | Familiar messages |
| `room` | room | Room descriptions |
| `logons` | logons | Character logins |
| `deaths` | deaths | Death messages |
| `arrivals` | arrivals | Arrivals/departures |
| `ambients` | ambients | Ambient messages |
| `announcements` | announcements | Game announcements |
| `loot` | loot | Loot messages |

Streams are automatically routed when you create windows. Multiple streams can go to the same window (e.g., `speech` and `whisper` both go to the speech window).

## Advanced Configuration

### Custom Windows in config.toml

You can define custom windows in `config.toml`:

**Available widget types:**
- `text` - Text window with scrollback
- `progress` - Progress bar
- `countdown` - Countdown timer
- `indicator` - Status indicator with icon
- `compass` - Compass display
- `injury_doll` (or `injuries`) - Injury doll display
- `hands` - Grouped hands display
- `lefthand` / `righthand` / `spellhand` - Individual hand displays
- `dashboard` - Dashboard container for indicators

**Example text window:**
```toml
[[ui.windows]]
name = "custom"
widget_type = "text"
streams = ["combat", "assess"]
row = 0
col = 0
rows = 15
cols = 60
buffer_size = 1000
show_border = true
border_style = "rounded"
border_color = "#ff0000"
title = "Combat Log"
```

**Example progress bar:**
```toml
[[ui.windows]]
name = "custom_bar"
widget_type = "progress"
streams = []
row = 0
col = 70
rows = 3
cols = 30
show_border = true
title = "Custom Stat"
bar_color = "#00ff00"
bar_background_color = "#000000"
```

**Example indicator:**
```toml
[[ui.windows]]
name = "custom_indicator"
widget_type = "indicator"
streams = []
row = 5
col = 5
rows = 3
cols = 3
show_border = false
title = "\u{f06d}"  # Nerd Font icon
indicator_colors = ["#000000", "#ff00ff"]  # [off, on]
```

### Highlights

Add regex-based text highlighting in `config.toml`:

```toml
[[highlights]]
pattern = "You swing"
fg = "#ff0000"
bold = true

[[highlights]]
pattern = "^\\[.*?\\]"
fg = "#00ff00"
```

### Keybinds

Define custom keybinds in `config.toml`:

```toml
[[keybinds]]
key = "F1"
command = "stance defensive"

[[keybinds]]
key = "F2"
command = "stance offensive"
```

## Commands Reference

### Window Commands
- `.createwindow <name>` - Create a window from template
- `.customwindow <name> <stream1,stream2,...>` - Create a custom window with specific streams
- `.deletewindow <name>` - Delete a window
- `.windows` - List all active windows
- `.templates` - List available window templates
- `.rename <window> <new title>` - Change window display title
- `.border <window> <style> [color]` - Change window border
- `.setprogress <window> <current> <max>` - Update progress bar value
- `.setbarcolor <window> <color> [bg_color]` - Change progress bar colors
- `.setcountdown <window> <seconds>` - Set countdown timer

### Layout Commands
- `.savelayout [name]` - Save current layout (default: "default")
- `.loadlayout [name]` - Load a saved layout (default: "default")
- `.layouts` - List all saved layouts

### Debug Commands
- `.indicatoron` - Force all status indicators to active state (for testing)
- `.indicatoroff` - Force all status indicators to inactive state (for testing)
- `.randominjuries` (or `.randinjuries`) - Randomly assign 3-8 injuries/scars to the injury doll
- `.randomcompass` (or `.randcompass`) - Randomly assign 2-6 compass exits
- `.randomprogress` (or `.randprog`) - Randomize all progress bars with realistic values
- `.randomcountdowns` (or `.randcountdowns`) - Set random countdowns (15-25 seconds each) for RT/Cast/Stun

### Application Commands
- `.quit` - Exit the application

## Troubleshooting

### Connection Issues

**"Connection refused"**
- Ensure Lich is running and started in detached mode
- Wait 5-10 seconds after launching Lich before starting profanity-rs
- Check that the port matches (default: 8000)

**"Connection reset"**
- Lich may have crashed or disconnected
- Check Lich logs for errors
- Try restarting both Lich and profanity-rs

### Window Issues

**Window not receiving text**
- Check that the stream is mapped to the window
- Use `.windows` to verify the window exists
- Check `config.toml` for correct stream mapping

**Can't move/resize windows**
- Ensure mouse support is working in your terminal
- Try clicking directly on borders/title bar
- Some terminals may have limited mouse support

### Performance Issues

**High CPU usage**
- Try reducing buffer sizes in window configs
- Close unused windows with `.deletewindow`
- Check for runaway highlights (complex regex)

**Scrolling lag**
- Reduce window buffer sizes
- Limit number of visible windows
- Try a different terminal emulator

## Development

### Project Structure

```
src/
├── main.rs           # Entry point
├── app.rs            # Main application loop
├── config.rs         # Configuration management
├── network/          # Lich connection
├── parser/           # XML parsing
└── ui/               # UI components
    ├── text.rs       # Text window widget
    ├── window_manager.rs  # Window management
    ├── command_input.rs   # Command input
    └── layout.rs     # Layout calculation
```

### Building for Development

```bash
cargo build
cargo run
```

### Enabling Debug Logs

Set the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug cargo run
```

Logs are written to `~/.profanity-rs/debug.log`.

## Contributing

Contributions are welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

MIT License - See LICENSE file for details

## Credits

- Original [ProfanityFE](https://github.com/elanthia-online/profanity) by Shaelynne
- Built with [Ratatui](https://github.com/ratatui-org/ratatui)
- For [GemStone IV](https://www.play.net/gs4/) by Simutronics

## Links

- [GemStone IV](https://www.play.net/gs4/)
- [Lich Scripting Engine](https://github.com/elanthia-online/lich-5)
- [Ratatui Documentation](https://ratatui.rs/)
