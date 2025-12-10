# Merchant Setup

Create a layout optimized for trading, crafting, and inventory management.

## Goal

Build a merchant-focused layout with:

- Expanded inventory display
- Transaction logging
- Crafting timer tracking
- Multiple chat channels
- Appraisal quick-access

## Prerequisites

- Completed [Your First Layout](./your-first-layout.md)
- Character with merchant activities
- Understanding of inventory commands

## Layout Overview

```
┌────────────────────────────────────────────────────────────┐
│ Room: [Town Square]                              [Compass] │
├────────────────────┬───────────────────┬───────────────────┤
│                    │                   │                   │
│   Main Game Text   │   Inventory       │   Transaction     │
│                    │   (tree view)     │   Log             │
│                    │                   │                   │
│                    │                   │                   │
├────────────────────┼───────────────────┴───────────────────┤
│   Trade Channel    │   Crafting Timer    │ Wealth Display  │
│                    │   [████████░░ 80%]  │ 12,345 silver   │
├────────────────────┴───────────────────────────────────────┤
│ > [command input]                                          │
└────────────────────────────────────────────────────────────┘
```

## Step 1: Create the Layout

Create `~/.vellum-fe/layout.toml`:

```toml
# Merchant Layout - Trading and Crafting Optimized
# Designed for inventory management and transactions

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
show_creatures = false
show_players = true

[[widgets]]
type = "compass"
name = "compass"
x = 86
y = 0
width = 14
height = 6
style = "minimal"
clickable = true

# ═══════════════════════════════════════════════════════════
# LEFT COLUMN - Main Game Text
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "text"
name = "main"
title = "Game"
x = 0
y = 4
width = 35
height = 55
streams = ["main", "room"]
scrollback = 3000
auto_scroll = true

# ═══════════════════════════════════════════════════════════
# CENTER COLUMN - Inventory
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "inventory"
name = "inventory"
title = "Inventory"
x = 36
y = 4
width = 32
height = 55
show_containers = true
show_weight = true
expand_containers = true
sort_by = "name"

# ═══════════════════════════════════════════════════════════
# RIGHT COLUMN - Transaction Log
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "text"
name = "transactions"
title = "Transactions"
x = 69
y = 4
width = 31
height = 55
streams = ["commerce", "merchant"]
scrollback = 1000
auto_scroll = true
border_color = "yellow"

# ═══════════════════════════════════════════════════════════
# BOTTOM ROW - Trade Chat and Utilities
# ═══════════════════════════════════════════════════════════

# Trade channel
[[widgets]]
type = "text"
name = "trade"
title = "Trade Chat"
x = 0
y = 60
width = 50
height = 30
streams = ["thoughts", "speech"]
scrollback = 500
auto_scroll = true

# Crafting/Activity Timer
[[widgets]]
type = "countdown"
name = "crafting"
title = "Activity"
x = 51
y = 60
width = 24
height = 5
data_source = "roundtime"
show_bar = true

# Wealth Display (Custom text widget)
[[widgets]]
type = "text"
name = "wealth"
title = "Wealth"
x = 76
y = 60
width = 24
height = 10
streams = ["wealth"]
scrollback = 50

# Status indicators (minimal)
[[widgets]]
type = "indicator"
name = "status"
title = "Status"
x = 51
y = 66
width = 49
height = 8
indicators = ["kneeling", "sitting", "encumbered"]
columns = 3

# Vital bars (compact)
[[widgets]]
type = "progress"
name = "health"
title = "HP"
x = 51
y = 75
width = 24
height = 3
data_source = "vitals.health"
color = "health"
show_text = true

[[widgets]]
type = "progress"
name = "stamina"
title = "ST"
x = 76
y = 75
width = 24
height = 3
data_source = "vitals.stamina"
color = "stamina"
show_text = true

# Active effects (spells, buffs)
[[widgets]]
type = "active_effects"
name = "effects"
title = "Effects"
x = 51
y = 79
width = 49
height = 11
show_duration = true
compact = true

# ═══════════════════════════════════════════════════════════
# COMMAND INPUT
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "command_input"
name = "input"
x = 0
y = 91
width = 100
height = 9
history_size = 500
prompt = "$ "
```

## Step 2: Merchant Keybinds

