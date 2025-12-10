# Sound Alerts

VellumFE can play sounds when specific text patterns match, providing audio alerts for important events.

## Sound Basics

Add sounds to highlight patterns:

```toml
[[highlights]]
name = "whisper_alert"
pattern = "whispers to you"
fg = "#00FFFF"
sound = "~/.vellum-fe/sounds/whisper.wav"
```

## Supported Formats

| Format | Extension | Notes |
|--------|-----------|-------|
| WAV | `.wav` | Best compatibility |
| MP3 | `.mp3` | Requires codec |
| OGG | `.ogg` | Good compression |

WAV is recommended for reliable playback.

## Sound Configuration

### Basic Sound

```toml
[[highlights]]
name = "my_alert"
pattern = "important event"
sound = "/path/to/sound.wav"
```

### Volume Control

```toml
[[highlights]]
name = "quiet_alert"
pattern = "minor event"
sound = "/path/to/sound.wav"
volume = 0.5    # 0.0 to 1.0
```

### Sound-Only (No Visual)

```toml
[[highlights]]
name = "audio_only"
pattern = "background event"
sound = "/path/to/sound.wav"
# No fg/bg colors - visual unchanged
```

## Sound File Management

### Directory Structure

```
~/.vellum-fe/
└── sounds/
    ├── whisper.wav
    ├── attack.wav
    ├── death.wav
    ├── loot.wav
    └── custom/
        └── my_sounds.wav
```

### Creating the Sounds Directory

```bash
mkdir -p ~/.vellum-fe/sounds
```

### Sound Sources

- **System sounds** - Copy from OS sound themes
- **Online libraries** - freesound.org, soundsnap.com
- **Create your own** - Record with Audacity
- **Game packs** - MUD sound packs

## Common Alert Sounds

### Combat Alerts

```toml
# Being attacked
[[highlights]]
name = "attack_received"
pattern = "(strikes|hits|bites|claws) you"
fg = "#FF4444"
sound = "~/.vellum-fe/sounds/hit.wav"

# Near death
[[highlights]]
name = "low_health"
pattern = "You feel weak"
fg = "#FF0000"
bg = "#400000"
sound = "~/.vellum-fe/sounds/warning.wav"
volume = 1.0

# Death
[[highlights]]
name = "death"
pattern = "You have died"
fg = "#FFFFFF"
bg = "#FF0000"
sound = "~/.vellum-fe/sounds/death.wav"
```

### Social Alerts

```toml
# Whispers
[[highlights]]
name = "whisper"
pattern = "whispers,"
fg = "#00FFFF"
sound = "~/.vellum-fe/sounds/whisper.wav"
volume = 0.7

# Your name mentioned
[[highlights]]
name = "name_mention"
pattern = "\\bYourCharacter\\b"
fg = "#FFFF00"
sound = "~/.vellum-fe/sounds/mention.wav"

# Group invite
[[highlights]]
name = "group_invite"
pattern = "invites you to join"
fg = "#00FF00"
sound = "~/.vellum-fe/sounds/invite.wav"
```

### Loot Alerts

```toml
# Treasure found
[[highlights]]
name = "treasure"
pattern = "(?i)(gold|gems|treasure|chest)"
fg = "#FFD700"
sound = "~/.vellum-fe/sounds/loot.wav"
volume = 0.5

# Rare item
[[highlights]]
name = "rare_item"
pattern = "(?i)(legendary|artifact|ancient)"
fg = "#FF00FF"
sound = "~/.vellum-fe/sounds/rare.wav"
```

### Status Alerts

```toml
# Stunned
[[highlights]]
name = "stunned"
pattern = "You are stunned"
fg = "#FFFF00"
sound = "~/.vellum-fe/sounds/stun.wav"

# Poisoned
[[highlights]]
name = "poisoned"
pattern = "(?i)poison|venom"
fg = "#00FF00"
sound = "~/.vellum-fe/sounds/poison.wav"
```

## Global Sound Settings

### In config.toml

```toml
[sound]
enabled = true           # Master switch
volume = 0.8             # Master volume (0.0-1.0)
concurrent_limit = 3     # Max simultaneous sounds
```

### Disable All Sounds

```toml
[sound]
enabled = false
```

## Sound Priority

When multiple patterns match:

```toml
[[highlights]]
name = "critical_alert"
pattern = "critical"
sound = "~/.vellum-fe/sounds/critical.wav"
priority = 100          # Plays first

[[highlights]]
name = "minor_alert"
pattern = "hit"
sound = "~/.vellum-fe/sounds/hit.wav"
priority = 50           # May be skipped if concurrent_limit reached
```

## Rate Limiting

Prevent sound spam:

```toml
[[highlights]]
name = "combat_sound"
pattern = "You attack"
sound = "~/.vellum-fe/sounds/attack.wav"
sound_cooldown = 500    # Milliseconds between plays
```

## Platform Notes

### Windows

- WAV files play natively
- No additional dependencies needed

### Linux

Requires audio system:

```bash
# PulseAudio (most distros)
pactl list short sinks

# ALSA
aplay -l
```

### macOS

- WAV files play natively
- Uses Core Audio

## Troubleshooting

### No Sound

1. Check `[sound] enabled = true`
2. Verify file path is correct
3. Test file with system player
4. Check volume settings

### Delayed Sound

1. Use WAV instead of MP3
2. Reduce file size
3. Check system audio latency

### Wrong Sound

1. Check pattern matching correctly
2. Verify highlight priority
3. Check for pattern conflicts

## Creating Custom Sounds

### Using Audacity

1. Open Audacity
2. Record or import audio
3. Trim to desired length (0.5-2 seconds recommended)
4. Export as WAV (16-bit PCM)
5. Save to `~/.vellum-fe/sounds/`

### Sound Guidelines

- **Duration**: 0.5-2 seconds
- **Format**: WAV 16-bit PCM
- **Sample rate**: 44100 Hz
- **Channels**: Mono or Stereo
- **File size**: Keep under 500KB

## Sound Packs

### Creating a Sound Pack

1. Create themed sounds
2. Package in zip/tar
3. Include README with highlight configs
4. Share with community

### Installing a Sound Pack

1. Extract to `~/.vellum-fe/sounds/`
2. Add highlights from pack's config
3. Run `.reload highlights`

## See Also

- [Highlights Configuration](../configuration/highlights-toml.md) - Pattern configuration
- [Highlight Patterns](./highlight-patterns.md) - Pattern syntax
- [TTS Setup](./tts-setup.md) - Text-to-speech alternative

