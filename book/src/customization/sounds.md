# Sound Alerts

Audio notifications for game events.

## Setup

1. Put sound files in `~/.vellum-fe/global/sounds/`
2. Reference them in highlights

## Using Sounds

In `highlights.toml`:

```toml
[death_alert]
pattern = "appears dead"
fg = "#00ff00"
sound = "kill.wav"
sound_volume = 0.8
```

## Sound Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `sound` | string | - | Filename in sounds directory |
| `sound_volume` | float | 1.0 | Volume (0.0 to 1.0) |

## Global Settings

In `config.toml`:

```toml
[sound]
enabled = true
volume = 0.7              # Master volume
cooldown_ms = 500         # Min time between sounds
startup_music = true      # Play music on launch
```

## Disabling Sounds

### All Sounds

```toml
[sound]
enabled = false
```

### Highlight Sounds Only

```toml
[highlights]
sounds_enabled = false
```

## Example Sounds Setup

Directory structure:
```
~/.vellum-fe/global/sounds/
├── kill.wav
├── alert.wav
├── whisper.wav
└── danger.wav
```

Highlights:
```toml
[kill]
pattern = "appears dead"
sound = "kill.wav"

[stunned]
pattern = "You are stunned"
sound = "alert.wav"
sound_volume = 1.0

[whisper]
pattern = "whispers to you"
sound = "whisper.wav"
sound_volume = 0.5

[bleeding]
pattern = "Blood runs down"
sound = "danger.wav"
```

## Supported Formats

- WAV (recommended)
- MP3
- OGG
- FLAC

## Troubleshooting

**No sound playing?**

1. Check `[sound] enabled = true` in config.toml
2. Check `[highlights] sounds_enabled = true`
3. Verify file exists in sounds directory
4. Check file format is supported
5. Try increasing volume
