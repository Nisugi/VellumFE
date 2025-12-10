# TTS Setup

VellumFE supports text-to-speech (TTS) for accessibility and hands-free notifications.

## Overview

TTS features:
- Read game text aloud
- Announce specific events
- Screen reader compatibility
- Multiple voice options

## Configuration

### Basic Setup

In `config.toml`:

```toml
[tts]
enabled = true
voice = "default"
rate = 1.0           # Speaking rate (0.5-2.0)
volume = 1.0         # Volume (0.0-1.0)
```

### Voice Selection

```toml
[tts]
voice = "default"    # System default
# Or specific voice name from your system
voice = "Microsoft David"    # Windows
voice = "Alex"               # macOS
voice = "espeak"             # Linux
```

## Stream Configuration

Control which streams are spoken:

```toml
[tts]
enabled = true
streams = ["main", "speech", "thoughts"]  # Streams to read

# Or exclude specific streams
exclude_streams = ["combat", "logons"]
```

### Stream Options

| Stream | Description |
|--------|-------------|
| `main` | Main game output |
| `speech` | Player dialogue |
| `thoughts` | ESP/telepathy |
| `combat` | Combat messages |
| `death` | Death notices |
| `logons` | Login/logout |
| `familiar` | Familiar messages |

## Pattern-Based TTS

Speak only when patterns match:

```toml
[[tts_patterns]]
name = "whisper_announce"
pattern = "whispers to you"
message = "You received a whisper"

[[tts_patterns]]
name = "death_announce"
pattern = "You have died"
message = "Warning: You have died"

[[tts_patterns]]
name = "stun_announce"
pattern = "You are stunned"
speak_match = true    # Speak the matched text
```

### Pattern Properties

```toml
[[tts_patterns]]
name = "my_pattern"
pattern = "regex pattern"

# What to speak:
message = "Custom message"    # Speak this text
speak_match = true            # Speak matched text
speak_line = true             # Speak entire line

# Control:
priority = 100                # Higher = more important
rate = 1.5                    # Override speaking rate
volume = 1.0                  # Override volume
```

## Platform Setup

### Windows

Windows has built-in TTS (SAPI):

1. TTS works out of the box
2. Configure voices in Windows Settings → Time & Language → Speech
3. Install additional voices from Microsoft Store

Available voices:
- Microsoft David (Male)
- Microsoft Zira (Female)
- Additional language packs

```toml
[tts]
enabled = true
voice = "Microsoft David"
```

### macOS

macOS uses built-in speech synthesis:

1. TTS works out of the box
2. Configure in System Preferences → Accessibility → Spoken Content
3. Download additional voices

Available voices:
- Alex (Male)
- Samantha (Female)
- Many international voices

```toml
[tts]
enabled = true
voice = "Alex"
```

### Linux

Linux requires a TTS engine:

#### eSpeak

```bash
# Debian/Ubuntu
sudo apt install espeak

# Fedora
sudo dnf install espeak

# Arch
sudo pacman -S espeak
```

```toml
[tts]
enabled = true
engine = "espeak"
voice = "en"
```

#### Festival

```bash
# Debian/Ubuntu
sudo apt install festival

# Configure
sudo apt install festvox-kallpc16k
```

```toml
[tts]
enabled = true
engine = "festival"
```

#### Speech Dispatcher

```bash
# Debian/Ubuntu
sudo apt install speech-dispatcher
spd-say "test"
```

```toml
[tts]
enabled = true
engine = "speechd"
```

## Rate and Pitch

### Speaking Rate

```toml
[tts]
rate = 1.0      # Normal speed
rate = 0.5      # Half speed (slower)
rate = 2.0      # Double speed (faster)
```

### Pitch (if supported)

```toml
[tts]
pitch = 1.0     # Normal pitch
pitch = 0.8     # Lower pitch
pitch = 1.2     # Higher pitch
```

## Accessibility Features

### Screen Reader Compatibility

VellumFE works with screen readers:
- NVDA (Windows)
- JAWS (Windows)
- VoiceOver (macOS)
- Orca (Linux)

```toml
[accessibility]
screen_reader_mode = true
announce_focus_changes = true
announce_new_content = true
```

### High Contrast Mode

Combine with TTS:

```toml
[ui]
high_contrast = true

[tts]
enabled = true
announce_colors = false    # Don't read color codes
```

## TTS Commands

### Toggle TTS

```
.tts on
.tts off
.tts toggle
```

### Change Rate

```
.tts rate 1.5
```

### Change Volume

```
.tts volume 0.8
```

### Test TTS

```
.tts test "This is a test message"
```

### List Voices

```
.tts voices
```

## Common Patterns

### Combat Awareness

```toml
[[tts_patterns]]
name = "attack"
pattern = "(strikes|hits|bites) you"
message = "Under attack"
priority = 90

[[tts_patterns]]
name = "stunned"
pattern = "You are stunned"
message = "Stunned"
priority = 100

[[tts_patterns]]
name = "roundtime"
pattern = "Roundtime: (\\d+)"
message = "Roundtime"
```

### Social Notifications

```toml
[[tts_patterns]]
name = "whisper"
pattern = "(\\w+) whispers,"
speak_match = true

[[tts_patterns]]
name = "name_mention"
pattern = "\\bYourCharacter\\b"
message = "Someone mentioned you"
```

### Navigation

```toml
[[tts_patterns]]
name = "room_name"
pattern = "^\\[.*?\\]$"
speak_line = true

[[tts_patterns]]
name = "exits"
pattern = "Obvious (paths|exits):"
speak_line = true
```

## Interruption

Control how new speech interrupts current:

```toml
[tts]
interrupt_mode = "queue"     # Wait for current to finish
interrupt_mode = "interrupt" # Stop current, start new
interrupt_mode = "priority"  # Interrupt only if higher priority
```

## Troubleshooting

### No Sound

1. Check `[tts] enabled = true`
2. Verify system TTS works:
   - Windows: `PowerShell: Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak("test")`
   - macOS: `say "test"`
   - Linux: `espeak "test"`
3. Check volume settings

### Wrong Voice

1. List available voices: `.tts voices`
2. Set correct voice name exactly
3. Voice names are case-sensitive on some systems

### Too Fast/Slow

Adjust rate:
```toml
[tts]
rate = 0.8    # Slower
rate = 1.2    # Faster
```

### Cuts Off

Increase buffer or reduce interrupt:
```toml
[tts]
interrupt_mode = "queue"
```

## See Also

- [Sound Alerts](./sound-alerts.md) - Audio alerts
- [Accessibility Tutorial](../tutorials/accessibility.md) - Full accessibility setup
- [Configuration](../configuration/config-toml.md) - Full config reference