Create `~/.vellum-fe/keybinds.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# INVENTORY MANAGEMENT - Function Keys
# ═══════════════════════════════════════════════════════════

[keybinds."f1"]
macro = "inventory"

[keybinds."f2"]
macro = "inventory full"

[keybinds."f3"]
macro = "look in my backpack"

[keybinds."f4"]
macro = "look in my cloak"

[keybinds."f5"]
macro = "wealth"

[keybinds."f6"]
macro = "experience"

# ═══════════════════════════════════════════════════════════
# TRADING ACTIONS - Ctrl Keys
# ═══════════════════════════════════════════════════════════

[keybinds."ctrl+a"]
macro = "appraise $input"

[keybinds."ctrl+g"]
macro = "get $input"

[keybinds."ctrl+d"]
macro = "drop $input"

[keybinds."ctrl+p"]
macro = "put $input in my backpack"

[keybinds."ctrl+s"]
macro = "sell $input"

[keybinds."ctrl+b"]
macro = "buy $input"

[keybinds."ctrl+o"]
macro = "order $input"

# ═══════════════════════════════════════════════════════════
# CONTAINER SHORTCUTS - Alt Keys
# ═══════════════════════════════════════════════════════════

[keybinds."alt+1"]
macro = "put $input in my backpack"

[keybinds."alt+2"]
macro = "put $input in my cloak"

[keybinds."alt+3"]
macro = "put $input in my sack"

[keybinds."alt+4"]
macro = "put $input in my pouch"

[keybinds."alt+g"]
macro = "get $input from my $input"

# ═══════════════════════════════════════════════════════════
# QUICK COMMERCE
# ═══════════════════════════════════════════════════════════

[keybinds."ctrl+shift+a"]
macro = "appraise all"

[keybinds."ctrl+shift+s"]
macro = "sell all"

# Give to player
[keybinds."ctrl+shift+g"]
macro = "give $input to $input"

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
macro = "go counter"

[keybinds."numpad_minus"]
macro = "go door"

# ═══════════════════════════════════════════════════════════
# SOCIAL/TRADE CHAT
# ═══════════════════════════════════════════════════════════

[keybinds."ctrl+t"]
macro = "think $input"

[keybinds."ctrl+w"]
macro = "whisper $input"

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

[keybinds."escape"]
action = "focus_input"

# Quick focus specific widgets
[keybinds."alt+i"]
action = "focus_widget"
widget = "inventory"

[keybinds."alt+t"]
action = "focus_widget"
widget = "transactions"

[keybinds."alt+m"]
action = "focus_widget"
widget = "main"
```

## Step 3: Commerce Highlights

Add to `~/.vellum-fe/highlights.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# CURRENCY HIGHLIGHTING
# ═══════════════════════════════════════════════════════════

# Silver amounts
[[highlights]]
pattern = "\\d+,?\\d* silver"
fg = "bright_white"
bold = true

# Gold (if applicable)
[[highlights]]
pattern = "\\d+,?\\d* gold"
fg = "bright_yellow"
bold = true

# ═══════════════════════════════════════════════════════════
# TRANSACTION HIGHLIGHTING
# ═══════════════════════════════════════════════════════════

# Purchase
[[highlights]]
pattern = "(?i)you (buy|purchase|acquire)"
fg = "green"

# Sale
[[highlights]]
pattern = "(?i)you (sell|sold)"
fg = "cyan"

# Trade accepted
[[highlights]]
pattern = "(?i)trade (complete|accepted|confirmed)"
fg = "bright_green"
bold = true

# ═══════════════════════════════════════════════════════════
# ITEM HIGHLIGHTING
# ═══════════════════════════════════════════════════════════

# Gems
[[highlights]]
pattern = "(?i)(diamond|ruby|emerald|sapphire|pearl|opal|topaz|garnet)"
fg = "bright_cyan"

# Rare items
[[highlights]]
pattern = "(?i)(rare|unique|enchanted|magical)"
fg = "bright_magenta"

# Containers
[[highlights]]
pattern = "(?i)(backpack|cloak|sack|pouch|bag|chest|box)"
fg = "yellow"

# ═══════════════════════════════════════════════════════════
# MERCHANT NPCS
# ═══════════════════════════════════════════════════════════

[[highlights]]
pattern = "(?i)(merchant|shopkeeper|vendor|trader|clerk)"
fg = "bright_yellow"

# ═══════════════════════════════════════════════════════════
# PLAYER NAMES (Trade Context)
# ═══════════════════════════════════════════════════════════

[[highlights]]
pattern = "\\b[A-Z][a-z]+\\b(?= (offers|gives|trades|whispers))"
fg = "bright_white"
bold = true
```

## Step 4: Transaction Triggers

Add to `~/.vellum-fe/triggers.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# TRANSACTION ALERTS
# ═══════════════════════════════════════════════════════════

# Whisper received
[[triggers]]
name = "whisper_alert"
pattern = "(\\w+) whispers,"
command = ".notify Whisper from $1"
category = "social"
priority = 100

# Trade offer
[[triggers]]
name = "trade_offer"
pattern = "(\\w+) offers to trade"
command = ".notify Trade offer from $1"
category = "commerce"
priority = 90

# Payment received
[[triggers]]
name = "payment"
pattern = "(\\d+) silver"
command = ".log Payment: $1 silver"
category = "commerce"
stream = "commerce"

# ═══════════════════════════════════════════════════════════
# INVENTORY ALERTS
# ═══════════════════════════════════════════════════════════

# Encumbered
[[triggers]]
name = "encumbered"
pattern = "(?i)encumbered"
command = ".notify Encumbered!"
category = "status"
cooldown = 5000

# Container full
[[triggers]]
name = "container_full"
pattern = "(?i)won't fit|too full"
command = ".notify Container full!"
category = "inventory"
cooldown = 2000

# ═══════════════════════════════════════════════════════════
# CRAFTING TIMERS
# ═══════════════════════════════════════════════════════════

# Roundtime for crafting
[[triggers]]
name = "craft_rt"
pattern = "Roundtime: (\\d+)"
command = ".rt $1"
category = "crafting"
```

