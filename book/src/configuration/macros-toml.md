# macros.toml

Macro buttons for the [mobile web frontend](../frontends/web.md). These are
the buttons shown on your phone — the desktop client doesn't use this file
(desktop macros live in [keybinds.toml](./keybinds-toml.md)).

After editing, apply with `.reloadmacros` — connected phones update live.

## Two Button Shapes

**Action button** — has a `command`, fires immediately on tap:

```toml
[[group]]
name = "Basics"

  [[group.button]]
  label = "Look"
  command = "look"
```

**Menu button** — has options instead, tap opens a bottom-sheet picker:

```toml
  [[group.button]]
  label = "Travel"
  color = "#d9b44f"

    [[group.button.option]]
    label = "To the bank"
    command = ";go2 bank"

    [[group.button.option]]
    label = "To the gate"
    command = ";go2 gate"
```

`command` can be anything you could type — including Lich `;scripts`.

## Floating Buttons

Always-visible overlay buttons on the text pane. Tap to fire; hold and drag
to reposition (positions are remembered per device, not in this file):

```toml
[[floating]]
label = "Atk"
color = "#d9534f"
command = ";bigshot"
x = 0.85     # starting position, fraction of screen (0.0-1.0)
y = 0.6
```

## Optional Fields

| Field | Applies to | Description |
|-------|-----------|-------------|
| `color` | button, option | Button face color, `"#rrggbb"` |
| `confirm` | button, option | Ask before sending (for dangerous commands) |

## Groups and the Macro Rail

Each `[[group]]` is a page of buttons in the phone's macro rail; switch
groups from the rail's left-hand button.

## File Locations

- Global: `~/.vellum-fe/global/macros.toml`
- Per-character: `profiles/<name>/macros.toml` — replaces the global file
  **wholesale** (no merging)
- `profiles/<name>/macros-local.toml` — buttons created **on the phone**
  are saved here as an overlay; your hand-written macros.toml is never
  rewritten by the app

Buttons created on the phone are editable from the phone; hand-written
buttons are read-only there.
