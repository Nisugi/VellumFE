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
fg = "#ff0000"    # bg = "..." also supported ("color" is a legacy alias for fg)
```

Defaults cover `R` (roundtime), `S` (stunned), `H` (hiding), `>` (prompt),
`!` (bleeding).

## UI Colors

Default colors for UI elements. Edit with `.uicolors`:

```toml
[ui]
command_echo_color = "#ffffff"
border_color = "#00ffff"
focused_border_color = "#ffff00"
text_color = "#ffffff"
background_color = "#000000"
selection_bg_color = "#4a4a4a"
textarea_background = "-"       # "-" or unchanged = fall through to theme
```

These sit in the middle of the window color chain: a per-window color in
layout.toml wins, then a `[ui]` value **you have changed** applies, and
anything left at its default (or set to `"-"`) falls through to the
active theme.

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

## Harmony Recipe

Generating preset colors with `.harmony` (or the GUI Colors editor's
Generate tab) also stores the generation recipe, so the look stays
reproducible and re-tunable instead of frozen as opaque hex:

```toml
[harmony]
seed = "#bf616a"        # theme swatch the set was seeded from
background = "#2e3440"  # theme background it was generated against
scheme = "triadic"      # color-theory scheme (.harmony schemes lists them)
variance = 1.0          # hue spread: 0.7 low / 1.0 medium / 1.4 high
min_contrast = 4.5      # WCAG floor vs background: 3.0 / 4.5 / 7.0
separation = 0.09       # min perceptual distance between roles
room_title_spread = 2.5 # room title vs its background plate: 2.5 / 7.0

[harmony.pins]          # roles held verbatim while the rest regenerate
speech = "#53a684"
```

The recipe is written for you; there is no need to hand-edit it. It is
ignored (and re-seeded) once the active theme's background changes.
