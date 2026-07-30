# Generating Skin Art with AI

Every image slot a [skin](./skins.md) can fill — window backgrounds,
nine-slice frames, status icons, hotbar icon sheets, the compass, the
injury doll — can be produced with an image model like Gemini in an
afternoon, without drawing anything. This chapter is a working prompt
kit: the technique, per-asset prompts, and the post-processing that
turns raw generations into game-ready PNGs.

The prompts below produce a dark-fantasy hand-painted look; swap the
style language for your own. The *technique* — style anchor, black
keying, batching, image conditioning — applies to any style.

## The three rules

1. **Anchor style with an image, not adjectives.** Generate one image
   you love, then attach it to every later request ("in the exact style
   of the attached reference"). Consistency comes from image
   conditioning; repeating adjectives drifts.
2. **Never ask for a transparent background.** Image models cannot emit
   an alpha channel — they ignore you or paint a fake checkerboard. Ask
   for a **solid pure black background** and key it out afterward (see
   [Post-processing](#post-processing-dekeypy)).
3. **Reject gloss immediately.** Shine creeps back with each generation
   unless the NO-gloss lines stay in the prompt (for matte styles).

## Master style block

Prepend something like this to every prompt, with your style anchor
attached:

> Art direction: dark-fantasy medieval game asset in the exact style of
> the attached reference image — hand-painted illustration with clean
> dark ink outlines, muted desaturated earth tones (olive greens, worn
> browns, iron greys, dull brass), weathered matte surfaces. Absolutely
> NO gloss, NO shiny highlights, NO glow, NO bloom, NO lens flare, NO
> neon or saturated colors. Lighting is flat and diffuse. Solid PURE
> BLACK background — no scenery, no ground shadow, no vignette, no
> gradient. No text, no watermark, no border.

Models approximate canvas sizes; aspect ratio is what they honor. The
skin loader scales everything and doll anchors are fractional, so exact
pixel dimensions never matter — internal consistency is everything.

## Compass (12 images, one shared canvas)

The widget stacks per-direction overlays on the rose, so registration
matters more than beauty. Generate two "master" states; cut the 11
overlays from the all-lit master in an editor (models cannot guarantee
pixel registration across separate generations — don't fight it).

**Prompt A — the rose (base, everything unlit):**

> A square compass rose for a fantasy game UI, viewed flat from directly
> above. An aged dark-iron ring with a worn brass center hub. Eight
> directional pointers (N, NE, E, SE, S, SW, W, NW) as tarnished
> arrowheads radiating from the hub, plus a small UP chevron at the top
> inside the ring and a small DOWN chevron at the bottom inside the
> ring. ALL pointers, both chevrons, and the hub are in a dormant, unlit
> state: dark gunmetal, barely distinguishable from the ring. Square
> canvas, composition perfectly centered and symmetrical.

**Prompt B — all-lit master (same composition):**

> The IDENTICAL compass rose, same canvas, same composition, same
> camera — but every pointer, both chevrons, and the center hub are now
> in an active state: dull ember-copper with a faint warm edge, still
> matte, like heated iron cooling — NOT glowing, NOT shiny.

Slice each lit pointer out of B onto a transparent canvas → `n.png`,
`ne.png` … `nw.png`, `up.png`, `down.png`, `out.png` (the hub). Rose
from A → `rose.png`. If a generation drifts, regenerate B by attaching A
and asking for "the same image with all pointers lit."

Manifest section: `[compass]` — see [Skins](./skins.md). Pool folder:
`~/.vellum-fe/global/images/compass/`.

## Status icons (16 glyphs, one sprite sheet)

One sheet keeps the style uniform; slice into individual PNGs after.

> A 4x4 grid sprite sheet of sixteen square fantasy game status icons,
> uniform in style, stroke weight, and visual density, each a single
> bold pictogram readable at 20 pixels. Flat matte colors from the
> reference palette, one restrained accent color where meaning demands
> it. Cells in row-major order:
> 1. an open left hand, palm out
> 2. an open right hand, palm out
> 3. a hand with a subtle rune circle above the palm (spellcasting hand)
> 4. a standing figure
> 5. a kneeling figure
> 6. a sitting cross-legged figure
> 7. a figure lying prone
> 8. a skull (dead)
> 9. stars circling a tilted head (stunned)
> 10. a falling blood drop (bleeding), dull crimson accent
> 11. a hooded figure half-dissolved into shadow (hidden)
> 12. a dotted outline of a figure (invisible)
> 13. a figure wrapped in spiderweb strands (webbed)
> 14. a serpent coiled around a drop (poisoned), dull green accent
> 15. a gaunt face with sunken cheeks (diseased), pale ochre accent
> 16. two linked shackle rings (group-joined)

Slice to `lefthand.png`, `righthand.png`, `spellhand.png`,
`standing.png`, `kneeling.png`, `sitting.png`, `prone.png`, `dead.png`,
`stunned.png`, `bleeding.png`, `hidden.png`, `invisible.png`,
`webbed.png`, `poisoned.png`, `diseased.png`, `joined.png` — these names
match the `[icons]` manifest keys.

## Injury dolls (one base image per race/class)

The app draws wounds itself as dots at calibrated anchor points, so the
art must expose every anchored body part. The requirements below are
non-negotiable — bake them into the prompt. Swap the {RACE} / {CLASS}
slots (GemStone IV races: Human, Giantman, Dwarf, Halfling, Elf, Dark
Elf, Half-Elf, Sylvankind, Forest Gnome, Burghal Gnome, Half-Krolvin,
Erithian, Aelotoi).

> A full-body character portrait of a {RACE} {CLASS} from a fantasy
> world, in the exact painted style, palette, framing, and proportional
> scale of the attached reference image. Requirements, all mandatory:
> - standing straight, facing the viewer directly, perfectly front-on
> - arms relaxed and held slightly away from the torso so upper arms,
>   forearms, and both open hands are fully visible and do not overlap
>   the body
> - legs slightly apart, both feet visible
> - head bare or with headwear that leaves the face open: BOTH eyes
>   must be clearly visible, open, and unobstructed
> - neck visible between head and collar
> - the full figure fits inside the frame with a small margin — nothing
>   cropped
> - portrait orientation, roughly 3:4
> - muted, weathered, matte clothing appropriate to a {CLASS}; no shiny
>   armor, no glowing effects, no magic auras
> - solid pure black background, no ground shadow

After dropping one into a skin, run Settings → Skin → "Calibrate injury
doll" and click through the parts — anchors are stored as fractions of
the image, so exact pixel size never matters. Generate dolls on demand
for characters that exist; the calibrator makes each new doll a
two-minute job. The greyscale `_grey.png` variant from post-processing
makes colored wound dots pop.

## Window frames (nine-slice borders)

The renderer slices the image into nine regions from the manifest's
`slice` insets: corners draw at fixed size, the four straight runs
stretch along one axis, and the center is never drawn. That dictates
the art:

- The frame must run **flush to all four canvas edges**, with a roughly
  **uniform band thickness** (target ~1/8 of the canvas width).
- Corner ornament is free — it draws unstretched. The straight runs
  between corners must be **continuous, uniform material** (riveted
  iron, wood grain along the run, plain rope) with no medallions or
  creatures mid-run: anything distinct there gets stretched.
- Corner caps **thicker than the runs are fine** — real generations
  come back that way and it looks good. Set `slice` to the *cap* size;
  the black between the runs' inner edge and the cap depth becomes
  transparent in post and simply never draws.
- Inspect the inner corners at full zoom before accepting — models like
  to drop small "glint" artifacts there, and anything inside the corner
  squares gets drawn over window content. A clean corner can be
  mirrored over a blemished one in an editor (the bands line up by
  construction).

**Prompt:**

> A square ornamental window frame for a fantasy game UI, viewed flat.
> An aged dark-iron band with worn brass corner caps, riveted along its
> length. The frame runs flush to all four edges of the canvas with a
> uniform thickness on all sides, about one eighth of the canvas wide.
> Corner ornamentation stays within the corner squares; the four
> straight runs between corners are plain, continuous, evenly textured
> metal with no emblems or breaks. The inner edge of the frame is a
> crisp straight line. The center of the frame is empty pure black.

Frames need the keying pass with a twist: the center is a sealed pocket
the edge flood can't reach, so seed it explicitly and use a low
threshold so near-black seams in the art survive:

```
python dekey.py frame.png --seed 1024,1024 --threshold 12 --no-grey
```

Worked example (a 2048px generation with ~307px caps and ~240px runs):

```toml
[window.default.border]
image = "frames/frame_iron.png"
slice = [310.0, 310.0, 310.0, 310.0]  # measure the CAPS in source pixels
scale = 0.045                         # 2048px source -> ~14pt caps on screen
```

Measure the caps once in an editor, set `slice` to that, then tune
`scale` by eye. Pool folder: `~/.vellum-fe/global/images/frames/`.

## Window background textures

Backgrounds sit **behind text**, so restraint is the whole game: low
contrast, no focal points, no vignette (windows crop the image at
arbitrary aspect ratios with `fit = "cover"`, so nothing about the
composition may matter). The manifest's `scrim` paints a theme-colored
wash on top for readability — author the texture legible-ish and let
scrim do the rest.

> A flat, even, borderless surface texture for a game UI background:
> {aged parchment with faint fiber flecks | worn dark leather with fine
> grain | rough hewn dark stone}. Perfectly uniform lighting across the
> whole canvas — no vignette, no hotspot, no directional shadow, no
> focal detail anywhere. Very low contrast, subtle texture only.
> Landscape orientation.

Don't ask for "seamless/tileable" — image models can't actually do it
and the seam will show. Use `fit = "cover"` and generate generously
sized. Backgrounds are opaque by design — no keying pass.

```toml
[window.default.background]
image = "backgrounds/leather.png"
fit = "cover"
opacity = 1.0
scrim = 0.35
```

## Hotbar icon sheets

Hotbar buttons pull from `[sheets]` sprite sheets: square cells, **no
padding**, indexed 1-based left→right then top→bottom (64px cells by
default). Models keep style coherent across about 16 icons per
generation, so build sheets from 4x4 grids — reuse the status-icon
prompt frame with new cell contents (ability and spell pictograms).

After acceptance, resample the image so cells land exactly on the cell
size (a 4x4 sheet → 256x256 for 64px cells) — the loader tiles by pixel
arithmetic, and drifted cell boundaries bleed neighbors into buttons.
Keep a note of which cell is which when you slice; the index is the
only key.

```toml
[sheets.combat]
path = "icons/combat.png"
cell = 64
```

## Post-processing (dekey.py)

Every keyed image (compass, icons, dolls, frames — not backgrounds)
goes through the same script to turn the black background into a real
alpha channel. It flood-fills from the image edges, so black ink lines
and dark clothing *inside* a figure survive; only background connected
to the border becomes transparent. It also emits a greyscale twin
derived from the same pixels (never generate a greyscale variant
separately — it will not match).

```
python dekey.py TheImage.png                  # -> _alpha.png + _grey.png
python dekey.py TheImage.png --threshold 40   # if a dark halo remains
python dekey.py frame.png --seed 1024,1024 --threshold 12 --no-grey
```

Save this as `dekey.py` (needs Python with `Pillow`:
`pip install Pillow`):

```python
#!/usr/bin/env python3
"""Give AI-generated skin art a REAL alpha channel."""

import argparse
import sys
from collections import deque
from pathlib import Path

from PIL import Image, ImageOps


def dekey(img, threshold, seeds=()):
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()

    def is_bg(x, y):
        r, g, b, _ = px[x, y]
        return r <= threshold and g <= threshold and b <= threshold

    seen = bytearray(w * h)
    queue = deque()
    for x in range(w):
        for y in (0, h - 1):
            if is_bg(x, y) and not seen[y * w + x]:
                seen[y * w + x] = 1
                queue.append((x, y))
    for y in range(h):
        for x in (0, w - 1):
            if is_bg(x, y) and not seen[y * w + x]:
                seen[y * w + x] = 1
                queue.append((x, y))
    for x, y in seeds:
        if not (0 <= x < w and 0 <= y < h):
            raise SystemExit(f"seed {x},{y} outside {w}x{h} image")
        if not is_bg(x, y):
            raise SystemExit(f"seed {x},{y} is not background")
        if not seen[y * w + x]:
            seen[y * w + x] = 1
            queue.append((x, y))

    while queue:
        x, y = queue.popleft()
        px[x, y] = (0, 0, 0, 0)
        for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
            if 0 <= nx < w and 0 <= ny < h and not seen[ny * w + nx] and is_bg(nx, ny):
                seen[ny * w + nx] = 1
                queue.append((nx, ny))
    return img


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("input", type=Path)
    ap.add_argument("--threshold", type=int, default=30,
                    help="max RGB channel value treated as background")
    ap.add_argument("--no-grey", action="store_true", help="skip greyscale variant")
    ap.add_argument("--seed", action="append", default=[], metavar="X,Y",
                    help="extra interior flood seed (repeatable); needed for "
                         "closed frames whose center the edge flood cannot reach")
    args = ap.parse_args()

    seeds = []
    for spec in args.seed:
        try:
            x, y = (int(part) for part in spec.split(","))
        except ValueError:
            ap.error(f"--seed wants X,Y integers, got {spec!r}")
        seeds.append((x, y))

    img = dekey(Image.open(args.input), args.threshold, seeds)
    out_alpha = args.input.with_name(args.input.stem + "_alpha.png")
    img.save(out_alpha)
    print(f"wrote {out_alpha}")

    if not args.no_grey:
        grey = ImageOps.grayscale(img).convert("RGBA")
        grey.putalpha(img.getchannel("A"))
        out_grey = args.input.with_name(args.input.stem + "_grey.png")
        grey.save(out_grey)
        print(f"wrote {out_grey}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

## Workflow notes

- Always attach the previous batch's accepted output as a reference
  when generating siblings.
- If a result comes back with scenery or a non-black background, ask to
  "re-render the same image on a solid pure black background" rather
  than regenerating fresh.
- Not every widget is skinnable: progress bars, countdown timers, the
  command input, title bars, and scroll bars have no manifest slots
  today. Check [Skins](./skins.md) for the current surface before
  generating art for something.
