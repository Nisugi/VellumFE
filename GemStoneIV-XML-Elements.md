# GemStone IV XML Log Analysis - Complete Element Documentation

## Overview
Analysis of 2,898 XML log files from the Lich5 client logs directory across 10 characters. These are game session logs from GemStone IV containing mixed XML and plain text output.

---

## 1. XML Elements (Tags)

### Core System Elements
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `mode` | Game mode indicator | `id` (GAME, LOGIN) |
| `playerID` | Player identifier | `id` (numeric) |
| `settingsInfo` | Client/game settings | `client`, `major`, `crc`, `instance` |
| `app` | Application metadata | `char`, `game`, `title` |
| `endSetup` | Marks end of initial setup | (none) |

### Stream Management
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `streamWindow` | UI window definition | `id`, `title`, `subtitle`, `location`, `target`, `ifClosed`, `resident`, `save`, `scroll`, `timestamp`, `styleIfClosed`, `appearance`, `nameFilterOption` |
| `clearStream` | Clear stream contents | `id`, `ifClosed` |
| `pushStream` | Push content to stream | `id` |
| `popStream` | Pop stream context | `id` |
| `stream` | Individual stream content line | `id` |
| `exposeStream` | Expose/show a stream | `id` |

### Component System
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `compDef` | Component definition | `id` |
| `component` | Component instance | `id` |

**Component IDs Found:**
- `room desc` - Room description
- `room objs` - Room objects
- `room players` - Players in room
- `room exits` - Available exits
- `sprite` - Player's familiar/sprite

### Container/Inventory System
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `container` | Container window | `id`, `title`, `location`, `target`, `resident`, `save` |
| `exposeContainer` | Expose container (special for stow) | `id` |
| `clearContainer` | Clear container contents | `id` |
| `inv` | Inventory item line | `id` |

**Container Stream Patterns:**

1. **Looking in a container** (e.g., `look in my bandolier`):
```xml
<container id='225766824' title='Bandolier' target='#225766824' location='right'/>
<clearContainer id="225766824"/>
<inv id='225766824'>In the <a exist="225766824" noun="bandolier">bandolier</a>:</inv>
<inv id='225766824'> a <a exist="225766858" noun="sword">slim short sword</a></inv>
<!-- ...more inv items... -->
```
Then followed by categorized display text (Weapons, Armor, Containers, etc.)

2. **Stow container** (default storage, special format):
```xml
<exposeContainer id='stow'/>
<container id='stow' title="My Shroud" target='#225766691' location='right' save='' resident='true'/>
<clearContainer id="stow"/>
<inv id='stow'>In the <a exist="225766691" noun="shroud">shroud</a>:</inv>
<inv id='stow'> a <a exist="225766734" noun="feather">nacreous disir feather</a></inv>
<!-- ...more inv items... -->
```

### Dialog System
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `openDialog` | Create dialog window | `id`, `type`, `title`, `location`, `target`, `height`, `width`, `resident` |
| `dialogData` | Dialog content | `id`, `clear` |

**Dialog IDs Found:**
- `combat` - Combat controls
- `injuries` - Injury display
- `minivitals` - Health/mana/stamina bars
- `stance` - Stance indicator
- `expr` - Experience/level info
- `encum` - Encumbrance
- `Active Spells` - Active spell list
- `Buffs` - Active buffs
- `Debuffs` - Active debuffs
- `Cooldowns` - Ability cooldowns
- `mapMaster` - Map controls
- `mapViewMain` - Map view
- `espMasterDialog` / `espMasterData` - ESP/telepathy controls
- `quick` - Quick action bar (main)
- `quick-combat` - Combat quick bar
- `quick-simu` - Information quick bar

### UI Control Elements
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `progressBar` | Progress indicator | `id`, `value`, `text`, `customText`, `top`, `left`, `height`, `width`, `align`, `tooltip` |
| `cmdButton` | Command button | `id`, `value`, `cmd`, `echo`, `tooltip`, `top`, `left`, `height`, `width`, `align`, `anchor_left`, `anchor_top` |
| `dropDownBox` | Dropdown selector | `id`, `value`, `cmd`, `content_text`, `content_value`, `tooltip`, `top`, `left`, `height`, `width`, `align`, `anchor_left`, `anchor_right` |
| `upDownEditBox` | Numeric input | `id`, `value`, `min`, `max`, `top`, `left`, `height`, `width`, `align`, `name`, `controls` |
| `label` | Text label | `id`, `value`, `justify`, `top`, `left`, `height`, `width`, `align`, `anchor_top`, `anchor_left`, `anchor_right`, `tooltip` |
| `link` | Clickable link | `id`, `value`, `cmd`, `echo`, `top`, `left`, `align`, `justify`, `width`, `URL`, `anchor_top`, `anchor_left` |
| `menuLink` | Menu link | `id`, `value`, `exist`, `noun`, `width`, `left` |
| `image` | UI image/button | `id`, `name`, `cmd`, `echo`, `tooltip`, `top`, `left`, `height`, `width`, `align`, `anchor_top`, `anchor_left` |
| `radio` | Radio button | `id`, `value`, `text`, `cmd`, `group`, `autosend`, `align`, `top`, `left`, `width` |
| `skin` | UI skin/theme | `id`, `name`, `controls`, `top`, `left`, `height`, `width`, `align` |
| `sep` | Separator | (none) |

