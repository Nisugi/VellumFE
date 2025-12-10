# Getting Started

This guide walks through prerequisites, the expected launch sequence, and first-run tips so you can connect to Lich quickly without compiling Rust.

## Prerequisites

- **Lich 5** installed and configured for your GemStone IV account.
- A **terminal that supports mouse input** (Windows Terminal, MobaXterm, iTerm2, Kitty, Alacritty, etc.).
- The prebuilt **`vellum-fe.exe` (or `vellum-fe` on Linux/macOS)** from the Releases page.

> You do *not* need Rust toolchains for normal play.

## Launch Sequence

1. **Start Lich in detached mode**
   - Windows PowerShell:
     ```powershell
     "C:\Ruby4Lich5\3.4.x\bin\rubyw.exe" "C:\Ruby4Lich5\Lich5\lich.rbw" --login CharacterName --gemstone --without-frontend --detachable-client=8000
     ```
   - Linux/macOS:
     ```bash
     ruby ~/lich5/lich.rbw --login CharacterName --gemstone --without-frontend --detachable-client=8000
     ```

2. **Launch VellumFE**
   ```powershell
   .\vellum-fe.exe --port 8000 --character CharacterName --links
   ```
   Replace the port or character name to match your Lich session. The `--links` flag turns on clickable links.

3. **Type `.menu` inside VellumFE** to explore layouts, highlights, colors, and other configurations.

## Command-Line Options

| Option | Description |
| --- | --- |
| `-p, --port <number>` | TCP port used by Lich detached mode (default `8000`). |
| `-c, --character <name>` | Character profile name; determines which config directory (*.toml, layouts, history) is loaded. |
| `--links` | Enables clickable link parsing for context menus. |
| `--nomusic` | Skips startup music on connect. |
| `--validate-layout <path>` | Validate a layout file against multiple terminal sizes and exit. Use with `--baseline` / `--sizes`. |
| `--baseline <WxH>` | Override the designed size when validating layouts (e.g., `--baseline 120x40`). |
| `--sizes <WxH[,WxH...]>` | Comma-separated list of terminal sizes to test, such as `--sizes 100x30,120x40,160x50`. |

## First-Run Behavior

Running the executable builds out `~/.vellum-fe/` (Windows: `C:\Users\<you>\.vellum-fe\`) with shared assets and a per-character profile:

- `layouts/` – shared layout library (`layout.toml`, `none.toml`, `sidebar.toml`, your saves).
- `sounds/` – drop custom audio files here (`.mp3`, `.wav`, `.ogg`, `.flac`).
- `cmdlist1.xml` – command/link map copied from the defaults directory for clickable menus.
- `<character>/` – character-specific data (name is lowercased to match Lich conventions):
  - `config.toml` – primary settings (connection, UI, sound, event patterns).
  - `colors.toml` – UI palette, presets, prompt coloring, spell colors, and shared color palette entries.
  - `highlights.toml` – highlight definitions organized by category.
  - `keybinds.toml` – key combination mapping (actions or macros).
  - `layout.toml` – current layout file, autosaved on terminal resize and on `.quit` or `Ctrl+C`.
  - `history.txt` – command history for the input box.
  - `debug.log` – logging output per character.

Default files ship inside the binary via `include_dir`, so a missing file is automatically recreated with safe defaults on launch.

## After Launch

- `.menu` opens the main menu with shortcuts to colors, highlights, keybinds, windows, and settings.
- `.savelayout <name>` stores your current layout to the shared `layouts` directory. `.loadlayout <name>` restores any layout you have saved.
- `.settings` exposes every configurable field with inline descriptions; press `Tab` to move between inputs and `Enter` to save.
- `.highlights`, `.keybinds`, `.uicolors`, `.palette`, and other dot commands open specialized managers that mirror the wiki sections listed on the home page.

If you ever need to reset to stock behavior, remove the `~/.vellum-fe/<character>/` directory while VellumFE is closed—the next run will rebuild defaults.
