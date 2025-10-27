# Colors and Theming

Color customization spans UI chrome, presets for highlights, spell bars, and a full shared palette. This guide explains how to use the in-game editors and how the underlying `colors.toml` file is structured.

## Tools & Popups

- `.colors` – opens the main color browser with tabs for presets, prompts, UI colors, and spell colors.
- `.uicolors` – shortcut directly to the UI color editor.
- `.palette` / `.colorpalette` – manage the shared palette (add/remove/favorite colors).
- `.addcolor` / `.createcolor` – open the palette editor in “add” mode.
- `.addspellcolor` – create a new spell ID range entry.
- `.spellcolors` – browse and edit spell color ranges.

All dialogs follow the popup styling guidelines in `POPUP_STYLE_GUIDE.md`: drag the header to reposition; `Tab` moves between fields; `Enter` saves; `Esc` cancels.

## Color Resolution Pipeline

When any widget requests a color it passes through `Config::resolve_color`:

1. If the value is `"-"`, the color is treated as “transparent/default”.
2. If the string matches a palette entry (`color_palette` name), the associated hex value is returned.
3. Otherwise the string is interpreted as a hex literal (`#RRGGBB`).

This means you can store palette names (e.g., `"bloodred"`) in window definitions, highlights, or preset colors and adjust the palette later without editing every reference.

## `colors.toml` Overview

### Presets

```
[presets.links]
fg = "#477ab3"
```

Presets give highlights and text windows named colors. Refer to them with `{ fg = "links" }` in highlight definitions or by selecting them in the editor.

### Prompt Colors

```
[[prompt_colors]]
character = "R"
fg = "#ff0000"
bg = null
```

Each entry maps a prompt character (e.g., `R`, `S`, `>`) to foreground/background colors. The settings editor exposes these under *Prompts*.

### UI Theme

```
[ui]
text_color = "#ffffff"
background_color = "#000000"
border_color = "#00ffff"
focused_border_color = "#ffff00"
selection_bg_color = "#4a4a4a"
textarea_background = "-"
command_echo_color = "#ffffff"
```

These values color the chrome around every widget and the command input. `textarea_background` controls the background of editable fields inside popups.

### Spell Colors

```
[[spell_colors]]
spells = [905, 911, 919]
bar_color = "#9370db"
text_color = "#909090"
bg_color = "#000000"
```

Each block applies to a list of spell IDs, coloring progress bars in the Spell Active Effects window. The parser consults `Config::get_spell_color` to pick the first matching range.

### Palette

```
[[color_palette]]
name = "deepsea"
color = "#006994"
category = "blue"
favorite = true
```

Palette entries appear inside the color browser with filtering by category (red, blue, green, neutral, etc.). Mark favorites for one-click access.

## In-Game Workflow

1. Run `.colors` and navigate with arrow keys, PageUp/PageDown, or the on-screen instructions.
2. Press `Enter` to edit, `Delete` to remove entries (where supported).
3. Use the color picker to type hex values or choose from palette entries.
4. Spells editor lets you assign new IDs or tweak colors for existing lists.
5. UI colors section updates immediately—watch your layout change while the popup remains open.

## Theming Tips

- Keep palette entries concise (e.g., `skyblue`, `ember`, `obsidian`) and reuse them to ensure consistent styling across highlights and windows.
- Favor high-contrast combinations for readability: the UI applies colors directly to Ratatui `Color::Rgb` values without fallback.
- When using transparent backgrounds, choose terminal themes with solid underlying colors to avoid flickering text.
- Share `colors.toml` along with layouts/highlights to guarantee identical visuals across machines.

## Troubleshooting

- If a color won’t apply, verify that the hex string contains exactly six hex digits and begins with `#`.
- When palette names aren’t recognized, check for duplicates—names are case-insensitive and must be unique.
- Spell color entries are evaluated in order; ensure narrower ranges appear before broader “catch-all” entries.
