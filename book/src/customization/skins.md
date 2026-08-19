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

The active skin is remembered in your GUI layout, so layout checkpoints
carry it (see below). `.skin` is an alias. The GUI settings editor
(`.settings`) has a Skin section with the same picker, an "Open skins
folder" button, and a "Create" button.

## Making a Skin

No art skills needed — [Generating Skin Art with AI](./skin-art-prompts.md)
is a working prompt kit for producing every image below with an image
model, including the keying script that turns black backgrounds into
real transparency.

The quickest start:

```
.makeskin myskin
```

This creates `~/.vellum-fe/global/skins/myskin/skin.toml` with **every
section present but commented out** — uncomment a line, point it at a
PNG, done. It never overwrites an existing skin.

While a skin is active, edits to its `skin.toml` **hot-reload within a
second**. Edited *images* don't touch the manifest, so after swapping an
image file run `.reloadskin` to force a full reload.

A skin is a folder under `~/.vellum-fe/global/skins/<name>/` containing a
`skin.toml` manifest plus image files (PNG, JPEG, WebP, or BMP):

```
~/.vellum-fe/global/skins/parchment/
├── skin.toml
└── bg/
    ├── paper.png
    └── vellum.png
```

Art that several skins share belongs in the image pool at
`~/.vellum-fe/global/images/` (subfolders: `icons/`, `frames/`,
`dolls/`, `compass/`, `backgrounds/`). Relative manifest paths look in
the skin folder first, then the pool, so
`image = "backgrounds/paper.png"` works from any skin without copying
the file. The applied skin is remembered in the GUI layout —
`.savelayout` checkpoints carry their skin, and `.loadlayout` brings it
back (a checkpoint saved without a skin keeps the current one).

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

## Named Frames (Per-Window Picker)

`[window.<name>.border]` bakes the frame choice into the skin. To let
players mix frames themselves, name them under `[frames.*]`:

```toml
[frames.ornate]
image = "borders/ornate.png"
slice = [16, 16, 16, 16]
scale = 0.75

[frames.plain]
image = "borders/plain.png"
slice = [6, 6, 6, 6]
```

Every named frame shows up in each window's right-click menu under
**Appearance → Frame → Skin frame**, alongside **Skin default** (follow
the skin's own
`[window.*]` mapping) and **None** (no frame on this window). The
choice is saved per window in the GUI layout, so it survives restarts
and travels with `.savelayout` checkpoints. If a layout names a frame
the active skin doesn't define, the window falls back to the skin's
default mapping.

The name `none` is reserved — a `[frames.none]` entry is ignored.

## No Skin Required: the Image Pool

Everything above describes skins, but **none of it requires one**. Art
installed into the shared pool (`~/.vellum-fe/global/images/<category>/`,
usually via `.jinx install`) is selectable straight from the GUI:

- **Injury doll** — right-click the injuries window → Appearance →
  *Doll image* lists every doll in the pool; *Calibrate doll…* places the
  wound anchors, saved into the image's sidecar toml so the calibration
  travels with the art (Jinx dolls can ship pre-calibrated). A
  *Grayscale doll art* checkbox desaturates the art (dots keep their
  colors); the grayscale copy is built only while it's checked.
- **Frames** — the per-window *Skin frame* picker lists pool frames
  (those with a slice/scale sidecar) alongside the skin's `[frames.*]`.
- **Backgrounds** — every window's Appearance menu has a *Background*
  picker over `backgrounds/` pool images.
- **Compass** — the compass window's Appearance menu picks a pool set
  (`compass/<set>_<role>.png`, roles `rose`, `n` … `nw`, `up`, `down`,
  `out`).
- **Status icons** — the Indicator Templates editor picks a pool set
  (`statusicons/<set>_<glyph>.png`, glyph = indicator id) plus
  per-indicator overrides: any pool image or a hotbar sheet cell. A
  *Grayscale when inactive* checkbox shows the desaturated icon for
  inactive statuses instead of fading it.

All of these choices live in your GUI layout (checkpoints and
`.uiexport` carry them). When an arrangement is worth sharing,
**`.saveskin <name>`** compiles it into `global/skins/<name>/skin.toml`
referencing the pool art — a publishable skin, without hand-writing any
TOML.

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

A part can instead ship full-canvas overlays per state, authored on the
same canvas as the base so they stack in place — useful for effects a dot
can't express, like nervous-system damage drawn across the whole body.
The state keys are `healthy` (uninjured), `injury1`–`injury3` (wounds),
and `scar1`–`scar3` (scars). The renderer draws the base, then exactly
one overlay per part — the one matching that part's current state.

A part with **any** overlay art is treated as fully hand-drawn: at a
state with no art, nothing is drawn over the base — never a generated
dot. Parts with no overlay art keep their dots, so one doll can mix
hand-drawn limbs with dot-only `back` / `nsys` / eyes.

