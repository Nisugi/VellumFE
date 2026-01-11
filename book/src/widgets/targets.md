# Targets

Lists creatures in the current room with status indicators.

## Basic Usage

```toml
[[windows]]
name = "targets"
widget_type = "targets"
row = 0
col = 0
rows = 10
cols = 35
```

## Display

```
┌─ Targets [03] ────────────┐
│ > a mud hog [stunned]     │
│   a mud hog               │
│   a large rat [dead]      │
└───────────────────────────┘
```

- `>` marks your current target
- Status shown in brackets
- Count in title

## Configuration

Configure via `config.toml`:

```toml
[target_list]
status_position = "end"         # "end" or "start"
truncation_mode = "noun"        # "full" or "noun"
excluded_nouns = ["arm", "coal"]

[target_list.status_abbrev]
stunned = "stu"
frozen = "frz"
dead = "ded"
```

## Interaction

- Click creature to target
- Right-click for context menu
- Drag to inventory to loot

## Example

```toml
[[windows]]
name = "targets"
widget_type = "targets"
row = 0
col = 100
rows = 12
cols = 30
show_border = true
border_color = "#AA4444"
title = "Enemies"
```
