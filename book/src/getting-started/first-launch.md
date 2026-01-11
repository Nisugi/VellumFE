# First Launch

## Connecting via Lich (Recommended)

Most players use [Lich](https://lichproject.org/) for scripting. Start Lich first, then:

```bash
vellum-fe --port 8000 --character YourCharacter
```

- `--port` - Lich's listening port (default: 8000)
- `--character` - Your character name (used for per-character layouts)

## Direct Connection

Connect without Lich using eAccess authentication:

```bash
vellum-fe --direct \
  --account YOUR_ACCOUNT \
  --password YOUR_PASSWORD \
  --character YourCharacter \
  --game prime
```

Games: `prime`, `platinum`, `fallen`, `test`

> **Note**: Direct mode requires OpenSSL on Windows. See [Installation](./installation.md).

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

### Basic Controls

| Key | Action |
|-----|--------|
| `Enter` | Send command |
| `Page Up/Down` | Scroll main window |
| `Ctrl+C` | Copy selected text |
| `Escape` | Close menus / cancel |
| `F1` | Open main menu |

### Mouse Controls

- **Click** links to interact with objects
- **Right-click** for context menus
- **Scroll wheel** to scroll windows
- **Drag** window borders to resize (in edit mode)

## Next Steps

- [Configuration](../configuration/README.md) - Customize settings
- [Widgets](../widgets/README.md) - Learn about available widgets
- [Customization](../customization/README.md) - Create custom layouts
