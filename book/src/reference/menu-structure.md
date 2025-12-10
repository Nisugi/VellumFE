# Menu Structure Reference

Access the main menu with `.menu` or the configured keybind.

## Main Menu

```
┌──────────────┐
│ Colors >     │
│ Highlights > │
│ Keybinds >   │
│ Layouts >    │
│ Settings     │
│ Windows >    │
└──────────────┘
```

---

## Windows Menu

```
┌────────────────┐
│ Add window >   │
│ Edit window >  │
│ Hide window >  │
│ List windows > │
└────────────────┘
```

### Add Window

Categories of widgets you can add:

```
┌──────────────────┐
│ Active Effects > │
│ Countdowns >     │
│ Entities >       │
│ Hands >          │
│ Other >          │
│ Progress Bars >  │
│ Status >         │
│ Text Windows >   │
└──────────────────┘
```

#### Active Effects

```
┌───────────────┐
│ Active Spells │
│ Buffs         │
│ Cooldowns     │
│ Debuffs       │
│ Custom        │
└───────────────┘
```

#### Countdowns

```
┌───────────┐
│ Casttime  │
│ Roundtime │
│ Stuntime  │
│ Custom    │
└───────────┘
```

#### Entities

```
┌─────────┐
│ Players │
│ Targets │
│ Custom  │
└─────────┘
```

#### Hands

```
┌────────────┐
│ Left Hand  │
│ Right Hand │
│ Spell Hand │
└────────────┘
```

#### Progress Bars

```
┌─────────────┐
│ Blood       │
│ Encumbrance │
│ Health      │
│ Mana        │
│ MindState   │
│ Spirit      │
│ Stamina     │
│ Stance      │
│ Custom      │
└─────────────┘
```

#### Status

```
┌──────────────┐
│ Dashboard    │
│ Indicators > │
└──────────────┘
```

##### Indicators

```
┌──────────┐
│ Bleeding │
│ Diseased │
│ Poisoned │
│ Stunned  │
│ Webbed   │
│ Editor   │
└──────────┘
```

#### Text Windows

```
┌───────────────┐
│ Tabbed        │
│ Ambients      │
│ Announcements │
│ Arrivals      │
│ Bounty        │
│ Deaths        │
│ Familiar      │
│ Loot          │
│ Story         │
│ Society       │
│ Speech        │
│ Thoughts      │
│ Custom        │
└───────────────┘
```

#### Other

```
┌─────────────┐
│ Compass     │
│ Injuries    │
│ Inventory   │
│ Room        │
│ Performance │
│ Spacer      │
│ Spells      │
└─────────────┘
```

---

## Colors Menu

```
┌────────┐
│ Add    │
│ Browse │
│ Spells │
│ Themes │
└────────┘
```

| Item | Command | Description |
|------|---------|-------------|
| Add | `.addcolor` | Create a new named color |
| Browse | `.colors` | Open color palette browser |
| Spells | `.spellcolors` | Configure spell-specific colors |
| Themes | `.themes` | Theme editor browser |

---

## Highlights Menu

```
┌────────┐
│ Add    │
│ Browse │
└────────┘
```

| Item | Command | Description |
|------|---------|-------------|
| Add | `.addhighlight` | Create a new highlight rule |
| Browse | `.highlights` | Open highlight browser |

### Add Highlight Form

```
┌ Add Highlight ─────────────────────────────────────────────────────┐
│                                                                    │
│ Name:            e.g., swing_highlight                             │
│ Pattern:         e.g., You swing.*                                 │
│ Category:        e.g., Combat, Loot, Spells                        │
│ Foreground:      #ff0000                                           │
│ Background:      (optional)                                        │
│ Sound:           none                                              │
│ Volume:          0.0-1.0                                           │
│ Replace:         replacement text                                  │
│ Redirect To:     stream name (e.g., combat)                        │
│ Redirect Mode:   Off                                               │
│                                                                    │
│ [ ] Bold                                                           │
│ [ ] Color entire line                                              │
│ [ ] Fast parse                                                     │
│ [ ] Squelch (ignore line)                                          │
│                                                                    │
└─[Ctrl+S: Save]─[Esc: Back]─────────────────────────────────────────┘
```

