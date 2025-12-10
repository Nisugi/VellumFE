# Roleplay Setup

Create an immersive layout focused on storytelling and social interaction.

## Goal

Build a roleplay-focused layout with:

- Maximum text visibility for story immersion
- Organized chat channels
- Minimal HUD elements
- Easy social command access
- Atmospheric preservation

## Prerequisites

- Completed [Your First Layout](./your-first-layout.md)
- Interest in roleplay and social gameplay
- Basic emote familiarity

## Design Philosophy

Roleplay layouts prioritize:

1. **Immersion** - Large text areas, minimal UI chrome
2. **Readability** - Clear text, comfortable spacing
3. **Organization** - Separate IC (In-Character) and OOC (Out-Of-Character)
4. **Quick Social** - Fast access to emotes and speech

## Layout Overview

```
┌────────────────────────────────────────────────────────────┐
│ [Room Name]                                                │
├────────────────────────────────────────────────────────────┤
│                                                            │
│                                                            │
│                    Main Story Text                         │
│              (room descriptions, actions)                  │
│                                                            │
│                                                            │
│                                                            │
├──────────────────────────────────┬─────────────────────────┤
│     Speech & Thoughts            │    ESP/OOC Chat         │
│     (IC communication)           │    (group chat)         │
├──────────────────────────────────┴─────────────────────────┤
│ > [command input]                                          │
└────────────────────────────────────────────────────────────┘
```

## Step 1: Create the Layout

Create `~/.vellum-fe/layout.toml`:

```toml
# Roleplay Layout - Immersive Storytelling
# Maximizes text space, organizes communication channels

# ═══════════════════════════════════════════════════════════
# TOP - Room Title Bar (Minimal)
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "room"
name = "room_title"
x = 0
y = 0
width = 100
height = 3
show_exits = false
show_creatures = false
show_players = false
title_only = true

# ═══════════════════════════════════════════════════════════
# MAIN AREA - Story Text (Largest Widget)
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "text"
name = "story"
title = ""
x = 0
y = 3
width = 100
height = 55
streams = ["main", "room"]
scrollback = 10000
auto_scroll = true
border = false
padding = 1

# ═══════════════════════════════════════════════════════════
# BOTTOM LEFT - In-Character Communication
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "text"
name = "speech"
title = "Speech"
x = 0
y = 59
width = 55
height = 31
streams = ["speech", "whisper"]
scrollback = 2000
auto_scroll = true
border_color = "blue"

# ═══════════════════════════════════════════════════════════
# BOTTOM RIGHT - Out-Of-Character / ESP
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "text"
name = "thoughts"
title = "ESP/OOC"
x = 56
y = 59
width = 44
height = 31
streams = ["thoughts", "group"]
scrollback = 1000
auto_scroll = true
border_color = "cyan"

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
prompt = "› "
```

## Step 2: Immersive Theme

Create `~/.vellum-fe/colors.toml`:

```toml
[theme]
name = "Parchment"

# Background - soft, easy on eyes
background = "#1a1814"     # Dark parchment

# Text colors - warm and readable
text = "#d4c5a9"           # Cream text
text_dim = "#8b7355"       # Muted brown

# UI elements - unobtrusive
border = "#3d3428"         # Dark border
border_focused = "#5c4a32" # Focused border

# Speech colors - distinguishable but not harsh
speech = "#87ceeb"         # Sky blue (say)
whisper = "#da70d6"        # Orchid (whisper)
thoughts = "#98fb98"       # Pale green (thoughts)
shout = "#ffa500"          # Orange (shout)

# Room descriptions - slightly brighter
room_name = "#ffd700"      # Gold
room_desc = "#d4c5a9"      # Cream

# Player names
player = "#ffffff"         # White
npc = "#daa520"            # Goldenrod

# Emotes and actions
emote = "#b8860b"          # Dark goldenrod

# Minimal status (if shown)
health = "#8fbc8f"         # Dark sea green
mana = "#6495ed"           # Cornflower blue
```

## Step 3: Roleplay Keybinds

