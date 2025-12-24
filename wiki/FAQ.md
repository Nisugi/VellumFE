# Frequently Asked Questions

## General

### What is VellumFE?

VellumFE is a modern, multi-frontend terminal client for GemStone IV (and potentially DragonRealms). It features:
- Customizable layouts and themes
- Text highlighting and triggers
- Multiple connection modes (Lich proxy or direct eAccess)
- Cross-platform support (Windows, macOS, Linux)

### Why "VellumFE"?

The name comes from the dual-frontend architecture - VellumFE can render to both TUI (terminal) and GUI backends from the same core.

### Is VellumFE free?

Yes! VellumFE is open source under the MIT license.

### Does VellumFE work with DragonRealms?

The parser supports the Wrayth XML protocol used by both GS4 and DR. DR support is planned but not fully tested.

## Connection

### Do I need Lich to use VellumFE?

No! VellumFE supports two connection modes:
1. **Lich proxy** (recommended) - Connect through Lich for script support
2. **Direct eAccess** - Connect directly without Lich

### How do I connect via Lich?

1. Start Lich and log into the game
2. Note the port Lich is listening on (default: 8000)
3. Run: `vellum-fe --host 127.0.0.1 --port 8000`

### How do I connect directly?

```bash
vellum-fe --direct \
  --account YOUR_ACCOUNT \
  --password YOUR_PASSWORD \
  --game prime \
  --character CHARACTER_NAME
```

### Can I save my login credentials?

For security, VellumFE doesn't save passwords. Use environment variables:
```bash
export GS4_ACCOUNT=your_account
export GS4_PASSWORD=your_password
vellum-fe --direct --game prime --character NAME
```

## Configuration

### Where are config files stored?

| Platform | Location |
|----------|----------|
| Linux | `~/.config/vellum-fe/` |
| macOS | `~/Library/Application Support/vellum-fe/` |
| Windows | `%APPDATA%\vellum-fe\` |

### How do I reset to default config?

Delete the config directory and restart VellumFE:
```bash
rm -rf ~/.config/vellum-fe/
vellum-fe
```

### Can I have multiple layouts?

Yes! Create multiple layout files and switch between them:
```bash
vellum-fe --layout hunting.toml
vellum-fe --layout merchant.toml
```

### How do I edit windows visually?

Press `F1` → Layout → Edit Windows, or use keyboard shortcuts to resize/move windows while in edit mode.

## Features

### Does VellumFE support scripts?

VellumFE itself doesn't run scripts. Use Lich for scripting and connect VellumFE as a frontend.

### Can I use VellumFE with Wrayth scripts?

If using Lich, yes - Lich handles all scripting. VellumFE just displays the output.

### Does VellumFE support macros?

Yes! Define macros in keybinds.toml:
```toml
[[keybinds]]
key = "F5"
action = "send"
command = "stance defensive;hide"
```

### Can I have sound alerts?

Yes! Configure in triggers.toml:
```toml
[[triggers]]
pattern = "(?i)you are stunned"
sound = "alert.wav"
```

### Does VellumFE support text-to-speech?

Yes! Enable in config.toml:
```toml
[tts]
enabled = true
```

And add TTS triggers:
```toml
[[triggers]]
pattern = "feel your life fading"
tts = "Health critical!"
```

## Troubleshooting

### Why is my display garbled?

Common causes:
1. Terminal doesn't support UTF-8
2. Font missing Unicode characters
3. Wrong TERM environment variable

Try: `export TERM=xterm-256color`

### Why are my keybinds not working?

1. Check keybinds.toml syntax
2. Some keys may be captured by your terminal
3. Try different key combinations

### Why can't I see colors?

1. Ensure terminal supports 256 colors
2. Check `TERM` environment variable
3. Verify colors.toml is valid

### Why does VellumFE crash on startup?

1. Check config file syntax (use a TOML validator)
2. Try resetting to defaults
3. Run with `--debug` flag for more info

## Comparison

### VellumFE vs Profanity?

| Feature | VellumFE | Profanity |
|---------|----------|-----------|
| TUI | Yes | Yes |
| GUI | Planned | No |
| Cross-platform | Yes | Unix only |
| Direct eAccess | Yes | No |
| Active development | Yes | Maintenance |

### VellumFE vs Wizard FE?

| Feature | VellumFE | Wizard FE |
|---------|----------|-----------|
| Platform | All | Windows |
| Open source | Yes | No |
| Customization | High | Medium |
| Scripting | Via Lich | Built-in |

## Contributing

### How can I contribute?

- Report bugs on [GitHub Issues](https://github.com/nisugi/vellum-fe/issues)
- Submit pull requests
- Improve documentation
- Share your layouts/themes

### Where's the source code?

GitHub: https://github.com/nisugi/vellum-fe

### What language is VellumFE written in?

Rust, using ratatui for the TUI frontend.
