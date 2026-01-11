# Configuration

VellumFE uses TOML files for configuration, stored in `~/.vellum-fe/`.

## Configuration Files

| File | Purpose |
|------|---------|
| [config.toml](./config-toml.md) | General settings (UI, connection, behavior) |
| [layout.toml](./layout-toml.md) | Window positions, sizes, and properties |
| [keybinds.toml](./keybinds-toml.md) | Keyboard shortcuts |
| [highlights.toml](./highlights-toml.md) | Text highlighting rules |
| [colors.toml](./colors-toml.md) | Color palette definitions |

## Per-Character Configuration

When you specify `--character NAME`, VellumFE looks for character-specific files:

```
~/.vellum-fe/
├── config.toml           # Global defaults
├── layout.toml           # Global layout
├── characters/
│   └── CharName/
│       ├── config.toml   # Character overrides
│       └── layout.toml   # Character layout
```

Character-specific files override global settings.

## Editing Configuration

You can edit files directly, or use the in-client menu:

1. Press `F1` to open the main menu
2. Navigate to **Config** submenu
3. Select the file to edit

Changes to layout are saved automatically. Other config changes require restart.

## Resetting to Defaults

Delete a configuration file to reset it to defaults on next launch:

```bash
rm ~/.vellum-fe/layout.toml
```

Or delete the entire directory for a full reset:

```bash
rm -rf ~/.vellum-fe
```
