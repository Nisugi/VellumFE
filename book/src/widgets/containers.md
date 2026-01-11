# Containers

Displays contents of open containers (bags, backpacks, chests).

## Basic Usage

Container windows are created dynamically when you look in a container.

## Enabling Containers

Enable container windows via command:

```
.containers
```

Or via menu: F1 → Config → Toggle Containers

## Behavior

When enabled:
1. Look in a container (`look in backpack`)
2. A window appears showing contents
3. Window closes when you close/leave the container

## Display

```
┌─ leather backpack ─────────┐
│ a silver ring              │
│ some gold coins            │
│ a healing herb             │
└────────────────────────────┘
```

## Interaction

- Click items to interact
- Right-click for context menu
- Drag items to inventory or other containers

## Manual Container Window

Create a persistent container window:

```toml
[[windows]]
name = "my_bag"
widget_type = "container"
container_title = "backpack"
row = 0
col = 100
rows = 10
cols = 30
```

The `container_title` must match the container name in-game.
