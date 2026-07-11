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
│ > a mud hog [stu]         │
│   a mud hog [stu,prn]     │
│   a large rat             │
└───────────────────────────┘
```

- `>` marks your current target
- Statuses shown abbreviated in brackets — when the game sends its
  structured status feed (`<crtrStatus>`), every active status shows at
  once (`[stu,prn]`); otherwise the single status parsed from the room
  text is used
- Dead creatures are filtered out of the list
- Count in title

## Boss Colors

The structured feed also classifies creatures. Boss-tier creatures
(Ascension bosses and mini-bosses) render in `boss_color`, and creatures
flagged "challenging" in `challenging_color`. The current-target color
always wins.

## Configuration

Configure via `config.toml`:

```toml
[target_list]
status_position = "end"         # "end" or "start"
truncation_mode = "noun"        # "full" or "noun"
excluded_nouns = ["arm", "coal"]
boss_color = "#ff5555"          # AscensionBoss / MiniBoss creatures
challenging_color = "#ffaa55"   # "challenging" creatures

[target_list.status_abbrev]
stunned = "stu"
frozen = "frz"
dead = "ded"
```

Statuses without an abbreviation fall back to their first three letters.

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
