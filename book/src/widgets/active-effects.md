# Active Effects

Displays active spells, buffs, debuffs, and cooldowns.

## Basic Usage

```toml
[[windows]]
name = "effects"
widget_type = "active_effects"
row = 0
col = 0
rows = 10
cols = 30
```

## Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `category` | string | `"all"` | Filter by type |

## Categories

| Category | Content |
|----------|---------|
| `all` | Everything |
| `spell` | Active spells |
| `buff` | Beneficial effects |
| `debuff` | Harmful effects |
| `cooldown` | Ability cooldowns |

## Display

Shows effect name with remaining duration:

```
┌─ Active Effects ──────┐
│ Spirit Shield   12:34 │
│ Haste            3:45 │
│ Bless            8:20 │
└───────────────────────┘
```

## Example

```toml
[[windows]]
name = "buffs"
widget_type = "active_effects"
category = "spell"
rows = 8
cols = 25
show_border = true
title = "Spells"
```
