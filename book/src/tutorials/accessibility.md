# Accessibility Setup

Configure VellumFE for screen readers, high contrast needs, and other accessibility requirements.

## Goal

Create an accessible setup with:

- Screen reader integration (TTS)
- High contrast color themes
- Keyboard-only navigation
- Large text options
- Simplified layouts

## Prerequisites

- VellumFE installed
- Screen reader software (optional)
- Understanding of your accessibility needs

## Accessibility Features Overview

| Feature | Purpose | Configuration |
|---------|---------|---------------|
| TTS | Audio output | `config.toml` |
| High Contrast | Visual clarity | `colors.toml` |
| Large Text | Readability | Terminal settings |
| Keyboard Nav | Mouse-free use | `keybinds.toml` |
| Audio Alerts | Status awareness | `triggers.toml` |

## Step 1: Enable Text-to-Speech

### Configure TTS

Edit `~/.vellum-fe/config.toml`:

```toml
[tts]
enabled = true
voice = "default"           # System default voice
rate = 1.0                  # 0.5 (slow) to 2.0 (fast)
volume = 1.0                # 0.0 to 1.0

# What to speak
speak_room_descriptions = true
speak_combat = true
speak_speech = true
speak_whispers = true
speak_thoughts = false      # Often too much
speak_system = true

# Filtering
skip_patterns = [
    "^\\s*$",               # Empty lines
    "^Obvious exits:",      # Repetitive
]

# Priority speaking (interrupts queue)
priority_patterns = [
    "stunned",
    "webbed",
    "died",
    "whispers,"
]
```

### Platform-Specific TTS

**Windows** (SAPI):
```toml
[tts]
engine = "sapi"
voice = "Microsoft David"   # or "Microsoft Zira"
```

**macOS** (say):
```toml
[tts]
engine = "say"
voice = "Alex"              # or "Samantha"
```

**Linux** (espeak):
```toml
[tts]
engine = "espeak"
voice = "en-us"
# Install: sudo apt install espeak
```

**Linux** (speech-dispatcher):
```toml
[tts]
engine = "speechd"
voice = "default"
# Install: sudo apt install speech-dispatcher
```

## Step 2: High Contrast Theme

Create `~/.vellum-fe/colors.toml`:

```toml
[theme]
name = "High Contrast"

# Maximum contrast backgrounds
background = "#000000"      # Pure black
text = "#ffffff"            # Pure white
text_dim = "#c0c0c0"        # Light gray (still readable)

# Bold, distinct borders
border = "#ffffff"
border_focused = "#ffff00"  # Yellow focus indicator

# Vitals - WCAG AAA contrast
health = "#00ff00"          # Bright green
health_low = "#ffff00"      # Yellow warning
health_critical = "#ff0000" # Red danger
mana = "#00ffff"            # Cyan
stamina = "#ff8800"         # Orange
spirit = "#ff00ff"          # Magenta

# Status indicators - distinct colors
hidden = "#00ff00"
stunned = "#ffff00"
webbed = "#ff00ff"
prone = "#00ffff"
kneeling = "#ff8800"

# Speech - distinguishable
speech = "#00ffff"          # Cyan
whisper = "#ff00ff"         # Magenta
thoughts = "#00ff00"        # Green
shout = "#ffff00"           # Yellow

# Combat
attack_hit = "#00ff00"
attack_miss = "#ff0000"
damage_high = "#ff0000"

# Links and interactive elements
link = "#00ffff"
link_hover = "#ffff00"
```

### Alternative: Yellow on Black

Some users prefer yellow text:

```toml
[theme]
name = "Yellow High Contrast"
background = "#000000"
text = "#ffff00"
text_dim = "#cccc00"
border = "#ffff00"
```

### Alternative: White on Blue

Classic high-contrast scheme:

```toml
[theme]
name = "White on Blue"
background = "#000080"
text = "#ffffff"
border = "#ffffff"
```

## Step 3: Accessible Layout

Create `~/.vellum-fe/layout.toml`:

