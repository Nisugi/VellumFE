# GUI Skins

Skins add user-supplied graphics on top of the GUI frontend. The split of
responsibilities is:

- **Themes** own colors and fonts (`.settheme`, theme editor).
- **Skins** own graphics: window background images today; border nine-slices
  and icon sets (compass, injury doll, dashboard) in later phases.

Skins are GUI-only. The TUI ignores them. When no skin is active — or an
asset fails to load — windows render exactly as before, using theme colors,
so nothing about accessibility or text-only setups changes.

## Installing a skin

A skin is a directory under `~/.vellum-fe/skins/` (or `$VELLUM_FE_DIR/skins/`)
containing a `skin.toml` manifest plus image assets:

```
~/.vellum-fe/skins/
└── parchment/
    ├── skin.toml
    └── bg/
        ├── paper.png
        └── vellum.png
```

Supported image formats: PNG, JPEG, WebP, BMP.

## Commands

| Command | Effect |
|---------|--------|
| `.skins` | List installed skins |
| `.setskin <name>` | Activate a skin (saved to config) |
| `.setskin none` | Disable the active skin |
| `.skin <name>` | Alias for `.setskin` |

The active skin is stored as `active_skin` in `config.toml`.

## Manifest format (`skin.toml`)

```toml
[meta]
name = "Parchment"
description = "Warm paper backgrounds for text windows"

# Applies to every window without its own [window.<name>] entry.
# Omit it to skin only specific windows.
[window.default.background]
image = "bg/paper.png"
fit = "cover"
opacity = 0.85
tint = "#c0a878"
scrim = 0.3

# Windows are matched by their layout window name ("main", "thoughts",
# "combat", ...) — the same names used in layout.toml and .addwindow.
[window.main.background]
image = "bg/vellum.png"
scrim = 0.5
```

### Background options

| Key | Default | Meaning |
|-----|---------|---------|
| `image` | required | Image path, relative to the skin directory. Absolute paths are allowed on purpose, so a skin can reference assets from another install (e.g. local Wrayth art) without copying them. |
| `fit` | `cover` | `stretch` (fill, distorting), `cover` (fill, cropping overflow), `contain` (whole image, letterboxed), `tile` (repeat at native size), `center` (native size, centered) |
| `opacity` | `1.0` | Image opacity, `0.0`–`1.0` |
| `tint` | none | Multiply tint as `"#rrggbb"` |
| `scrim` | `0.0` | Strength (`0.0`–`1.0`) of a theme-colored overlay painted over the image so window text stays readable. Busy images usually want `0.3`–`0.6`. |

### Notes

- Backgrounds follow the window everywhere it renders: docked, in a group,
  or detached into its own OS window.
- A bad image path logs one warning and that window falls back to the plain
  theme background; the rest of the skin still applies.
- Edits to `skin.toml` are picked up by re-activating the skin
  (`.setskin <name>` again) or restarting.
