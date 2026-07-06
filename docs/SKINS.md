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

# Nine-slice border image (replaces the plain window stroke).
[window.default.border]
image = "border/brass.png"
slice = [8.0, 8.0, 8.0, 8.0]
scale = 1.0

# Status icon sprites (dashboard + indicator widgets), keyed by
# indicator id (case-insensitive). Replace the built-in pictograms.
[icons]
kneeling = "icons/kneel.png"
stunned = "icons/stunned.png"

# Sprite compass: a rose image plus one overlay per direction, drawn only
# while that exit is available. Author all images on the same canvas —
# overlays are aligned to the rose, so positioning lives in the art.
[compass]
rose = "compass/rose.png"
n = "compass/n_lit.png"
ne = "compass/ne_lit.png"
# ... e, se, s, sw, w, nw — plus optional up / down / out overlays

# Sprite injury doll: base body plus full-canvas overlays per part and
# severity (injury1-3, scar1-3). Parts use the protocol names: head,
# neck, chest, abdomen, back, leftArm, rightArm, leftHand, rightHand,
# leftLeg, rightLeg, leftEye, rightEye, nsys.
[injury_doll]
base = "doll/base.png"

[injury_doll.head]
injury1 = "doll/head_i1.png"
injury2 = "doll/head_i2.png"
scar1 = "doll/head_s1.png"
```

### Background options

| Key | Default | Meaning |
|-----|---------|---------|
| `image` | required | Image path, relative to the skin directory. Absolute paths are allowed on purpose, so a skin can reference assets from another install (e.g. local Wrayth art) without copying them. |
| `fit` | `cover` | `stretch` (fill, distorting), `cover` (fill, cropping overflow), `contain` (whole image, letterboxed), `tile` (repeat at native size), `center` (native size, centered) |
| `opacity` | `1.0` | Image opacity, `0.0`–`1.0` |
| `tint` | none | Multiply tint as `"#rrggbb"` |
| `scrim` | `0.0` | Strength (`0.0`–`1.0`) of a theme-colored overlay painted over the image so window text stays readable. Busy images usually want `0.3`–`0.6`. |

### Border options

Borders use the standard nine-slice (9-patch) technique: `slice` gives
insets in **source-image pixels** as `[top, right, bottom, left]`, splitting
the image into four corners (drawn at fixed size), four edges (stretched
along their axis), and a center (never drawn — the window fill or background
image shows through).

| Key | Default | Meaning |
|-----|---------|---------|
| `image` | required | Border image path (same path rules as backgrounds) |
| `slice` | required | `[top, right, bottom, left]` insets in source pixels |
| `scale` | `1.0` | Multiplier from source pixels to on-screen border thickness |

When a border applies, the window's plain stroke is dropped and its content
margin widens to clear the border art. Background and border fall back to
the `default` entry independently, so a window can override one without
losing the other. Borders currently apply to docked windows; detached
windows keep their OS chrome.

### Widget sprite art

- `[icons]`: sprites draw as-is (aspect-fit) at the dashboard icon size;
  in single indicator widgets they dim when the state is inactive.
  Indicator ids without a sprite fall back to the built-in vector
  pictogram, then to a text label.
- `[compass]`: the rose replaces the vector rose; direction overlays
  (the eight rose directions plus `up`/`down`/`out`) light up per
  available exit. Click regions and tooltips are unchanged: the hub is
  the out exit, and up/down arrows sit beside the rose.
- `[injury_doll]`: overlays stack on the base image in author-canvas
  alignment; hovering shows a summary of current wounds. Without a `base`
  the vector paperdoll renders instead.

### Notes

- Backgrounds follow the window everywhere it renders: docked, in a group,
  or detached into its own OS window.
- A bad image path logs one warning and that window falls back to the plain
  theme background; the rest of the skin still applies.
- Edits to `skin.toml` are picked up by re-activating the skin
  (`.setskin <name>` again) or restarting.
