# Reserve

Displays your reserved items — the GS4 `RESERVE` list — in its own window.
GemStone IV only.

Works exactly like the [Inventory](./inventory.md) window: the game sends
the full list as a snapshot on the `reserve` stream, and the window
replaces its contents whenever the snapshot changes.

## Basic Usage

```toml
[[windows]]
name = "reserve"
widget_type = "reserve"
row = 0
col = 0
rows = 15
cols = 35
```

Or in-app: `.addwindow reserve` (listed under **Other** in the add-window
menu).

## Display

```
┌─ Reserve ──────────────────┐
│ a sprig of wild lilac      │
│ a blue potion              │
└────────────────────────────┘
```

## Interaction

- Click an item to interact
- Right-click for the context menu

## Notes

- No scrollback: the content is replaced on each update, like inventory.
- The streams field is editable in the window editor, but the default
  (`reserve`) is what the game sends.
