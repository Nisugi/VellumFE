# Configuration

VellumFE uses TOML files for configuration, stored in `~/.vellum-fe/`
(override the location with the `VELLUM_FE_DIR` environment variable or `--data-dir`).

## Configuration Files

| File | Purpose |
|------|---------|
| [config.toml](./config-toml.md) | General settings (connection, UI, sound, TTS, web server) |
| [layout.toml](./layout-toml.md) | Window positions, sizes, and properties (TUI) |
| [keybinds.toml](./keybinds-toml.md) | Keyboard shortcuts |
| [controller.toml](./controller-toml.md) | Gamepad binds, wheels, rumble, tuning (global) |
| [highlights.toml](./highlights-toml.md) | Text highlighting, sounds, squelch rules |
| [colors.toml](./colors-toml.md) | Color palette, stream presets, spell colors |
| [hotbars.toml](./hotbars-toml.md) | Hotkey bars (command buttons) |
| [macros.toml](./macros-toml.md) | Macro buttons for the mobile web frontend |

## Directory Layout

```
~/.vellum-fe/
├── launcher.toml         # Launcher profiles (passwords live in the OS keyring)
├── global/               # Shared settings for all characters
│   ├── config.toml
│   ├── keybinds.toml
│   ├── controller.toml    # Gamepad binds/wheels/rumble/tuning
│   ├── highlights.toml
│   ├── colors.toml
│   ├── hotbars.toml
│   ├── macros.toml
│   └── sounds/           # Sound files for highlight alerts
├── layouts/              # Saved layouts (.savelayout / .loadlayout)
├── profiles/
│   └── CharName/         # Per-character overrides + auto-saved layout.toml
├── themes/               # Custom themes (.edittheme saves here)
├── skins/                # GUI skins (one folder per skin: skin.toml + images)
└── vellum-fe.log
```

Files in `profiles/<name>/` override the matching global file for that character.

## Editing Configuration

Most things can be edited in-app without touching files:

| Command | Opens |
|---------|-------|
| `.settings` | Settings editor — every registered setting, on both frontends |
| `.highlights` | Highlights browser |
| `.keybinds` | Keybinds browser |
| `.hotbars` | Hotbar editor |
| `.streams` | Stream routing editor |
| `.colors` | Color palette browser |
| `.uicolors` / `.spellcolors` | UI element / spell-circle colors |
| `.themes` | Theme browser |
| `.tts` | Text-to-speech controls (GUI: Settings > Speech) |

If you edit files directly, apply changes without restarting:

```
.reload              # reload everything
.reload highlights   # or just one: highlights, keybinds, settings, colors, layout
.reloadmacros        # macros.toml (also pushes to connected phones)
```

## How Saves Work

You shouldn't need to care, but for the curious:

- **Sparse saves** — user files only contain what you've changed from the
  shipped defaults, with your comments preserved. Settings you never
  touched pick up new defaults automatically on upgrade.
- **Atomic writes with backups** — every save writes a temp file and swaps
  it in, keeping a `.bak` of the previous version. A crash mid-save can't
  corrupt your config.
- **Additive default refresh** — new shipped highlights/keybinds/hotbars
  appear after an upgrade, but ones you deleted stay deleted (a
  `.defaults-seen.toml` sidecar remembers what you've already been given).

## Resetting to Defaults

Delete a configuration file and it is recreated with defaults on next launch:

```bash
rm ~/.vellum-fe/global/keybinds.toml
```

Or delete the entire directory for a full reset. (Individual shipped
entries you deleted from collection files like highlights stay deleted
across upgrades — deleting the whole file is the reset switch.)
