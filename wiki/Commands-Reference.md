# Commands Reference

This page documents all dot commands available in vellum-fe. Dot commands are local commands that are not sent to the game server.

## Table of Contents

- [Application Commands](#application-commands)
- [Window Commands](#window-commands)
  - [Tabbed Window Commands](#createtabbed--tabbedwindow)
- [Layout Commands](#layout-commands)
- [Progress Bar Commands](#progress-bar-commands)
- [Countdown Commands](#countdown-commands)
- [Indicator Commands](#indicator-commands)
- [Active Effects Commands](#active-effects-commands)
- [Highlight Commands](#highlight-commands)
- [Keybind Commands](#keybind-commands)
- [Color Palette Commands](#color-palette-commands)
- [Combat Tracking Commands](#combat-tracking-commands)
- [Debug Commands](#debug-commands)

---

## Application Commands

### `.quit` / `.q`

Exit the application.

**Syntax:**
```
.quit
.q
```

**Example:**
```
.quit
```

---

## Window Commands

### `.createwindow` / `.createwin`

Create a new window from a built-in template.

**Syntax:**
```
.createwindow <template_name>
.createwin <template_name>
```

**Parameters:**
- `template_name` - Name of the window template to create

**Examples:**
```
.createwindow loot
.createwindow health
.createwindow compass
.createwin thoughts
```

**See also:** `.templates` to list available templates

---

### `.customwindow` / `.customwin`

Create a custom text window with specified stream routing.

**Syntax:**
```
.customwindow <name> <stream1,stream2,...>
.customwin <name> <stream1,stream2,...>
```

**Parameters:**
- `name` - Name for the new window
- `stream1,stream2,...` - Comma-separated list of streams to route to this window

**Examples:**
```
.customwindow combat death
.customwindow chatter speech,whisper
.customwin mywindow main,thoughts
```

**Notes:**
- The window will be created at position (0,0) with default size 10x40
- Use mouse to move and resize the window
- Available streams: main, thoughts, speech, whisper, familiar, room, logons, deaths, arrivals, ambients, announcements, loot

**See also:** [Stream Routing Guide](https://github.com/Nisugi/VellumFE/wiki/Stream-Routing)

---

### `.deletewindow` / `.deletewin`

Delete an existing window.

**Syntax:**
```
.deletewindow <window_name>
.deletewin <window_name>
```

**Parameters:**
- `window_name` - Name of the window to delete

**Examples:**
```
.deletewindow loot
.deletewin health
```

---

### `.editwindow` / `.editwin`

Open the window editor to edit an existing window's properties.

**Syntax:**
```
.editwindow [window_name]
.editwin [window_name]
```

**Parameters:**
- `window_name` - (Optional) Name of the window to edit. If omitted, opens a selection list.

**Examples:**
```
.editwindow main
.editwin thoughts
.editwindow
```

**Notes:**
- Opens the visual window editor interface
- Navigate with Tab/Arrow keys, save with Ctrl+S
- Changes are not persisted until you `.savelayout`

**See also:** [Window Editor Guide](Window-Editor.md)

---

### `.addwindow` / `.newwindow`

Open the window editor to create a new window.

**Syntax:**
```
.addwindow
.newwindow
```

**Examples:**
```
.addwindow
```

**Notes:**
- Opens the visual window editor in creation mode
- First select widget type, then optionally select a template
- Navigate with Tab/Arrow keys, save with Ctrl+S
- New windows default to position (0,0) - move with mouse after creation

**See also:** [Window Editor Guide](Window-Editor.md)

---

### `.editinput` / `.editcommandbox`

Edit the command input box position, size, and appearance.

**Syntax:**
```
.editinput
.editcommandbox
```

**Examples:**
```
.editinput
```

**Notes:**
- Opens the window editor for the command input box
- Configure position, size, border style, colors, and title
- Changes are not persisted until you `.savelayout`
- Command input box cannot be deleted (it's not in the window list)

**See also:** [Window Editor Guide](Window-Editor.md)

---

### `.windows` / `.listwindows`

List all active windows.

**Syntax:**
```
.windows
.listwindows
```

**Example:**
```
.windows
→ Windows: main, thoughts, health, mana, roundtime
```

---

### `.templates` / `.availablewindows`

List all available window templates.

**Syntax:**
```
.templates
.availablewindows
```

**Example:**
```
.templates
→ Available window templates: main, thoughts, speech, familiar, room, logons, deaths, arrivals, ambients, announcements, loot, health, mana, stamina, spirit, mindstate, encumbrance, stance, bloodpoints, roundtime, casttime, stun, compass, injuries, poisoned, diseased, bleeding, stunned, webbed, status, activeeffects, targets, players, performancestats
```

---

### `.rename`

Change the title of a window.

**Syntax:**
```
.rename <window_name> <new_title>
```

**Parameters:**
- `window_name` - Name of the window to rename
- `new_title` - New title (can contain spaces)

**Examples:**
```
.rename loot My Precious Loot
.rename main Game Output
```

---

### `.border`

Change the border style and color of a window.

**Syntax:**
```
.border <window_name> <style> [color] [sides...]
```

**Parameters:**
- `window_name` - Name of the window
- `style` - Border style: `single`, `double`, `rounded`, `thick`, `none`
- `color` - (Optional) Border color in hex format (e.g., `#00ff00`) or color name (e.g., `red`, `cyan`)
- `sides` - (Optional) Which sides to show: `top`, `bottom`, `left`, `right`, `all`, `none` (default: `all`)

**Examples:**
```
.border main rounded
.border health double #00ff00
.border health double green
.border loot single red top bottom
.border thoughts none
```

**Notes:**
- Color names are resolved from the color palette (see `.colors`)
- Use `.addcolor` to create custom color names

---

### `.contentalign` / `.align`

Set content alignment for widgets within their window area. Useful when borders are removed to position content where you want it.

**Syntax:**
```
.contentalign <window_name> <alignment>
.align <window_name> <alignment>
```

**Parameters:**
- `window_name` - Name of the window
- `alignment` - Alignment position (see below)

**Alignment Options:**
- Corner alignments: `top-left`, `top-right`, `bottom-left`, `bottom-right`
- Edge alignments: `top`, `bottom`, `left`, `right`
- Center: `center`

**Examples:**
```
.contentalign compass bottom-left
.contentalign injuries center
.contentalign health bottom
.align roundtime top
```

**Supported Widgets:**
- **Compass** (7x3 fixed size) - Align the compass grid within the window area
- **InjuryDoll** (5x6 fixed size) - Align the injury doll figure within the window area
- **ProgressBar** (1 row height) - Vertical alignment (top, center, bottom) within multi-row areas
- Other widgets fill their areas and don't need alignment

**Common Use Cases:**
```
# Remove border from compass, align to bottom-left with transparent space above
.border compass none
.contentalign compass bottom-left

# Progress bars with 3-row height, align bar to bottom
.contentalign health bottom
.contentalign mana bottom

# Center small widgets in larger areas
.contentalign injuries center
```

**Notes:**
- Changes take effect immediately without restart
- Content alignment only matters when the window area is larger than the widget's content
- With borders disabled, widgets use their full configured dimensions
- Alignment is saved when you save the layout

---

### `.background` / `.bgcolor`

Set background color for a window. Useful for making borderless windows more visible.

**Syntax:**
```
.background <window_name> <color>
.bgcolor <window_name> <color>
```

**Parameters:**
- `window_name` - Name of the window
- `color` - Hex color code (e.g., `#1a1a1a`), color name (e.g., `navy`, `darkgray`), or `none` to remove

**Examples:**
```
.background command #1a1a1a
.background command darkgray
.background compass navy
.bgcolor injuries darkred
.background main none
```

**Notes:**
- Background color fills the entire widget content area
- When not set, widgets have transparent backgrounds
- Particularly useful for borderless windows to show boundaries
- Works with all widget types
- Color names are resolved from the color palette (see `.colors`)
- Use `.addcolor` to create custom color names

---

### `.createtabbed` / `.tabbedwindow`

Create a new tabbed text window with multiple tabs for different streams.

**Syntax:**
```
.createtabbed <window_name> <tab1:stream1,tab2:stream2,...>
.tabbedwindow <window_name> <tab1:stream1,tab2:stream2,...>
```

**Parameters:**
- `window_name` - Name for the new tabbed window
- `tab1:stream1,...` - Comma-separated list of tab definitions in format `TabName:stream`

**Examples:**
```
.createtabbed chat Speech:speech,Thoughts:thoughts,Whisper:whisper
.createtabbed comms LNet:logons,Deaths:deaths,Arrivals:arrivals
.tabbedwindow msgs Speech:speech,Whisper:whisper
```

**Notes:**
- Creates a window with tabs at the top by default
- Each tab routes to its specified stream
- Click tabs to switch between them
- Inactive tabs show `* ` prefix when they receive new messages
- Window created at position (0,0) with default size 20x60
- Use mouse to move and resize

**See also:** `.addtab`, `.removetab`, `.switchtab`, `.movetab`, `.tabcolors`

---

### `.addtab`

Add a new tab to an existing tabbed window.

**Syntax:**
```
.addtab <window_name> <tab_name> <stream>
```

**Parameters:**
- `window_name` - Name of the tabbed window
- `tab_name` - Display name for the new tab
- `stream` - Game stream to route to this tab

**Examples:**
```
.addtab chat LNet logons
.addtab comms Announcements announcements
.addtab msgs Loot loot
```

**Notes:**
- The tab appears immediately in the window
- Stream routing is updated automatically
- Cannot add duplicate tab names to the same window

---

### `.removetab`

Remove a tab from a tabbed window.

**Syntax:**
```
.removetab <window_name> <tab_name>
```

**Parameters:**
- `window_name` - Name of the tabbed window
- `tab_name` - Name of the tab to remove

**Examples:**
```
.removetab chat LNet
.removetab comms Arrivals
```

**Notes:**
- Cannot remove the last tab from a window
- If viewing the removed tab, switches to first remaining tab
- Stream routing is updated automatically

---

### `.switchtab`

Switch to a specific tab in a tabbed window.

**Syntax:**
```
.switchtab <window_name> <tab_name|index>
```

**Parameters:**
- `window_name` - Name of the tabbed window
- `tab_name|index` - Either the tab name or 0-based index number

**Examples:**
```
.switchtab chat Speech
.switchtab chat 0
.switchtab comms 2
```

**Notes:**
- Tab indices are 0-based (first tab is 0)
- Can also click tabs with mouse to switch
- Clears unread indicator when switching to a tab

---

### `.movetab` / `.reordertab`

Reorder tabs within a tabbed window.

**Syntax:**
```
.movetab <window_name> <tab_name> <new_position>
.reordertab <window_name> <tab_name> <new_position>
```

**Parameters:**
- `window_name` - Name of the tabbed window
- `tab_name` - Name of the tab to move
- `new_position` - New position index (0-based)

**Examples:**
```
.movetab chat Speech 0
.movetab chat Whisper 2
.reordertab comms Deaths 1
```

**Notes:**
- Position indices are 0-based (0 = first position)
- Changes take effect immediately
- The tab order is persisted when you save the layout

---

### `.tabcolors` / `.settabcolors`

Configure colors for tabbed window tabs.

**Syntax:**
```
.tabcolors <window_name> <active_color> [unread_color] [inactive_color]
.settabcolors <window_name> <active_color> [unread_color] [inactive_color]
```

**Parameters:**
- `window_name` - Name of the tabbed window
- `active_color` - Hex color for the active/selected tab (e.g., `#ffff00`)
- `unread_color` - (Optional) Hex color for tabs with unread messages (default: `#ffffff`)
- `inactive_color` - (Optional) Hex color for inactive tabs (default: `#808080`)

**Examples:**
```
.tabcolors chat #ffff00
.tabcolors chat #ffff00 #ffffff
.tabcolors chat #ffff00 #ffffff #808080
.settabcolors comms #00ff00 #ffaa00 #555555
```

**Notes:**
- Colors must be in hex format with `#` prefix
- Active tab is the currently visible tab (bold, colored)
- Unread tabs show with prefix (default `* `) and color
- Inactive tabs are read but not currently visible
- Changes take effect immediately

---

## Layout Commands

### `.savelayout`

Save the current window layout.

**Syntax:**
```
.savelayout [layout_name]
```

**Parameters:**
- `layout_name` - (Optional) Name for the layout (default: "default")

**Examples:**
```
.savelayout
.savelayout hunting
.savelayout combat
```

**Notes:**
- Layouts are saved to `~/.vellum-fe/layouts/<name>.toml`
- The layout includes all window positions, sizes, and configurations
- An autosave layout is created when you exit the application

---

### `.loadlayout`

Load a previously saved layout.

**Syntax:**
```
.loadlayout [layout_name]
```

**Parameters:**
- `layout_name` - (Optional) Name of the layout to load (default: "default")

**Examples:**
```
.loadlayout
.loadlayout hunting
.loadlayout combat
```

**Notes:**
- Loading a layout will replace all current windows
- The autosave layout is loaded automatically on startup if it exists

---

### `.layouts`

List all saved layouts.

**Syntax:**
```
.layouts
```

**Example:**
```
.layouts
→ Saved layouts: default, hunting, combat, roleplay
```

---

## Progress Bar Commands

### `.setprogress`

Manually set the value of a progress bar widget.

**Syntax:**
```
.setprogress <window_name> <current> <max>
```

**Parameters:**
- `window_name` - Name of the progress bar window
- `current` - Current value
- `max` - Maximum value

**Examples:**
```
.setprogress health 150 200
.setprogress mana 50 100
.setprogress stamina 200 250
```

**Notes:**
- This is primarily for testing; progress bars are normally auto-updated from game data
- Progress bars: health, mana, stamina, spirit, mindstate, encumbrance, stance, bloodpoints

---

### `.setbarcolor`

Change the colors of a progress bar.

**Syntax:**
```
.setbarcolor <window_name> <bar_color> [background_color]
```

**Parameters:**
- `window_name` - Name of the progress bar window
- `bar_color` - Color for the filled portion (hex format or color name, e.g., `#00ff00` or `green`)
- `background_color` - (Optional) Color for the unfilled portion

**Examples:**
```
.setbarcolor health #00ff00
.setbarcolor health green
.setbarcolor mana blue darkgray
.setbarcolor stamina #ffff00 #222222
```

**Notes:**
- Color names are resolved from the color palette (see `.colors`)
- Use `.addcolor` to create custom color names

---

## Countdown Commands

### `.setcountdown`

Manually set a countdown timer.

**Syntax:**
```
.setcountdown <window_name> <seconds>
```

**Parameters:**
- `window_name` - Name of the countdown window
- `seconds` - Number of seconds for the countdown

**Examples:**
```
.setcountdown roundtime 5
.setcountdown casttime 3
.setcountdown stun 10
```

**Notes:**
- This is primarily for testing; countdown timers are normally auto-updated from game data
- Countdown widgets: roundtime, casttime, stun

---

## Indicator Commands

### `.indicatoron`

Force all status indicators ON (for testing).

**Syntax:**
```
.indicatoron
```

**Example:**
```
.indicatoron
→ Forced all status indicators ON
```

**Notes:**
- Affects indicators: poisoned, diseased, bleeding, stunned, webbed
- Also updates dashboard indicators if present

---

### `.indicatoroff`

Force all status indicators OFF (for testing).

**Syntax:**
```
.indicatoroff
```

**Example:**
```
.indicatoroff
→ Forced all status indicators OFF
```

---

## Active Effects Commands

### `.togglespellid` / `.toggleeffectid`

Toggle between displaying spell names and spell IDs in active effects windows.

**Syntax:**
```
.togglespellid <window_name>
.toggleeffectid <window_name>
```

**Parameters:**
- `window_name` - Name of the active effects window

**Examples:**
```
.togglespellid activeeffects
.toggleeffectid activeeffects
```

**Notes:**
- Only works on active effects widgets
- Useful for debugging or when you need to reference spell IDs

---

## Highlight Commands

### `.addhighlight` / `.addhl`

Open the interactive highlight management form to create a new highlight pattern.

**Syntax:**
```
.addhighlight
.addhl
```

**Example:**
```
.addhighlight
```

**Notes:**
- Opens a full-screen form for creating highlights
- Use Tab/Shift+Tab to navigate between fields
- Press Enter on Save button or Escape to exit
- See [Highlight Management Guide](https://github.com/Nisugi/VellumFE/wiki/Highlight-Management) for detailed form usage

---

### `.edithighlight` / `.edithl`

Edit an existing highlight pattern.

**Syntax:**
```
.edithighlight <highlight_name>
.edithl <highlight_name>
```

**Parameters:**
- `highlight_name` - Name of the highlight to edit

**Examples:**
```
.edithighlight combat_swing
.edithl death_message
```

**Notes:**
- Opens the highlight form pre-filled with existing values
- Save to update the highlight or Cancel to discard changes
- Delete button available to remove the highlight

---

### `.deletehighlight` / `.delhl`

Delete a highlight pattern.

**Syntax:**
```
.deletehighlight <highlight_name>
.delhl <highlight_name>
```

**Parameters:**
- `highlight_name` - Name of the highlight to delete

**Examples:**
```
.deletehighlight combat_swing
.delhl old_pattern
```

**Notes:**
- Deletes immediately without confirmation
- Saves config automatically after deletion
- Cannot be undone (unless you reload config from backup)

---

### `.listhighlights` / `.listhl` / `.highlights`

List all configured highlight patterns.

**Syntax:**
```
.listhighlights
.listhl
.highlights
```

**Example:**
```
.listhighlights
→ 12 highlights: combat_swing, death_message, loot_found, player_arrives, ...
```

---

### `.testhighlight` / `.testhl`

Test a highlight pattern against sample text to see if it matches.

**Syntax:**
```
.testhighlight <highlight_name> <text to test>
.testhl <highlight_name> <text to test>
```

**Parameters:**
- `highlight_name` - Name of the highlight to test
- `text to test` - Sample text to match against the pattern

**Examples:**
```
.testhighlight combat_swing You swing a sword at the kobold!
.testhl death_message The kobold falls to the ground and dies.
```

**Output:**
- Shows whether the pattern matched
- Displays the matched text
- Shows the position in the string
- Reports what styling would be applied (colors, bold, etc.)

**See also:** [Highlight Management Guide](https://github.com/Nisugi/VellumFE/wiki/Highlight-Management)

---

## Keybind Commands

### `.addkeybind` / `.addkey`

Open the interactive keybind management form to create a new keybind.

**Syntax:**
```
.addkeybind
.addkey
```

**Example:**
```
.addkeybind
```

**Notes:**
- Opens a full-screen form for creating keybinds
- Use Tab to navigate between fields
- Press Enter on Save button or Escape to exit
- See [Keybind Management Guide](https://github.com/Nisugi/VellumFE/wiki/Keybind-Management) for detailed form usage

---

### `.editkeybind` / `.editkey`

Edit an existing keybind.

**Syntax:**
```
.editkeybind <key_combo>
.editkey <key_combo>
```

**Parameters:**
- `key_combo` - Key combination to edit (e.g., `ctrl+e`, `f5`, `alt+shift+a`)

**Examples:**
```
.editkeybind ctrl+e
.editkey f5
.editkey alt+shift+a
```

**Notes:**
- Opens the keybind form pre-filled with existing values
- Save to update the keybind or Cancel to discard changes
- Delete button available to remove the keybind

---

### `.deletekeybind` / `.delkey`

Delete a keybind.

**Syntax:**
```
.deletekeybind <key_combo>
.delkey <key_combo>
```

**Parameters:**
- `key_combo` - Key combination to delete (e.g., `ctrl+e`, `f5`)

**Examples:**
```
.deletekeybind ctrl+e
.delkey f5
```

**Notes:**
- Deletes immediately without confirmation
- Saves config automatically after deletion
- Cannot be undone (unless you reload config from backup)

---

### `.listkeybinds` / `.listkeys` / `.keybinds`

List all configured keybinds.

**Syntax:**
```
.listkeybinds
.listkeys
.keybinds
```

**Example:**
```
.listkeybinds
→ 8 keybinds: alt+1, alt+2, ctrl+e, ctrl+f, f1, f5, shift+f1, shift+up
```

**See also:** [Keybind Management Guide](https://github.com/Nisugi/VellumFE/wiki/Keybind-Management)

---

## Color Palette Commands

### `.colors` / `.palette` / `.colorpalette`

Open the color palette browser to view, edit, and manage color definitions.

**Syntax:**
```
.colors
.palette
.colorpalette
```

**Example:**
```
.colors
```

**Features:**
- Browse 85+ preconfigured colors organized by category (red, blue, green, etc.)
- Navigate with Up/Down arrows, PgUp/PgDn
- Press Enter to edit selected color
- Press F to toggle favorite status
- Press Del to delete selected color
- Category headers automatically appear when scrolling
- Draggable popup window (click title bar to move)

**Notes:**
- Colors are saved to `~/.vellum-fe/configs/default.toml` (or character-specific config)
- All default colors are editable - customize as needed
- Use color names anywhere colors are supported (window editor, border colors, etc.)
- Color names automatically resolve to hex codes

**See also:** `.addcolor`, [Configuration Guide](https://github.com/Nisugi/VellumFE/wiki/Configuration-Guide)

---

### `.addcolor` / `.newcolor` / `.createcolor`

Open the color editor form to create a new color definition.

**Syntax:**
```
.addcolor
.newcolor
.createcolor
```

**Example:**
```
.addcolor
```

**Form Fields:**
- **Name** - Unique name for the color (e.g., `myblue`, `darkpurple`)
- **Color** - Hex color code in format `#RRGGBB` (e.g., `#0033FF`)
  - Only accepts valid hex characters (0-9, A-F)
  - Live color preview shown as `███`
- **Category** - Category for organization (e.g., `blue`, `custom`, `ui`)
- **Favorite** - Mark as favorite (checkbox, toggle with Space or X)

**Navigation:**
- Tab/Shift+Tab - Navigate between fields
- Enter on Save button - Validates and saves color
- Enter on Cancel or Esc - Cancel without saving
- Arrow keys, Home, End - Text navigation in fields

**Examples:**
```
# Create a custom blue
.addcolor
Name: myblue
Color: #0033FF
Category: blue
Favorite: [X]

# Create UI accent color
.addcolor
Name: accent
Color: #FF6B35
Category: ui
Favorite: [ ]
```

**Notes:**
- Color names must be unique (duplicates are rejected)
- Hex color validation ensures valid format (#RRGGBB)
- Category names are displayed in lowercase
- Colors are immediately available for use after saving
- Draggable popup window (click title bar to move)

**See also:** `.colors` (to browse and edit existing colors)

---

## Combat Tracking Commands

### Creating Targets Widget

Create a scrollable targets list widget for tracking combat targets.

**Syntax:**
```
.createwindow targets
```

**Features:**
- Displays all combat targets with count in title (e.g., "Targets [05]")
- Current target marked with "►" prefix
- Status indicators shown as suffix: `[stu]`, `[sit]`, `[kne]`, `[sle]`, `[fro]`, `[fly]`, `[dead]`
- Scrollable with mouse wheel or keyboard (Tab to focus, then arrow keys)
- Requires `targetlist.lic` to be running

**Example:**
```
.createwindow targets
.border targets rounded #ff0000
.rename targets "Combat Targets"
```

**See also:** [Targets and Players Widget Guide](https://github.com/Nisugi/VellumFE/wiki/Targets-and-Players)

---

### Creating Players Widget

Create a scrollable players list widget for tracking players in the room.

**Syntax:**
```
.createwindow players
```

**Features:**
- Displays all players in room with count in title (e.g., "Players [19]")
- Status indicators shown as suffix: `[sit]`, `[kne]`, `[sle]`, `[fly]`
- Scrollable with mouse wheel or keyboard (Tab to focus, then arrow keys)
- Requires `targetlist.lic` to be running

**Example:**
```
.createwindow players
.border players rounded #00ff00
.rename players "Room Players"
```

**Notes:**
- Both targets and players widgets use the ScrollableContainer pattern (same as Active Effects)
- Data updates continuously from `targetlist.lic`
- Scrolling works when list exceeds window height
- Stream routing: targets → `combat` stream, players → `playerlist` stream

**Required Lich Script:**
```ruby
;go2 targetlist.lic
# Or add to autostart:
;autostart add targetlist
```

**See also:** [Targets and Players Widget Guide](https://github.com/Nisugi/VellumFE/wiki/Targets-and-Players)

---

## Debug Commands

### `.randominjuries` / `.randinjuries`

Generate random injuries/scars on the injury doll (for testing).

**Syntax:**
```
.randominjuries
.randinjuries
```

**Example:**
```
.randominjuries
→ Randomized 5 injuries/scars
```

**Notes:**
- Generates 3-8 random injuries/scars on various body parts
- 30% chance each injury is a scar instead of a wound
- Wounds are levels 1-3, scars are levels 4-6

---

### `.randomcompass` / `.randcompass`

Generate random compass directions (for testing).

**Syntax:**
```
.randomcompass
.randcompass
```

**Example:**
```
.randomcompass
→ Randomized 4 compass exits
```

**Notes:**
- Generates 2-6 random exits
- Possible directions: n, ne, e, se, s, sw, w, nw, out

---

### `.randomprogress` / `.randprog`

Randomize all progress bars (for testing).

**Syntax:**
```
.randomprogress
.randprog
```

**Example:**
```
.randomprogress
→ Randomized all progress bars
```

**Notes:**
- Sets random values for: health, mana, stamina, spirit, blood points, mind state, encumbrance, stance
- Each bar uses realistic maximum values for the stat

---

### `.randomcountdowns` / `.randcountdowns`

Randomize all countdown timers (for testing).

**Syntax:**
```
.randomcountdowns
.randcountdowns
```

**Example:**
```
.randomcountdowns
→ Randomized countdowns: RT=18s, Cast=22s, Stun=15s
```

**Notes:**
- Sets each countdown to a random duration between 15-25 seconds

---

## Command Tips

1. **Case Sensitivity**: Command names are case-sensitive. Use lowercase.

2. **Aliases**: Many commands have shorter aliases (e.g., `.q` for `.quit`, `.createwin` for `.createwindow`)

3. **Tab Completion**: (Not yet implemented) Will autocomplete command names and window names

4. **Command History**: Use Up/Down arrow keys to navigate through previous commands

5. **Error Messages**: If a command fails, check the main window for error messages explaining why

6. **Testing**: Debug commands (`.random*`) are useful for testing layouts without needing to be in-game

---

[← Back to Wiki Home](https://github.com/Nisugi/VellumFE/wiki/Home) | [Next: Configuration Guide →](https://github.com/Nisugi/VellumFE/wiki/Configuration-Guide)