## Step 5: Command Lists for Items

Add to `~/.vellum-fe/cmdlist.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# GENERAL ITEMS
# ═══════════════════════════════════════════════════════════

[[cmdlist]]
category = "item"
noun = ".*"
match_mode = "regex"
commands = [
    "look",
    "get",
    "drop",
    "---",
    "appraise",
    "sell",
    "---",
    "put in>put {noun} in my backpack,put {noun} in my cloak,put {noun} in my sack"
]
priority = 10

# ═══════════════════════════════════════════════════════════
# CONTAINERS
# ═══════════════════════════════════════════════════════════

[[cmdlist]]
category = "container"
noun = "(?i)(backpack|cloak|sack|pouch|bag|chest|box)"
match_mode = "regex"
commands = [
    "look in",
    "open",
    "close",
    "---",
    "get from:get {input} from my {noun}",
    "inventory check"
]
priority = 50

# ═══════════════════════════════════════════════════════════
# CURRENCY
# ═══════════════════════════════════════════════════════════

[[cmdlist]]
category = "currency"
noun = "(?i)(silver|gold|coins?|copper)"
match_mode = "regex"
commands = [
    "get",
    "count",
    "deposit"
]
priority = 40

# ═══════════════════════════════════════════════════════════
# GEMS AND VALUABLES
# ═══════════════════════════════════════════════════════════

[[cmdlist]]
category = "gems"
noun = "(?i)(gem|jewel|diamond|ruby|emerald|sapphire|pearl|stone)"
match_mode = "regex"
commands = [
    "look",
    "get",
    "appraise",
    "sell",
    "---",
    "put in pouch:put {noun} in my gem pouch"
]
priority = 45

# ═══════════════════════════════════════════════════════════
# PLAYERS (Trade Context)
# ═══════════════════════════════════════════════════════════

[[cmdlist]]
category = "player"
noun = "^[A-Z][a-z]+$"
match_mode = "regex"
commands = [
    "look",
    "---",
    "whisper:whisper {noun} {input}",
    "give:give {input} to {noun}",
    "trade:trade {noun}",
    "---",
    "smile",
    "bow"
]
priority = 60
```

## Testing Your Setup

### Merchant Workflow Test

1. **Inventory Display**
   - [ ] Inventory widget shows items
   - [ ] Container contents expand
   - [ ] Weight displays correctly

2. **Transaction Logging**
   - [ ] Commerce messages appear in transaction log
   - [ ] Trade whispers show in trade chat

3. **Quick Commands**
   - [ ] F1 shows inventory
   - [ ] Ctrl+A prompts for appraise target
   - [ ] Ctrl+S prompts for sell target

4. **Context Menus**
   - [ ] Right-click item shows merchant options
   - [ ] Put in container submenu works
   - [ ] Player right-click shows trade options

### Typical Trade Session

1. Go to town square
2. Check inventory (F1)
3. Appraise items (Ctrl+A)
4. Watch transaction log
5. Test whisper alerts
6. Try context menu trading

## Customization Tips

### For Crafters

Add crafting-specific bindings:

```toml
# Crafting macros
[keybinds."ctrl+c"]
macro = "craft $input"

[keybinds."ctrl+r"]
macro = "repair $input"

[keybinds."ctrl+e"]
macro = "enhance $input"
```

### For Auctioneers

Track auction activity:

```toml
[[triggers]]
name = "auction_bid"
pattern = "(?i)bid.*?(\\d+) silver"
command = ".notify Bid: $1 silver"
category = "auction"
```

### For Gatherers

Quick deposit macros:

```toml
[keybinds."ctrl+shift+d"]
macro = "deposit all"

[keybinds."ctrl+shift+w"]
macro = "withdraw $input silver"
```

## Troubleshooting

### Transaction Log Empty

Check stream configuration:
```toml
streams = ["commerce", "merchant"]
```

If no dedicated streams exist, try:
```toml
streams = ["main"]
```
Then filter with highlights.

### Inventory Not Updating

Inventory requires game state updates:
1. Type `inventory` to refresh
2. Check for parsing errors in logs
3. Verify widget data source

### Context Menu Missing

1. Check cmdlist.toml syntax
2. Verify pattern matches item nouns
3. Reload: `.reload cmdlist`

### Keybind Prompts Not Appearing

Ensure `$input` syntax is correct:
```toml
macro = "sell $input"     # Correct
macro = "sell {input}"    # Wrong
```

## See Also

- [Inventory Widget](../widgets/inventory.md)
- [Command Lists](../automation/cmdlists.md)
- [Triggers](../automation/triggers.md)
- [Keybind Actions](../customization/keybind-actions.md)

