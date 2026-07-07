# Configuration

VellumFE uses TOML files for configuration, stored in `~/.vellum-fe/`
(override the location with the `VELLUM_FE_DIR` environment variable or `--data-dir`).

## Configuration Files

| File | Purpose |
|------|---------|
| [config.toml](./config-toml.md) | General settings (connection, UI, sound, TTS, web server) |
| [layout.toml](./layout-toml.md) | Window positions, sizes, and properties (TUI) |
| [keybinds.toml](./keybinds-toml.md) | Keyboard shortcuts |
| [highlights.toml](./highlights-toml.md) | Text highlighting, sounds, squelch rules |
| [colors.toml](./colors-toml.md) | Color palette, stream presets, spell colors |
| [macros.toml](./macros-toml.md) | Macro buttons for the mobile web frontend |

## Directory Layout

```
~/.vellum-fe/
├── launcher.toml         # Launcher profiles (passwords live in the OS keyring)
├── global/               # Shared settings for all characters
│   ├── config.toml
│   ├── keybinds.toml
│   ├── highlights.toml
│   ├── colors.toml
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
| `.settings` | Settings editor (connection, UI, sound, theme) |
| `.highlights` | Highlights browser |
| `.keybinds` | Keybinds browser |
| `.colors` | Color palette browser |
| `.themes` | Theme browser |

If you edit files directly, apply changes without restarting:

```
.reload              # reload everything
.reload highlights   # or just one: highlights, keybinds, settings, colors, layout
.reloadmacros        # macros.toml (also pushes to connected phones)
```

## Resetting to Defaults

Delete a configuration file and it is recreated with defaults on next launch:

```bash
rm ~/.vellum-fe/global/keybinds.toml
```

Or delete the entire directory for a full reset.
