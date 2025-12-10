# Configuration

VellumFE uses TOML files for all configuration. This section provides complete documentation for each configuration file.

## Configuration Files

VellumFE loads configuration from these files in your data directory (`~/.vellum-fe/`):

| File | Purpose | Hot Reload |
|------|---------|------------|
| [config.toml](./config-toml.md) | Main settings, connection, behavior | Yes |
| [layout.toml](./layout-toml.md) | Window positions, sizes, arrangement | Yes |
| [keybinds.toml](./keybinds-toml.md) | Keyboard shortcuts and actions | Yes |
| [highlights.toml](./highlights-toml.md) | Text highlighting patterns | Yes |
| [colors.toml](./colors-toml.md) | Color theme, presets, palettes | Yes |

**Hot Reload**: Press `F5` to reload all configuration without restarting.

---

## File Locations

### Default Location

| Platform | Path |
|----------|------|
| Windows | `%USERPROFILE%\.vellum-fe\` |
| Linux | `~/.vellum-fe/` |
| macOS | `~/.vellum-fe/` |

### Custom Location

Override with `--data-dir` or `VELLUM_FE_DIR`:

```bash
# Command line
vellum-fe --data-dir /custom/path

# Environment variable
export VELLUM_FE_DIR=/custom/path
```

---

## Character Profiles

Per-character configuration lives in `~/.vellum-fe/profiles/<CharName>/`:

```
~/.vellum-fe/
├── config.toml           # Global defaults
├── layout.toml
├── keybinds.toml
├── highlights.toml
├── colors.toml
└── profiles/
    ├── Warrior/          # Character-specific overrides
    │   ├── layout.toml
    │   └── highlights.toml
    └── Wizard/
        ├── layout.toml
        └── highlights.toml
```

### Loading Order

1. Load global defaults from `~/.vellum-fe/`
2. If `--character` specified, overlay from `profiles/<CharName>/`
3. Character files only need to contain overrides, not complete configs

### Usage

```bash
# Load Warrior's profile
vellum-fe --character Warrior
```

---

## Configuration Syntax

All config files use [TOML](https://toml.io/) syntax:

```toml
# Comments start with #

# Simple key-value
setting = "value"
number = 42
enabled = true

# Nested tables
[section]
key = "value"

# Inline tables
point = { x = 10, y = 20 }

# Arrays
list = ["one", "two", "three"]

# Array of tables
[[items]]
name = "first"

[[items]]
name = "second"
```

### Color Values

Colors can be specified as:

```toml
# Hex RGB
color = "#FF5500"

# Hex RRGGBB (same as above)
color = "#ff5500"

# Named color (from palette)
color = "bright_red"

# Preset reference
color = "@speech"  # Uses the 'speech' preset color
```

### Size Values

```toml
# Fixed pixels
width = 40

# Percentage of parent
width = "50%"
```

---

## Default Files

VellumFE ships with sensible defaults embedded in the binary. On first run, these are written to your data directory if the files don't exist.

See [Default Files Reference](../reference/default-files.md) for complete default content.

---

## Validation

VellumFE validates configuration on load:

- **Errors**: Invalid syntax or required fields prevent loading
- **Warnings**: Unknown keys or deprecated settings log warnings but allow loading
- **Missing files**: Created from defaults

Check `~/.vellum-fe/vellum-fe.log` for configuration warnings.

---

## Quick Reference

### Most Common Settings

| Setting | File | Key |
|---------|------|-----|
| Connection port | config.toml | `connection.port` |
| Default character | config.toml | `connection.character` |
| Main window size | layout.toml | `[[windows]]` with `name = "main"` |
| Monsterbold color | colors.toml | `[presets.monsterbold]` |
| Attack keybind | keybinds.toml | Entry with `action = "send"` |

### Reload Configuration

- **F5**: Reload all config files
- **Ctrl+M → Reload**: Same via menu
- **Restart**: Always picks up changes

---

## File Reference

| Page | Contents |
|------|----------|
| [config.toml](./config-toml.md) | Connection, sound, TTS, general behavior |
| [layout.toml](./layout-toml.md) | Window definitions, positions, sizes |
| [keybinds.toml](./keybinds-toml.md) | Key mappings, actions, modifiers |
| [highlights.toml](./highlights-toml.md) | Pattern matching, colors, conditions |
| [colors.toml](./colors-toml.md) | Theme colors, presets, UI styling |
| [profiles.md](./profiles.md) | Per-character configuration |

---

## See Also

- [Creating Layouts](../customization/creating-layouts.md) - Layout design guide
- [Creating Themes](../customization/creating-themes.md) - Theme authoring
- [Default Files](../reference/default-files.md) - Built-in defaults
