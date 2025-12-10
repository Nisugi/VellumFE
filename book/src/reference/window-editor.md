# Window Editor Reference

The window editor lets you configure any widget. Access it via:
- `.editwindow windowname` - edit a specific window directly
- `.menu` → Windows → Edit window → select category → select window

Each widget type has a base set of fields plus widget-specific options.

## Common Fields (All Widgets)

All widgets share these base configuration options:

```
┌ Edit Window ───────────────────────────────────────────────────────┐
│                                                                    │
│ →  Name:  internal name                Lock Window     [ ]         │
│   Title:  display name                 Show Title      [✓]         │
│     Title Align: top-left ▼            Transparent BG  [✓]         │
│   Content Align: top-left ▼            Show Border     [✓]         │
│    Border Style: single ▼              Top Border      [✓]         │
│                                        Bottom Border   [✓]         │
│    Row: 31         Col: 20             Left Border     [✓]         │
│   Rows: 20        Cols: 100            Right Border    [✓]         │
│    Min:            Min:                [  ] BG Color:              │
│    Max:            Max:                [  ] Border:                │
│                                                                    │
└─[Ctrl+S: Save]───────────────────────────────────────[Esc: Cancel]─┘
```

| Field | Description |
|-------|-------------|
| Name | Internal identifier (used in layout.toml) |
| Title | Display name shown in title bar |
| Title Align | Position of title (top-left, top-center, top-right) |
| Content Align | How content is aligned within the widget |
| Border Style | single, double, rounded, thick, or none |
| Row/Col | Position on screen |
| Rows/Cols | Size of widget |
| Min/Max | Size constraints |
| Lock Window | Prevent moving/resizing |
| Transparent BG | See-through background |
| Border toggles | Show/hide individual borders |
| BG Color | Override background color |
| Border | Override border color |

---

## Text Widget

For scrollable text windows (main, thoughts, combat, etc.):

```
│                                                                    │
│   Streams: main                        Wordwrap        [✓]         │
│   Buffer Size: 10000                   Timestamps      [✓]         │
│                                                                    │
```

| Field | Description |
|-------|-------------|
| Streams | Which game stream(s) to display |
| Buffer Size | Lines to keep in history |
| Wordwrap | Wrap long lines |
| Timestamps | Show time for each line |

---

## Tabbed Text Widget

Multi-tab text window with individual stream tabs:

```
│                                                                    │
│    Tab Bar Pos: top                    [  ] Active                 │
│    Tab Border:  [✓]                    [  ] Inactive               │
│   New Msg Icon:                        [  ] Unread                 │
│   [ Edit Tabs ]                                                    │
│                                                                    │
```

### Tab Editor

Press `[ Edit Tabs ]` to manage tabs:

```
┌ Edit Window ───────────────────────────────────────────────────────┐
│Tab Editor                                                          │
│                                                                    │
│> Thoughts             ->  Thoughts                                 │
│  Speech               ->  speech                                   │
│  Announcements        ->  announcements                            │
│  Ambients             ->  ambients                                 │
│  Loot                 ->  loot                                     │
│                                                                    │
└─[A: Add]─[E: Edit]─[Del: Delete]─[Shift+↑/↓: Re-order]─[Esc: Back]─┘
```

### Edit Individual Tab

```
│  Tab Name  Speech                                                  │
│  Stream    speech                                                  │
│  [✓] Timestamps                                                    │
│  [ ] Ignore Activity                                               │
```

---

## Command Input

The command entry line:

```
│                                                                    │
│   Icon:                                [  ] Text                   │
│   [  ] Icon Color                      [  ] Cursor FG              │
│                                        [  ] Cursor BG              │
│                                                                    │
```

---

## Progress Bar

Health, mana, stamina, and other progress indicators:

```
│                                                                    │
│   Progress ID:  health                 [  ] Text Color             │
│   Numbers Only: [ ]                    [  ] Bar Color              │
│   Current Only: [ ]                                                │
│                                                                    │
```

| Field | Description |
|-------|-------------|
| Progress ID | Which stat to display (health, mana, stamina, etc.) |
| Numbers Only | Show only the numeric value |
| Current Only | Hide the maximum value |