Create `~/.vellum-fe/keybinds.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# SPEECH SHORTCUTS
# ═══════════════════════════════════════════════════════════

# Quick say (most common)
[keybinds."enter"]
action = "submit_input"

# Say with prompt
[keybinds."ctrl+s"]
macro = "say $input"

# Whisper
[keybinds."ctrl+w"]
macro = "whisper $input"

# Ask
[keybinds."ctrl+a"]
macro = "ask $input"

# Exclaim
[keybinds."ctrl+e"]
macro = "exclaim $input"

# ═══════════════════════════════════════════════════════════
# ESP/THOUGHTS
# ═══════════════════════════════════════════════════════════

[keybinds."ctrl+t"]
macro = "think $input"

[keybinds."ctrl+g"]
macro = "think [group] $input"

# ═══════════════════════════════════════════════════════════
# COMMON EMOTES - Function Keys
# ═══════════════════════════════════════════════════════════

[keybinds."f1"]
macro = "smile"

[keybinds."f2"]
macro = "nod"

[keybinds."f3"]
macro = "bow"

[keybinds."f4"]
macro = "wave"

[keybinds."f5"]
macro = "laugh"

[keybinds."f6"]
macro = "grin"

[keybinds."f7"]
macro = "sigh"

[keybinds."f8"]
macro = "shrug"

# Targeted emotes
[keybinds."shift+f1"]
macro = "smile $input"

[keybinds."shift+f2"]
macro = "nod $input"

[keybinds."shift+f3"]
macro = "bow $input"

[keybinds."shift+f4"]
macro = "wave $input"

# ═══════════════════════════════════════════════════════════
# ACTIONS AND POSES
# ═══════════════════════════════════════════════════════════

[keybinds."ctrl+1"]
macro = "act $input"

[keybinds."ctrl+2"]
macro = "pose $input"

[keybinds."ctrl+3"]
macro = "emote $input"

# ═══════════════════════════════════════════════════════════
# MOVEMENT (Unobtrusive)
# ═══════════════════════════════════════════════════════════

[keybinds."numpad8"]
macro = "go north"

[keybinds."numpad2"]
macro = "go south"

[keybinds."numpad4"]
macro = "go west"

[keybinds."numpad6"]
macro = "go east"

[keybinds."numpad7"]
macro = "go northwest"

[keybinds."numpad9"]
macro = "go northeast"

[keybinds."numpad1"]
macro = "go southwest"

[keybinds."numpad3"]
macro = "go southeast"

[keybinds."numpad5"]
macro = "go out"

# ═══════════════════════════════════════════════════════════
# LOOK AROUND
# ═══════════════════════════════════════════════════════════

[keybinds."ctrl+l"]
macro = "look"

[keybinds."ctrl+shift+l"]
macro = "look $input"

# ═══════════════════════════════════════════════════════════
# WINDOW NAVIGATION
# ═══════════════════════════════════════════════════════════

[keybinds."page_up"]
action = "scroll_up"

[keybinds."page_down"]
action = "scroll_down"

[keybinds."home"]
action = "scroll_top"

[keybinds."end"]
action = "scroll_bottom"

[keybinds."tab"]
action = "next_widget"

[keybinds."escape"]
action = "focus_input"

# Quick focus to specific windows
[keybinds."alt+1"]
action = "focus_widget"
widget = "story"

[keybinds."alt+2"]
action = "focus_widget"
widget = "speech"

[keybinds."alt+3"]
action = "focus_widget"
widget = "thoughts"
```

## Step 4: Immersive Highlights

Add to `~/.vellum-fe/highlights.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# SPEECH PATTERNS
# ═══════════════════════════════════════════════════════════

# Someone says
[[highlights]]
pattern = '(\\w+) says?, "'
fg = "#87ceeb"

# Someone whispers
[[highlights]]
pattern = "(\\w+) whispers,"
fg = "#da70d6"

# Someone asks
[[highlights]]
pattern = "(\\w+) asks?,"
fg = "#87ceeb"

# Someone exclaims
[[highlights]]
pattern = "(\\w+) exclaims?,"
fg = "#ffa500"

# Quoted speech
[[highlights]]
pattern = '"[^"]*"'
fg = "#ffffff"
bold = false

# ═══════════════════════════════════════════════════════════
# EMOTES AND ACTIONS
# ═══════════════════════════════════════════════════════════

# Emotes (verbs at start of line after name)
[[highlights]]
pattern = "^\\w+ (smiles|nods|bows|waves|laughs|grins|sighs|shrugs)"
fg = "#b8860b"

# ═══════════════════════════════════════════════════════════
# ROOM ELEMENTS
# ═══════════════════════════════════════════════════════════

# Room name (usually in brackets or emphasized)
[[highlights]]
pattern = "^\\[.*\\]$"
fg = "#ffd700"
bold = true

# Obvious exits
[[highlights]]
pattern = "Obvious (exits|paths):"
fg = "#8b7355"

# ═══════════════════════════════════════════════════════════
# PLAYER NAMES
# ═══════════════════════════════════════════════════════════

# Capital names (likely players)
[[highlights]]
pattern = "\\b[A-Z][a-z]{2,}\\b"
fg = "#ffffff"

# ═══════════════════════════════════════════════════════════
# OBJECTS AND ITEMS (Subtle)
# ═══════════════════════════════════════════════════════════

[[highlights]]
pattern = "(a|an|the|some) [a-z]+ [a-z]+"
fg = "#d4c5a9"

# ═══════════════════════════════════════════════════════════
# ESP/THOUGHTS
# ═══════════════════════════════════════════════════════════

[[highlights]]
pattern = "\\[.*?\\]:"
fg = "#98fb98"

# ═══════════════════════════════════════════════════════════
# TIME AND ATMOSPHERE
# ═══════════════════════════════════════════════════════════

[[highlights]]
pattern = "(?i)(dawn|morning|noon|afternoon|dusk|evening|night|midnight)"
fg = "#6495ed"
italic = true

[[highlights]]
pattern = "(?i)(rain|snow|fog|mist|wind|storm|clear|cloudy)"
fg = "#6495ed"
italic = true
```