```toml
# Accessible Layout - Clear, Logical Structure
# Optimized for screen readers and keyboard navigation

# ═══════════════════════════════════════════════════════════
# MAIN TEXT - Primary Focus
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "text"
name = "main"
title = "Game Output"
x = 0
y = 0
width = 100
height = 70
streams = ["main", "room", "combat", "speech"]
scrollback = 2000
auto_scroll = true
focus_order = 1             # First in tab order

# ═══════════════════════════════════════════════════════════
# STATUS - Spoken Summary
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "progress"
name = "health"
title = "Health"
x = 0
y = 71
width = 25
height = 5
data_source = "vitals.health"
color = "health"
show_text = true
show_percentage = true
announce_changes = true     # TTS announces changes
focus_order = 2

[[widgets]]
type = "progress"
name = "mana"
title = "Mana"
x = 26
y = 71
width = 25
height = 5
data_source = "vitals.mana"
color = "mana"
show_text = true
show_percentage = true
announce_changes = true
focus_order = 3

[[widgets]]
type = "progress"
name = "stamina"
title = "Stamina"
x = 52
y = 71
width = 24
height = 5
data_source = "vitals.stamina"
color = "stamina"
show_text = true
show_percentage = true
focus_order = 4

[[widgets]]
type = "countdown"
name = "roundtime"
title = "Roundtime"
x = 77
y = 71
width = 23
height = 5
data_source = "roundtime"
announce_start = true       # "Roundtime 5 seconds"
announce_end = true         # "Ready"
focus_order = 5

# ═══════════════════════════════════════════════════════════
# STATUS INDICATORS - Audio Alerts
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "indicator"
name = "status"
title = "Status"
x = 0
y = 77
width = 100
height = 5
indicators = [
    "hidden",
    "stunned",
    "webbed",
    "prone",
    "kneeling"
]
columns = 5
announce_changes = true     # TTS announces status changes
focus_order = 6

# ═══════════════════════════════════════════════════════════
# COMMAND INPUT - Primary Interaction
# ═══════════════════════════════════════════════════════════

[[widgets]]
type = "command_input"
name = "input"
title = "Command"
x = 0
y = 83
width = 100
height = 17
history_size = 500
prompt = "Enter command: "
announce_echo = false       # Don't repeat typed text
focus_order = 0             # Default focus
```

## Step 4: Keyboard Navigation

Create `~/.vellum-fe/keybinds.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# WIDGET NAVIGATION - Tab Order
# ═══════════════════════════════════════════════════════════

[keybinds."tab"]
action = "next_widget"

[keybinds."shift+tab"]
action = "prev_widget"

[keybinds."escape"]
action = "focus_input"

# Quick focus to specific widgets
[keybinds."alt+m"]
action = "focus_widget"
widget = "main"

[keybinds."alt+h"]
action = "focus_widget"
widget = "health"

[keybinds."alt+i"]
action = "focus_widget"
widget = "input"

# ═══════════════════════════════════════════════════════════
# SCROLLING - Standard Keys
# ═══════════════════════════════════════════════════════════

[keybinds."page_up"]
action = "scroll_up"

[keybinds."page_down"]
action = "scroll_down"

[keybinds."home"]
action = "scroll_top"

[keybinds."end"]
action = "scroll_bottom"

[keybinds."ctrl+home"]
action = "scroll_top"

[keybinds."ctrl+end"]
action = "scroll_bottom"

# Line-by-line scrolling
[keybinds."up"]
action = "scroll_line_up"

[keybinds."down"]
action = "scroll_line_down"

# ═══════════════════════════════════════════════════════════
# STATUS QUERIES - Audio Feedback
# ═══════════════════════════════════════════════════════════

[keybinds."f1"]
action = "speak_status"     # TTS reads all vitals

[keybinds."f2"]
action = "speak_room"       # TTS reads room description

[keybinds."f3"]
action = "speak_last"       # Repeat last spoken text

[keybinds."f4"]
action = "stop_speaking"    # Interrupt TTS

# ═══════════════════════════════════════════════════════════
# MOVEMENT - Consistent Keys
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

# ═══════════════════════════════════════════════════════════
# COMMON ACTIONS
# ═══════════════════════════════════════════════════════════

[keybinds."f5"]
macro = "look"

[keybinds."f6"]
macro = "inventory"

[keybinds."f7"]
macro = "experience"

[keybinds."f8"]
macro = "health"

[keybinds."f9"]
macro = "attack target"

[keybinds."f10"]
macro = "hide"

[keybinds."f11"]
macro = "search"

[keybinds."f12"]
macro = "flee"

# ═══════════════════════════════════════════════════════════
# TTS CONTROL
# ═══════════════════════════════════════════════════════════

[keybinds."ctrl+space"]
action = "toggle_tts"

[keybinds."ctrl+up"]
action = "tts_rate_up"

[keybinds."ctrl+down"]
action = "tts_rate_down"

[keybinds."ctrl+shift+up"]
action = "tts_volume_up"

[keybinds."ctrl+shift+down"]
action = "tts_volume_down"
```

