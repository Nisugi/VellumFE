# VellumFE

A modern terminal client for GemStone IV, built in Rust.

## Features

- **Fast rendering** - 60+ FPS with efficient text handling
- **Customizable layouts** - Position and size every window
- **Flexible connections** - Lich proxy or direct eAccess authentication
- **Rich highlighting** - Regex-based text coloring with sound alerts
- **Full keyboard control** - Rebindable keys for all actions

## Quick Start

```bash
# Connect via Lich (most common)
vellum-fe --port 8000

# Direct connection (no Lich required)
vellum-fe --direct --account ACCOUNT --password PASS --character NAME
```

## Configuration

VellumFE stores configuration in `~/.vellum-fe/`:

| File | Purpose |
|------|---------|
| `config.toml` | General settings |
| `layout.toml` | Window positions and sizes |
| `keybinds.toml` | Keyboard shortcuts |
| `highlights.toml` | Text highlighting rules |
| `colors.toml` | Color palette |

## Getting Help

- **GitHub Issues**: [github.com/Nisugi/VellumFE/issues](https://github.com/Nisugi/VellumFE/issues)
- **In-Game**: Find us on the amunet channel