### Highlight Browser

```
┌ Highlight Browser ─────────────────────────────────────────────────┐
│ ═══ PLAYERS ═══                                                    │
│               enemies                                              │
│      [-]      friends                                              │
│ ═══ ROOM ═══                                                       │
│      [-]      exits                                                │
│ ═══ SQUELCH ═══                                                    │
│  -   [-]      arrival_spam [SQUELCH]                               │
│ ═══ TESTING ═══                                                    │
│               test_obvious                                         │
│                                                                    │
└─[Ctrl+S: Save]─[A: Add]─[E: Edit]─[Del: Delete]─[Esc: Back]────────┘
```

---

## Keybinds Menu

```
┌────────┐
│ Add    │
│ Browse │
└────────┘
```

| Item | Command | Description |
|------|---------|-------------|
| Add | `.addkeybind` | Create a new keybind |
| Browse | `.keybinds` | Open keybind browser |

### Add Keybind Form

**Action Type:**

```
┌ Add Keybind ───────────────────────────────────────────────────────┐
│                                                                    │
│ Type: [X] Action     [ ] Macro                                     │
│ Key Combo:  e.g., ctrl+e, f5, alt+shift+a                          │
│                                                                    │
│ Action:    send_command                                            │
│                                                                    │
└─[Ctrl+S: Save]─[Del: Delete]─[Esc: Back]───────────────────────────┘
```

**Macro Type:**

```
┌ Add Keybind ───────────────────────────────────────────────────────┐
│                                                                    │
│ Type: [ ] Action     [X] Macro                                     │
│ Key Combo:  e.g., ctrl+e, f5, alt+shift+a                          │
│                                                                    │
│ Macro Text:     e.g., run left\r                                   │
│                                                                    │
└─[Ctrl+S: Save]─[Del: Delete]─[Esc: Back]───────────────────────────┘
```

### Keybind Browser

```
┌ Keybinds (35) ─────────────────────────────────────────────────────┐
│ ═══ ACTIONS ═══                                                    │
│ alt+page_down       Action    scroll_current_window_down_one       │
│ alt+page_up         Action    scroll_current_window_up_one         │
│ backspace           Action    cursor_backspace                     │
│ ctrl+f              Action    start_search                         │
│ ctrl+left           Action    cursor_word_left                     │
│ ctrl+page_down      Action    next_search_match                    │
│ ctrl+page_up        Action    prev_search_match                    │
│ ctrl+right          Action    cursor_word_right                    │
│ ...                                                                │
│                                                                    │
└─[Ctrl+S: Save]─[A: Add]─[E: Edit]─[Del: Delete]─[Esc: Back]────────┘
```

---

## Layouts Menu

Lists all saved layouts from `~/.vellum-fe/layouts/`:

```
┌─────────────┐
│ Default     │
│ None        │
│ Sidebar     │
└─────────────┘
```

Selecting a layout loads it immediately.

---

## Settings

Opens the configuration editor for `config.toml` settings.

---

## Dot Commands

You can also access these functions directly via dot commands:

| Command | Description |
|---------|-------------|
| `.menu` | Open main menu |
| `.addwindow` | Open widget picker |
| `.editwindow [name]` | Edit a window (or open picker) |
| `.hidewindow [name]` | Hide a window (or open picker) |
| `.windows` | List all windows |
| `.highlights` | Open highlight browser |
| `.addhighlight` | Add new highlight |
| `.keybinds` | Open keybind browser |
| `.addkeybind` | Add new keybind |
| `.colors` | Open color browser |
| `.themes` | Open theme editor |
| `.savelayout [name]` | Save current layout |
| `.loadlayout [name]` | Load a saved layout |
