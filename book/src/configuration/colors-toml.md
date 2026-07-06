# colors.toml

The central color file: a named color palette, game-text stream colors,
prompt colors, UI element colors, and spell-circle colors. Edit in-app with
`.colors` (palette), `.uicolors`, and `.spellcolors` — or edit the file and
`.reload colors`.

> Themes are a separate system — they control *widget UI* colors and are
> covered in [Themes](../customization/themes.md). colors.toml controls
> *game text* and palette colors, and is not switched by `.settheme`.

## Color Palette

Named colors you can reference anywhere a color is accepted
(highlights `fg`/`bg`, layout `border_color`, theme files, ...):

```toml
[[color_palette]]
name = "Link"
color = "#477ab3"
category = "presets"
slot = 16
```

- `name` — the name you reference elsewhere (case-insensitive)
- `color` — hex value
- `category` — grouping in the `.colors` browser
- `slot` — optional terminal palette slot (16–231) for 256-color mode

In `color_mode = "slot"` (see [config.toml](./config-toml.md)), run
`.setpalette` to load every slotted color into your terminal's palette, and
`.resetpalette` to undo it. The default palette pre-loads color sets for the
built-in themes so theme switching works instantly in slot mode.

## Stream Presets

Colors for game text streams. Values can be hex or palette names:

```toml
[presets.speech]
fg = "Speech"

[presets.roomName]
fg = "Room Name"
bg = "Room Name BG"

[presets.monsterbold]
fg = "Monsterbold"
```

Available presets include `links`, `commands`, `speech`, `whisper`,
`thought`, `roomName`, `monsterbold`, `familiar`, `voln`, `percWindow`,
and `target_indicator`.

## Prompt Colors

Color individual prompt status characters:

```toml
[[prompt_colors]]
character = "R"   # roundtime
color = "#ff0000"
```

Defaults cover `R` (roundtime), `S` (stunned), `H` (hiding), `>` (prompt),
`!` (bleeding).

## UI Colors

Default colors for UI elements (per-window overrides live in layout.toml).
Edit with `.uicolors`:

```toml
[ui]
command_echo_color = "#ffffff"
border_color = "#00ffff"
focused_border_color = "#ffff00"
text_color = "#ffffff"
background_color = "#000000"
selection_bg_color = "#4a4a4a"
```

## Spell Colors

Color active-spell indicators by spell circle. Edit with `.spellcolors`,
add with `.addspellcolor`:

```toml
[[spell_colors]]
spells = [601, 602, 604, 605, 606]   # spell numbers
color = "#1c731c"                    # indicator color
bar_color = "#1c731c"                # progress bar color
text_color = "#909090"
bg_color = "#000000"
```

## Color Values

Anywhere a color is accepted:

```toml
color1 = "#RRGGBB"      # hex (6-digit)
color2 = "#abc"         # hex (3-digit, expanded)
color3 = "Link Blue"    # palette color name from [[color_palette]]
```
