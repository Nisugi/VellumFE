# Themes

Themes control the client's UI colors — windows, menus, editors, buttons —
as one switchable unit. They work in both the TUI and GUI.

## Switching Themes

```
.themes            # browse and preview all themes
.settheme nord     # switch directly by name
```

The active theme is saved to your config (`active_theme`) and restored on
launch.

## Built-in Themes

35+ presets, including:

- **Classics**: `dark` (default), `light`, `nord`, `dracula`,
  `solarized-dark`, `solarized-light`, `monokai`, `gruvbox-dark`,
  `catppuccin`, `night-owl`
- **Flavors**: `cyberpunk`, `synthwave`, `retro-terminal`,
  `ocean-depths`, `forest-canopy`, `sunset-boulevard`, `arctic-night`,
  `sepia-parchment`, `cherry-blossom`, `slate-professional`, and more
- **Accessibility**: `high-contrast-dark`, `high-contrast-light`,
  `deuteranopia`, `protanopia`, `tritanopia`, `monochrome`,
  `low-blue-light`, `photophobia`, `adhd-focus`, `reduced-motion`

## Custom Themes

```
.edittheme         # edit the current theme in-app
```

Saving writes a TOML file to `~/.vellum-fe/themes/<name>.toml` and makes it
active. A custom theme with the same name as a built-in overrides it.

Theme files are flat tables of color fields — hex values or
[palette color names](../configuration/colors-toml.md):

```toml
name = "my-theme"
description = "My tweaked dark theme"
window_background = "#1a1b26"
text_primary = "#c0caf5"
link_color = "Link"        # palette name from colors.toml
# ... every color field must be present — a file with missing
# fields is skipped when themes are loaded
```

Hand-written theme files must define **all** color fields; there is no
per-field fallback. The easiest (and recommended) way to author one is
`.edittheme` on a built-in you like — it writes a complete file — then
save under a new name.

## Themes vs. colors.toml

Two separate systems:

| | Themes | colors.toml |
|--|--------|-------------|
| Controls | Widget/UI colors (windows, menus, editors) | Game text colors (speech, links, monsterbold), spell colors, terminal palette |
| Switch | `.settheme`, instant | Edited in place (`.uicolors`, `.spellcolors`, `.colors`) |

Switching a theme does not change your game-text colors, and vice versa.

For image-based decoration in the GUI (background art, icon sprites),
see [Skins](./skins.md) — themes own colors, skins own graphics.
