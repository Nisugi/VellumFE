# Hunting Setup

Create an optimized layout for combat and hunting with real-time status monitoring.

## Goal

Build a hunting-focused layout with:

- Combat text in a dedicated window
- Real-time vital monitoring with alerts
- Quick-access combat macros
- Target tracking
- Roundtime and casttime displays
- Injury awareness

## Prerequisites

- Completed [Your First Layout](./your-first-layout.md)
- Understanding of basic keybinds
- Active hunting character

## Layout Overview

```
┌────────────────────────────────────────────────────────────┐
│ Room: [name]                    Exits: N S E W    [Compass]│
├────────────────────────────────────┬───────────────────────┤
│                                    │ ████████████ Health   │
│                                    │ ████████░░░░ Mana     │
│       Main Game Text               │ ████████████ Stamina  │
│                                    ├───────────────────────┤
│                                    │ RT: 3s    Cast: --    │
│                                    ├───────────────────────┤
├────────────────────────────────────┤ [Hidden] [Prone]      │
│       Combat Log                   │ [Stunned] [Webbed]    │
│       (filtered combat text)       ├───────────────────────┤
│                                    │ Head:  ░░░░░          │
│                                    │ Chest: ██░░░          │
│                                    │ Arms:  ░░░░░          │
├────────────────────────────────────┴───────────────────────┤
│ > [command input]                                          │
└────────────────────────────────────────────────────────────┘
```

## Step 1: Create the Layout

Create `~/.vellum-fe/layout.toml`:

```toml
# Hunting Layout - Combat Optimized
# Designed for active hunting with real-time monitoring

# ═══════════════════════════════════════════════════════════
# TOP BAR - Room and Navigation
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "room"
name = "room_info"
x = 0
y = 0
width = 85
height = 4
show_exits = true
show_creatures = true
creature_highlight = true

[[widgets]]
type = "compass"
name = "compass"
x = 86
y = 0
width = 14
height = 7
style = "unicode"
clickable = true

# ═══════════════════════════════════════════════════════════
# MAIN AREA - Game Text
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "text"
name = "main"
title = "Game"
x = 0
y = 4
width = 65
height = 50
streams = ["main", "room", "thoughts", "speech"]
scrollback = 5000
auto_scroll = true

# ═══════════════════════════════════════════════════════════
# COMBAT WINDOW - Filtered Combat Text
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "text"
name = "combat"
title = "Combat"
x = 0
y = 55
width = 65
height = 35
streams = ["combat"]
scrollback = 1000
auto_scroll = true
border_color = "red"

# ═══════════════════════════════════════════════════════════
# RIGHT SIDEBAR - Status Panel
# ═══════════════════════════════════════════════════════════

# Vital Bars
[[widgets]]
type = "progress"
name = "health"
title = "HP"
x = 66
y = 8
width = 34
height = 3
data_source = "vitals.health"
color = "health"
show_text = true
show_percentage = true

[[widgets]]
type = "progress"
name = "mana"
title = "MP"
x = 66
y = 12
width = 34
height = 3
data_source = "vitals.mana"
color = "mana"
show_text = true
show_percentage = true

[[widgets]]
type = "progress"
name = "stamina"
title = "ST"
x = 66
y = 16
width = 34
height = 3
data_source = "vitals.stamina"
color = "stamina"
show_text = true
show_percentage = true

[[widgets]]
type = "progress"
name = "spirit"
title = "SP"
x = 66
y = 20
width = 34
height = 3
data_source = "vitals.spirit"
color = "spirit"
show_text = true
show_percentage = true

# Timing Displays
[[widgets]]
type = "countdown"
name = "roundtime"
title = "RT"
x = 66
y = 24
width = 16
height = 3
data_source = "roundtime"
warning_threshold = 2
critical_threshold = 0

[[widgets]]
type = "countdown"
name = "casttime"
title = "Cast"
x = 83
y = 24
width = 17
height = 3
data_source = "casttime"

# Status Indicators
[[widgets]]
type = "indicator"
name = "status"
title = "Status"
x = 66
y = 28
width = 34
height = 10
indicators = [
    "hidden",
    "invisible",
    "stunned",
    "webbed",
    "prone",
    "kneeling",
    "sitting",
    "bleeding",
    "poisoned"
]
columns = 2

# Injury Display
[[widgets]]
type = "injury_doll"
name = "injuries"
title = "Injuries"
x = 66
y = 39
width = 34
height = 20
style = "bars"
show_scars = false

# Active Effects
[[widgets]]
type = "active_effects"
name = "effects"
title = "Active"
x = 66
y = 60
width = 34
height = 30
show_duration = true
group_by_type = true

# ═══════════════════════════════════════════════════════════
# BOTTOM - Command Input
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "command_input"
name = "input"
x = 0
y = 91
width = 100
height = 9
history_size = 500
prompt = "› "
show_roundtime = true
```

