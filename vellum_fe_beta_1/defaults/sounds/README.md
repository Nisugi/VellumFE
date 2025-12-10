# VellumFE Default Sounds

This directory contains default sound files that are embedded into the VellumFE binary.

## How Sounds Work

When VellumFE starts, it creates a `~/.vellum-fe/sounds/` directory on your system and extracts any default sounds to that location (if they don't already exist). Users can then:

1. Use the default sounds in their highlight configurations
2. Add their own custom sound files to `~/.vellum-fe/sounds/`
3. Reference sounds in their config by filename

## Supported Formats

VellumFE uses the `rodio` audio library, which supports:
- **WAV** - Uncompressed audio (recommended for low latency)
- **MP3** - Compressed audio
- **OGG** - Open source compressed audio
- **FLAC** - Lossless compressed audio

## Adding Default Sounds

To add a default sound that ships with VellumFE:

1. Place the sound file in this directory (`defaults/sounds/`)
2. Edit `src/sound.rs` and add an entry to the `DEFAULT_SOUNDS` array:
   ```rust
   const DEFAULT_SOUNDS: &[(&str, &[u8])] = &[
       ("beep.wav", include_bytes!("../defaults/sounds/beep.wav")),
       ("death.ogg", include_bytes!("../defaults/sounds/death.ogg")),
   ];
   ```
3. Rebuild VellumFE - the sounds will now be embedded in the binary

## Using Sounds in Configuration

Add sound triggers to highlights in your `~/.vellum-fe/configs/default.toml`:

```toml
[highlights]
# Play a sound when someone dies
death_alert = {
    pattern = ".*dies.*",
    fg = "#ffffff",
    bg = "#ff0000",
    bold = true,
    color_entire_line = true,
    sound = "death.wav",      # Sound file in ~/.vellum-fe/sounds/
    sound_volume = 0.8         # Optional volume override (0.0 to 1.0)
}

# Play a sound when you're stunned
stun_alert = {
    pattern = "You are stunned",
    fg = "#ffff00",
    bold = true,
    sound = "stun.ogg"
}
```

## Sound Configuration

Global sound settings in config:

```toml
[sound]
enabled = true          # Enable/disable all sounds
volume = 0.7            # Master volume (0.0 to 1.0)
cooldown_ms = 500       # Cooldown between same sound plays (milliseconds)
```

## Cooldown System

To prevent sound spam, each sound file has a cooldown period (default 500ms). If the same sound is triggered multiple times rapidly, only the first play will go through. This is especially useful for patterns that might match multiple times in quick succession.

## Tips

- **Keep sounds short** - Game events happen quickly, you want instant audio feedback
- **WAV format recommended** - Lowest latency for playback
- **Test volume levels** - Make sure sounds aren't too loud or too quiet
- **Use descriptive names** - `death.wav`, `stun.ogg`, `loot.wav`, etc.
- **Consider file size** - Embedded sounds increase binary size

## Example Sound Packs

Once you have a collection of sounds you like, you can share them by:
1. Zipping your `~/.vellum-fe/sounds/` directory
2. Sharing the zip along with your highlight config snippets
3. Others can extract to their sounds directory and use your config

## Finding Sounds

Free sound resources:
- [Freesound.org](https://freesound.org/) - Community sound library
- [OpenGameArt.org](https://opengameart.org/) - Game audio assets
- [Zapsplat.com](https://www.zapsplat.com/) - Free sound effects

Make sure to respect licensing when using sounds from these sources!
