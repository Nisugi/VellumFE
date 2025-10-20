# Commands Reference

VellumFE has special dot commands (starting with `.`) that are handled locally and not sent to the game. This is a complete reference of all available commands.

## Application Control

### .quit
Exit VellumFE.

```bash
.quit
```

**Note:** Automatically saves layout as `auto_<character>.toml` on exit.

### .help
Display help information (lists all dot commands).

```bash
.help
```

## Settings and Configuration

### .settings / .config
Open the settings editor popup.

```bash
.settings
.config          # Alias
```

**Navigation:**
- `↑/↓` - Navigate settings
- `PgUp/PgDn` - Page up/down
- `Enter` or `Space` - Edit/toggle setting
- `Esc` - Close editor

**Categories:**
- Connection (host, port)
- UI (colors, icons, poll timeout)
- Sound (enable, volume)
- Presets (text style colors)
- Spells (spell-specific colors)
- Prompts (prompt indicator colors)

Changes save immediately to config file.

## Window Management

### .windows / .listwindows
List all active windows with their properties.

```bash
.windows
.listwindows     # Alias
```

Shows: name, type, position, size, streams.

### .templates
List available window templates.

```bash
.templates
```

Shows all built-in window templates you can create with `.createwindow`.

### .createwindow
Create a window from a template.

```bash
.createwindow <template_name>
```

**Examples:**
```bash
.createwindow thoughts
.createwindow health
.createwindow casttime
.createwindow compass
```

**Common templates:**
- Text: `main`, `thoughts`, `speech`, `room`, `familiar`, `logons`, `deaths`, `arrivals`, `ambients`
- Progress: `health`, `mana`, `stamina`, `spirit`, `mindstate`, `encumbrance`, `stance`, `bloodpoints`
- Countdown: `roundtime`, `casttime`, `stun`
- Special: `compass`, `injuries`, `hands`, `dashboard`, `effects`

### .customwindow
Create a custom text window with specified streams.

```bash
.customwindow <name> <stream1,stream2,...>
```

**Examples:**
```bash
.customwindow mywindow main
.customwindow allchat speech,thoughts,whisper
.customwindow combat main,room
```

### .deletewindow
Delete a window.

```bash
.deletewindow <name>
```

**Examples:**
```bash
.deletewindow thoughts
.deletewindow oldwindow
```

**Warning:** This permanently removes the window. Save your layout first if you might want it back.

### .rename
Change a window's title.

```bash
.rename <window_name> <new_title>
```

**Examples:**
```bash
.rename main "Game Output"
.rename thoughts "My Thoughts"
```

**Note:** This changes the title bar text, not the window's internal name.

### .border
Change a window's border style and optionally color.

```bash
.border <window_name> <style> [color]
```

**Border Styles:**
- `single` - Single-line border (─│┌┐└┘)
- `double` - Double-line border (═║╔╗╚╝)
- `rounded` - Rounded corners (─│╭╮╰╯)
- `thick` - Thick border (━┃┏┓┗┛)
- `none` - No border

**Examples:**
```bash
.border main single
.border main double
.border main rounded #ff0000
.border thoughts none
```

### .editwindow
Open window editor for an existing window.

```bash
.editwindow [window_name]
```

**Examples:**
```bash
.editwindow main
.editwindow            # Opens window selector
```

**Window Editor:**
- Edit all window properties
- Dynamic fields based on widget type
- Tab/Shift+Tab to navigate
- Enter to save, Esc to cancel
- Draggable via title bar

### .newwindow / .addwindow
Open window editor to create a new window.

```bash
.newwindow
.addwindow       # Alias
```

### .editinput
Edit the command input box properties.

```bash
.editinput
```

## Tabbed Windows

### .createtabbed
Create a tabbed window with specified tabs.

```bash
.createtabbed <window_name> <tab1:stream1,tab2:stream2,...>
```

**Examples:**
```bash
.createtabbed chat Speech:speech,Thoughts:thoughts,Whisper:whisper
.createtabbed social Speech:speech,Emote:ambients
```

### .addtab
Add a tab to an existing tabbed window.

```bash
.addtab <window_name> <tab_name> <stream>
```

**Examples:**
```bash
.addtab chat LNet logons
.addtab chat Familiar familiar
```

### .removetab
Remove a tab from a tabbed window.

```bash
.removetab <window_name> <tab_name>
```

**Examples:**
```bash
.removetab chat LNet
.removetab chat Whisper
```

### .switchtab
Switch to a specific tab in a tabbed window.

```bash
.switchtab <window_name> <tab_name|index>
```

**Examples:**
```bash
.switchtab chat Speech
.switchtab chat Thoughts
.switchtab chat 0              # By index (0-based)
```

## Widget-Specific Commands

### .setprogress
Manually update a progress bar.

```bash
.setprogress <window_name> <current> <max>
```

**Examples:**
```bash
.setprogress health 50 100
.setprogress mana 75 150
```

**Note:** Progress bars are normally auto-updated by the game. This is for testing or custom windows.

### .setbarcolor
Change a progress bar's colors.

```bash
.setbarcolor <window_name> <bar_color> [bg_color]
```

