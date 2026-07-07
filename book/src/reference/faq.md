# FAQ

## General

**What is VellumFE?**
A modern, multi-frontend client for GemStone IV built in Rust — terminal
(TUI), desktop GUI, and a mobile web sidecar, all driving the same core.
DragonRealms is supported by the parser and connection layer but less
battle-tested.

**Do I need Lich?**
No. VellumFE can connect through Lich (recommended, for scripting) or
directly via eAccess with `--direct`. VellumFE itself doesn't run scripts —
use Lich for that.

**Is it free?**
Yes, open source: [github.com/Nisugi/VellumFE](https://github.com/Nisugi/VellumFE).

## Connection

**How do I connect via Lich?**
Start Lich, then `vellum-fe --port 8000 --character Name`. VellumFE
identifies as Stormfront to Lich, so scripts behave as they would under
Wrayth. See [First Launch](../getting-started/first-launch.md).

**Can I save my login credentials?**
Yes — use [the Launcher](../getting-started/launcher.md): its "Save
password" option stores the password in your OS's secure credential store
(keyring), never in a file. Storing a password in `config.toml`'s
`[connection]` section also works but is plain text; if you store nothing,
VellumFE prompts at startup.

## Configuration

**Where are config files stored?**
`~/.vellum-fe/` on every platform (Windows: `C:\Users\you\.vellum-fe\`).
Override with `--data-dir` or the `VELLUM_FE_DIR` environment variable.
See [Configuration](../configuration/README.md) for the directory layout.

**Can I have per-character settings?**
Yes — files in `profiles/<name>/` override the global ones when you launch
with `--character` (or `--profile`).

**Can I have multiple layouts?**
Yes: `.savelayout hunting`, then `.loadlayout hunting`. You can also switch
automatically by terminal size with `layout_mappings` in config.toml.

**How do I reset to defaults?**
Delete the file (or the whole `~/.vellum-fe/` directory); defaults are
recreated on next launch.

## Features

**Does VellumFE support macros?**
Two kinds: keyboard macros in [keybinds.toml](../configuration/keybinds-toml.md)
(`f5 = { macro_text = "stance defensive\r" }`), and tap-button macros for
the phone in [macros.toml](../configuration/macros-toml.md).

**Sound alerts?**
Yes — add `sound = "alert.wav"` to any highlight. See
[Sound Alerts](../customization/sounds.md).

**Text-to-speech?**
Yes — set `enabled = true` in config.toml's `[tts]` section. Navigation and
volume/rate keys are in [keybinds.toml](../configuration/keybinds-toml.md).

**Can I hide spammy lines?**
Yes — `squelch = true` on a highlight pattern. See
[highlights.toml](../configuration/highlights-toml.md).

**Can I import my Wrayth highlights?**
Yes: `vellum-fe import-highlights settings.xml`. See the
[CLI Reference](./cli.md).
