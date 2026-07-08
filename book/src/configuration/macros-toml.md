# macros.toml

Macro buttons for the [mobile web frontend](../frontends/web.md). These are
the buttons shown on your phone — the desktop client doesn't use this file
(desktop macros live in [keybinds.toml](./keybinds-toml.md)).

You never *need* to edit this file: everything here can be created and
edited from the phone itself (the ＋ button on the macro rail). This page
documents the format for hand-authoring, bulk edits, and version control.

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

## Type-in Buttons (Composing Commands)

Either shape can set `insert = true`: instead of sending, the tap **types
the text into the command input**, so a tray of word buttons composes
phrases tap by tap. Spacing is automatic (`go` + `second` → `go second`).
A trailing `\r` means "then press Send" — it submits the whole composed
line, which makes a good finisher button:

```toml
[[group]]
name = "Words"

  [[group.button]]
  label = "go"
  command = "go"
  insert = true

  [[group.button]]
  label = "second"
  command = "second"
  insert = true

  # tapping this sends e.g. "go second door" in one go
  [[group.button]]
  label = "door"
  command = "door\r"
  insert = true
```

In TOML basic strings `"door\r"` is a real carriage return — write it
exactly like that. In the phone editor you never type `\r`: the **On
tap** picker offers *Send the command*, *Type into input*, and *Type,
then send*.

Type-in taps are handled entirely on the phone (the text never rounds
through the server as a command), so composing is instant.

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
| `insert` | button, option | Type into the command input instead of sending; trailing `\r` submits |

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
