# Window Types Reference

VellumFE supports multiple widget types, each optimized for different display purposes. This guide details all available window types and their properties.

## Widget Types Overview

| Widget Type | Purpose | Auto-Updates | Mouse Scrolling |
|-------------|---------|--------------|-----------------|
| text | Scrollable game text | ✓ | ✓ |
| progress | Horizontal progress bar | ✓ | ✗ |
| countdown | Animated countdown timer | ✓ | ✗ |
| tabbed | Multi-tab text display | ✓ | ✓ |
| indicator | Single-value status | ✓ | ✗ |
| compass | Directional exits | ✓ | ✗ |
| injury_doll | Wound/scar display | ✓ | ✗ |
| hands | Single hand display | ✓ | ✗ |
| hands_dual | Both hands display | ✓ | ✗ |
| dashboard | Multi-stat display | ✓ | ✗ |
| active_effects | Spell/buff display | ✓ | ✓ |

## Text Windows

**Purpose:** Display scrollable game text with word wrapping and styling.

**Widget Type:** `text`

### Properties

```toml
[[ui.windows]]
name = "main"
widget_type = "text"
streams = ["main"]               # Game streams to display
row = 0
col = 0
rows = 30
cols = 80
buffer_size = 10000              # Lines of scrollback
show_border = true
border_style = "single"
title = "Main"
content_align = "top"            # "top" or "center"
```

### Features

- **Word wrapping** - Automatic text wrapping to window width
- **Scrollback buffer** - Configurable history (default 10,000 lines)
- **Styled text** - Colors, bold, presets from game
- **Mouse scrolling** - Scroll with mouse wheel
- **Keyboard scrolling** - PgUp/PgDn when focused
- **Text selection** - Click and drag to select

### Content Alignment

- **top** (default) - Text starts at top, scrolls normally
- **center** - Centers text until window fills, then scrolls normally

**Use cases:**
- `top` - Standard scrolling text (main window, chat)
- `center` - Short messages that look better centered

### Common Text Windows

**Main game output:**
```bash
.createwindow main
```

**Character thoughts:**
```bash
.createwindow thoughts
```

**Player speech:**
```bash
.createwindow speech
```

**Room descriptions:**
```bash
.createwindow room
```

**Familiar messages:**
```bash
.createwindow familiar
```

**Login/logout notifications:**
```bash
.createwindow logons
```

**Death messages:**
```bash
.createwindow deaths
```

**Custom multi-stream window:**
```bash
.customwindow allchat speech,thoughts,whisper
```

## Progress Bars

**Purpose:** Display vitals and stats as horizontal bars with current/max values.

**Widget Type:** `progress`

### Properties

```toml
[[ui.windows]]
name = "health"
widget_type = "progress"
streams = []                     # Auto-updated, no streams needed
row = 40
col = 0
rows = 1                         # Typically 1 row tall
cols = 30
bar_color = "#00ff00"            # Bar fill color
bar_bg_color = "#003300"         # Bar background color
text_color = "#ffffff"           # Text overlay color
show_border = false              # Usually no border for bars
```

### Features

- **Auto-updates** - Game sends `<progressBar>` XML tags
- **ProfanityFE-style** - Background fills from left
- **Text overlay** - Shows "current/max" or custom text
- **Color coding** - Bar color can change based on value

### Special Behaviors

**Encumbrance bar:**
- Automatically colors based on load:
  - Green: Light load
  - Yellow: Moderate load
  - Brown: Heavy load
  - Red: Very heavy load

**Mind state bar:**
- Can show text like "clear as a bell" instead of numbers
- Custom text from game data

### Common Progress Bars

**Health points:**
```bash
.createwindow health
```

**Mana points:**
```bash
.createwindow mana
```

**Stamina:**
```bash
.createwindow stamina
```

**Spirit points:**
```bash
.createwindow spirit
```

**Mind state:**
```bash
.createwindow mindstate
```

**Encumbrance:**
```bash
.createwindow encumbrance
```

**Combat stance:**
```bash
.createwindow stance
```

**Blood points (betrayer):**
```bash
.createwindow bloodpoints
```

### Manual Control

For testing or custom progress bars:

```bash
.setprogress health 50 100
.setbarcolor health #ff0000 #330000
```

## Countdown Timers

**Purpose:** Display animated countdown timers for roundtime, casting, stun.

**Widget Type:** `countdown`

### Properties

