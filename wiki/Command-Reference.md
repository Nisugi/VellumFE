# Command Reference

This page centralizes both command-line switches and in-game dot commands exposed by VellumFE.

## Executable Flags

| Flag | Description |
| --- | --- |
| `-p, --port <number>` | Lich detached-mode port (defaults to `8000`). |
| `-c, --character <name>` | Character profile to load/save under `~/.vellum-fe/<name>/`. |
| `--links` | Enable clickable link parsing for MI/ME streams and right-click menus (disabled by default). |
| `--nomusic` | Disable startup music when a session connects. |
| `--validate-layout <path>` | Validate a layout file against multiple terminal sizes and exit with status `0` (pass) or `2` (errors). |
| `--baseline <WxH>` | Override the designed baseline for layout validation (used with `--validate-layout`). |
| `--sizes <WxH[,WxH...]>` | Comma-separated list of terminal dimensions to validate (defaults to `80x24,100x30,120x40,140x40,160x50`). |

## Dot Commands

Type dot commands in the command input pane. Commands ignore case; aliases behave identically.

### Session & Help

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.menu` | – | – | Opens the main popup menu (colors, highlights, keybinds, windows, settings). |
| `.help` | `.h`, `.?` | – | Prints a categorized cheat sheet of the most important commands. |
| `.quit` | `.q` | – | Cleanly exits VellumFE. |

### Layout Management

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.savelayout` | – | `[name]` | Saves the current layout to `~/.vellum-fe/layouts/<name>.toml`. Defaults to `default`. |
| `.loadlayout` | – | `[name]` | Loads a saved layout, replacing the active one and updating the baseline reference. |
| `.layouts` | – | – | Lists all layout files discovered in the layouts directory. |
| `.baseline` | – | – | Captures the current terminal size as the baseline for proportional resizing. |
| `.resize` | – | – | Rescales the active layout to the current terminal size using the stored baseline. Autosaves to `auto_<character>.toml`. |

### Window Creation & Editing

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.windows` | `.listwindows` | – | Lists every window name defined in the current layout. |
| `.templates` | `.availablewindows` | – | Lists built-in window templates that can be cloned via `.addwindow`. |
| `.addwindow` | `.newwindow` | `[template?]` | Launches the window editor to create a window (optionally seeded from a template). |
| `.editwindow` | `.editwin` | `[name]` | Opens the window editor for an existing window (or prompts for a window when omitted). |
| `.deletewindow` | `.deletewin` | `<name>` | Removes a window and its stream mapping. |
| `.customwindow` | `.customwin` | `<name> <stream1,stream2,...>` | Adds a blank text window subscribed to the specified streams. |
| `.editinput` | `.editcommandbox` | – | Opens the editor for the command-input widget. |
| `.lockwindows` | `.lockall` | – | Locks every window in place (prevents drag/resize). |
| `.unlockwindows` | `.unlockall` | – | Unlocks all windows. |
| `.rename` | – | `<window> <new title...>` | Changes a window's title; updates both live config and baseline copy. |
| `.border` | – | `<window> <style> [color] [sides...]` | Sets border style (`single`, `double`, `rounded`, `thick`, `none`) and optional hex color or specific sides. |
| `.contentalign` | `.align` | `<window> <top-left|top-right|bottom-left|bottom-right|center>` | Controls vertical alignment when content is shorter than the viewport. |

### Tabbed Windows

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.createtabbed` | `.tabbedwindow` | `<name> <tab:stream,...>` | Creates a tabbed window with the given tab-to-stream mapping. |
| `.addtab` | – | `<window> <name> <stream>` | Appends a tab to a tabbed window. |
| `.removetab` | – | `<window> <name>` | Deletes a tab (requires at least one tab remaining). |
| `.switchtab` | – | `<window> <name|index>` | Switches the active tab by display name or zero-based index. |
| `.movetab` | `.reordertab` | `<window> <name> <index>` | Repositions a tab to a new index. |
| `.tabcolors` | `.settabcolors` | `<window> <active> [unread] [inactive]` | Overrides tab highlight colors using hex values. |

### Progress, Countdown, Indicators

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.setprogress` | `.setprog` | `<window> <current> <max>` | Manually sets a progress-bar value (for testing). |
| `.setcountdown` | – | `<window> <seconds>` | Starts a countdown widget at the specified duration. |
| `.setbarcolor` | `.barcolor` | `<window> <fill_color> [background]` | Adjusts colors for countdown/progress widgets. |
| `.indicatoron` | – | – | Forces the built-in status indicator widgets (and dashboard mirrors) to state `1`. |
| `.indicatoroff` | – | – | Forces the same indicators back to `0`. |
| `.togglespellid` | `.toggleeffectid` | `<window>` | Switches an active-effects window between spell name and spell ID display. |

### Highlights & Alerts

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.listhighlights` | `.listhl`, `.highlights` | – | Lists highlight names grouped by category. |
| `.addhighlight` | `.addhl` | – | Opens the highlight form in create mode. |
| `.edithighlight` | `.edithl` | `<name>` | Opens the highlight form pre-filled with an existing highlight. |
| `.deletehighlight` | `.delhl` | `<name>` | Removes a highlight and saves the config. |
| `.testhighlight` | `.testhl` | `<name> <sample text...>` | Runs a highlight against sample text and reports matches, styling, and errors. |

### Keybinds & Input

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.listkeybinds` | `.listkeys`, `.keybinds` | – | Lists keybinds currently registered. |
| `.addkeybind` | `.addkey` | – | Opens the keybind form to create a binding. |
| `.editkeybind` | `.editkey` | `<combo>` | Edits an existing keybind (e.g., `ctrl+e`). |
| `.deletekeybind` | `.delkey` | `<combo>` | Removes a keybind. |

### Color Management

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.colors` | – | – | Opens the color browser for presets, prompts, and UI colors. |
| `.uicolors` | – | – | Opens the UI color browser/editor directly. |
| `.palette` | `.colorpalette` | – | Shows the shared color palette manager. |
| `.addcolor` | `.newcolor`, `.createcolor` | – | Adds a new entry to the color palette. |
| `.addspellcolor` | `.newspellcolor` | – | Adds a new spell-color range definition. |
| `.spellcolors` | – | – | Opens the spell color browser/editor. |

### Settings & Configuration

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.settings` | `.config` | – | Opens the settings editor popup (connection, UI, sound, prompts, presets). |

### Context Menus & Debug

| Command | Aliases | Parameters | Summary |
| --- | --- | --- | --- |
| `.testmenu` | – | `<exist_id> [noun]` | Sends an `_menu` request to Lich to test context menus for a given exist ID. |
| `.randominjuries` | `.randinjuries` | – | Randomizes the injury doll for visual testing. |
| `.randomcompass` | `.randcompass` | – | Randomizes compass exits. |
| `.randomprogress` | `.randprog` | – | Randomizes all progress bars. |
| `.randomcountdowns` | `.randcountdowns` | – | Seeds countdown timers with random values. |

These commands persist config changes immediately by writing the appropriate TOML file (layouts, highlights, keybinds, colors). Remember to keep `~/.vellum-fe/` under version control if you want to track your customizations over time.