## Step 5: Social Command List

Add to `~/.vellum-fe/cmdlist.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# PLAYERS - Social Context
# ═══════════════════════════════════════════════════════════

[[cmdlist]]
category = "player"
noun = "^[A-Z][a-z]+$"
match_mode = "regex"
commands = [
    "look",
    "---",
    "Greet>smile {noun},bow {noun},wave {noun},nod {noun}",
    "Say>say (to {noun}) {input},whisper {noun} {input},ask {noun} {input}",
    "---",
    "appraise"
]
priority = 50

# ═══════════════════════════════════════════════════════════
# ITEMS - RP Context
# ═══════════════════════════════════════════════════════════

[[cmdlist]]
category = "item"
noun = ".*"
match_mode = "regex"
commands = [
    "look",
    "get",
    "touch",
    "smell",
    "taste",
    "---",
    "show:show my {noun} to {input}",
    "give:give my {noun} to {input}"
]
priority = 10

# ═══════════════════════════════════════════════════════════
# FURNITURE AND SCENERY
# ═══════════════════════════════════════════════════════════

[[cmdlist]]
category = "furniture"
noun = "(?i)(chair|bench|stool|table|bed|couch|throne)"
match_mode = "regex"
commands = [
    "look",
    "sit on",
    "stand",
    "lean on"
]
priority = 30
```

## Step 6: Optional - Minimal Status

If you want some status without breaking immersion:

```toml
# Add to layout.toml - minimalist status bar

[[widgets]]
type = "progress"
name = "health"
x = 0
y = 56
width = 50
height = 2
data_source = "vitals.health"
color = "health"
show_text = false
border = false

[[widgets]]
type = "progress"
name = "mana"
x = 50
y = 56
width = 50
height = 2
data_source = "vitals.mana"
color = "mana"
show_text = false
border = false
```

This adds thin, borderless bars between story and chat.

## Testing Your Setup

### Roleplay Flow Test

1. **Text Visibility**
   - [ ] Room descriptions fill story window
   - [ ] Text is comfortable to read
   - [ ] Scrolling works smoothly

2. **Communication**
   - [ ] Speech appears in speech window
   - [ ] ESP/thoughts in separate window
   - [ ] Colors distinguish speakers

3. **Quick Actions**
   - [ ] F1-F8 emotes work
   - [ ] Ctrl+S opens say prompt
   - [ ] Movement numpad works

4. **Immersion**
   - [ ] UI doesn't distract
   - [ ] Colors are comfortable
   - [ ] Layout feels spacious

### Social Interaction Test

1. Go to a social area (town square)
2. Use F1 to smile
3. Use Ctrl+S to say something
4. Check speech appears in correct window
5. Verify ESP goes to thoughts window

## Customization Ideas

### Faction-Based Themes

Create themes matching your character's background:

**Elven Theme**:
```toml
background = "#0a1510"
text = "#c0d890"
border = "#2a4530"
```

**Dark Theme**:
```toml
background = "#0d0d0d"
text = "#b0b0b0"
border = "#1a1a1a"
```

### Event-Specific Layouts

For large events, expand thoughts window:

```toml
# Event layout - larger ESP
[[widgets]]
type = "text"
name = "thoughts"
x = 0
y = 60
width = 100
height = 30
streams = ["thoughts", "group"]
```

### Character Macros

Pre-written character expressions:

```toml
[keybinds."ctrl+shift+1"]
macro = "emote adjusts their cloak with practiced ease"

[keybinds."ctrl+shift+2"]
macro = "emote glances around thoughtfully"

[keybinds."ctrl+shift+3"]
macro = "emote inclines their head in greeting"
```

## Troubleshooting

### Text Too Dense

Increase padding:
```toml
padding = 2
```

Or add line spacing if supported.

### Can't See Who's Speaking

Adjust highlight patterns to catch speaker names before quotes.

### ESP Flooding Main Window

Verify stream separation:
- Story: `["main", "room"]`
- Speech: `["speech", "whisper"]`
- Thoughts: `["thoughts", "group"]`

### Emotes Going to Wrong Window

Emotes typically go to "main" stream. If you want them separate, check parser configuration.

## See Also

- [Text Windows](../widgets/text-windows.md)
- [Creating Themes](../customization/creating-themes.md)
- [Highlight Patterns](../customization/highlight-patterns.md)
- [Macros](../automation/macros.md)

