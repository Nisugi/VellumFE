# Skins (GUI Graphics)

Skins layer **your own images** on top of the GUI: window backgrounds,
nine-slice window borders, status icon sprites, a sprite compass, and a
sprite injury paperdoll. Themes own colors and fonts; skins own graphics.

Skins apply to the [Desktop GUI](../frontends/gui.md) only — the terminal
has no image pipeline. Without a skin, the GUI uses its built-in vector
graphics and theme colors, and anything a skin doesn't cover (or fails to
load) falls back to that.

## Using Skins

```
.skins            # list installed skins
.setskin parchment
.setskin none     # back to plain theme rendering
```

The active skin is remembered in your config. `.skin` is an alias.

## Making a Skin

A skin is a folder under `~/.vellum-fe/skins/<name>/` containing a
`skin.toml` manifest plus image files (PNG, JPEG, WebP, or BMP):

```
~/.vellum-fe/skins/parchment/
├── skin.toml
└── bg/
    ├── paper.png
    └── vellum.png
```

```toml
[meta]
name = "Parchment"
description = "Warm paper backgrounds for text windows"

# Applies to every window without its own [window.<name>] entry.
[window.default.background]
image = "bg/paper.png"   # relative to the skin folder (absolute paths allowed)
fit = "cover"            # stretch | cover | contain | tile | center
opacity = 0.85           # 0.0-1.0
tint = "#c0a878"         # optional multiply tint
scrim = 0.3              # 0.0-1.0 theme-colored overlay for text readability

# Windows are matched by their layout window name ("main", "thoughts", ...).
[window.main.background]
image = "bg/vellum.png"
scrim = 0.5
```

> **Tip**: `scrim` paints a theme-colored wash over the image so text
> stays readable — start around `0.3` and adjust.

## Window Borders (Nine-Slice)

```toml
[window.main.border]
image = "borders/frame.png"
slice = [12, 12, 12, 12]   # insets in source pixels: top, right, bottom, left
scale = 1.0                # source pixels -> screen points
```

## Status Icons

Replace the built-in vector pictograms in the dashboard and indicator
widgets, keyed by indicator id (case-insensitive):

```toml
[icons]
kneeling = "icons/kneeling.png"
stunned = "icons/stunned.png"
hidden = "icons/hidden.png"
```

## Sprite Compass

A rose image plus one overlay per direction, drawn only while that exit
exists. Author every overlay at the same canvas size as the rose, so
positioning lives in the art:

```toml
[compass]
rose = "compass/rose.png"
n = "compass/n.png"
ne = "compass/ne.png"
# ... e, se, s, sw, w, nw
```

## Sprite Injury Paperdoll

A base body image plus full-canvas overlays per body part and severity.
Parts use the protocol names (`head`, `neck`, `chest`, `abdomen`, `back`,
`leftArm`, `rightArm`, `leftHand`, `rightHand`, `leftLeg`, `rightLeg`,
`leftEye`, `rightEye`, `nsys`), each with `injury1`–`injury3` and
`scar1`–`scar3` entries:

```toml
[injury_doll]
base = "doll/body.png"

[injury_doll.head]
injury1 = "doll/head_i1.png"
injury2 = "doll/head_i2.png"
injury3 = "doll/head_i3.png"
scar1 = "doll/head_s1.png"
```

## Notes

- Absolute image paths are allowed on purpose, so a skin can point at art
  from another install (e.g. your local Wrayth graphics) without copying.
- Every piece is optional — a skin can be nothing but one background.
