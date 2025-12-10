# Reference

Comprehensive reference documentation for VellumFE.

## Quick Access

| Reference | Description |
|-----------|-------------|
| [CLI Options](./cli-options.md) | Command line flags |
| [Config Schema](./config-schema.md) | Complete configuration reference |
| [Keybind Actions](./keybind-actions.md) | All keybind actions A-Z |
| [Parsed Elements](./parsed-elements.md) | All ParsedElement variants |
| [Stream IDs](./stream-ids.md) | All stream identifiers |
| [Preset Colors](./preset-colors.md) | Named color presets |
| [Environment Variables](./environment-vars.md) | Environment configuration |
| [Default Files](./default-files.md) | Default configuration contents |

## How to Use This Section

This reference section is designed for **lookup**, not learning. Use it when you:

- Need exact syntax for a configuration option
- Want to know all available values for a setting
- Need to look up a specific color name or stream ID
- Are troubleshooting a configuration issue

For learning and tutorials, see:
- [Getting Started](../getting-started/README.md) - Initial setup
- [Tutorials](../tutorials/README.md) - Step-by-step guides
- [Configuration](../configuration/README.md) - Configuration concepts

## Quick Reference Cards

### Common CLI Flags

```bash
vellum-fe --host HOST --port PORT    # Lich mode
vellum-fe --direct --account X ...   # Direct mode
vellum-fe --config PATH              # Custom config
vellum-fe --debug                    # Debug logging
```

### Essential Config Keys

```toml
[connection]
mode = "lich"           # or "direct"
host = "127.0.0.1"
port = 8000

[[widgets]]
type = "text"           # Widget type
name = "main"           # Unique name
x = 0                   # Position (0-100)
y = 0
width = 100             # Size (0-100)
height = 100
```

### Common Keybind Actions

```toml
[keybinds."key"]
action = "scroll_up"        # Widget action
macro = "command"           # Send command
```

### Stream Quick Reference

| Stream | Content |
|--------|---------|
| main | Primary game output |
| room | Room descriptions |
| combat | Combat messages |
| speech | Player speech |
| thoughts | ESP/thoughts |
| whisper | Whispers |

## Conventions

### Value Types

| Type | Example | Description |
|------|---------|-------------|
| string | `"value"` | Text in quotes |
| integer | `100` | Whole number |
| float | `1.5` | Decimal number |
| boolean | `true` | true or false |
| array | `["a", "b"]` | List of values |

### Color Values

Colors can be specified as:
- Preset name: `"red"`, `"bright_blue"`
- Hex code: `"#ff0000"`
- RGB: (in some contexts)

### Path Values

Paths support:
- Absolute: `/home/user/.vellum-fe/`
- Home expansion: `~/.vellum-fe/`
- Relative (from config location)

## See Also

- [Configuration Guide](../configuration/README.md)
- [Tutorials](../tutorials/README.md)
- [Troubleshooting](../troubleshooting/README.md)

