# VellumFE

A modern client for GemStone IV (and DragonRealms), built in Rust.

One core, three ways to play:

- **Terminal (TUI)** — the default. Runs in any modern terminal.
- **Desktop GUI** — native windowed client (`--frontend gui`).
- **Mobile Web** — an optional sidecar server that lets your phone's browser join the same session, with tap-to-move exits and configurable macro buttons.

## Features

- **Customizable layouts** — position and size every window
- **Flexible connections** — Lich proxy or direct eAccess (no Lich required)
- **Rich highlighting** — regex coloring, sounds, line squelching, redirects, text replacement
- **Themes** — 35+ built-in themes including accessibility variants, plus custom themes
- **Full keyboard control** — rebindable keys for all actions
- **Text-to-speech** — screen reader support built in

## Quick Start

**Double-click `vellum-fe`** — the [Launcher](./getting-started/launcher.md)
opens with saved connection profiles (passwords kept in the OS keyring).

Or from a terminal:

```bash
# Connect via Lich (most common)
vellum-fe --port 8000 --character YourName

# Direct connection (no Lich required)
vellum-fe --direct --account ACCOUNT --character YourName --game prime
```

See [First Launch](./getting-started/first-launch.md) for details.

## Configuration

VellumFE stores configuration in `~/.vellum-fe/` (override with `VELLUM_FE_DIR` or `--data-dir`):

| File | Purpose |
|------|---------|
| `global/config.toml` | General settings (connection, UI, sound, TTS, web server) |
| `global/keybinds.toml` | Keyboard shortcuts |
| `global/highlights.toml` | Text highlighting, sounds, squelch rules |
| `global/colors.toml` | Color palette, stream presets, spell colors |
| `global/macros.toml` | Macro buttons for the mobile web frontend |
| `profiles/<name>/` | Per-character overrides of any of the above |
| `themes/*.toml` | Custom themes |

Most settings can also be changed in-app: type `.settings`, or see the
[Command Reference](./reference/commands.md).

## Getting Help

- **GitHub Issues**: [github.com/Nisugi/VellumFE/issues](https://github.com/Nisugi/VellumFE/issues)
- **In-Game**: Find us on the amunet channel
