# Widget Reference

This page documents all 40+ widgets available in profanity-rs. Widgets are the building blocks of your interface - they can be text windows, progress bars, countdown timers, status indicators, and more.

All widgets can be created using the `.createwindow <template>` command (or its alias `.createwin`). Once created, widgets can be moved, resized, and styled to your liking.

## Table of Contents

- [Text Windows](#text-windows)
- [Tabbed Text Windows](#tabbed-text-windows)
- [Progress Bars](#progress-bars)
- [Countdown Timers](#countdown-timers)
- [Compass](#compass)
- [Injury Doll](#injury-doll)
- [Hands Widgets](#hands-widgets)
- [Status Indicators](#status-indicators)
- [Dashboard](#dashboard)
- [Active Effects](#active-effects)

---

## Text Windows

Text windows display scrolling game text from specific streams. They support scrollback, text wrapping, highlighting, and text selection.

### main

**Description:** Primary game output window. Displays all general gameplay text, combat messages, and any output not routed to a specific stream.

**Auto-update:** Yes (receives `main` stream from game)

**Default Size:** 30 rows x 120 cols

**Creation:**
```
.createwindow main
```

**Stream:** `main`

**Buffer Size:** 10000 lines

**Notes:** This is typically the largest window in your layout. Most game output appears here unless redirected to specialized windows.

---

### thoughts

**Aliases:** `thought`

**Description:** Displays character thoughts - both your own and those of other characters you can hear via psinet or other telepathic means.

**Auto-update:** Yes (receives `thoughts` stream from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow thoughts
.createwindow thought
```

**Stream:** `thoughts`

**Buffer Size:** 500 lines

**Notes:** Useful for keeping track of psinet communications separately from main game text.

---

### speech

**Description:** Displays all spoken communication including speech, whispers, and other verbal messages.

**Auto-update:** Yes (receives `speech` and `whisper` streams from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow speech
```

**Streams:** `speech`, `whisper`

**Buffer Size:** 1000 lines

**Notes:** Combines both speech and whispers into one window. Both types of communication are automatically routed here.

---

### familiar

**Description:** Shows messages from your familiar if you have one active.

**Auto-update:** Yes (receives `familiar` stream from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow familiar
```

**Stream:** `familiar`

**Buffer Size:** 500 lines

**Notes:** Only relevant for characters with familiars. Shows what your familiar sees and does.

---

### room

**Description:** Displays room descriptions, room names, and environmental text.

**Auto-update:** Yes (receives `room` stream from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow room
```

**Stream:** `room`

**Buffer Size:** 100 lines

**Notes:** Useful for keeping room descriptions visible without cluttering your main window. Small buffer since room descriptions don't scroll as quickly.

---

### logons

**Aliases:** `logon`

**Description:** Shows character login and logout messages for characters on your friends list or in your vicinity.

**Auto-update:** Yes (receives `logons` stream from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow logons
.createwindow logon
```

**Stream:** `logons`

**Buffer Size:** 500 lines

**Notes:** Helps track when friends come online or go offline.

---

### deaths

**Aliases:** `death`

**Description:** Displays death messages for characters who die in the game.

**Auto-update:** Yes (receives `deaths` stream from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow deaths
.createwindow death
```

**Stream:** `deaths`

**Buffer Size:** 500 lines

**Notes:** Separate window for tracking who died and when. Useful for roleplay or keeping track of hunting area dangers.

---

### arrivals

**Description:** Shows messages about characters arriving and departing from your location.

**Auto-update:** Yes (receives `arrivals` stream from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow arrivals
```

**Stream:** `arrivals`

**Buffer Size:** 500 lines

**Notes:** Keeps track of traffic in your current room without cluttering main output.

---

### ambients

**Description:** Displays ambient messages and atmospheric text that occurs in rooms and during gameplay.

**Auto-update:** Yes (receives `ambients` stream from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow ambients
```

**Stream:** `ambients`

**Buffer Size:** 500 lines

**Notes:** Ambient messages are environmental flavor text that add atmosphere but aren't critical to gameplay.

---

### announcements

**Description:** Shows game-wide announcements from GameMasters and system messages.

**Auto-update:** Yes (receives `announcements` stream from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow announcements
```

**Stream:** `announcements`

**Buffer Size:** 500 lines

**Notes:** Important for catching GM announcements, server messages, and game-wide events.

---

### loot

**Description:** Displays loot-related messages including items found, picked up, or dropped.

**Auto-update:** Yes (receives `loot` stream from game)

**Default Size:** 10 rows x 40 cols

**Creation:**
```
.createwindow loot
```

**Stream:** `loot`

**Buffer Size:** 500 lines

**Notes:** Useful for tracking what items you've picked up during hunting or looting sessions.

---

## Tabbed Text Windows

Tabbed text windows combine multiple text streams into a single window with tabs, allowing you to switch between different streams (like thoughts, speech, whispers, etc.) in one location. Each tab acts as its own text window with activity indicators to show when inactive tabs receive new messages.

### Creating Tabbed Windows

Tabbed windows are created using the `.createtabbed` command:

```
.createtabbed chat Speech:speech,Thoughts:thoughts,Whisper:whisper
```

This creates a window named "chat" with three tabs:
- **Speech** tab showing the `speech` stream
- **Thoughts** tab showing the `thoughts` stream
- **Whisper** tab showing the `whisper` stream

**Default Size:** 20 rows x 60 cols

**Features:**
- Click on tabs to switch between them (mouse support)
- Inactive tabs show unread indicator (`* ` prefix) when they receive new messages
- Tab colors change based on state (active/inactive/unread)
- Each tab maintains its own scrollback buffer
- Tab bar can be positioned at top or bottom

### Activity Indicators

When you're viewing one tab and a message arrives in another tab, the inactive tab will:
- Show a prefix (default: `* `) before the tab name
- Change color to indicate unread status (default: white/bold)
- Clear the indicator when you switch to that tab

**Example:**
```
Thoughts | Speech | * Whisper
```
In this example, you're viewing the Thoughts tab, and the Whisper tab has unread messages.

### Tab Management Commands

**Add a new tab to an existing tabbed window:**
```
.addtab chat Announcements announcements
```

**Remove a tab:**
```
.removetab chat Announcements
```

**Switch to a specific tab (by name or index):**
```
.switchtab chat Speech
.switchtab chat 0
```

### Configuration Options

Tabbed windows can be customized in your `config.toml`:

```toml
[[ui.windows]]
name = "chat"
widget_type = "tabbed"
streams = []  # Tabs handle their own streams
row = 0
col = 120
rows = 24
cols = 60
buffer_size = 5000
show_border = true
title = "Chat"
tab_bar_position = "top"  # or "bottom"
tab_active_color = "#ffff00"  # Yellow for active tab
tab_inactive_color = "#808080"  # Gray for inactive tabs
tab_unread_color = "#ffffff"  # White/bold for unread tabs
tab_unread_prefix = "* "  # Prefix shown on tabs with unread

[[ui.windows.tabs]]
name = "Speech"
stream = "speech"

[[ui.windows.tabs]]
name = "Thoughts"
stream = "thoughts"

[[ui.windows.tabs]]
name = "Whisper"
stream = "whisper"
```

**Configuration Fields:**
- `widget_type` - Must be `"tabbed"`
- `streams` - Leave empty (tabs manage their own streams)
- `tab_bar_position` - `"top"` or `"bottom"` (default: `"top"`)
- `tab_active_color` - Hex color for the active tab (default: yellow)
- `tab_inactive_color` - Hex color for inactive tabs (default: gray)
- `tab_unread_color` - Hex color for tabs with unread messages (default: white)
- `tab_unread_prefix` - Text shown before tab name when it has unread (default: `"* "`)

**Each tab is defined with:**
- `name` - Display name shown in the tab bar
- `stream` - Game stream that routes to this tab

### Use Cases

**Communication Hub:**
```
.createtabbed chat Speech:speech,Thoughts:thoughts,Whisper:whisper,LNet:logons
```
Combines all communication streams into one window.

**Combat and Tracking:**
```
.createtabbed combat Main:main,Deaths:deaths,Arrivals:arrivals
```
Switch between main combat view and tracking arrivals/deaths.

**Custom Stream Organization:**
Create any combination of streams that makes sense for your workflow. Tabbed windows work with any text stream from the game.

### Notes

- Stream routing: Once a stream is assigned to a tabbed window, it will no longer route to its standalone window
- Scrollback: Each tab maintains its own scrollback buffer (configured via `buffer_size`)
- Mouse support: Click tabs to switch, drag title bar to move, drag edges to resize
- Dynamic management: Add or remove tabs on the fly without recreating the window
- Multiple tabbed windows: You can have multiple tabbed windows for different purposes

---

## Progress Bars

Progress bars display numeric values as visual bars with automatic updates from game data. All progress bars use ProfanityFE-style background coloring where the bar fills from left with a background color.

### health

**Aliases:** `hp`

**Description:** Displays your current and maximum health points as a progress bar.

**Auto-update:** Yes (automatically updated from `<progressBar id='health'>` XML tags)

**Default Size:** 3 rows x 30 cols

**Creation:**
```
.createwindow health
.createwindow hp
```

**Colors:** Dark red bar (#6e0202) on black background (#000000)

**Notes:** The most critical vital. Shows both numeric values (e.g., "150/200") and a visual bar representation.

---

### mana

**Aliases:** `mp`

**Description:** Displays your current and maximum mana points as a progress bar.

**Auto-update:** Yes (automatically updated from `<progressBar id='mana'>` XML tags)

**Default Size:** 3 rows x 30 cols

**Creation:**
```
.createwindow mana
.createwindow mp
```

**Colors:** Dark blue bar (#08086d) on black background (#000000)

**Notes:** Essential for spellcasters. Shows mana pool for casting spells.

---

### stamina

**Aliases:** `stam`

**Description:** Displays your current and maximum stamina points as a progress bar.

**Auto-update:** Yes (automatically updated from `<progressBar id='stamina'>` XML tags)

**Default Size:** 3 rows x 30 cols

**Creation:**
```
.createwindow stamina
.createwindow stam
```

**Colors:** Orange bar (#bd7b00) on black background (#000000)

**Notes:** Used by combat maneuvers and physical abilities. Important for warriors and monks.

---

### spirit

**Description:** Displays your current and maximum spirit points as a progress bar.

**Auto-update:** Yes (automatically updated from `<progressBar id='spirit'>` XML tags)

**Default Size:** 3 rows x 30 cols

**Creation:**
```
.createwindow spirit
```

**Colors:** Grey bar (#6e727c) on black background (#000000)

**Notes:** Critical when dead - shows how much spirit you have for resurrection. Also used for some abilities.

---

### mindstate

**Aliases:** `mind`

**Description:** Displays your mental state (experience absorption rate) as a progress bar with descriptive text.

**Auto-update:** Yes (automatically updated from `<progressBar id='mindState'>` XML tags)

**Default Size:** 3 rows x 30 cols

**Creation:**
```
.createwindow mindstate
.createwindow mind
```

**Colors:** Cyan bar (#008b8b) on black background (#000000)

**Notes:** Shows descriptive text instead of numbers (e.g., "clear as a bell", "saturated"). Important for experience absorption optimization.

---

### encumbrance

**Aliases:** `encum`, `encumlevel`

**Description:** Displays your encumbrance level with dynamic color changes based on load.

**Auto-update:** Yes (automatically updated from `<progressBar id='encumlevel'>` XML tags)

**Default Size:** 3 rows x 30 cols

**Creation:**
```
.createwindow encumbrance
.createwindow encum
.createwindow encumlevel
```

**Colors:** Dynamic - changes from green (#006400) at low encumbrance through yellow, brown, to red at high encumbrance

**Notes:** Special feature: Bar color automatically changes based on your encumbrance level. Critical for managing inventory weight.

---

### stance

**Aliases:** `pbarstance`

**Description:** Displays your combat stance as a progress bar with stance name.

**Auto-update:** Yes (automatically updated from `<progressBar id='pbarStance'>` XML tags)

**Default Size:** 3 rows x 30 cols

**Creation:**
```
.createwindow stance
.createwindow pbarstance
```

**Colors:** Navy blue bar (#000080) on black background (#000000)

**Notes:** Shows stance name (defensive/guarded/neutral/forward/advance/offensive) instead of numeric values. Affects combat balance between offense and defense.

---

### bloodpoints

**Aliases:** `blood`, `lblbps`

**Description:** Displays your current blood points (Dark Elves only).

**Auto-update:** Yes (automatically updated from `<progressBar id='lblBPs'>` XML tags)

**Default Size:** 3 rows x 30 cols

**Creation:**
```
.createwindow bloodpoints
.createwindow blood
.createwindow lblbps
```

**Colors:** Purple bar (#4d0085) on black background (#000000)

**Notes:** Only relevant for Dark Elf characters. Blood points are used for racial abilities.

---

## Countdown Timers

Countdown timers display remaining time for timed effects using a character-based fill animation. They show remaining seconds centered with a colored bar that grows/shrinks based on time left.

### roundtime

**Aliases:** `rt`

**Description:** Shows remaining roundtime in seconds with visual countdown animation.

**Auto-update:** Yes (automatically updated from `<roundTime value='timestamp'>` XML tags)

**Default Size:** 3 rows x 15 cols

**Creation:**
```
.createwindow roundtime
.createwindow rt
```

**Colors:** Red bar (#ff0000) on black background (#000000)

**Manual Set:**
```
.setcountdown roundtime 5
```

**Notes:** Roundtime prevents you from performing most actions. The countdown fills N characters from left where N equals remaining seconds.

---

### casttime

**Aliases:** `cast`

**Description:** Shows remaining cast time in seconds with visual countdown animation.

**Auto-update:** Yes (automatically updated from `<castTime value='timestamp'>` XML tags)

**Default Size:** 3 rows x 15 cols

**Creation:**
```
.createwindow casttime
.createwindow cast
```

**Colors:** Blue bar (#0000ff) on black background (#000000)

**Manual Set:**
```
.setcountdown casttime 3
```

**Notes:** Cast time is the delay before a spell completes. Shows how long until your spell finishes casting.

---

### stuntime

**Aliases:** `stun`

**Description:** Shows remaining stun duration in seconds with visual countdown animation.

**Auto-update:** Can be set manually or via scripts (no automatic game tag)

**Default Size:** 3 rows x 15 cols

**Creation:**
```
.createwindow stuntime
.createwindow stun
```

**Colors:** Yellow bar (#ffff00) on black background (#000000)

**Manual Set:**
```
.setcountdown stuntime 8
```

**Notes:** Stun prevents most actions. Unlike roundtime/casttime, this must be set manually or via Lich scripts as there's no built-in XML tag for stun duration.

---

## Compass

The compass widget displays available room exits in a visual directional layout.

### compass

**Description:** Shows available exits in a compass pattern with eight directions plus OUT.

**Auto-update:** Yes (automatically updated from `<compass>` XML tags)

**Default Size:** 5 rows x 17 cols

**Creation:**
```
.createwindow compass
```

**Display Format:**
```
  NW  N  NE
   W OUT  E
  SW  S  SE
```

**Colors:** Active exits shown in color, inactive exits dimmed

**Notes:** Provides quick visual reference for available movement directions. The OUT exit appears in the center. Very useful for navigation.

---

## Injury Doll

The injury doll displays character wounds and scars as a visual ASCII representation of the body.

### injuries

**Aliases:** `injury_doll`

**Description:** Visual representation of wounds and scars on different body parts with severity indicators.

**Auto-update:** Yes (automatically updated from `<dialogData id='injuriesText'>` XML tags)

**Default Size:** 8 rows x 15 cols

**Creation:**
```
.createwindow injuries
.createwindow injury_doll
```

**Display Format:**
```
    HEAD
    NECK
   CHEST
  ABDOMEN
    BACK
 L.ARM R.ARM
 L.HAND R.HAND
 L.LEG R.LEG
```

**Injury Indicators:**
- `?` - Rank 1 wound (minor)
- `!` - Rank 2 wound (moderate)
- `*` - Rank 3 wound (severe)
- `S` prefix - Scar (e.g., `S?` = rank 1 scar)

**Colors:** Yellow to red based on severity (higher ranks = more red)

**Notes:** Critical for monitoring your health status in combat. Scars persist after healing and affect your stats.

---

## Hands Widgets

Hands widgets display what you're holding in each hand and what spell you have prepared.

### hands

**Description:** Combined display showing left hand, right hand, and prepared spell in one window.

**Auto-update:** Yes (automatically updated from `<left>`, `<right>`, and `<spell>` XML tags)

**Default Size:** 5 rows x 29 cols

**Creation:**
```
.createwindow hands
```

**Display Format:**
```
L: <item in left hand>
R: <item in right hand>
S: <prepared spell>
```

**Notes:** Convenient all-in-one display for your held items and prepared spell. Three rows of information in one compact window.

---

### lefthand

**Description:** Shows only what's in your left hand.

**Auto-update:** Yes (automatically updated from `<left>` XML tags)

**Default Size:** 3 rows x 29 cols

**Creation:**
```
.createwindow lefthand
```

**Display Format:**
```
L: <item in left hand>
```

**Notes:** Individual left hand display. Useful if you want to position hand displays separately.

---

### righthand

**Description:** Shows only what's in your right hand.

**Auto-update:** Yes (automatically updated from `<right>` XML tags)

**Default Size:** 3 rows x 29 cols

**Creation:**
```
.createwindow righthand
```

**Display Format:**
```
R: <item in right hand>
```

**Notes:** Individual right hand display. Useful if you want to position hand displays separately.

---

### spellhand

**Description:** Shows your currently prepared spell.

**Auto-update:** Yes (automatically updated from `<spell>` XML tags)

**Default Size:** 3 rows x 29 cols

**Creation:**
```
.createwindow spellhand
```

**Display Format:**
```
S: <prepared spell>
```

**Notes:** Individual prepared spell display. Useful for spellcasters who want to monitor their prepared spell separately from held items.

---

## Status Indicators

Status indicators show active conditions using Nerd Font icons with a 2-color scheme (black when off, colored when active).

**Note:** Status indicators require a Nerd Font compatible terminal and font to display icons correctly.

### poisoned

**Description:** Shows poison status with a flask/poison icon.

**Auto-update:** Yes (automatically updated from `<dialogData id='IconPOISONED'>` XML tags)

**Default Size:** 3 rows x 3 cols

**Creation:**
```
.createwindow poisoned
```

**Icon:** (Nerd Font poison icon)

**Colors:** Black (#000000) when inactive, green (#00ff00) when poisoned

**Notes:** Indicates active poison effect. Requires treatment with herbs or spells.

---

### diseased

**Description:** Shows disease status with a disease/biohazard icon.

**Auto-update:** Yes (automatically updated from `<dialogData id='IconDISEASED'>` XML tags)

**Default Size:** 3 rows x 3 cols

**Creation:**
```
.createwindow diseased
```

**Icon:** (Nerd Font disease icon)

**Colors:** Black (#000000) when inactive, brownish-red (#8b4513) when diseased

**Notes:** Indicates active disease effect. Requires treatment with herbs or spells.

---

### bleeding

**Description:** Shows bleeding status with a blood drop icon.

**Auto-update:** Yes (automatically updated from `<dialogData id='IconBLEEDING'>` XML tags)

**Default Size:** 3 rows x 3 cols

**Creation:**
```
.createwindow bleeding
```

**Icon:** (Nerd Font blood drop icon)

**Colors:** Black (#000000) when inactive, red (#ff0000) when bleeding

**Notes:** Indicates active bleeding effect. Causes ongoing health loss until stopped.

---

### stunned

**Description:** Shows stun status with a lightning bolt icon.

**Auto-update:** Yes (automatically updated from `<dialogData id='IconSTUNNED'>` XML tags)

**Default Size:** 3 rows x 3 cols

**Creation:**
```
.createwindow stunned
```

**Icon:** (Nerd Font lightning icon)

**Colors:** Black (#000000) when inactive, yellow (#ffff00) when stunned

**Notes:** Indicates stun effect. Prevents most actions while active.

---

### webbed

**Description:** Shows webbed/immobilized status with a web icon.

**Auto-update:** Yes (automatically updated from `<dialogData id='IconWEBBED'>` XML tags)

**Default Size:** 3 rows x 3 cols

**Creation:**
```
.createwindow webbed
```

**Icon:** (Nerd Font web icon)

**Colors:** Black (#000000) when inactive, light grey (#cccccc) when webbed

**Notes:** Indicates web or immobilization effect. Prevents movement until freed.

---

## Dashboard

The dashboard widget is a container that groups multiple status indicators together in configurable layouts.

### status_dashboard

**Description:** Pre-configured dashboard showing all five status indicators (poisoned, diseased, bleeding, stunned, webbed) in a horizontal layout.

**Auto-update:** Yes (all contained indicators auto-update from their respective XML tags)

**Default Size:** 3 rows x 15 cols

**Creation:**
```
.createwindow status_dashboard
```

**Layout:** Horizontal (indicators displayed left to right)

**Indicators Included:**
1. Poisoned (green)
2. Diseased (brownish-red)
3. Bleeding (red)
4. Stunned (yellow)
5. Webbed (grey)

**Special Features:**
- Hides inactive indicators by default (`dashboard_hide_inactive: true`)
- 1 space between icons (`dashboard_spacing: 1`)
- All indicators use same 2-color scheme as individual indicators

**Notes:**
- Very space-efficient way to monitor all status effects
- Cannot be created via `.createwindow` - must be defined in `config.toml` for custom configurations
- The built-in `status_dashboard` template works out of the box

**Custom Dashboard Configuration:**

To create a custom dashboard, add to your `config.toml`:

```toml
[[ui.windows]]
name = "my_dashboard"
widget_type = "dashboard"
streams = []
row = 0
col = 80
rows = 3
cols = 15
show_border = true
title = "Status"
dashboard_layout = "horizontal"  # or "vertical" or "grid_2x3"
dashboard_spacing = 1
dashboard_hide_inactive = true

[[ui.windows.dashboard_indicators]]
id = "poisoned"
icon = "\u{e231}"
colors = ["#000000", "#00ff00"]

[[ui.windows.dashboard_indicators]]
id = "diseased"
icon = "\u{e286}"
colors = ["#000000", "#8b4513"]
```

**Available Layouts:**
- `horizontal` - Row of icons
- `vertical` - Column of icons
- `grid_2x3` - Grid format (2 rows x 3 columns, etc.)

---

## Active Effects

Active effects widgets display time-limited buffs, debuffs, cooldowns, and active spells in scrollable lists with automatic expiration tracking.

All active effects widgets use the `active_effects` widget type but filter to different categories. They support scrolling when there are more effects than can fit in the visible area.

### buffs

**Description:** Displays beneficial effects (buffs) currently active on your character.

**Auto-update:** Yes (automatically updated from game XML effect tags)

**Default Size:** 7 rows x 40 cols

**Creation:**
```
.createwindow buffs
```

**Colors:** Green-tinted bar (#40FF40) for highlighting

**Visible Count:** 5 effects shown at a time (scrollable)

**Border:** Rounded

**Effect Category:** Buffs only

**Notes:** Shows effects like spells that enhance your abilities, defensive bonuses, stat increases, etc. Auto-scrolls or can be manually scrolled.

---

### debuffs

**Description:** Displays harmful effects (debuffs) currently active on your character.

**Auto-update:** Yes (automatically updated from game XML effect tags)

**Default Size:** 5 rows x 40 cols

**Creation:**
```
.createwindow debuffs
```

**Colors:** Red-tinted bar (#FF4040) for highlighting

**Visible Count:** 3 effects shown at a time (scrollable)

**Border:** Rounded

**Effect Category:** Debuffs only

**Notes:** Shows negative effects like curses, stat penalties, ongoing damage effects, etc. Smaller than buffs window since you typically have fewer debuffs.

---

### cooldowns

**Description:** Displays abilities currently on cooldown with remaining cooldown time.

**Auto-update:** Yes (automatically updated from game XML cooldown tags)

**Default Size:** 5 rows x 40 cols

**Creation:**
```
.createwindow cooldowns
```

**Colors:** Orange-tinted bar (#FFB040) for highlighting

**Visible Count:** 3 effects shown at a time (scrollable)

**Border:** Rounded

**Effect Category:** Cooldowns only

**Notes:** Tracks abilities that can't be used again until the cooldown expires. Useful for combat maneuvers and special abilities.

---

### active_spells

**Aliases:** `spells`

**Description:** Displays all spells currently active on your character, regardless of whether they're buffs or debuffs.

**Auto-update:** Yes (automatically updated from game XML spell tags)

**Default Size:** 18 rows x 40 cols

**Creation:**
```
.createwindow active_spells
.createwindow spells
```

**Colors:** Blue-tinted bar (#4080FF) for highlighting

**Visible Count:** All effects shown (no limit, window scrolls as needed)

**Border:** Rounded

**Effect Category:** Active Spells only

**Notes:** Larger window for comprehensive spell tracking. Shows both beneficial and harmful spells. Ideal for spellcasters who want to monitor their entire spell suite.

---

### all_effects

**Aliases:** `effects`

**Description:** Displays ALL active effects - buffs, debuffs, cooldowns, and spells - in one combined window.

**Auto-update:** Yes (automatically updated from all game XML effect tags)

**Default Size:** 12 rows x 40 cols

**Creation:**
```
.createwindow all_effects
.createwindow effects
```

**Colors:** Grey bar (#808080) for neutral highlighting

**Visible Count:** 10 effects shown at a time (scrollable)

**Border:** Rounded

**Effect Category:** All effects

**Notes:** Comprehensive view of everything active on your character. Good for players who want one window to monitor all temporary effects rather than separate windows per category.

---

## Summary Table

### Text Windows (11 total)
| Template | Aliases | Stream(s) | Buffer | Purpose |
|----------|---------|-----------|--------|---------|
| main | - | main | 10000 | Primary game output |
| thoughts | thought | thoughts | 500 | Character thoughts |
| speech | - | speech, whisper | 1000 | Spoken communication |
| familiar | - | familiar | 500 | Familiar messages |
| room | - | room | 100 | Room descriptions |
| logons | logon | logons | 500 | Login/logout notices |
| deaths | death | deaths | 500 | Death messages |
| arrivals | - | arrivals | 500 | Arrival/departure notices |
| ambients | - | ambients | 500 | Ambient messages |
| announcements | - | announcements | 500 | GM/system announcements |
| loot | - | loot | 500 | Loot messages |

### Progress Bars (8 total)
| Template | Aliases | Auto-Update | Default Color | Special Features |
|----------|---------|-------------|---------------|------------------|
| health | hp | Yes | Dark red | Core vital |
| mana | mp | Yes | Dark blue | Spell resource |
| stamina | stam | Yes | Orange | Maneuver resource |
| spirit | - | Yes | Grey | Death/resurrection |
| mindstate | mind | Yes | Cyan | Shows text descriptions |
| encumbrance | encum, encumlevel | Yes | Dynamic | Color changes with load |
| stance | pbarstance | Yes | Navy | Shows stance name |
| bloodpoints | blood, lblbps | Yes | Purple | Dark Elf only |

### Countdown Timers (3 total)
| Template | Aliases | Auto-Update | Color | Notes |
|----------|---------|-------------|-------|-------|
| roundtime | rt | Yes | Red | Combat delay |
| casttime | cast | Yes | Blue | Spell cast delay |
| stuntime | stun | Manual | Yellow | Stun duration |

### Special Widgets (8 total)
| Template | Aliases | Type | Auto-Update | Description |
|----------|---------|------|-------------|-------------|
| compass | - | compass | Yes | Directional exits display |
| injuries | injury_doll | injury_doll | Yes | Wound/scar visualization |
| hands | - | hands | Yes | All hands (L/R/S) |
| lefthand | - | lefthand | Yes | Left hand only |
| righthand | - | righthand | Yes | Right hand only |
| spellhand | - | spellhand | Yes | Prepared spell only |
| poisoned | - | indicator | Yes | Poison status icon |
| diseased | - | indicator | Yes | Disease status icon |
| bleeding | - | indicator | Yes | Bleeding status icon |
| stunned | - | indicator | Yes | Stun status icon |
| webbed | - | indicator | Yes | Web status icon |
| status_dashboard | - | dashboard | Yes | Combined status indicators |

### Active Effects (5 total)
| Template | Aliases | Category | Visible Count | Description |
|----------|---------|----------|---------------|-------------|
| buffs | - | Buffs | 5 | Beneficial effects only |
| debuffs | - | Debuffs | 3 | Harmful effects only |
| cooldowns | - | Cooldowns | 3 | Ability cooldowns |
| active_spells | spells | Active Spells | All | All active spells |
| all_effects | effects | All | 10 | All effects combined |

---

## Creating Custom Windows

If the built-in templates don't meet your needs, you can create custom windows:

**Custom text window with specific streams:**
```
.customwindow combat combat,assess,death
```

**Define completely custom windows in config.toml:**
```toml
[[ui.windows]]
name = "my_window"
widget_type = "text"
streams = ["custom_stream"]
row = 0
col = 0
rows = 15
cols = 60
buffer_size = 1000
show_border = true
border_style = "rounded"
title = "My Custom Window"
```

See [Configuration](Configuration.md) for more details on custom window definitions.

---

## Widget Management Commands

**Create a widget:**
```
.createwindow <template>
.createwin <template>
```

**Delete a widget:**
```
.deletewindow <name>
.deletewin <name>
```

**List all active widgets:**
```
.windows
.listwindows
```

**List available templates:**
```
.templates
```

**Rename a widget:**
```
.rename <window> <new_title>
```

**Change border style:**
```
.border <window> <style> [color]
```
Styles: `single`, `double`, `rounded`, `thick`, `none`

**Manual progress update:**
```
.setprogress <window> <current> <max>
```

**Change progress bar colors:**
```
.setbarcolor <window> <color> [bg_color]
```

**Manual countdown:**
```
.setcountdown <window> <seconds>
```

---

## Notes

- All widgets support mouse operations: click to focus, drag title bar to move, drag edges/corners to resize
- Widgets can overlap (absolute positioning) - arrange them however you like
- Window layouts can be saved with `.savelayout <name>` and loaded with `.loadlayout <name>`
- Most widgets auto-update from game XML data - no manual configuration needed
- Status indicators require Nerd Fonts for proper icon display
- Active effects widgets automatically scroll when there are more items than visible space