```toml
[injury_doll.nsys]
injury1 = "doll/nerves_i1.png"
injury2 = "doll/nerves_i2.png"
injury3 = "doll/nerves_i3.png"
```

Because a missing state deliberately reveals the base, two authoring
schemes both work:

- **Worst-case base**: the base is the fully-wounded body, with
  transparent holes where limbs can be severed. Each part's overlays
  paint it back toward health (`healthy`, `injury1`, `injury2`, scars);
  the severe state ships no overlay so the base shows through. Omitting
  a state reveals the hole — a severed limb.
- **Empty base**: the base is empty (or just a backdrop) and every state,
  including the severe one, is its own transparent overlay.

```toml
[injury_doll.leftArm]
healthy = "doll/arm_ok.png"
injury1 = "doll/arm_i1.png"
injury2 = "doll/arm_i2.png"
# injury3 omitted -> the base shows at rank 3 (e.g. a severed arm)
scar1 = "doll/arm_s1.png"
scar2 = "doll/arm_s2.png"
scar3 = "doll/arm_s3.png"
```

### Suppressing a part conditionally

A part can be skipped entirely — no overlay, no dot, at any severity —
while a condition holds, using the same condition vocabulary as
variants. This encodes anatomical dependencies in the skin: a severed
arm's base hole would otherwise leave a healthy hand floating next to
the stump.

```toml
[injury_doll.leftHand]
hidden_when = { type = "injury", area = "leftArm", cmp = ">=", level = 3 }
healthy = "doll/leftHand_ok.png"
injury1 = "doll/leftHand_i1.png"
```

While `leftArm` is at rank 3, `leftHand` draws nothing; its wound (if
any) still lists in the hover tooltip. Each doll set — the default and
every variant — carries its own `hidden_when` rules.

### Doll variants (optional)

Named alternate dolls selected by game state — for example a body on the
ground when the character is prone or has lost both legs. Each variant
is a **complete replacement doll**: its own base, anchors, dot styling,
and overlays. That's deliberate — a prone body repositions every limb,
so inheriting standing anchors would misplace every wound.

Variants are checked in the order they appear; the first whose `when`
condition matches wins, and none matching falls back to the default
`[injury_doll]` set. Conditions use the same vocabulary as hotbar
button states: `indicator` (prone, kneeling, sitting, dead, …),
`injury` (`area` / `cmp` / `level`), and `all` / `any` nesting.

```toml
[injury_doll]
base = "doll/standing.png"        # default when no variant matches

[[injury_doll.variants]]
name = "downed"
[injury_doll.variants.when]
type = "any"
conditions = [
  { type = "indicator", id = "prone", active = true },
  { type = "all", conditions = [
      { type = "injury", area = "leftLeg",  cmp = ">=", level = 3 },
      { type = "injury", area = "rightLeg", cmp = ">=", level = 3 } ] },
]
[injury_doll.variants.skin]
base = "doll/downed.png"
[injury_doll.variants.skin.anchors]
head = [0.2, 0.7]
[injury_doll.variants.skin.leftArm]
healthy = "doll/downed_arm_ok.png"
```

Variants apply to your own doll only — another player's injuries popup
always shows the default set, since the conditions read your character's
state. Doll images picked from the pool (right-click → Appearance →
*Doll image*) carry no overlays or variants; these features live in a
skin's `[injury_doll]` section.

Each variant calibrates separately: the calibrator's **Doll set** picker
switches between the default doll and each variant (a prone body puts
every anchor somewhere new), and Save writes into that variant's own
tables inside `[[injury_doll.variants]]`. The phone client follows along
automatically — the host resolves which variant and which suppressed
parts are active and pushes them to the phone, which just switches art.

### Named doll sets (optional)

Standalone doll sets bound **by name from a window** rather than by
condition — so two injury doll windows can render different art from the
same wound data (say a detailed anatomical doll for reference plus a
compact silhouette in a sidebar). Each set has the same shape as a
variant's `skin` table: base, anchors, dot styling, per-part overlays,
and `hidden_when` rules.

```toml
[injury_doll.sets.silhouette]
base = "doll/silhouette.png"
[injury_doll.sets.silhouette.anchors]
head = [0.5, 0.08]
[injury_doll.sets.silhouette.leftArm]
healthy = "doll/silhouette_arm_ok.png"
```

Bind a window to a set from its right-click menu (**Appearance → Doll
set**), which writes `doll_set = "silhouette"` onto that window's
layout.toml entry. A bound window's art is **pinned**: condition
variants don't swap it (its own `hidden_when` rules still apply), and
the global *Doll image* pool override doesn't touch it either — the
override replaces only the default doll. Windows without a binding keep
the current behavior exactly. The picker also accepts a **variant's**
name, which pins that variant's art regardless of its condition. A
binding the active skin doesn't name falls back to the default doll
until the set exists again.

## Notes

- Absolute image paths are allowed on purpose, so a skin can point at art
  from another install (e.g. your local Wrayth graphics) without copying.
- Every piece is optional — a skin can be nothing but one background.