### Navigation Elements
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `nav` | Navigation/room info | `rm` (room ID) |
| `compass` | Compass container | (contains `dir` elements) |
| `dir` | Direction indicator | `value` (n, s, e, w, ne, nw, se, sw, up, down, out) |

### Interactive Elements
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `a` | Hyperlink/object reference | `exist`, `noun`, `coord`, `char`, `game`, `title`, `href` |
| `d` | Direction link in text | (contains direction text) |

### Character State Elements
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `indicator` | Status indicator | `id`, `visible` (y/n) |
| `spell` | Current prepared spell | (text content) |
| `left` | Left hand item | `exist`, `noun` |
| `right` | Right hand item | `exist`, `noun` |
| `prompt` | Command prompt | `time` (unix timestamp) |
| `roundTime` | Action round time | `value` |
| `castTime` | Spell cast time | `value` |

**Indicator IDs Found:**
- `IconKNEELING` - Kneeling status
- `IconPRONE` - Prone status
- `IconSITTING` - Sitting status
- `IconSTANDING` - Standing status
- `IconSTUNNED` - Stunned status
- `IconHIDDEN` - Hidden status
- `IconINVISIBLE` - Invisible status
- `IconDEAD` - Dead status
- `IconWEBBED` - Webbed status
- `IconJOINED` - Joined group status

### Text Formatting
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `pushBold` | Start bold text | (none) |
| `popBold` | End bold text | (none) |
| `b` | Bold wrapper | (none) |
| `output` | Output formatting/font switch | `class` |
| `style` | Text style | `id` (roomName, roomDesc, etc.) |
| `resource` | Resource reference | `picture` |

**Output Class Values (Font Switching for GUI):**
- `<output class="mono"/>` - Switch to monospace font (font2)
- `<output class=""/>` - Switch back to normal font (font1)

Note: In TUI mode, always mono so this doesn't matter.

### Dialog Control
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `closeDialog` | Close a dialog window | `id` |

### Settings/Flags System
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `flag` | Player setting flag | `id`, `status` (on/off), `desc` |

**Example flags:**
- `Player Log On`, `Player Log Off`, `Player Disconnect`
- `Room Names`, `Room Descriptions`, `Brief Room Description`
- `Monster Bold`, `Default Group Open`
- `Automatically Activate ESP Amulets`, `Automatically Gather Coins`

### Client Command Markers (Lich5 Scripting)
| Pattern | Description |
|---------|-------------|
| `<!-- CLIENT --><c>command</c><!-- ENDCLIENT -->` | Client-side command marker |

Commands prefixed with `;` are Lich5 script commands (e.g., `;list all`, `;autostart add script`)

### Miscellaneous
| Element | Description | Key Attributes |
|---------|-------------|----------------|
| `switchQuickBar` | Switch active quick bar | `id` |
| `updateverbs` | Update available verbs | `default` |
| `cmdlist` | Command list | (none) |
| `cmdtimestamp` | Command timestamp | `data` |
| `pushInputState` | Input state control | `state` |
| `popInputState` | Pop input state | (none) |

---

## 2. Stream Window IDs

| Stream ID | Title | Purpose |
|-----------|-------|---------|
| `main` | Story | Main game output |
| `room` | Room | Room description display |
| `inv` | My Inventory | Worn items |
| `Spells` | Spells | Available spells list |
| `familiar` | Familiar | Familiar messages |
| `thoughts` | Thoughts | ESP/telepathy |
| `logons` | Arrivals | Player arrivals |
| `death` | Deaths | Death notices |
| `speech` | Speech | Speech window |
| `ambients` | Ambients | Ambient messages |
| `announcements` | Announcements | Game announcements |
| `bounty` | Bounties | Bounty task info |
| `society` | Society Tasks | Society task info |
| `loot` | Loot | Loot window |
| `charprofile` | [Character]'s Profile | Character profile |
| `charsheet` | Character Sheet | Character sheet |

