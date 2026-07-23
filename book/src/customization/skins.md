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

The active skin is remembered in your config. `.skin` is an alias. The
GUI settings editor (`.settings`) has a Skin section with the same
picker, an "Open skins folder" button, and a "Create" button.

## Making a Skin

The quickest start:

```
.makeskin myskin
```

This creates `~/.vellum-fe/skins/myskin/skin.toml` with **every section
present but commented out** — uncomment a line, point it at a PNG, done.
It never overwrites an existing skin.

While a skin is active, edits to its `skin.toml` **hot-reload within a
second**. Edited *images* don't touch the manifest, so after swapping an
image file run `.reloadskin` to force a full reload.

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
# ... e, se, s, sw, w, nw — plus optional up / down / out overlays
```

## Sprite Injury Paperdoll

A base body image; wounds and scars render as generated dots on top of it —
a solid circle for wounds, a ring for scars, with the severity rank (1–3)
inside. Parts use the protocol names (`head`, `neck`, `chest`, `abdomen`,
`back`, `leftArm`, `rightArm`, `leftHand`, `rightHand`, `leftLeg`,
`rightLeg`, `leftEye`, `rightEye`, `nsys`).

```toml
[injury_doll]
base = "doll/body.png"
```

### Calibrating dot positions

Where each part's dot lands is set by clicking, not by hand-editing:
**Settings > Appearance > Skin > Calibrate injury doll**. The calibrator
shows your doll art with every part's dot live; click to place the
highlighted part, adjust dot size, opacity, and the wound/scar colors with
the controls below, then **Save to skin**. It writes the
`[injury_doll.anchors]` and `[injury_doll.dots]` tables into the skin's
`skin.toml` (everything else in the file, comments included, is left
alone), so calibration travels with the skin when you share the folder.

Coordinates are stored as fractions of the base image, so any image size
works and a uniform resize never needs recalibrating. Parts you don't
calibrate use sensible built-in defaults. `back` and `nsys` (and usually
the eyes) have no natural spot on a front-view silhouette — the convention
is eyes at the top corners, back bottom-left, nerves bottom-right, but any
click position works; the base art can mark those spots (letters, icons)
or leave them as empty margin.

```toml
# Written by the calibrator — shown here for reference.
[injury_doll.anchors]
head = [0.5, 0.09]
chest = [0.5, 0.3]

[injury_doll.dots]
wound_color = "#e02020"
scar_color = "#b8b8b8"
opacity = 0.9
diameter = 0.07     # fraction of the drawn doll height
```

### Hand-drawn overlays (optional)

A part can instead ship full-canvas overlays per severity, authored on the
same canvas as the base so they stack in place — useful for effects a dot
can't express, like nervous-system damage drawn across the whole body.
Overlays take precedence over the generated dot for that part and
severity; every other part keeps its dot.

```toml
[injury_doll.nsys]
injury1 = "doll/nerves_i1.png"
injury2 = "doll/nerves_i2.png"
injury3 = "doll/nerves_i3.png"
```

## Notes

- Absolute image paths are allowed on purpose, so a skin can point at art
  from another install (e.g. your local Wrayth graphics) without copying.
- Every piece is optional — a skin can be nothing but one background.
