# VellumFE Wiki

Welcome to the documentation hub for **VellumFE**, the modern terminal frontend for GemStone IV. This wiki explains how to launch the bundled executable, configure every subsystem, and understand the underlying code so you can extend or troubleshoot the client with confidence.

## Quick Start Snapshot

1. **Start Lich in detached mode**  
   - Windows PowerShell:  
     ```
     "C:\Ruby4Lich5\3.4.x\bin\rubyw.exe" "C:\Ruby4Lich5\Lich5\lich.rbw" --login CharacterName --gemstone --without-frontend --detachable-client=8000
     ```
   - Linux/macOS:  
     ```
     ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
     ```
2. **Launch VellumFE**
   ```
   .\vellum-fe.exe --port 8000 --character CharacterName --links
   ```
   Add `--nomusic` if you prefer no startup music. You can change the track later via `[ui] startup_music_file`.
3. **Official links open automatically**  
   When the game emits a `<LaunchURL>` tag (goals), VellumFE launches the URL in your default browser.

Detailed setup notes live in [Getting Started](Getting-Started.md).

## Documentation Map

- [Getting Started](Getting-Started.md) – requirements, first launch checklist, CLI options.
- [Command Reference](Command-Reference.md) – dot commands, CLI flags, and keybind defaults.
- [Configuration Guide](Configuration.md) – file locations, TOML schema, layout mapping.
- [Layouts & Windows](Layouts-and-Windows.md) – layout workflow, window editor, resizing model.
- [Window Reference](Window-Reference.md) – behavior and config knobs for every widget, including the new Room and Inventory panes.
- [Highlights & Alerts](Highlights-and-Alerts.md) – regex vs literal matches, sounds, event patterns.
- [Keybinds & Macros](Keybinds-and-Macros.md) – binding syntax, builtin actions, macro tips.
- [Colors & Theming](Colors-and-Theming.md) – UI palette, presets, spell colors, theme sharing.
- [Performance & Debugging](Performance-and-Debugging.md) – FPS panel, logging, layout validator.

## Contributing & Support

- File bugs or feature requests on the main repo issue tracker.
- Sync wiki updates by committing to `/wiki/`; the GitHub Action publishes to the project wiki automatically.
- For realtime help, join the GemStone IV Discord (`#scripting` channel) and mention VellumFE.