## Step 2: Combat Keybinds

Create combat-focused `~/.vellum-fe/keybinds.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# COMBAT MACROS - Function Keys
# ═══════════════════════════════════════════════════════════

# Basic attacks
[keybinds."f1"]
macro = "attack target"

[keybinds."f2"]
macro = "attack left target"

[keybinds."f3"]
macro = "attack right target"

[keybinds."f4"]
macro = "feint target"

# Defensive actions
[keybinds."f5"]
macro = "stance defensive"

[keybinds."f6"]
macro = "hide"

[keybinds."f7"]
macro = "stance offensive"

[keybinds."f8"]
macro = "stance neutral"

# Combat utilities
[keybinds."f9"]
macro = "search"

[keybinds."f10"]
macro = "search;loot"

[keybinds."f11"]
macro = "skin"

[keybinds."f12"]
macro = "aim target"

# ═══════════════════════════════════════════════════════════
# SPELL MACROS - Ctrl+Number
# ═══════════════════════════════════════════════════════════

[keybinds."ctrl+1"]
macro = "incant 101"

[keybinds."ctrl+2"]
macro = "incant 102"

[keybinds."ctrl+3"]
macro = "incant 103"

[keybinds."ctrl+4"]
macro = "incant 104"

[keybinds."ctrl+5"]
macro = "incant 105"

[keybinds."ctrl+6"]
macro = "incant 106"

[keybinds."ctrl+7"]
macro = "incant 107"

[keybinds."ctrl+8"]
macro = "incant 108"

[keybinds."ctrl+9"]
macro = "incant 109"

[keybinds."ctrl+0"]
macro = "incant 110"

# ═══════════════════════════════════════════════════════════
# MOVEMENT - Numpad
# ═══════════════════════════════════════════════════════════

[keybinds."numpad8"]
macro = "north"

[keybinds."numpad2"]
macro = "south"

[keybinds."numpad4"]
macro = "west"

[keybinds."numpad6"]
macro = "east"

[keybinds."numpad7"]
macro = "northwest"

[keybinds."numpad9"]
macro = "northeast"

[keybinds."numpad1"]
macro = "southwest"

[keybinds."numpad3"]
macro = "southeast"

[keybinds."numpad5"]
macro = "out"

[keybinds."numpad_plus"]
macro = "go gate"

[keybinds."numpad_minus"]
macro = "go door"

# ═══════════════════════════════════════════════════════════
# EMERGENCY ACTIONS - Shift+Function
# ═══════════════════════════════════════════════════════════

[keybinds."shift+f1"]
macro = "flee"

[keybinds."shift+f2"]
macro = "stance defensive;hide"

[keybinds."shift+f3"]
macro = "get acantha from my pouch;eat my acantha"

[keybinds."shift+f4"]
macro = "get basal from my pouch;eat my basal"

[keybinds."shift+f5"]
macro = "get cactacae from my pouch;eat my cactacae"

# ═══════════════════════════════════════════════════════════
# TARGETING - Alt+Keys
# ═══════════════════════════════════════════════════════════

[keybinds."alt+1"]
macro = "target first"

[keybinds."alt+2"]
macro = "target second"

[keybinds."alt+3"]
macro = "target third"

[keybinds."alt+t"]
macro = "target $input"

[keybinds."alt+c"]
macro = "target clear"

# ═══════════════════════════════════════════════════════════
# WIDGET NAVIGATION
# ═══════════════════════════════════════════════════════════

[keybinds."tab"]
action = "next_widget"

[keybinds."shift+tab"]
action = "prev_widget"

[keybinds."page_up"]
action = "scroll_up"

[keybinds."page_down"]
action = "scroll_down"

[keybinds."home"]
action = "scroll_top"

[keybinds."end"]
action = "scroll_bottom"

[keybinds."escape"]
action = "focus_input"
```