---

## 3. Progress Bar IDs

| ProgressBar ID | Dialog | Purpose |
|----------------|--------|---------|
| `health` | minivitals | Health bar (main) |
| `health2` | injuries | Health bar (injuries) |
| `mana` | minivitals | Mana bar |
| `spirit` | minivitals | Spirit bar |
| `stamina` | minivitals | Stamina bar |
| `pbarStance` | combat/stance | Stance indicator |
| `encumlevel` | encum | Encumbrance level |
| `mindState` | expr | Mind state (experience absorption) |
| `nextLvlPB` | expr | Progress to next level |

---

## 4. Common Attributes Reference

### Layout/Positioning
- `top`, `left` - Position coordinates
- `height`, `width` - Size
- `align` - Alignment (n, s, e, w, nw, ne, sw, se, center)
- `anchor_top`, `anchor_left`, `anchor_right` - Relative anchoring
- `justify` - Text justification
- `location` - Window location (right, left, center, statBar, quickBar, force-center)

### Identity
- `id` - Element identifier
- `exist` - Game object ID (numeric, can be negative)
- `noun` - Object noun/type

### Behavioral
- `cmd` - Command to execute
- `echo` - Text to echo to client
- `value` - Current value
- `visible` - Visibility (y/n)
- `resident` - Persistent element
- `ifClosed` - Condition for display

### Content
- `title`, `subtitle` - Display titles
- `text` - Display text
- `tooltip` - Hover text
- `content_text`, `content_value` - Dropdown options

---

## 5. File Structure

```
logs/
  GSIV-[CharacterName]/
    2025/
      09/  (September)
      10/  (October)
      11/  (November)
      12/  (December)
        2025-12-21_19-26-11.xml  (timestamp format)
  GST-[CharacterName]/  (Test server)
  debug/
```

**Characters Found:** Armler, Bodegap, Brodega, Dicate, Getho, Hypate, Monstr, Nisugi, Sugiin, Zoleta

---

## Implementation Notes for VellumFE

### Critical Elements to Handle:
1. **Mixed Content** - Files contain both XML tags and plain text on same lines
2. **Self-closing vs Content Tags** - Some elements self-close, others have content
3. **Nested DialogData** - Multiple `dialogData` elements can update same dialog
4. **Dynamic Updates** - Elements like `progressBar` update with new `value` attributes
5. **Clear Operations** - `clear='t'` attribute resets dialog contents
6. **Negative exist IDs** - Room/NPC objects often have negative exist values
7. **Coordinate System** - `coord` attribute uses "x,y" format for click handling

### Stream Priority:
1. `main` - Primary game output
2. `room` - Room state (compDef updates)
3. `inv` - Inventory changes
4. Dialog updates (minivitals, combat, etc.)

### Real-time Updates:
- `prompt` with `time` - Server tick
- `indicator` visibility changes
- `progressBar` value updates
- `left`/`right` hand changes
- `spell` prepared spell changes

---

## 6. Additional/Rare Elements Found

### Rare UI Elements
| Element | Description | Context |
|---------|-------------|---------|
| `closeDialog` | Closes a dialog | Used when leaving tables, etc. |
| `deleteContainer` | Delete container from UI | Rare cleanup operation |
| `menuImage` | Image in menu | Quick bar menus |
| `annotate` | Annotation marker | Special text markup |
| `columnFont` | Column font definition | Table formatting |

### Formatting Elements (Additional)
| Element | Description |
|---------|-------------|
| `c` | Client command wrapper (inside `<!-- CLIENT -->`) |
| `m` | Styled text/metadata |
| `o` | Object styling |
| `r` | Red/styled text |
| `s` | Span/styled text |
| `w` | Warning/styled text |

### Combat/Action Timing
| Element | Description | Attributes |
|---------|-------------|------------|
| `roundTime` | Combat round timer | `value` (seconds) |
| `castTime` | Spell casting timer | `value` (seconds) |

### Direction Element (`d`)
The `d` element is used for clickable directions in text:
```xml
<a exist="-11225598" noun="Ludge">Ludge</a> just went <d cmd='go north'>north</d>.
```
Attributes: `cmd` (optional, command to execute)

---

## 7. Complete Element Summary

**Total Unique Elements: 60+**

### By Category:

**Core (5):** `mode`, `playerID`, `settingsInfo`, `app`, `endSetup`

**Streams (6):** `streamWindow`, `clearStream`, `pushStream`, `popStream`, `stream`, `exposeStream`

**Components (2):** `compDef`, `component`

**Containers (4):** `container`, `exposeContainer`, `clearContainer`, `inv`

