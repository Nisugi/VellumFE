# Containers

Displays contents of open containers (bags, backpacks, chests).

## Basic Usage

Container windows are per-container choices in the **Windows** list.
Every container the game mentions (a `LOOK IN`, opening a bag, the stow
feed) is added to the list automatically; tick its row to open a window
for it, untick to close it again.

Open the list from the menu: **Windows → Show/Hide windows** (TUI popup
menu; on the GUI the same list is a checkbox panel). `.hidecontainers
[title]` closes container windows without changing anything else.

Your choice is remembered for the session: a ticked container's window
re-opens whenever the game mentions it again. Containers are session-only
and are not saved across relogs.

## Behavior

1. Look in a container (`look in backpack`) — it appears in the Windows
   windows list
2. Tick it — a window appears showing contents
3. Contents refresh as the game re-sends them; untick to close

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
