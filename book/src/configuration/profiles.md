# Character Profiles

VellumFE supports per-character configuration through profiles, allowing different layouts, highlights, and settings for each character.

## Overview

Profiles let you:
- Use different layouts for different playstyles (hunting warrior vs. merchant)
- Customize highlights per character (different friend lists, creature lists)
- Override any global setting on a per-character basis

---

## Profile Structure

Profiles are stored in `~/.vellum-fe/profiles/<CharacterName>/`:

```
~/.vellum-fe/
├── config.toml           # Global defaults
├── layout.toml           # Global layout
├── keybinds.toml         # Global keybinds
├── highlights.toml       # Global highlights
├── colors.toml           # Global colors
│
└── profiles/
    ├── Warrior/          # Warrior's profile
    │   ├── layout.toml   # Warrior's layout
    │   └── highlights.toml
    │
    ├── Wizard/           # Wizard's profile
    │   ├── layout.toml
    │   ├── highlights.toml
    │   └── keybinds.toml
    │
    └── Merchant/         # Merchant's profile
        └── layout.toml
```

---

## Using Profiles

### Loading a Profile

Specify the character name when launching:

```bash
vellum-fe --character Warrior
```

Or set a default in `config.toml`:

```toml
[connection]
character = "Warrior"
```

### Profile Loading Order

1. **Load global config** from `~/.vellum-fe/`
2. **If profile exists**, overlay files from `profiles/<CharName>/`
3. **Missing profile files** fall back to global

**Example flow for `--character Warrior`:**

| File | Source |
|------|--------|
| config.toml | Global (no profile override) |
| layout.toml | `profiles/Warrior/layout.toml` |
| keybinds.toml | Global (no profile override) |
| highlights.toml | `profiles/Warrior/highlights.toml` |
| colors.toml | Global (no profile override) |

---

## Creating a Profile

### Method 1: Manual Creation

```bash
# Create profile directory
mkdir -p ~/.vellum-fe/profiles/MyCharacter

# Copy files you want to customize
cp ~/.vellum-fe/layout.toml ~/.vellum-fe/profiles/MyCharacter/
cp ~/.vellum-fe/highlights.toml ~/.vellum-fe/profiles/MyCharacter/

# Edit the copies
vim ~/.vellum-fe/profiles/MyCharacter/layout.toml
```

### Method 2: Start from Defaults

Launch with a new character name:

```bash
vellum-fe --character NewCharacter
```

VellumFE uses global defaults. Then copy and customize as needed.

---

## Profile-Specific Settings

### Layout Differences

A warrior might want:
```toml
# profiles/Warrior/layout.toml

[[windows]]
name = "main"
type = "text"
width = "70%"
height = "80%"

# Large status area for combat
[[windows]]
name = "vitals"
type = "dashboard"
width = "30%"
height = "40%"
```

A merchant might prefer:
```toml
# profiles/Merchant/layout.toml

[[windows]]
name = "main"
type = "text"
width = "100%"
height = "90%"

# No combat widgets, maximize text area
```

### Highlight Differences

Warrior with creature highlights:
```toml
# profiles/Warrior/highlights.toml

[[highlights]]
name = "hunting_targets"
pattern = "(?i)\\b(goblin|orc|troll|giant)s?\\b"
fg = "#FF6600"
bold = true
```

Merchant with item highlights:
```toml
# profiles/Merchant/highlights.toml

[[highlights]]
name = "valuable_items"
pattern = "(?i)\\b(gold|silver|gem|diamond)s?\\b"
fg = "#FFD700"
bold = true

[[highlights]]
name = "materials"
pattern = "(?i)\\b(leather|silk|velvet|mithril)\\b"
fg = "#00FFFF"
```

### Keybind Differences

Combat character:
```toml
# profiles/Warrior/keybinds.toml

[[keybinds]]
key = "F1"
action = "send"
argument = "attack"

[[keybinds]]
key = "F2"
action = "send"
argument = "feint"
```

Spellcaster:
```toml
# profiles/Wizard/keybinds.toml

[[keybinds]]
key = "F1"
action = "send"
argument = "incant 901"

[[keybinds]]
key = "F2"
action = "send"
argument = "incant 903"
```

---

## Shared vs. Profile-Specific

### Keep Global (Shared)

- **colors.toml** - Usually same theme everywhere
- **config.toml** - Connection settings shared

### Make Profile-Specific

- **layout.toml** - Different layouts per playstyle
- **highlights.toml** - Different creatures/items/friends
- **keybinds.toml** - Different combat macros

---

## Profile Tips

### 1. Start Simple

Only create profile overrides for files you need to change. Let others fall back to global.

### 2. Use Consistent Names

Match your character name exactly (case-sensitive on some systems):

```bash
# Good
--character Nisugi
~/.vellum-fe/profiles/Nisugi/

# May not work
--character nisugi
~/.vellum-fe/profiles/Nisugi/  # Case mismatch!
```

### 3. Share Common Elements

For settings shared across some (but not all) characters, create a "base" profile and copy from it:

```bash
# Create a hunting base
cp -r ~/.vellum-fe/profiles/Warrior ~/.vellum-fe/profiles/hunting-base

# New hunting character
cp -r ~/.vellum-fe/profiles/hunting-base ~/.vellum-fe/profiles/NewHunter
```

### 4. Version Control

Consider version controlling your profiles:

```bash
cd ~/.vellum-fe
git init
git add .
git commit -m "Initial VellumFE configuration"
```

---

## Troubleshooting Profiles

### Profile Not Loading

1. Check character name spelling (case-sensitive)
2. Verify directory exists: `ls ~/.vellum-fe/profiles/`
3. Check file permissions
4. Look for errors in `~/.vellum-fe/vellum-fe.log`

### Wrong Settings Applied

1. Check which profile is loaded (shown on startup)
2. Verify the file exists in the profile directory
3. Check for syntax errors in the profile file

### Resetting a Profile

```bash
# Remove profile to use global defaults
rm -rf ~/.vellum-fe/profiles/CharName/

# Or reset specific file
rm ~/.vellum-fe/profiles/CharName/layout.toml
```

---

## Example: Multi-Character Setup

```
~/.vellum-fe/
├── config.toml
├── layout.toml           # Balanced default layout
├── keybinds.toml         # Common keybinds
├── highlights.toml       # Common highlights
├── colors.toml           # Dark theme for all
│
└── profiles/
    ├── Ranger/
    │   ├── layout.toml   # Compact hunting layout
    │   ├── highlights.toml # Creature + foraging
    │   └── keybinds.toml # Ranged combat macros
    │
    ├── Empath/
    │   ├── layout.toml   # Group-focused layout
    │   └── highlights.toml # Wounds + group members
    │
    └── Bard/
        ├── layout.toml   # RP-friendly layout
        └── highlights.toml # Song + speech focus
```

---

## See Also

- [Configuration Overview](./README.md) - All config files
- [Creating Layouts](../customization/creating-layouts.md) - Layout design
- [Tutorials](../tutorials/README.md) - Complete setup examples
