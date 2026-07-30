# Submitting Art to the Community Repo

Made a nice injury doll, window frame, or icon sheet? The community
asset repository accepts submissions from anyone with a GitHub account —
no git knowledge required. You fill out a short form, attach your image,
and a robot does the rest: it cleans the image up, checks the name isn't
taken, and stages it for a quick human review. Once approved, it's live
for every VellumFE user.

The whole thing runs on GitHub's issue system:

**[Submit art here → github.com/Nisugi/vellum-assets/issues/new/choose](https://github.com/Nisugi/vellum-assets/issues/new/choose)**

## What can be submitted

Each kind of art has its own form, which asks only for what that kind
needs:

| Form | What it is | One submission is… |
|------|------------|--------------------|
| **Injury doll** | a full-body character image for the injury display | one image |
| **Hotbar icon sheet** | a grid of square ability icons for hotbar buttons | one sheet image |
| **Window frame** | an ornamental nine-slice border drawn around windows | one image |
| **Window background** | a flat texture that sits behind window text | one image |
| **Compass art** | a compass rose and/or its lit direction overlays | a zip of images |
| **Status icons** | the glyph set for stance and conditions | a zip of images |
| **Hand icons** | left, right, and spell hand icons for the hands widget | a zip of images |

Full skins (complete graphics themes) and layouts aren't submitted
through these forms — they're folders rather than single images, and go
in by pull request instead; see the
[contributor notes in the asset repository](https://github.com/Nisugi/vellum-assets/blob/main/SUBMISSIONS.md).

## The black-background trick

Game art needs transparency — a doll or a frame has to sit on top of
whatever is behind it. But AI image generators *cannot* produce
transparency; ask for it and you'll get a fake checkerboard. So the
pipeline uses a convention borrowed from film: **paint your art on a
solid pure-black background, and the submission robot turns the black
into real transparency for you.**

It's smarter than "delete everything black." The robot flood-fills
inward from the edges of the canvas, so only background *connected to
the border* is removed — black ink outlines, dark hair, and iron armor
inside your artwork are untouched. (For window frames it also clears
the enclosed center pocket, and treads gently around near-black
metalwork.)

Two consequences worth knowing:

- If your image **already has real transparency** — you cleaned it up
  yourself in an editor — the robot detects that and leaves it alone.
  (Detection requires meaningful transparency; a few stray transparent
  pixels from an export won't disable keying. Filling in the threshold
  field always forces a re-key either way.)
- **Don't submit greyscale variants** of anything. VellumFE desaturates
  art at runtime when it needs to (the injury doll base, dormant
  states), so only the color version is ever stored.

Window backgrounds are the one exception to all of this: they must be
**fully opaque**, and transparency in one gets a submission rejected.

### If the cleanup comes out wrong

The pull request the robot opens shows your processed images — look at
them. Two failure modes, one knob:

- a **dark halo** clinging around your art means the cutoff was too
  strict — edit your issue and set **Background threshold (advanced)**
  to something higher, like `40`;
- **dark details getting erased** where they touch the background means
  it was too loose — set it lower, like `15`.

Every edit to the issue re-runs the robot and updates the pull request
in place, so you can tune until it looks right. Leave the field blank
to use the defaults, which are right for most art.

For art direction — style, palette, and ready-to-use generation prompts
that produce correctly black-keyed images — see
[Generating Skin Art with AI](./skin-art-prompts.md).

## Names

Every asset needs a name: lowercase letters, numbers, `_` and `-` only
(the form explains this too). Names are **first come, first served** —
if `elf_wizard` is taken in the dolls category, the robot will tell you
and you just pick another.

For the two zip categories, files inside your zip must be named for
their **role** so the client knows what each image is:

- **Compass**: `rose` (the base), `n` `ne` `e` `se` `s` `sw` `w` `nw`
  (lit pointers), `up` `down` (chevrons), `out` (the hub)
- **Status icons**: `standing` `kneeling` `sitting` `prone` `dead`
  `stunned` `bleeding` `hidden` `invisible` `webbed` `poisoned`
  `diseased` `joined`
- **Hand icons**: `lefthand` `righthand` `spellhand`

Name each file either `<role>.png` or `<anything>_<role>.png` — both
work. A partial set is fine (just a rose, say). Your set name from the
form becomes the prefix, so `emberiron` + `rose.png` is published as
`emberiron_rose.png`.

## What happens after you click Submit

1. Within a minute or two, the robot processes your submission and
   comments on your issue with the result.
2. **Accepted** — it opens a pull request with your finished, cleaned-up
   files. A maintainer eyeballs the art (that's the whole review) and
   merges. The site rebuilds itself, and your asset is live in the
   gallery and in everyone's `.jinx list`, credited to you.
3. **Rejected** — the comment says exactly what's wrong: name taken,
   file the wrong shape, no attachment found, a zip filename that
   doesn't match a role. **Edit your issue to fix it** — don't open a
   new one — and the robot automatically re-runs on the edit.

Nothing is ever published automatically; a person approves every
submission before it goes live.

## Per-category fine print

- **Injury dolls** — front-facing, standing straight, arms slightly away
  from the torso, both hands, both feet, and both eyes visible, full
  figure in frame, roughly 3:4 portrait. The
  [prompt kit](./skin-art-prompts.md) bakes all of this in.
- **Hotbar icon sheets** — square cells, no padding between them, and
  the form asks for your **cell size** in pixels. Cell boundaries must
  land exactly on multiples of that size, or buttons will bleed into
  their neighbors. Sheets are used as-is — no background removal.
- **Window frames** — uniform band thickness, ornament confined to the
  corners, continuous plain material along the straight runs (they get
  stretched), pure-black center. The renderer slices the frame at the
  **corner cap size** — the smallest square, anchored at a canvas
  corner, that contains the corner ornament. **The robot measures this
  automatically** from your image; the number it found is shown in the
  pull request. If it looks wrong, measure the caps yourself in an
  editor and put the pixel value in the form's cap-size field to
  override.
- **Compass overlays** — the lit pointers are stacked on top of the rose
  at runtime, so they must be registered pixel-perfect against it.
  Generate them from one master image; don't generate each separately.
- **Status icons** — one bold pictogram per glyph, readable at 20
  pixels.
- **Backgrounds** — perfectly even lighting, no vignette, no focal
  point anywhere, very low contrast. Windows crop backgrounds at
  arbitrary shapes, so no part of the composition may matter.
