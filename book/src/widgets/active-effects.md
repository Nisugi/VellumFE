# Active Effects

Displays active spells, buffs, debuffs, and cooldowns.

## Basic Usage

```toml
[[windows]]
name = "buffs"
widget_type = "active_effects"
category = "Buffs"
row = 0
col = 0
rows = 10
cols = 30
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `category` | string | required | Which effects dialog to show |

## Categories

Each widget shows exactly one of the game's effect dialogs — there is no
combined view. Built-in templates exist for all four (`buffs`, `debuffs`,
`cooldowns`, `active_spells`):

| Category | Content |
|----------|---------|
| `Buffs` | Beneficial effects |
| `Debuffs` | Harmful effects |
| `Cooldowns` | Ability cooldowns |
| `ActiveSpells` | Active spells |

## Display

Shows effect name with remaining duration in brackets (`[MM:SS]`, or
`[HH:MM]` for long durations):

```
┌─ Buffs ─────────────────┐
│ Spirit Shield   [12:34] │
│ Haste           [03:45] │
│ Bless           [08:20] │
└─────────────────────────┘
```

## Example

```toml
[[windows]]
name = "spells"
widget_type = "active_effects"
category = "ActiveSpells"
rows = 8
cols = 25
show_border = true
title = "Spells"
```