```toml
[[ui.windows]]
name = "roundtime"
widget_type = "countdown"
streams = []                     # Auto-updated, no streams needed
row = 42
col = 0
rows = 1                         # Typically 1 row tall
cols = 60
bar_color = "#ff0000"            # Timer color (red for RT)
countdown_icon = "\u{f0c8}"      # Character to fill with
show_border = false
```

### Features

- **Auto-updates** - Game sends `<roundTime>`, `<castTime>`, `<stun>` tags
- **Character fill** - Fills N characters where N = seconds remaining
- **Centered text** - Shows remaining seconds in center
- **Real-time countdown** - Updates automatically via system time

### Color Conventions

- **Roundtime** - Red (`#ff0000`)
- **Cast time** - Blue (`#0000ff`)
- **Stun** - Yellow (`#ffff00`)

### Fill Characters

Customize the fill character:

```toml
countdown_icon = "\u{f0c8}"      # Square (default, Nerd Font)
countdown_icon = "█"             # Block
countdown_icon = "="             # Equals
countdown_icon = "#"             # Hash
countdown_icon = "\u{f111}"      # Circle (Nerd Font)
```

### Common Countdown Timers

**Roundtime:**
```bash
.createwindow roundtime
```

**Cast time:**
```bash
.createwindow casttime
```

**Stun timer:**
```bash
.createwindow stun
```

### Manual Control

For testing:

```bash
.setcountdown roundtime 5
```

## Tabbed Windows

**Purpose:** Multi-tab text display with unread indicators.

**Widget Type:** `tabbed`

### Properties

```toml
[[ui.windows]]
name = "chat"
widget_type = "tabbed"
streams = []                     # Tabs handle their own streams
row = 0
col = 80
rows = 30
cols = 60
buffer_size = 5000               # Per-tab buffer
show_border = true
title = "Chat"
tab_bar_position = "top"         # "top" or "bottom"
tab_active_color = "#ffff00"     # Active tab color
tab_inactive_color = "#808080"   # Inactive tab color
tab_unread_color = "#ffffff"     # Unread tab color
tab_unread_prefix = "* "         # Unread indicator prefix

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

### Features

- **Multiple tabs** - Each tab has own text buffer
- **Unread indicators** - Tabs with new messages highlighted
- **Click to switch** - Mouse click on tab name
- **Keyboard switch** - `.switchtab chat Speech`
- **Dynamic tabs** - Add/remove tabs at runtime
- **Independent scrolling** - Each tab maintains scroll position

### Tab Management

**Create tabbed window:**
```bash
.createtabbed chat Speech:speech,Thoughts:thoughts,Whisper:whisper
```

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
.switchtab chat 0              # By index
```

### Unread Behavior

- Tab receives text while inactive → Unread indicator appears
- Click tab or switch to it → Unread indicator clears
- Customizable prefix (default: `* `)
- Customizable colors

**Example:**
```
┌─[ * Speech | Thoughts | Whisper ]─────┐
    ↑ Unread indicator
```

### Common Use Cases

**Social chat:**
```bash
.createtabbed social Speech:speech,Thoughts:thoughts,Whisper:whisper
```

**System messages:**
```bash
.createtabbed system LNet:logons,Deaths:deaths,Arrivals:arrivals
```

**Profession-specific:**
```bash
.createtabbed guild Main:main,Familiar:familiar,Guild:guild_channel
```

## Indicator Windows

**Purpose:** Display single-value status indicators.

**Widget Type:** `indicator`

### Properties

```toml
[[ui.windows]]
name = "status"
widget_type = "indicator"
streams = ["status"]
row = 0
col = 0
rows = 3
cols = 20
show_border = true
title = "Status"
```

### Features

- **Simple text display** - Shows single value or short message
- **Auto-updates** - From game stream
- **Compact** - Minimal space usage

### Use Cases

- Status flags (hidden, invisible, etc.)
- Simple counters
- State indicators

## Compass Windows

**Purpose:** Display directional exits with visual highlighting.

**Widget Type:** `compass`

### Properties

```toml
[[ui.windows]]
name = "compass"
widget_type = "compass"
streams = []                     # Auto-updated from game
row = 0
col = 0
rows = 5
cols = 10
show_border = true
title = "Exits"
compass_active_color = "#00ff00"     # Available exits (green)
compass_inactive_color = "#333333"   # Unavailable exits (gray)
```

### Features

- **Cardinal directions** - N, S, E, W, NE, NW, SE, SW, Up, Down, Out
- **Color coding** - Active (green) vs inactive (gray)
- **Auto-updates** - Game sends room exit data
- **Visual layout** - Directional arrangement