**Dialogs (3):** `openDialog`, `dialogData`, `closeDialog`

**Controls (10):** `progressBar`, `cmdButton`, `dropDownBox`, `upDownEditBox`, `label`, `link`, `menuLink`, `image`, `radio`, `skin`, `sep`

**Navigation (3):** `nav`, `compass`, `dir`

**Interactive (2):** `a`, `d`

**Character State (7):** `indicator`, `spell`, `left`, `right`, `prompt`, `roundTime`, `castTime`

**Formatting (6):** `pushBold`, `popBold`, `b`, `output`, `style`, `resource`

**Settings (1):** `flag`

**Miscellaneous (6):** `switchQuickBar`, `updateverbs`, `cmdlist`, `cmdtimestamp`, `pushInputState`, `popInputState`

**Rare/Styling (6+):** `c`, `m`, `o`, `r`, `s`, `w`, `annotate`, `menuImage`, `columnFont`, `deleteContainer`

---

## 8. Additional Dialogs Discovered

### Special Dialog Types
| Dialog ID | Purpose | Key Elements |
|-----------|---------|--------------|
| `BetrayerPanel` | Blood Points tracking (Betrayer items) | `lblBPs`, `lblitem1` |
| `befriend` | Friends & Enemies list | (dynamic content) |
| `bank` | Banking interface | `depositallLnk`, `wealthnotesLnk`, `closeMe` |

**BetrayerPanel Example:**
```xml
<dialogData id='BetrayerPanel' clear='t'></dialogData>
<dialogData id='BetrayerPanel'>
  <label id='lblBPs' value='Blood Points: 100' justify='4' width='189'/>
  <label id='lblitem1' value='!a patchwork dwarf skin backpack' justify='4' width='189'/>
</dialogData>
```

### Speech/Action Presets
| Element | Description | Example |
|---------|-------------|---------|
| `preset` | Styled action text | `<preset id="speech">You recite to a table:</preset>` |

Preset IDs: `speech`, `whisper`, `thought`

---

## 9. Client Settings Update (`stgupd`)

The `stgupd` element is used within `<!-- CLIENT -->` markers for client-side settings:

```xml
<!-- CLIENT --><stgupd><panels>...</panels><!-- ENDCLIENT -->
<!-- CLIENT --><stgupd><stream>...</stream><!-- ENDCLIENT -->
<!-- CLIENT --><stgupd><misc>...</misc><toggles>...</toggles><!-- ENDCLIENT -->
```

### Nested Elements in `stgupd`:
| Element | Description |
|---------|-------------|
| `panels` | Panel layout configuration |
| `stream` | Stream window settings |
| `misc` | Miscellaneous settings |
| `toggles` | Toggle settings |
| `group` | Group container (id: Left, Right) |
| `dialog` | Dialog reference |
| `builtin` | Built-in element reference |
| `w` | Window configuration |
| `m` | Setting value |
| `s` | Toggle state |
| `detach` | Detached window settings |

**Window (`w`) Attributes:**
- `id` - Window ID (smain, sroom, cstow, etc.)
- `frame` - Frame type (float, panel)
- `vis` - Visibility (t/f)
- `width`, `height`, `x`, `y` - Position/size
- `location` - Location (center, detach)
- `ts` - Timestamp
- `maximized` - Maximized state

---

## 10. UI State Control

### Monopolize Element
| Element | Description |
|---------|-------------|
| `monopolize` | Game takes exclusive control of a stream |

```xml
<monopolize id="main"/>You get a room key from the innkeeper and wander off to your room...
<!-- All other streams blocked until released -->
<!-- later, after character manager/rest is done -->
<monopolize id=""/>  <!-- releases monopolize, other streams resume -->
```

**Behavior:** When `monopolize id="main"` is set, the game blocks all other text/streams and only accepts output from that system (inn rest, character manager, etc.). This is a game-side control mechanism - not particularly useful for VellumFE to handle specially beyond passing through the text.

### Expose/Close Elements
| Element | Description | Attributes |
|---------|-------------|------------|
| `exposeDialog` | Show a hidden dialog | `id` |
| `closeButton` | Close button in dialog | `id`, `value`, `cmd`, `align`, `top`, `left`, `width` |

**Bank Dialog Example:**
```xml
<dialogData id='bank'>
  <link id='depositallLnk' value='Deposit All' cmd='deposit all'.../>
  <link id='wealthnotesLnk' value='Check Notes' cmd='wealth notes'.../>
  <closeButton id='closeMe' value='Close' cmd='' align='s'.../>
</dialogData>
<exposeDialog id='bank'/>
```
