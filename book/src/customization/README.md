# Customization Overview

VellumFE is highly customizable, allowing you to tailor every aspect of your gameplay experience.

## Customization Areas

VellumFE can be customized in these areas:

| Area | Purpose | Configuration |
|------|---------|---------------|
| **Layouts** | Window arrangement | `layout.toml` |
| **Themes** | Colors and styling | `colors.toml` |
| **Highlights** | Text pattern matching | `highlights.toml` |
| **Keybinds** | Keyboard shortcuts | `keybinds.toml` |
| **Sounds** | Audio alerts | Per-highlight |
| **TTS** | Text-to-speech | `config.toml` |

## Quick Customization Commands

| Command | Opens |
|---------|-------|
| `.layout` | Layout editor |
| `.highlights` | Highlight browser |
| `.keybinds` | Keybind browser |
| `.colors` | Color palette browser |
| `.themes` | Theme browser |
| `.window <name>` | Window editor |

## Customization Guides

This section covers:

- [Creating Layouts](./creating-layouts.md) - Design your window arrangement
- [Creating Themes](./creating-themes.md) - Customize colors and styling
- [Highlight Patterns](./highlight-patterns.md) - Advanced text matching
- [Keybind Actions](./keybind-actions.md) - All available actions
- [Sound Alerts](./sound-alerts.md) - Audio notifications
- [TTS Setup](./tts-setup.md) - Text-to-speech configuration

## Per-Character Customization

Each character can have unique settings:

```
~/.vellum-fe/
├── config.toml              # Global defaults
├── layout.toml              # Default layout
├── colors.toml              # Default colors
├── highlights.toml          # Default highlights
├── keybinds.toml            # Default keybinds
└── characters/
    └── MyCharacter/
        ├── layout.toml      # Character-specific layout
        ├── colors.toml      # Character-specific colors
        ├── highlights.toml  # Character-specific highlights
        └── keybinds.toml    # Character-specific keybinds
```

### Configuration Loading Order

1. Character-specific file (if exists)
2. Global file (fallback)
3. Embedded defaults (final fallback)

### Example: Profession-Based Layouts

Create different layouts for different professions:

```
~/.vellum-fe/characters/
├── Warrior/
│   └── layout.toml    # Combat-focused layout
├── Empath/
│   └── layout.toml    # Healing-focused layout
└── Wizard/
    └── layout.toml    # Spell-focused layout
```

## Hot-Reloading

Many settings can be reloaded without restarting:

| Command | Reloads |
|---------|---------|
| `.reload colors` | Color configuration |
| `.reload highlights` | Highlight patterns |
| `.reload keybinds` | Keyboard shortcuts |
| `.reload layout` | Window layout |
| `.reload config` | All configuration |

## Backup Your Customizations

Before major changes:

```bash
# Backup all configuration
cp -r ~/.vellum-fe ~/.vellum-fe-backup

# Backup specific file
cp ~/.vellum-fe/layout.toml ~/.vellum-fe/layout.toml.bak
```

## Sharing Customizations

Share your configurations with others:

1. Export relevant `.toml` files
2. Share via GitHub Gist, Discord, etc.
3. Others can copy to their `~/.vellum-fe/` directory

### Popular Layout Packs

The VellumFE community shares layout packs:
- Hunting layouts
- Merchant layouts
- Roleplay layouts
- Accessibility layouts

## Customization Tips

### Start Simple

Begin with default settings and customize incrementally:

1. Play with defaults for a few sessions
2. Identify pain points
3. Make one change at a time
4. Test each change before moving on

### Use the Editors

The built-in editors are often easier than editing files:

```
.layout          # Visual layout editor
.highlights      # Browse and edit highlights
.keybinds        # Browse and edit keybinds
.window main     # Edit main window properties
```

### Keep Notes

Document your customizations:

```toml
# In any .toml file, add comments:

# Combat creature highlighting
# Red for dangerous, orange for normal
[creature_dangerous]
pattern = "(?i)\\b(dragon|lich|demon)\\b"
fg = "#FF0000"
```

### Test in Safe Areas

Test new configurations in safe game areas before combat:

1. Make changes
2. Reload: `.reload highlights`
3. Test with game output
4. Adjust as needed

## See Also

- [Configuration Files](../configuration/README.md) - File reference
- [Widgets Reference](../widgets/README.md) - Widget options
- [Tutorials](../tutorials/README.md) - Step-by-step guides

