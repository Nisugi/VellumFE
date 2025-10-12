# Window Editor

The Window Editor is a visual interface for creating and editing windows in VellumFE. It provides a user-friendly way to configure window properties without manually editing configuration files.

## Table of Contents

- [Opening the Window Editor](#opening-the-window-editor)
- [Editor Modes](#editor-modes)
- [Creating New Windows](#creating-new-windows)
- [Editing Existing Windows](#editing-existing-windows)
- [Editing Command Input Box](#editing-command-input-box)
- [Window Properties](#window-properties)
- [Navigation and Controls](#navigation-and-controls)
- [Widget-Specific Fields](#widget-specific-fields)
- [Tips and Tricks](#tips-and-tricks)

---

## Opening the Window Editor

### Edit Existing Windows

Use the `.editwindow` or `.editwin` command:

```
.editwindow main
.editwin thoughts
```

This opens the editor with a list of all windows. Use arrow keys to select a window and press Enter to edit it.

### Create New Windows

Use the `.addwindow` or `.addwin` command:

```
.addwindow
.addwin
```

This opens the editor in creation mode, where you'll first select a widget type, then optionally select a template.

---

## Editor Modes

The window editor has four distinct modes:

### 1. Window Selection Mode

**When:** First screen when editing existing windows

**Purpose:** Select which window to edit

**Controls:**
- `↑/↓` - Navigate through window list
- `Enter` - Select window to edit
- `Esc` - Cancel and close editor

**Features:**
- Shows all active windows
- Scrollable list with position indicator (e.g., "Windows (3/15) ↑↓")
- Smart scrolling keeps selected item centered

### 2. Widget Type Selection Mode

**When:** First screen when creating new windows

**Purpose:** Choose the type of widget to create

**Available Widget Types:**
- `text` - Text display window
- `tabbed` - Multi-tab text window
- `progress` - Progress bar (health, mana, etc.)
- `countdown` - Countdown timer (roundtime, casttime)
- `active_effects` - Active effects display
- `entity` - Entity list (targets, players)
- `dashboard` - Status dashboard
- `indicator` - Single status indicator
- `compass` - Compass widget
- `injury_doll` - Injury visualization
- `hands` - Hand/inventory display

**Controls:**
- `↑/↓` - Navigate through widget types
- `Enter` - Select widget type
- `Esc` - Cancel and close editor

### 3. Template Selection Mode

**When:** After selecting widget type (for types with templates)

**Purpose:** Choose a pre-configured template or start from scratch

**Templates by Widget Type:**

- **text:** thoughts, speech, familiar, room, logons, deaths, arrivals, ambients, announcements, loot, custom
- **tabbed:** custom
- **progress:** health, mana, stamina, spirit, bloodpoints, stance, encumbrance, mindstate, custom
- **countdown:** roundtime, casttime, stuntime, custom
- **active_effects:** active_spells, buffs, debuffs, cooldowns, all_effects, custom
- **entity:** targets, players, custom
- **dashboard:** status_dashboard, custom

**Controls:**
- `↑/↓` - Navigate through templates
- `Enter` - Load template and proceed to field editing
- `Esc` - Go back to widget type selection

**Note:** Selecting "custom" loads default settings for that widget type.

### 4. Field Editing Mode

**When:** After selecting a window/template

**Purpose:** Configure all window properties

**Controls:**
- `Tab` / `Shift+Tab` - Navigate between fields
- `↑/↓` - Navigate dropdowns / multi-checkboxes
- `←/→` - Navigate multi-checkboxes horizontally
- `Space` - Toggle checkboxes
- `Enter` - Activate text input / submit on Save button
- `Ctrl+S` - Save and apply changes
- `Esc` - Cancel and discard changes

---

## Creating New Windows

### Step-by-Step Process

1. **Type `.addwindow` or `.addwin`**
   - Opens the editor in Widget Type Selection mode

2. **Select Widget Type**
   - Use `↑/↓` to highlight desired type
   - Press `Enter` to select

3. **Select Template** (if applicable)
   - Use `↑/↓` to browse available templates
   - Press `Enter` to load template
   - Select "custom" to start with defaults

4. **Configure Fields**
   - Editor loads with template settings pre-filled
   - Use `Tab` to navigate through fields
   - Modify any settings as needed
   - See [Window Properties](#window-properties) for field details

5. **Save**
   - Press `Ctrl+S` or navigate to "Save" button and press `Enter`
   - Window is created and added to your layout

### Quick Creation Tips

- **Use templates:** Most templates have sensible defaults - just save immediately
- **Name convention:** Templates auto-generate names like `loot_new` - rename to something unique
- **Position:** New windows default to (0,0) - move them immediately with the mouse
- **Preview:** Save and see the window, then `.editwindow` again to adjust

---

## Editing Existing Windows

### Step-by-Step Process

1. **Type `.editwindow` or `.editwin`**
   - Opens Window Selection mode with list of all windows

2. **Select Window**
   - Use `↑/↓` to highlight window name
   - Press `Enter` to load for editing

3. **Modify Fields**
   - All current settings are pre-filled
   - Navigate with `Tab`
   - Change any properties
   - See [Window Properties](#window-properties) for field details

4. **Save or Cancel**
   - `Ctrl+S` or Save button - Apply changes
   - `Esc` or Cancel button - Discard changes

### Editing Tips

- **Live preview:** Save, check the window, edit again if needed
- **Undo:** If you save by mistake, `.editwindow` and restore old values
- **Bulk changes:** For multiple windows, edit each one, but only `.savelayout` at the end

---

## Editing Command Input Box

The command input box (where you type commands and game input) can also be configured through the window editor.

### Opening the Editor

Type `.editinput` or `.editcommandbox`:

```
.editinput
```

### What You Can Configure

The command input box has the following editable properties:

- **Title** - Optional title text shown in border
- **Position** - Row and column position
- **Size** - Height and width (width=0 means full width from column to edge)
- **Border Settings** - Show/hide border, border style, border color, border sides
- **Display Settings** - Transparent background, background color

### What's Different from Windows

- **No Name field** - The command input box always has the internal name "command_input"
- **No Streams** - Command input doesn't use stream routing
- **No Buffer Size** - Has its own dedicated buffer
- **Cannot be deleted** - The command input box is not in the window list and cannot be removed
- **No Lock option** - Command input box positioning is always via config/editor only

### Default Configuration

If not customized, the command input box defaults to:
- Position: Bottom of screen (row=0 means auto-bottom)
- Width: Full terminal width (width=0)
- Height: 3 rows
- Border: Single line border
- Title: None

### Common Customizations

#### Move to Top of Screen
```
Row: 0
Col: 0
Height: 3
Width: 0  (full width)
```
**Note:** Row=0 actually means bottom by default. To place at top, use Row: 1

#### Narrow Input Box
```
Row: 0  (bottom)
Col: 80
Height: 3
Width: 40
```

#### Side-Mounted Input
```
Row: 20
Col: 0
Height: 10
Width: 60
```

#### Custom Title and Colors
```
Title: "Command Input"
Border Color: #00ff00
BG Color: #1a1a1a
```

### Saving Changes

Like window edits, command input box changes are **not persisted** until you run:

```
.savelayout
```

The command input configuration is saved as part of your layout file in `~/.vellum-fe/layouts/`.

### Tips

- **Width=0 is special:** It means "use full width from starting column to edge"
- **Row=0 is special:** It means "auto-position at bottom of screen"
- **Test before saving:** After editing, try typing a command to see if the positioning works
- **Coordinate with windows:** Make sure your command input doesn't overlap important windows

---

## Window Properties

The editor organizes fields into logical sections:

### Window Identity

#### Name
- **Type:** Text field
- **Purpose:** Internal identifier for the window
- **Notes:**
  - Used in commands (`.editwindow <name>`, `.deletewindow <name>`)
  - Must be unique across all windows
  - Cannot be changed once created (delete and recreate instead)

#### Title
- **Type:** Text field
- **Purpose:** Display text shown in window border
- **Notes:**
  - Optional - controlled by Show Title checkbox
  - Can contain spaces and special characters
  - Shown in the top-left of the window border

#### Show Title
- **Type:** Checkbox
- **Purpose:** Controls title display behavior
- **Options:**
  - **Checked + Text in field** → Shows custom title
  - **Checked + Empty field** → Shows window name as title (default)
  - **Unchecked** → Hides title completely (no text in title bar)
- **Notes:**
  - Use unchecked to create borderless-looking windows
  - Useful for command input box to hide all title text
  - Cannot hide title bar itself, only the text within it

---

### Position & Size

#### Row
- **Type:** Number field
- **Purpose:** Vertical position (distance from top of terminal)
- **Range:** 0 to terminal height
- **Notes:** 0 = top edge

#### Col
- **Type:** Number field
- **Purpose:** Horizontal position (distance from left of terminal)
- **Range:** 0 to terminal width
- **Notes:** 0 = left edge

#### Height (Rows)
- **Type:** Number field
- **Purpose:** Height of window in terminal rows
- **Minimum:** 3 rows (varies by widget type)
- **Notes:** Includes border if shown

#### Width (Cols)
- **Type:** Number field
- **Purpose:** Width of window in terminal columns
- **Minimum:** 10 cols (varies by widget type)
- **Notes:** Includes border if shown

---

### Border Settings

#### Show Border
- **Type:** Checkbox
- **Purpose:** Enable/disable window border
- **Default:** Checked
- **Notes:** Disabling saves 2 rows and 2 cols of space

#### Border Style
- **Type:** Dropdown
- **Purpose:** Choose border appearance
- **Options:**
  - `none` - No border
  - `single` - Single line (─│┌┐└┘)
  - `double` - Double line (═║╔╗╚╝)
  - `rounded` - Rounded corners (─│╭╮╰╯)
  - `thick` - Thick line (━┃┏┓┗┛)
- **Default:** single
- **Controls:** `↑/↓` to change selection

#### Border Sides
- **Type:** Multi-checkbox
- **Purpose:** Select which sides show border
- **Options:** top, bottom, left, right
- **Controls:**
  - `←/→` or `↑/↓` to navigate options
  - `Space` to toggle selection
- **Default:** All sides selected
- **Notes:** Useful for creating borderless edges between windows

#### Border Color
- **Type:** Text field (hex color)
- **Purpose:** Color for border
- **Format:** `#RRGGBB` (e.g., `#ff0000` for red)
- **Default:** Empty (uses terminal default)
- **Examples:**
  - `#ffffff` - White
  - `#00ff00` - Green
  - `#ff00ff` - Magenta

---

### Display Settings

#### Transparent BG
- **Type:** Checkbox
- **Purpose:** Use transparent background instead of solid color
- **Default:** Unchecked
- **Notes:** Shows terminal default background

#### Lock
- **Type:** Checkbox
- **Purpose:** Prevent window from being moved or resized with mouse
- **Default:** Unchecked
- **Notes:** Useful for fixed layouts

#### BG Color
- **Type:** Text field (hex color)
- **Purpose:** Background color for window content area
- **Format:** `#RRGGBB`
- **Default:** Empty (uses terminal default)
- **Notes:** Only used if Transparent BG is unchecked

#### Content Align
- **Type:** Dropdown
- **Purpose:** Alignment of content within window
- **Options:**
  - `top-left`, `top-center`, `top-right`
  - `center-left`, `center`, `center-right`
  - `bottom-left`, `bottom-center`, `bottom-right`
- **Default:** `top-left`
- **Controls:** `↑/↓` to change selection
- **Notes:** Primarily affects text windows with smaller content than window size

---

### Widget-Specific Settings

These fields appear conditionally based on widget type:

#### Streams
- **Type:** Text field (comma-separated)
- **Appears for:** text, entity widgets
- **Purpose:** Game streams to route to this window
- **Format:** `stream1, stream2, stream3`
- **Examples:**
  - `main` - Main game output
  - `speech, whisper` - All speech and whisper messages
  - `combat, death` - Combat messages and death notifications
- **Available Streams:** main, thoughts, speech, whisper, familiar, room, logons, deaths, arrivals, ambients, announcements, loot
- **Notes:** Leave empty to create a display-only window

#### Effect Category
- **Type:** Dropdown
- **Appears for:** active_effects widgets
- **Purpose:** Which category of effects to display
- **Options:**
  - `ActiveSpells` - Active spell effects
  - `Buffs` - Beneficial effects
  - `Debuffs` - Negative effects
  - `Cooldowns` - Ability cooldowns
  - `All` - All effect types
- **Default:** All
- **Controls:** `↑/↓` to change selection

#### Hand Icon
- **Type:** Text field
- **Appears for:** lefthand, righthand, spellhand widgets
- **Purpose:** Icon/symbol to display for empty hand
- **Format:** Single character or UTF-8 symbol
- **Examples:**
  - `✋` - Hand emoji
  - `□` - Empty box
  - `-` - Dash
- **Default:** Empty (no icon)

#### Buffer Size
- **Type:** Number field
- **Appears for:** text widgets (not tabbed)
- **Purpose:** Maximum lines to keep in scrollback buffer
- **Range:** 100 to 1,000,000
- **Default:** 1,000
- **Notes:**
  - Larger buffer uses more memory
  - Tabbed windows have per-tab buffers set on the tab configuration

---

## Navigation and Controls

### Field Navigation

The editor uses a **custom tab order** that follows the visual layout:

**Tab Order:**
1. Name
2. Title
3. Row
4. Col
5. Height
6. Width
7. Show Border
8. Border Style
9. Border Sides
10. Border Color
11. Transparent BG
12. Lock
13. BG Color
14. Content Align
15. Streams (if applicable)
16. Effect Category (if applicable)
17. Hand Icon (if applicable)
18. Buffer Size (if applicable)
19. Save button
20. Cancel button

**Navigation Keys:**
- `Tab` - Move to next field
- `Shift+Tab` - Move to previous field
- `Enter` - On text fields: activate for editing; On buttons: execute action

### Field Types and Controls

#### Text Input Fields
- **Activate:** Press `Enter` or start typing
- **Edit:** Type normally, use backspace/delete
- **Complete:** Press `Enter` or `Tab` to move to next field
- **Examples:** Name, Title, Row, Col, Border Color

#### Dropdowns
- **Navigate:** `↑/↓` arrow keys cycle through options
- **Current value:** Highlighted in selection
- **Examples:** Border Style, Content Align, Effect Category

#### Checkboxes
- **Toggle:** Press `Space` to check/uncheck
- **Visual:** `[✓]` checked, `[ ]` unchecked
- **Examples:** Show Border, Transparent BG, Lock

#### Multi-Checkboxes (Border Sides)
- **Navigate options:** `←/→` or `↑/↓` arrow keys
- **Toggle current:** Press `Space`
- **Currently selected:** Highlighted option
- **Status:** Each option shows `[✓]` or `[ ]`

### Saving and Canceling

#### Save Changes
- **Method 1:** Press `Ctrl+S` from any field
- **Method 2:** Tab to "Save" button and press `Enter`
- **Result:** Window is updated/created and editor closes
- **Note:** Changes are applied immediately to the layout

#### Cancel Changes
- **Method 1:** Press `Esc` from any field
- **Method 2:** Tab to "Cancel" button and press `Enter`
- **Result:** All changes discarded, editor closes
- **Note:** Original window remains unchanged

---

## Widget-Specific Fields

### Text Windows

**Required Fields:**
- Name, Position, Size

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- **Streams:** Comma-separated list of game streams
- **Buffer Size:** Maximum scrollback lines (default: 1,000)

**Example Configuration:**
```
Name: combat
Title: Combat Log
Streams: combat, death
Buffer Size: 5000
```

### Tabbed Windows

**Required Fields:**
- Name, Position, Size

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- None (tabs configured separately via `.addtab` command)

**Notes:**
- Tabs themselves have streams, not the parent window
- Each tab has its own buffer
- Use `.addtab` to add tabs after creation

**Example Configuration:**
```
Name: chat
Title: Chat
Tab Bar Position: top
```

### Progress Bars

**Required Fields:**
- Name, Position, Size

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- None (updated automatically from game `<progressBar>` tags)

**Notes:**
- Do not need streams - updated via XML tags
- Use templates (health, mana, etc.) for automatic routing
- Custom progress bars require scripts to send proper XML format

**Example Configuration:**
```
Name: health
Title: HP
Size: 3 rows × 30 cols
```

### Countdown Timers

**Required Fields:**
- Name, Position, Size

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- None (updated automatically from game `<roundTime>` / `<castTime>` tags)

**Notes:**
- Do not need streams - updated via XML tags
- Use templates (roundtime, casttime, stuntime) for automatic routing
- Shows character-based fill animation

**Example Configuration:**
```
Name: roundtime
Title: RT
Size: 3 rows × 15 cols
```

### Active Effects

**Required Fields:**
- Name, Position, Size

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- **Effect Category:** Dropdown to filter effect types
  - ActiveSpells, Buffs, Debuffs, Cooldowns, All

**Notes:**
- Updates automatically from game effect data
- Category filters which effects are displayed
- Each effect shows with icon and duration

**Example Configuration:**
```
Name: buffs
Title: Buffs
Effect Category: Buffs
Size: 15 rows × 35 cols
```

### Entity Widgets (Targets/Players)

**Required Fields:**
- Name, Position, Size

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- **Streams:** Determines if tracking targets or players
  - Use template "targets" for target tracking
  - Use template "players" for player tracking

**Notes:**
- Shows list of entities with health bars
- Auto-updates as entities enter/leave
- Click to select/target

**Example Configuration:**
```
Name: targets
Title: Targets
Streams: targets
Size: 10 rows × 30 cols
```

### Dashboard

**Required Fields:**
- Name, Position, Size

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- None (configured via dashboard indicators)

**Notes:**
- Displays multiple status indicators in a grid
- Configure indicators separately in config file
- Shows icons and values for various game states

**Example Configuration:**
```
Name: status
Title: Status
Size: 5 rows × 40 cols
```

### Compass

**Required Fields:**
- Name, Position, Size (minimum 5×10)

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- None

**Notes:**
- Displays directional exits from current room
- Updates automatically from game room data
- Shows obvious/hidden exits

### Injury Doll

**Required Fields:**
- Name, Position, Size (minimum 12×20)

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- None

**Notes:**
- Visual representation of character injuries
- Color-coded by severity
- Updates automatically from game injury data

### Hands Widgets

**Required Fields:**
- Name, Position, Size

**Optional Fields:**
- Title, Border settings, Display settings

**Specific Fields:**
- **Hand Icon:** Symbol to display when hand is empty

**Notes:**
- Shows what's held in hands
- Auto-updates from game inventory data
- Separate widgets for left/right/spell hand

---

## Tips and Tricks

### Efficient Workflow

1. **Use templates** - They have pre-configured settings that work well
2. **Tab through quickly** - Most fields can stay at defaults
3. **Save early, adjust later** - Create the window, see it, then edit
4. **Mouse for positioning** - Easier to drag than typing coordinates

### Common Patterns

#### Creating a Text Window
1. `.addwindow`
2. Select `text`
3. Select `custom`
4. Set Name: `mywindow`
5. Set Streams: `main, thoughts`
6. `Ctrl+S`
7. Drag to position with mouse

#### Creating Progress Bar Set
1. `.createwindow health` - Save, drag to position
2. `.createwindow mana` - Save, drag below health
3. `.createwindow stamina` - Save, drag below mana
4. `.savelayout` - Save the arrangement

#### Creating Tabbed Chat Window
1. `.addwindow`
2. Select `tabbed`
3. Set Name: `chat`
4. Set Size: 20 rows × 60 cols
5. `Ctrl+S`
6. `.addtab chat Speech speech`
7. `.addtab chat Thoughts thoughts`
8. `.addtab chat Whisper whisper`

### Border Tips

#### Borderless Windows
- Set Border Style to `none`, OR
- Uncheck Show Border

#### Partial Borders
- Keep Show Border checked
- Use Border Sides to select which edges
- Useful for tiled layouts

#### Border Color Coordination
- Use same color for related windows
- Example: All combat windows with red borders (`#ff0000`)
- Example: All chat windows with green borders (`#00ff00`)

### Layout Organization

#### Layered Windows
- Create main window at (0,0)
- Create popup-style windows on top
- Use `.savelayout` to preserve arrangement

#### Side-by-Side Windows
- Calculate positions carefully
  - Window 1: col=0, cols=60
  - Window 2: col=60, cols=60
- No gaps or overlaps

#### Grid Layouts
- Use consistent sizing
  - All windows 20 rows × 40 cols
- Align row/col to grid
  - Row: 0, 20, 40...
  - Col: 0, 40, 80...

### Troubleshooting

#### Window Not Visible
- Check if position is off-screen
- Resize terminal larger
- Edit in window editor to fix position

#### Text Not Appearing
- Check Streams field - must match game output streams
- Verify window has non-zero size
- Check if border is hiding content (reduce size or remove border)

#### Can't Click Window
- Window might be behind another window (z-order)
- Check if Lock is enabled (prevents mouse interaction)
- Verify window is within terminal bounds

#### Lost Changes
- Editor discards on Esc or Cancel
- Always use Ctrl+S or Save button
- Use `.savelayout` after editing to persist to disk

---

## Next Steps

- **[Window Management](Window-Management.md)** - Learn about moving, resizing, and managing windows
- **[Widget Reference](Widget-Reference.md)** - Detailed information on all widget types
- **[Layout Management](Layout-Management.md)** - Save and load window arrangements
- **[Commands Reference](Commands-Reference.md)** - Complete list of all commands

---

← [Window Management](Window-Management.md) | [Widget Reference](Widget-Reference.md) →