## Step 5: Audio Alerts

Create `~/.vellum-fe/triggers.toml`:

```toml
# ═══════════════════════════════════════════════════════════
# CRITICAL STATUS - Spoken Alerts
# ═══════════════════════════════════════════════════════════

[[triggers]]
name = "stun_announce"
pattern = "(?i)you are stunned"
command = ".tts You are stunned!"
priority = 100
cooldown = 1000

[[triggers]]
name = "web_announce"
pattern = "(?i)webs? (stick|entangle)"
command = ".tts You are webbed!"
priority = 100
cooldown = 1000

[[triggers]]
name = "prone_announce"
pattern = "(?i)(knock|fall).*?(down|prone)"
command = ".tts You fell down!"
priority = 100
cooldown = 1000

[[triggers]]
name = "death_announce"
pattern = "You have died"
command = ".tts You have died!"
priority = 100

# ═══════════════════════════════════════════════════════════
# HEALTH WARNINGS
# ═══════════════════════════════════════════════════════════

[[triggers]]
name = "low_health"
pattern = "(?i)feel (weak|faint)"
command = ".tts Warning! Low health!"
priority = 100
cooldown = 5000

# ═══════════════════════════════════════════════════════════
# SOCIAL - Important Communications
# ═══════════════════════════════════════════════════════════

[[triggers]]
name = "whisper_announce"
pattern = "(\\w+) whispers,"
command = ".tts $1 whispers to you"
priority = 90
cooldown = 500

[[triggers]]
name = "name_mention"
pattern = "\\bYOURNAME\\b"
command = ".tts Your name was mentioned"
priority = 80
cooldown = 3000

# ═══════════════════════════════════════════════════════════
# ROUNDTIME
# ═══════════════════════════════════════════════════════════

[[triggers]]
name = "rt_long"
pattern = "Roundtime: ([5-9]|\\d{2,})"
command = ".tts Roundtime $1 seconds"
priority = 50

[[triggers]]
name = "rt_done"
pattern = "Roundtime: 0"
command = ".tts Ready"
priority = 50
enabled = false  # Enable if you want ready alerts
```

## Step 6: Large Text Configuration

VellumFE inherits font size from your terminal. Configure your terminal:

### Windows Terminal

Settings → Profiles → Default → Appearance:
- Font size: 14-18 for comfortable reading
- Font face: Consolas, Cascadia Code, or other monospace

### macOS Terminal

Terminal → Preferences → Profiles → Text:
- Font: Monaco or SF Mono
- Size: 14-18 pt

### Linux Terminal (GNOME)

Preferences → Profiles → Default → Text:
- Custom font: DejaVu Sans Mono
- Size: 14-18

### Recommended Fonts

High-readability monospace fonts:

| Font | Platform | Notes |
|------|----------|-------|
| JetBrains Mono | All | Designed for readability |
| Fira Code | All | Excellent legibility |
| Cascadia Code | Windows | Microsoft's modern font |
| SF Mono | macOS | Apple's system font |
| DejaVu Sans Mono | Linux | Comprehensive Unicode |
| OpenDyslexic Mono | All | Dyslexia-friendly |