## Step 3: Combat Highlights

Add to `~/.vellum-fe/highlights.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# COMBAT HIGHLIGHTING
# ═══════════════════════════════════════════════════════════

# Critical hits
[[highlights]]
pattern = "\\*\\* .+ \\*\\*"
fg = "bright_red"
bold = true

# Your attacks
[[highlights]]
pattern = "You (swing|thrust|slash|punch|kick)"
fg = "green"

# Enemy attacks
[[highlights]]
pattern = "(swings|thrusts|slashes|punches|kicks) at you"
fg = "red"

# Misses
[[highlights]]
pattern = "(miss|barely miss|narrowly miss)"
fg = "dark_gray"
italic = true

# Death messages
[[highlights]]
pattern = "falls dead"
fg = "bright_yellow"
bold = true

# ═══════════════════════════════════════════════════════════
# STATUS ALERTS
# ═══════════════════════════════════════════════════════════

# Stunned
[[highlights]]
pattern = "(?i)you are stunned"
fg = "black"
bg = "yellow"
bold = true

# Webbed
[[highlights]]
pattern = "(?i)webs? (stick|entangle)"
fg = "black"
bg = "magenta"
bold = true

# Prone
[[highlights]]
pattern = "(?i)(knock|fall).*?(down|prone)"
fg = "black"
bg = "cyan"
bold = true

# ═══════════════════════════════════════════════════════════
# DAMAGE NUMBERS
# ═══════════════════════════════════════════════════════════

# High damage
[[highlights]]
pattern = "\\b([5-9]\\d|[1-9]\\d{2,}) points? of damage"
fg = "bright_green"
bold = true

# Medium damage
[[highlights]]
pattern = "\\b([2-4]\\d) points? of damage"
fg = "green"

# Low damage
[[highlights]]
pattern = "\\b([0-1]\\d) points? of damage"
fg = "dark_green"

# ═══════════════════════════════════════════════════════════
# LOOT
# ═══════════════════════════════════════════════════════════

# Coins
[[highlights]]
pattern = "\\d+ (silver|gold|copper)"
fg = "bright_yellow"

# Gems
[[highlights]]
pattern = "(?i)(gem|jewel|diamond|ruby|emerald|sapphire)"
fg = "bright_cyan"

# Treasure
[[highlights]]
pattern = "(?i)treasure"
fg = "bright_yellow"
bold = true
```

## Step 4: Combat Triggers

Add to `~/.vellum-fe/triggers.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# COMBAT ALERTS
# ═══════════════════════════════════════════════════════════

# Stunned alert
[[triggers]]
name = "stun_alert"
pattern = "(?i)you are stunned"
command = ".notify STUNNED!"
category = "combat"
priority = 100
cooldown = 1000

# Webbed alert
[[triggers]]
name = "web_alert"
pattern = "(?i)webs? (stick|entangle)"
command = ".notify WEBBED!"
category = "combat"
priority = 100
cooldown = 1000

# Prone alert
[[triggers]]
name = "prone_alert"
pattern = "(?i)(knock|fall).*?(down|prone)"
command = ".notify PRONE!"
category = "combat"
priority = 100
cooldown = 1000

# Low health warning
[[triggers]]
name = "low_health"
pattern = "You feel (weak|faint|dizzy)"
command = ".notify LOW HEALTH!;stance defensive"
category = "combat"
priority = 100

# ═══════════════════════════════════════════════════════════
# HUNTING AUTOMATION (Optional - Use Carefully)
# ═══════════════════════════════════════════════════════════

# Auto-search on kill (disabled by default)
[[triggers]]
name = "auto_search"
pattern = "falls dead"
command = "search"
category = "loot"
cooldown = 2000
enabled = false

# Roundtime notification
[[triggers]]
name = "roundtime"
pattern = "Roundtime: (\\d+)"
command = ".rt $1"
category = "status"
```