---

## Countdown

Roundtime, casttime, and other timers:

```
│                                                                    │
│   Icon:                                Countdown ID:               │
│   [  ] Icon Color:                     [  ] BG Color:              │
│                                                                    │
```

| Field | Description |
|-------|-------------|
| Countdown ID | Which timer (roundtime, casttime, stuntime) |
| Icon | Unicode character to display |

---

## Compass

Navigation directions:

```
│                                                                    │
│                                        [  ] Active:                │
│                                        [  ] Inactive:              │
│                                                                    │
```

| Field | Description |
|-------|-------------|
| Active | Color for available exits |
| Inactive | Color for unavailable directions |

---

## Hand

Left/right hand item display:

```
│                                                                    │
│   Icon:                                [  ] Icon Color             │
│                                        [  ] Text Color             │
│                                                                    │
```

---

## Indicator

Status indicators (poisoned, stunned, etc.):

```
│                                                                    │
│   Icon:                                [  ] Active                 │
│                                        [  ] Inactive               │
│                                                                    │
```

| Field | Description |
|-------|-------------|
| Icon | Unicode character when active |
| Active | Color when condition is true |
| Inactive | Color when condition is false |

---

## Dashboard

Grid of multiple status indicators:

```
│                                                                    │
│   Layout: horizontal ▼                 Spacing: 1                  │
│   [ Edit Indicators ]                  Hide Inactive   [✓]         │
│                                                                    │
```

### Indicator Selector

```
┌ Edit Window ──────────────────────────────────────────────────────┐
│Indicator Selector                                                  │
│                                                                    │
│>  poisoned                                                        │
│   diseased                                                        │
│   bleeding                                                        │
│   stunned                                                         │
│  󰯊 webbed                                                          │
│                                                                    │
└─[T: Toggle]─[Del: Delete]────────[Shift+↑/↓: Re-order]─[Esc: Back]─┘
```

---

## Injury Display

Body part injury visualization:

```
│                                                                    │
│   [  ] Wound1                          [  ] Scar1                  │
│   [  ] Wound2                          [  ] Scar2                  │
│   [  ] Wound3                          [  ] Scar3                  │
│   [  ] Uninjured                                                   │
```

---

## Room Window

Room name, description, and contents:

```
│                                                                    │
│   Show Name       [✓]                                              │
│   Show Desc       [✓]                  Show Players    [✓]         │
│   Show Objects    [✓]                  Show Exits      [✓]         │
│                                                                    │
```

---

## Active Effects

Buffs, debuffs, spells:

```
│                                                                    │
│   Effect ID:                           [  ] Default                │
│                                        [  ] Text                   │
│                                                                    │
```

---

## Spells Window

```
│                                                                    │
│   Wordwrap        [ ]                                              │
│                                                                    │
```

---

## Performance Monitor

```
│                                                                    │
│   Enable Monitor  [✓]                  [ Choose Metrics ]          │
│                                                                    │
```

### Metrics Selector

```
┌ Performance Metrics ──────────────────────────────────────────────┐
│                                                                    │
│> [ ] FPS                               [ ] Frame Times             │
│  [ ] Frame Spikes                      [ ] Frame Jitter            │
│  [ ] Render Times                      [ ] UI Times                │
│  [ ] Wrap Times                                                    │
│  [ ] Parser                            [ ] Network                 │
│  [ ] Events                            [ ] Event lag               │
│  [ ] Memory                            [ ] Memory Delta            │
│  [ ] Uptime                            [ ] Lines / Window          │
│                                                                    │
└─[Space: Toggle]─────────────────────────────────────────[Esc: Back]─┘
```

---

## Entity Widgets

Targets and players lists:

```
│                                                                    │
│   Entity ID: targetcount                                           │
│                                                                    │
```

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Tab | Move to next field |
| Shift+Tab | Move to previous field |
| Space | Toggle checkbox |
| Enter | Activate button / open dropdown |
| Ctrl+S | Save changes |
| Esc | Cancel and close |
