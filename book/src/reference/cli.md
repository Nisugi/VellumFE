# CLI Reference

```
vellum-fe [OPTIONS] [SUBCOMMAND]
```

## Options

| Flag | Description |
|------|-------------|
| `-f, --frontend <tui\|gui>` | Frontend to run (default `tui`) |
| `-p, --port <PORT>` | Lich proxy port (overrides config.toml) |
| `--host <HOST>` | Lich proxy host (overrides config.toml) |
| `--character <NAME>` | Character name (login + per-character profile) |
| `--profile <NAME>` | Use a different profile directory than the character name |
| `--key <KEY>` | Login key from Lich launcher (`%key%`) |
| `--direct` | Connect directly via eAccess (no Lich) |
| `--account <ACCOUNT>` | Account name (direct mode) |
| `--password <PASSWORD>` | Password (direct mode; omit to be prompted securely) |
| `--game <GAME>` | World for direct mode: `prime`, `platinum`, `shattered`, `test`, `dr`, `dr-platinum`, `dr-fallen`, `dr-test` |
| `-c, --config <FILE>` | Use a specific config.toml |
| `--data-dir <DIR>` | Data directory (default `~/.vellum-fe`; also `VELLUM_FE_DIR` env var) |
| `--web-port <PORT>` | Enable the [mobile web server](../frontends/web.md) on this port |
| `--color-mode <direct\|slot>` | Override color rendering mode |
| `--setup-palette` | Load the terminal palette at startup (use with `--color-mode slot`) |
| `--nosound` | Disable the sound system entirely |

## Subcommands

### validate-layout

Check a layout file for errors:

```bash
vellum-fe validate-layout                 # default layout for --character
vellum-fe validate-layout mylayout.toml
```

### migrate-layout

Convert layouts from older VellumFE versions:

```bash
vellum-fe migrate-layout --src <DIR> [--out <DIR>] [--dry-run] [-v]
```

### import-highlights

Convert a Wrayth/StormFront settings XML into highlights.toml format:

```bash
vellum-fe import-highlights settings.xml [--out FILE] [--dry-run]
```

## Common Invocations

```bash
# Lich, most common
vellum-fe --port 8000 --character Rolfard

# Lich launcher integration
vellum-fe --port %port% --key %key%

# Direct, prompted for password
vellum-fe --direct --account MYACCT --character Rolfard --game prime

# GUI frontend
vellum-fe --frontend gui --port 8000 --character Rolfard

# TUI plus phone access
vellum-fe --port 8000 --character Rolfard --web-port 8040

# Debug logging
RUST_LOG=debug vellum-fe --port 8000
```
