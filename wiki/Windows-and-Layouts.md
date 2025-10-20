# Windows and Layouts

VellumFE uses a flexible window system with absolute positioning. This guide covers window management, types, and layout customization.

## Window Basics

### Absolute Positioning

Unlike traditional grid layouts, VellumFE windows use absolute positioning:

- **row** - Starting row (Y coordinate)
- **col** - Starting column (X coordinate)
- **rows** - Height in rows
- **cols** - Width in columns

**Example:**
```toml
row = 0    # Top of screen
col = 0    # Left of screen
rows = 30  # 30 rows tall
cols = 80  # 80 columns wide
```

### Window Properties

Every window has these properties:

```toml
[[ui.windows]]
name = "main"                    # Unique identifier
widget_type = "text"             # Widget type (see below)
streams = ["main"]               # Game streams to route here
row = 0                          # Y position
col = 0                          # X position
rows = 30                        # Height
cols = 120                       # Width
buffer_size = 10000              # Lines of scrollback
show_border = true               # Show window border
border_style = "single"          # Border style
title = "Main"                   # Title bar text
```

## Widget Types

VellumFE supports multiple widget types for different purposes.

### Text Windows

Display scrollable game text with word wrapping.

```toml
[[ui.windows]]
name = "main"
widget_type = "text"
streams = ["main"]
buffer_size = 10000
content_align = "top"            # or "center" (centers until full)
```

**Common text windows:**
- `main` - Main game output
- `thoughts` - Character thoughts
- `speech` - Player speech
- `room` - Room descriptions
- `familiar` - Familiar messages
- `logons` - Login/logout notifications
- `deaths` - Death messages

See [Window Types](Window-Types.md) for complete list.

### Progress Bars

Display vitals and stats as horizontal bars.

```toml
[[ui.windows]]
name = "health"
widget_type = "progress"
streams = []                     # Auto-updated by game
bar_color = "#00ff00"            # Bar color
bar_bg_color = "#003300"         # Background color
text_color = "#ffffff"           # Text color
```

**Common progress bars:**
- `health` - Hit points
- `mana` - Mana points
- `stamina` - Stamina
- `spirit` - Spirit points
- `mindstate` - Mind state
- `encumbrance` - Encumbrance (auto-colors)
- `stance` - Combat stance
- `bloodpoints` - Blood points (betrayer system)

### Countdown Timers

Display countdowns with animated fill.

```toml
[[ui.windows]]
name = "roundtime"
widget_type = "countdown"
streams = []                     # Auto-updated by game
bar_color = "#ff0000"            # Timer color
countdown_icon = "\u{f0c8}"      # Fill character
```

**Common countdown timers:**
- `roundtime` - Roundtime (red)
- `casttime` - Cast time (blue)
- `stun` - Stun timer (yellow)

### Tabbed Windows

Multi-tab windows with unread indicators.

```toml
[[ui.windows]]
name = "chat"
widget_type = "tabbed"
tab_bar_position = "top"         # or "bottom"
tab_active_color = "#ffff00"     # Active tab
tab_inactive_color = "#808080"   # Inactive tabs
tab_unread_color = "#ffffff"     # Unread tabs
tab_unread_prefix = "* "         # Unread indicator

[[ui.windows.tabs]]
name = "Speech"
stream = "speech"

[[ui.windows.tabs]]
name = "Thoughts"
stream = "thoughts"

[[ui.windows.tabs]]
name = "Whisper"
stream = "whisper"
```

**Features:**
- Each tab has its own text buffer
- Unread indicators for inactive tabs with new messages
- Click tabs to switch
- Keyboard: `.switchtab chat Speech`

### Indicator Windows

Display single-value status indicators.

```toml
[[ui.windows]]
name = "status"
widget_type = "indicator"
streams = ["status"]
```

### Compass Windows

Display directional exits.

```toml
[[ui.windows]]
name = "compass"
widget_type = "compass"
compass_active_color = "#00ff00"     # Active exits (green)
compass_inactive_color = "#333333"   # Inactive exits (gray)
```

Shows available exits in cardinal directions with visual highlighting.

### Injury Doll

Display character wounds and scars.

```toml
[[ui.windows]]
name = "injuries"
widget_type = "injury_doll"
```

### Hands Display

Show what's in your character's hands.