## Testing Your Setup

### TTS Test

1. Enable TTS in config
2. Launch VellumFE
3. Move to a new room
4. Verify room description is spoken
5. Test F1 (speak status) keybind

### Navigation Test

1. Use Tab to cycle through widgets
2. Verify focus indicator is visible (yellow border)
3. Use Page Up/Down in main window
4. Use Escape to return to input

### Contrast Test

1. Check all text is readable
2. Verify borders are visible
3. Test in both light and dark room lighting
4. Check vital bar colors are distinguishable

### Checklist

- [ ] TTS speaks room descriptions
- [ ] TTS announces status changes
- [ ] Tab cycles through widgets
- [ ] Focus indicator clearly visible
- [ ] All text high contrast
- [ ] Vital bars distinguishable by color
- [ ] Keybinds work as expected
- [ ] F1 speaks current status

## Customization

### Adjust TTS Speed

For faster reading:
```toml
[tts]
rate = 1.5
```

For clearer pronunciation:
```toml
[tts]
rate = 0.8
```

### Selective TTS

Only speak important things:
```toml
[tts]
speak_room_descriptions = true
speak_combat = false
speak_speech = true
speak_whispers = true
speak_thoughts = false
```

### Different Voices

Some systems support multiple voices:
```toml
[tts]
combat_voice = "Microsoft David"
speech_voice = "Microsoft Zira"
```

### Custom Alert Sounds

Combine TTS with audio:
```toml
[[triggers]]
name = "stun_alert"
pattern = "(?i)you are stunned"
command = ".sound stun.wav;.tts Stunned!"
```

## Screen Reader Integration

VellumFE can work alongside dedicated screen readers:

### NVDA (Windows)

- VellumFE TTS can coexist with NVDA
- Consider disabling VellumFE TTS if using NVDA
- NVDA will read terminal output automatically

### VoiceOver (macOS)

- VoiceOver reads terminal content
- Use VoiceOver commands for navigation
- VellumFE keybinds may conflict - adjust as needed

### Orca (Linux)

- Orca works with accessible terminals
- Ensure terminal has accessibility enabled
- May need to disable VellumFE TTS to avoid overlap

## Troubleshooting

### TTS Not Working

1. Check TTS is enabled in config
2. Verify system TTS works: `say "test"` (macOS) or test in Windows settings
3. Check TTS engine setting matches installed software
4. Review logs for TTS errors

### Focus Not Visible

Increase border contrast:
```toml
border_focused = "#ffff00"  # Bright yellow
```

Or add a focus background:
```toml
focused_bg = "#333300"
```

### Colors Not Distinct

Test with colorblind simulation tools, then adjust:
```toml
# For red-green colorblindness
health = "#00ffff"    # Cyan instead of green
danger = "#ffff00"    # Yellow instead of red
```

### Keybinds Conflicting

Screen readers use many keybinds. Check for conflicts and remap:
```toml
# If F1 conflicts with screen reader
[keybinds."ctrl+f1"]
action = "speak_status"
```

## Resources

### Color Contrast Tools

- [WebAIM Contrast Checker](https://webaim.org/resources/contrastchecker/)
- [Contrast Ratio](https://contrast-ratio.com/)

### Screen Reader Documentation

- [NVDA User Guide](https://www.nvaccess.org/files/nvda/documentation/userGuide.html)
- [VoiceOver Getting Started](https://support.apple.com/guide/voiceover/)
- [Orca Documentation](https://help.gnome.org/users/orca/)

### Accessibility Standards

- [WCAG 2.1 Guidelines](https://www.w3.org/WAI/WCAG21/quickref/)
- [Section 508](https://www.section508.gov/)

## See Also

- [TTS Setup](../customization/tts-setup.md) - Detailed TTS configuration
- [Creating Themes](../customization/creating-themes.md) - Theme customization
- [Keybind Actions](../customization/keybind-actions.md) - All available actions

