# Keybind Actions

Recipes for common keybind tasks. Full format and action reference:
[keybinds.toml](../configuration/keybinds-toml.md). The in-app editor
(`.keybinds` / `.addkeybind`) edits the same file.

Your custom binds go in the `[user]` section, as `"key" = value`.

## Send a Command with One Key

Use `macro_text`; end with `\r` to press Enter:

```toml
[user]
f5 = { macro_text = "stance defensive\r" }
f6 = { macro_text = "look in my backpack\r" }
"ctrl+g" = { macro_text = "group\r" }
```

Omit the `\r` to type text into the input line without sending — useful
for prefixes you finish by hand:

```toml
"ctrl+w" = { macro_text = "whisper Rolfard " }
```

## Numpad Movement

Ships by default; adjust to taste:

```toml
num_8 = { macro_text = "n\r" }
num_2 = { macro_text = "s\r" }
num_5 = { macro_text = "out\r" }
"num_." = { macro_text = "up\r" }
"num_+" = { macro_text = "look\r" }
```

## Rebind a Client Action

```toml
[user]
"ctrl+r" = "send_last_command"
tab = "switch_current_window"
page_up = "scroll_current_window_up_page"
```

## Change the Quit / Search Keys

Application-level keys live in `[app]`:

```toml
[app]
quit = "ctrl+q"          # default is ctrl+c
start_search = "ctrl+f"
```

## Swap Keybind Sets

```
.savekeybinds hunting
.loadkeybinds hunting
.keybindprofiles
```

## When a Key Doesn't Work

Terminals differ in what they send. Run with `RUST_LOG=debug`, press the
key, and look for `KEY EVENT` lines in the log — then bind exactly that
name. Classic case: terminals that send `delete` for backspace.