```toml
[[ui.windows]]
name = "hands"
widget_type = "hands"
text_color = "#ffffff"           # Text color
```

Or dual-hand display:

```toml
[[ui.windows]]
name = "both_hands"
widget_type = "hands_dual"
text_color = "#ffffff"
```

### Dashboard

Multi-stat display with icons and values.

```toml
[[ui.windows]]
name = "dashboard"
widget_type = "dashboard"
```

Shows combined stats: level, health, mana, etc.

### Active Effects

Display active spells, buffs, and debuffs.

```toml
[[ui.windows]]
name = "effects"
widget_type = "active_effects"
```

## Window Management Commands

### Listing Windows

```bash
.windows          # List all active windows
.listwindows      # Alias for .windows
.templates        # List available window templates
```

### Creating Windows

**From template:**
```bash
.createwindow thoughts
.createwindow health
.createwindow casttime
```

**Custom text window:**
```bash
.customwindow mywindow main,speech,thoughts
```

**Tabbed window:**
```bash
.createtabbed chat Speech:speech,Thoughts:thoughts,Whisper:whisper
```

### Modifying Windows

**Rename window:**
```bash
.rename main "Game Output"
```

**Change border:**
```bash
.border main single
.border main double
.border main rounded
.border main thick
.border main none
```

**Add border color:**
```bash
.border main single #ff0000
```

### Deleting Windows

```bash
.deletewindow thoughts
```

**Warning:** This permanently removes the window from your layout.

### Tabbed Window Management

**Add tab:**
```bash
.addtab chat LNet logons
```

**Remove tab:**
```bash
.removetab chat LNet
```

**Switch tab:**
```bash
.switchtab chat Speech
.switchtab chat 0             # By index
```

### Window Editor

Open the comprehensive window editor:

```bash
.editwindow main              # Edit existing window
.newwindow                    # Create new window
.addwindow                    # Alias for .newwindow
.editinput                    # Edit command input box
```

**Window Editor Features:**
- Edit all window properties in one place
- Widget-specific fields (only shows relevant options)
- Tab/Shift+Tab to navigate fields
- Enter to save, Esc to cancel
- Draggable via title bar

## Layout Management

### Saving Layouts

**Save with name:**
```bash
.savelayout combat
.savelayout social
.savelayout default
```

Saves to `~/.vellum-fe/layouts/<name>.toml`

**Auto-save on exit:**
VellumFE automatically saves your current layout as `auto_<character>.toml` when you quit.

### Loading Layouts

```bash
.loadlayout combat
.loadlayout social
```

**Tip:** Create multiple layouts for different activities:
- `combat.toml` - Combat-focused layout
- `social.toml` - Roleplay/social layout
- `hunting.toml` - Hunting layout
- `scripting.toml` - Scripting/automation layout

### Listing Layouts

```bash
.layouts
```

Lists all saved layouts in `~/.vellum-fe/layouts/`.

## Layout Examples

### Compact Layout (80x24)

Minimal layout for small terminals:

```toml
[[ui.windows]]
name = "main"
widget_type = "text"
streams = ["main", "room", "speech", "thoughts"]
row = 0
col = 0
rows = 20
cols = 80
show_border = true
title = "Game"

[[ui.windows]]
name = "vitals"
widget_type = "dashboard"
row = 20
col = 0
rows = 3
cols = 80
show_border = true
```

### Widescreen Layout (200x50)

Full-featured layout for large terminals:

```toml
# Main game window (left side)
[[ui.windows]]
name = "main"
widget_type = "text"
streams = ["main"]
row = 0
col = 0
rows = 42
cols = 120
show_border = true
title = "Main"

# Room window (top right)
[[ui.windows]]
name = "room"
widget_type = "text"
streams = ["room"]
row = 0
col = 120
rows = 15
cols = 80
show_border = true
title = "Room"

# Tabbed chat (middle right)
[[ui.windows]]
name = "chat"
widget_type = "tabbed"
row = 15
col = 120
rows = 27
cols = 80
show_border = true
title = "Chat"
tab_bar_position = "top"

[[ui.windows.tabs]]
name = "Speech"
stream = "speech"

[[ui.windows.tabs]]
name = "Thoughts"
stream = "thoughts"

[[ui.windows.tabs]]
name = "Whisper"
stream = "whisper"

# Vitals (bottom)
[[ui.windows]]
name = "health"
widget_type = "progress"
row = 42
col = 0
rows = 1
cols = 30

[[ui.windows]]
name = "mana"
widget_type = "progress"
row = 42
col = 30
rows = 1
cols = 30

[[ui.windows]]
name = "stamina"
widget_type = "progress"
row = 42
col = 60
rows = 1
cols = 30

[[ui.windows]]
name = "spirit"
widget_type = "progress"
row = 42
col = 90
rows = 1
cols = 30

# Timers (bottom)
[[ui.windows]]
name = "roundtime"
widget_type = "countdown"
row = 43
col = 0
rows = 1
cols = 60

[[ui.windows]]
name = "casttime"
widget_type = "countdown"
row = 43
col = 60
rows = 1
cols = 60

# Compass (top right corner)
[[ui.windows]]
name = "compass"
widget_type = "compass"
row = 0
col = 190
rows = 5
cols = 10
show_border = true
```

### Split-Screen Layout

Side-by-side windows for dual focus:

```toml
# Left: Main game
[[ui.windows]]
name = "main"
widget_type = "text"
streams = ["main", "room"]
row = 0
col = 0
rows = 40
cols = 100
show_border = true
title = "Game"

# Right: Social
[[ui.windows]]
name = "social"
widget_type = "text"
streams = ["speech", "thoughts", "whisper"]
row = 0
col = 100
rows = 40
cols = 100
show_border = true
title = "Social"

# Bottom: Vitals
[[ui.windows]]
name = "vitals"
widget_type = "dashboard"
row = 40
col = 0
rows = 5
cols = 200
show_border = true
```

## Window Positioning Tips

### Avoiding Overlaps

Windows can overlap, but it's usually not desired. Check for overlaps:

1. Window A: `row=0, col=0, rows=30, cols=80`
2. Window B: `row=10, col=40, rows=20, cols=60`

Window B overlaps A because:
- B's row (10) is less than A's row + rows (30)
- B's col (40) is less than A's col + cols (80)

### Maximizing Space

To fill your terminal exactly:

1. Check terminal size (rows x cols)
2. Divide space between windows
3. Ensure sum of rows ≤ terminal rows
4. Ensure sum of cols ≤ terminal cols (per row)

### Dynamic Resizing

VellumFE handles terminal resizing automatically:
- Windows maintain their configured positions
- If terminal becomes too small, windows may be partially hidden
- Increasing terminal size reveals hidden windows

### Border Considerations

Borders consume space:
- Border adds 2 to width (left + right)
- Border adds 2 to height (top + bottom)
- Inner content area = configured size - border

**Example:**
```toml
rows = 30    # Total height including border
cols = 80    # Total width including border
```

If `show_border = true`, text area is 28 rows × 78 cols.

## Stream Routing

Windows subscribe to game streams to receive text. Multiple streams can route to one window:

```toml
streams = ["main", "room", "speech"]
```

**Common streams:**
- `main` - Main game output
- `room` - Room descriptions
- `speech` - Player speech
- `thoughts` - Character thoughts
- `whisper` - Whispers
- `familiar` - Familiar messages
- `logons` - Login/logout
- `deaths` - Death messages
- `arrivals` - Arrival messages
- `ambients` - Ambient messages

**Important:** If no window subscribes to a stream, that text is discarded!

See [Advanced Streams](Advanced-Streams.md) for detailed stream routing.

## Best Practices

1. **Start with templates** - Use `.templates` and `.createwindow` for standard windows
2. **Save often** - Use `.savelayout` after arranging windows
3. **Test layouts** - Load with `.loadlayout` to verify before making permanent
4. **Use character configs** - Keep separate layouts per character with `--character`
5. **Plan for resizing** - Test your layout at different terminal sizes
6. **Group related content** - Use tabbed windows for related streams (speech, thoughts, whisper)
7. **Prioritize visibility** - Put critical info (vitals, timers) where you'll see them

## See Also

- [Window Types](Window-Types.md) - Detailed widget type reference
- [Commands Reference](Commands.md) - All window management commands
- [Configuration](Configuration.md) - Window configuration in TOML
- [Advanced Streams](Advanced-Streams.md) - Stream routing deep dive