**Examples:**
```bash
.setbarcolor health #00ff00
.setbarcolor health #00ff00 #003300
.setbarcolor mana #0000ff #000033
```

### .setcountdown
Manually set a countdown timer.

```bash
.setcountdown <window_name> <seconds>
```

**Examples:**
```bash
.setcountdown roundtime 5
.setcountdown casttime 3
```

**Note:** Countdown timers are normally auto-updated by the game. This is for testing.

## Layout Management

### .savelayout
Save the current window layout.

```bash
.savelayout [name]
```

**Examples:**
```bash
.savelayout
.savelayout combat
.savelayout social
.savelayout default
```

Saves to `~/.vellum-fe/layouts/<name>.toml` (default: `default.toml`).

**Note:** Auto-save happens on `.quit` as `auto_<character>.toml`.

### .loadlayout
Load a saved window layout.

```bash
.loadlayout [name]
```

**Examples:**
```bash
.loadlayout
.loadlayout combat
.loadlayout social
```

Loads from `~/.vellum-fe/layouts/<name>.toml` (default: `default.toml`).

### .layouts
List all saved layouts.

```bash
.layouts
```

Shows all `.toml` files in `~/.vellum-fe/layouts/`.

## Highlights

### .highlights / .listhl
Open the highlights browser.

```bash
.highlights
.listhl          # Alias
```

**Highlights Browser:**
- `↑/↓` - Navigate highlights
- `PgUp/PgDn` - Page up/down
- `Enter` - Edit selected highlight
- `Delete` - Delete selected highlight
- `Esc` - Close browser

Shows:
- Grouped by category
- Color previews `[#RRGGBB]`
- Sound indicators ♫

### .addhl
Open highlight form to create a new highlight.

```bash
.addhl
```

**Highlight Form Fields:**
- **Name** - Unique identifier
- **Pattern** - Regex pattern to match
- **Category** - Grouping category (optional)
- **FG Color** - Foreground color (#RRGGBB)
- **BG Color** - Background color (#RRGGBB or -)
- **Bold** - Bold text (checkbox)
- **Color Entire Line** - Color whole line vs matched text (checkbox)
- **Fast Parse** - Use optimized Aho-Corasick (checkbox)
- **Sound File** - Path to .wav file (optional)
- **Volume** - Sound volume 0.0-1.0

**Navigation:**
- Tab/Shift+Tab - Navigate fields
- Enter - Save
- Esc - Cancel

**Examples of patterns:**
```regex
^You (?:swing|thrust|slice)    # Your attacks
arrives?$                        # Arrivals
^\[LNet\]                       # LNet messages
```

## Keybinds

### .addkeybind
Open keybind form to create a new keybind.

```bash
.addkeybind
```

**Keybind Form Fields:**
- **Key Combo** - Key combination (e.g., `Ctrl+K`, `F1`)
- **Action Type** - Action (built-in) or Macro (text)
- **Action/Macro** - Action name or text to send

**Navigation:**
- Tab/Shift+Tab - Navigate fields
- Enter - Save
- Esc - Cancel

**Built-in Actions:**
- ScrollUp, ScrollDown, PageUp, PageDown
- FocusNextWindow, FocusPreviousWindow
- ClearSelection
- CopySelectedText
- ToggleTimestamps
- ToggleBorders
- IncreaseFontSize, DecreaseFontSize
- SaveLayout, LoadLayout
- Quit

**Examples:**
```
Key: F1
Type: Macro
Macro: stance offensive

Key: Ctrl+S
Type: Action
Action: SaveLayout
```

## Command-Line Arguments

While not dot commands, these are passed when launching VellumFE:

```bash
vellumfe [OPTIONS]
```

**Options:**
- `--port <PORT>` or `-p <PORT>` - Port number (default: 8000)
- `--character <NAME>` or `-c <NAME>` - Character name for configs
- `--links` - Enable clickable links with context menus

**Examples:**
```bash
.\vellumfe.exe --port 8001
.\vellumfe.exe --port 8001 --character Nisugi
.\vellumfe.exe --port 8001 --character Nisugi --links
```

## Tips and Tricks

### Command History
- VellumFE maintains command history
- Use `↑` and `↓` arrows to cycle through previous commands

### Tab Completion
Not currently implemented, but planned for future release.

### Combining Commands
You can run multiple dot commands in sequence, but you must send them as separate inputs (not combined on one line).

### Case Sensitivity
- Dot commands are case-insensitive
- Window names are case-sensitive
- Stream names are case-sensitive

### Aliases
Some commands have aliases for convenience:
- `.config` = `.settings`
- `.listwindows` = `.windows`
- `.listhl` = `.highlights`
- `.addwindow` = `.newwindow`

### Escape Character
If you need to send a message starting with `.` to the game, use `..`:

```bash
..quit
```

This sends `.quit` to the game instead of executing the quit command.

## See Also

- [Windows and Layouts](Windows-and-Layouts.md) - Window management details
- [Highlights](Highlights.md) - Highlight system deep dive
- [Keybinds](Keybinds.md) - Keybind system details
- [Configuration](Configuration.md) - Config file reference