## Step 5: Color Theme for Combat

Add combat-focused colors to `~/.vellum-fe/colors.toml`:

```toml
[theme]
name = "Combat Focus"

# Vital colors - easy visibility
health = "#00ff00"          # Bright green
health_low = "#ffff00"      # Yellow warning
health_critical = "#ff0000" # Red danger
mana = "#0080ff"            # Blue
stamina = "#ff8000"         # Orange
spirit = "#ff00ff"          # Magenta

# Combat window
combat_border = "#ff4444"
combat_bg = "#1a0000"

# Status indicators
hidden = "#00ff00"
stunned = "#ffff00"
webbed = "#ff00ff"
prone = "#00ffff"

# High contrast text
text = "#ffffff"
text_dim = "#808080"
```

## Testing Your Setup

### Combat Test Checklist

1. **Vital Monitoring**
   - [ ] Health bar updates when damaged
   - [ ] Mana bar updates when casting
   - [ ] Low health triggers visual warning

2. **Combat Window**
   - [ ] Combat text appears in combat window
   - [ ] Non-combat text stays in main window
   - [ ] Combat highlights are visible

3. **Roundtime Display**
   - [ ] RT countdown shows during actions
   - [ ] Cast timer shows during spellcasting

4. **Status Indicators**
   - [ ] Hidden indicator shows when hiding
   - [ ] Stun indicator triggers on stun

5. **Keybinds**
   - [ ] F1 attacks target
   - [ ] Numpad moves correctly
   - [ ] Emergency flee works

### Combat Workflow Test

1. Find a creature
2. Use F1 to attack
3. Watch combat window for hit/miss
4. Check RT countdown
5. Use numpad to navigate away
6. Verify flee (Shift+F1) works

## Customization Tips

### Profession-Specific Adjustments

**Rogues**: Prioritize hidden indicator, add ambush macros

```toml
[keybinds."f1"]
macro = "ambush target"

[keybinds."f2"]
macro = "hide;ambush target"
```

**Wizards**: Add spell tracking, larger mana display

```toml
[[widgets]]
type = "spells"
name = "known_spells"
x = 66
y = 60
width = 34
height = 30
```

**Warriors**: Add CM tracking, shield macros

```toml
[keybinds."f4"]
macro = "cman surge"

[keybinds."f5"]
macro = "cman feint"
```

### Multi-Target Hunting

For areas with multiple creatures:

```toml
# Target cycling
[keybinds."alt+n"]
macro = "target next"

[keybinds."alt+p"]
macro = "target previous"
```

## Troubleshooting

### Combat Text Not Filtering

Verify stream configuration:
- Main window: `streams = ["main", "room", "thoughts", "speech"]`
- Combat window: `streams = ["combat"]`

### Roundtime Not Showing

Check data source is correct:
```toml
data_source = "roundtime"
```

### Keybinds Not Responding During Combat

Ensure focus is on input widget:
```toml
[keybinds."escape"]
action = "focus_input"
```

### Triggers Not Firing

1. Check trigger syntax
2. Verify `enabled = true` (default)
3. Test pattern matches actual game text

## See Also

- [Combat Alerts Cookbook](../cookbook/combat-alerts.md)
- [Progress Bars](../widgets/progress-bars.md)
- [Indicators](../widgets/indicators.md)
- [Triggers](../automation/triggers.md)

