# First Launch

> **Shortcut**: double-clicking `vellum-fe` opens [the Launcher](./launcher.md),
> which handles everything below with saved profiles. This page covers the
> command-line paths.

## Connecting via Lich (Recommended)

Most players use [Lich](https://lichproject.org/) for scripting. Start Lich first, then:

```bash
vellum-fe --port 8000 --character YourCharacter
```

- `--port` - Lich's listening port
- `--character` - Your character name (used for per-character profiles and direct login)

VellumFE identifies itself to Lich as a Stormfront frontend, so scripts that
check `$frontend` get full feature parity.

## Lich Launcher Integration

Configure Lich launcher to spawn VellumFE automatically:

1. In Lich launcher, add VellumFE as a custom frontend
2. Set the command line to:

```
path\to\vellum-fe.exe --port %port% --key %key%
```

- `%port%` - Lich fills in the connection port
- `%key%` - Lich provides the authentication key

This method handles authentication automatically—no need to enter credentials in VellumFE.

## Direct Connection

Connect without Lich using eAccess authentication:

```bash
vellum-fe --direct \
  --account YOUR_ACCOUNT \
  --character YourCharacter \
  --game prime
```

If you omit `--password`, VellumFE prompts for it securely at startup
(recommended — passwords on the command line end up in shell history).
Credentials can also be stored in `config.toml` under `[connection]`.

| `--game` value | World |
|-------|-------|
| `prime` | GemStone IV Prime |
| `platinum` | GemStone IV Platinum |
| `shattered` | GemStone IV Shattered |
| `test` | GemStone IV Test |
| `dr`, `dr-platinum`, `dr-fallen`, `dr-test` | DragonRealms worlds |

> **Note**: In `config.toml` and launcher profiles the DragonRealms worlds are spelled without hyphens: `drplatinum`, `drfallen`, `drtest`.

> **Note**: Direct mode uses your operating system's native TLS stack — no extra setup on Windows or macOS. On Linux, see [Installation](./installation.md).

## The Interface

On successful connection, you'll see the default layout:

```
┌─────────────────────────────────────────────────────────┐
│                     Main Window                         │
│  [Game text appears here]                               │
│                                                         │
├─────────────────────────────────────────────────────────┤
│ > [Command Input]                                       │
└─────────────────────────────────────────────────────────┘
```

### Default Keys

All of these can be changed in [keybinds.toml](../configuration/keybinds-toml.md).

| Key | Action |
|-----|--------|
| `Enter` | Send command |
| `Up` / `Down` | Command history |
| `Page Up/Down` | Scroll focused window |
| `Tab` | Switch focused window |
| `Ctrl+F` | Search in window (`F3` next match) |
| `Ctrl+R` | Repeat last command |
| `Escape` | Close dialogs / cancel |
| `Ctrl+C` | **Quit VellumFE** |
| Numpad | Movement macros (`8`=north, `2`=south, ...) |

> **Copying text**: select with the mouse — it's copied to the clipboard on
> release (`selection_auto_copy`). `Ctrl+C` quits; it does not copy.

### Mouse Controls

- **Click** links to interact with objects
- **Right-click** for context menus
- **Scroll wheel** to scroll windows
- **Ctrl+drag** to move windows (modifier configurable via `drag_modifier_key`)

### Dot-Commands

Anything starting with `.` is a client command rather than game input.
Start with:

- `.menu` — open the main menu
- `.settings` — in-app settings editor
- `.help` — list every command

See the full [Command Reference](../reference/commands.md).

## Next Steps

- [Configuration](../configuration/README.md) - Customize settings
- [Widgets](../widgets/README.md) - Learn about available widgets
- [Customization](../customization/README.md) - Layouts, highlights, themes