### Layout

```
    NW  N  NE
      \ | /
    W --*-- E
      / | \
    SW  S  SE

    U (up) D (down) O (out)
```

### Customization

```bash
# Edit compass window to change colors
.editwindow compass
# Set compass_active_color and compass_inactive_color
```

## Injury Doll

**Purpose:** Display character wounds and scars by body part.

**Widget Type:** `injury_doll`

### Properties

```toml
[[ui.windows]]
name = "injuries"
widget_type = "injury_doll"
streams = []                     # Auto-updated from game
row = 0
col = 0
rows = 20
cols = 30
show_border = true
title = "Injuries"
```

### Features

- **Body diagram** - Visual representation of character
- **Wound display** - Shows injuries by severity
- **Scar display** - Shows permanent scars
- **Color coding** - Injury severity indicated by color

### Body Parts

- Head
- Neck
- Chest
- Abdomen
- Back
- Right arm
- Left arm
- Right hand
- Left hand
- Right leg
- Left leg
- Right eye
- Left eye

## Hands Display

**Purpose:** Show what's in your character's hands.

**Widget Type:** `hands` (single) or `hands_dual` (both)

### Properties

**Single hand:**
```toml
[[ui.windows]]
name = "right_hand"
widget_type = "hands"
streams = []                     # Auto-updated from game
row = 0
col = 0
rows = 3
cols = 30
text_color = "#ffffff"           # Text color
show_border = true
title = "Right Hand"
```

**Both hands:**
```toml
[[ui.windows]]
name = "hands"
widget_type = "hands_dual"
streams = []
row = 0
col = 0
rows = 5
cols = 40
text_color = "#ffffff"
show_border = true
title = "Hands"
```

### Features

- **Auto-updates** - Game sends inventory updates
- **Item display** - Shows held items
- **Empty indication** - Shows when hands are empty
- **Dual display** - Shows both hands in one window

### Use Cases

- Quick inventory check
- Combat readiness
- Spell preparation monitoring

## Dashboard

**Purpose:** Multi-stat display with icons and values.

**Widget Type:** `dashboard`

### Properties

```toml
[[ui.windows]]
name = "dashboard"
widget_type = "dashboard"
streams = []                     # Auto-updated from game
row = 0
col = 0
rows = 5
cols = 80
show_border = true
title = "Stats"
```

### Features

- **Combined stats** - Level, health, mana, stamina, spirit, etc.
- **Compact layout** - Efficient space usage
- **Icons** - Visual indicators for each stat
- **Auto-updates** - Game sends stat updates

### Displayed Stats

- Character level
- Health (current/max)
- Mana (current/max)
- Stamina (current/max)
- Spirit (current/max)
- Experience (current/next level)

### Use Cases

- Overview window
- Quick stat check
- Space-efficient alternative to multiple progress bars

## Active Effects

**Purpose:** Display active spells, buffs, and debuffs.

**Widget Type:** `active_effects`

### Properties

```toml
[[ui.windows]]
name = "effects"
widget_type = "active_effects"
streams = []                     # Auto-updated from game
row = 0
col = 0
rows = 20
cols = 40
show_border = true
title = "Active Effects"
buffer_size = 1000
```

### Features

- **Spell list** - Shows active spells by number/name
- **Duration** - Time remaining (if applicable)
- **Scrollable** - Mouse wheel scrolling
- **Auto-updates** - Game sends spell status updates
- **Color coding** - Buffs vs debuffs (future)

### Use Cases

- Spell tracking
- Buff management
- Debuff monitoring

## Choosing the Right Widget Type

### For Game Text
- **Single stream** → `text` window
- **Multiple related streams** → `tabbed` window
- **Very short messages** → `indicator` window

### For Stats
- **Vitals (health, mana)** → `progress` bar
- **Timers** → `countdown` widget
- **Multiple stats combined** → `dashboard`

### For Inventory
- **Hands** → `hands` or `hands_dual`
- **Full inventory** → `text` window with inventory stream

### For Combat
- **Injuries** → `injury_doll`
- **Spells** → `active_effects`
- **Timers** → `countdown` (roundtime, stun)

### For Navigation
- **Exits** → `compass`
- **Room description** → `text` window with room stream

## See Also

- [Windows and Layouts](Windows-and-Layouts.md) - Window positioning and management
- [Configuration](Configuration.md) - Widget configuration in TOML
- [Commands Reference](Commands.md) - Window creation commands
- [Advanced Streams](Advanced-Streams.md) - Stream routing details
