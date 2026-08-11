# Injury Display

> Know which limb is about to fail you — and how badly — without typing `health` between
> every swing.

## What it's for

`HEALTH` tells you everything, once, in a block of text that scrolls away. Mid-fight you don't
want everything. You want to know whether it's your right arm or your left leg, and whether
it's the scratch you took two rooms ago or the thing that's about to cost you the fight.

The injury display is a body, drawn once and updated as the game reports damage. Each part
carries its own severity, so a glance answers "where" and "how bad" at the same time. Wounds
and scars are shown apart, which matters because a scar is permanent and a wound is the one
you can still do something about.

It is also the widget that changes the most between frontends, and deliberately so. In the
desktop GUI it can be real artwork — a painted body that swaps to a different pose when you go
down, with hand-drawn wounds that stack in place. In the terminal it's a compact figure of
characters that colors by severity. Both read the same game data; only the drawing differs.

## Set it up

{{#tabs global="frontend"}}
{{#tab name="Desktop GUI"}}

1. Click **Windows** in the top toolbar, expand **Character**, and tick **Injuries**. Set the
   row's **zone** to place it.
   (Typed equivalent: `.addwindow injuries injuries 0 0 10 8`.)
2. Right-click the window. Its widget section is named **Injury doll**.
3. Use **Doll image** to pick the art: **Skin default** takes whatever body your active skin
   ships, **None** draws the built-in vector body, and the rest are dolls installed in your
   image pool. Install more with `.jinx list` and `.jinx install`.
4. Tick **Grayscale doll art** if you want the body desaturated. The wound and scar dots keep
   their colors regardless.

<figure class="shot" data-shot="widgets/injury-doll-gui-art-picker">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The injuries window's right-click <b>Injury doll</b> section, with the <b>Doll image</b> drop-down open on <b>Skin default</b>, <b>None</b>, and installed pool dolls, above the <b>Grayscale doll art</b> checkbox and <b>Calibrate doll…</b>.</figcaption>
</figure>

→ **Expected result:** a body appears in the window. Take a wound and that part changes — a
colored dot at the part's position, or its hand-drawn wound art if the doll has any. Hover a
wounded part and a tooltip names it and its severity, like `left arm: severe injury`.
{{#endtab}}
{{#tab name="Terminal (TUI)"}}

Type `.addwindow injuries injuries 0 0 10 8` — name, type, then column, row, width, height.
Type `.addwindow` with no arguments for a picker instead.

> **`.addwindow` takes 1 argument or 6 or more — never 2 through 5.** `.addwindow injuries`
> alone prints usage and adds nothing.

The terminal draws a figure out of characters: eye glyphs on the top row, `0` for the head,
`/|\` for arms and chest, `|` for the abdomen, `o` for hands and feet, and `/ \` for the legs.
Three parts have no place on that figure, so they get two-letter labels down the right side —
`nk` for neck, `bk` for back, and `ns` for the nervous system. Every glyph takes the color of
its part's severity.

Type `.editwindow injuries` for the window's form. Seven color fields sit there in a grid:
**Wound1**, **Scar1**, **Wound2**, **Scar2**, **Wound3**, **Scar3**, and **Uninjured**.
`Tab` and `Shift+Tab` move between fields, `Ctrl+S` saves, `Esc` cancels.

> ⚠️ **The terminal cannot draw doll artwork, and that's deliberate — not a missing feature.**
> Dolls are images: a painted base, hand-drawn wound overlays, pose variants. The TUI renders
> characters, so **Doll image**, **Grayscale doll art**, and the calibrator are GUI-only.
> **The trade runs both ways: the seven severity colors can only be authored here.** The GUI
> reads them from your layout and paints its vector body with them, but offers no color
> pickers of its own — so the terminal form is where that palette gets set for both frontends.

→ **Expected result:** a small stick figure whose parts change color as you take damage, with
`nk`, `bk`, and `ns` coloring beside it.
{{#endtab}}
{{#tab name="Mobile"}}

The phone uses fixed chrome, so you don't place this window — the doll lives in the **status
drawer**, the right-hand panel you open with the handle on the screen's right edge.

**Your desktop's doll art follows you.** If the host machine has skin doll artwork configured,
the phone fetches it and draws the same painted body, the same hand-drawn wound overlays, and
the same dots at the same calibrated positions. Without doll art it falls back to a simple
vector body. Below the doll, an **Injuries** section lists every hurt part in words — rows like
`left arm: wound 2` and `right leg: scar 1`, or **No injuries.** when you're whole.

Two things are decided on the desktop and sent to the phone as answers, not as rules: which
pose variant is active, and which parts are suppressed. Your phone never evaluates a condition
— it's told the result. That's why a variant you author in a skin works on the phone with no
phone-side setup.

> ⚠️ **The phone shows only your own injuries, and nothing on it is tappable.** Another
> player's injuries popup has no phone surface at all, and neither the doll nor the injury
> rows respond to touch. Art, calibration, and colors are all desktop authoring.

→ **Expected result:** open the status drawer and your doll is there with its wounds marked,
above a plain-language list of every injured part.
{{#endtab}}
{{#endtabs}}

## Common setups

### A painted body that lies down when you do

This is the feature that makes the widget more than a picture, and it needs a skin with doll
art. In your active skin's `skin.toml`, the `[injury_doll]` table is the default body. Add a
`[[injury_doll.variants]]` entry with a `name`, a `when` condition, and its own complete
`skin` table:

```toml
[[injury_doll.variants]]
name = "downed"
when = { type = "indicator", id = "prone", active = true }

[injury_doll.variants.skin]
base = "doll/downed.png"
```

Then right-click the injuries window ▸ **Injury doll** ▸ **Calibrate doll…**, switch the
**Doll set** drop-down from **Default** to **downed**, and click each body part on the prone
art to place its dot. Click **Save to skin**.

**You'll see:** your doll standing while you're upright, and the whole body swapping to the
prone painting the instant you're knocked down — with every wound dot landing in the right
place on the new pose, because you calibrated that set separately.

### Dots that read at a glance instead of a squint

Open **Calibrate doll…** on the default set. The bottom rows style the generated dots: a
**Wound** color, a **Scar** color, a **dot size** slider as a percentage of the doll's height,
and an **opacity** slider. Flip **Preview:** between **wounds** and **scars** and drag the
**rank** slider from 1 to 3 to see each severity live on the art before you commit.

Push **dot size** up until the dots are unmistakable at your normal window size, then click
**Save to skin**.

**You'll see:** wounds as solid circles with their rank numeral inside and scars as rings, big
enough to read from your seat — and the change takes effect on the very next frame rather than
waiting for a skin reload.

### A hand that disappears with the arm above it

For dolls with hand-drawn art, a part can suppress itself. Give the part a `hidden_when`
condition in the manifest:

```toml
[injury_doll.leftHand]
hidden_when = { type = "injury", area = "leftArm", cmp = ">=", level = 3 }
healthy = "doll/hand_ok.png"
```

**You'll see:** the left hand stop drawing entirely once the left arm hits severity 3 — no
orphaned hand floating below a missing arm. The hand's own wound still appears in the hover
tooltip, so you don't lose the information, only the drawing.

## Tips & gotchas

> ⚠️ **Scar 1 is level 4, not level 1.** The scale runs 0 through 6: `0` is healthy, `1`–`3`
> are wounds, and `4`–`6` are scars. So a condition of **Injury >= 4** means "any scar at all",
> and **Injury >= 1** means "anything wrong". The editor labels (**Scar1**), the TOML color
> keys (`scar1_color`), and the skin art keys (`scar1`) all use the short name while the
> underlying level is 4. Counting scars from 1 in a condition is the most common way to write
> a rule that never fires.

> ⚠️ **A variant replaces the doll wholesale — it does not layer on top.** A matched variant
> brings its own base image, its own anchors, its own part art, and its own dot styling. That
> is the point: a prone body repositions every limb, so nothing from the standing set would
> land correctly. It also means a variant inherits nothing — a `hidden_when` you wrote on the
> default set does not apply while a variant is active. Author each set completely.

> ⚠️ **The calibrator's Doll set drop-down discards unsaved work when you switch.** Changing
> from **Default** to a variant reseeds every anchor and dot value from the manifest. Click
> **Save to skin** before you switch sets, every time.

**The window may appear on its own.** The injuries window is bound to the game's own injuries
dialog, and windows bound that way are created for you the first time the game opens their
dialog — that is how the GemStone IV experience window arrives. If a doll turns up unbidden,
this is why, and `.hidewindow injuries` puts it away. It won't duplicate: if you already have
one, the existing window takes the feed.

**A pool doll and a skin doll are not the same thing.** A doll picked from **Doll image** is a
single flat image with a sidecar file beside it holding its anchors and dot styling — that's
all. **Pool dolls carry no pose variants and no hidden parts**, because those live in a skin's
manifest. Choose **Skin default** if you want variants. Calibrating a pool doll writes to its
sidecar, so the calibration travels with the artwork and a shared doll can arrive already
calibrated.

**A part with hand-drawn art never gets a dot, and that's a feature.** Once any severity key is
authored for a part, that part is fully hand-drawn: at a level with no art the base shows
through. Skins that paint the *worst case* into the base rely on this — "no overlay" is a
deliberate reveal, like the hole where a severed limb was. Parts with no art at all keep their
generated dots, so one doll can mix both approaches freely.

**The nervous system draws underneath everything else.** It's a full-body underlay rather than
a badge, so limb wounds and dots paint over it instead of being covered by it.

**The tooltip only appears when something is wrong.** Hovering a healthy GUI doll says nothing
at all — an "uninjured" tooltip read as a stray badge, so it was removed. If you get no
tooltip, you have no wounds.

**Grayscale is built on demand.** Ticking **Grayscale doll art** generates desaturated copies
of the art at that moment and drops them when you untick it. Skins can't ship pre-made
grayscale files; the client makes its own.

**Clicking another player can open their doll in a popup.** The game sends a separate
per-player injuries dialog, and both desktop frontends draw it as a floating window titled with
that player's name — the terminal closes it with `Esc`, the GUI with its window close button.
**That popup always uses the default doll set and the default palette**: variants and hidden
parts read *your* state, so your own prone flag must never reshape someone else's body.

**Your severity colors don't reach the phone.** The phone ships the same seven default colors,
so an unmodified setup matches everywhere — but per-window `injury*_color` overrides are not
forwarded, so a recolored desktop doll and the phone's doll will differ.

## See also

- [Indicators](./indicators.md) — the same **Injury** condition, driving a status light; a
  bleeding indicator says "somewhere", this says where
- [Dashboard](./dashboard.md) — those statuses in one grid
- [Mini Vitals](./minivitals.md) — how much health is left, next to which parts took it
- [Make your health bar shout when you're hurt](../how-to/vitals-flash.md) — turning a wound
  into a sound, a light, and a buzz
- [Skins (GUI Graphics)](../customization/skins.md) — authoring doll art, overlays, and
  variants in a skin manifest
- [Compass](./compass.md) — the other art-driven widget with its own picker

<details>
<summary>Config reference (TOML)</summary>

`widget_type = "injury_doll"`. **Both `injuries` and `injury_doll` are accepted** as the type
string, and as the `.addwindow` type argument. Set the colors through `.editwindow` in the
terminal; art and calibration are GUI-only.

**Watch the type string.** An unrecognized widget type does not error — it silently falls back
to a plain **text** window. A typo like `injurydoll` or `injury-doll` gets you an empty text
window, not a warning.

### Window fields

| Field | Type | Default | What it does |
|---|---|---|---|
| `injury_default_color` | string | `#333333` | Level 0 — an uninjured part |
| `injury1_color` | string | `#aa5500` | Level 1 wound |
| `injury2_color` | string | `#ff8800` | Level 2 wound |
| `injury3_color` | string | `#ff0000` | Level 3 wound |
| `scar1_color` | string | `#999999` | Level **4** — scar 1 |
| `scar2_color` | string | `#777777` | Level **5** — scar 2 |
| `scar3_color` | string | `#555555` | Level **6** — scar 3 |

A blank or whitespace value falls back to the default. The preset ships 8 rows by 10 columns
with a minimum of 6 by 8.

### Body parts

Fourteen parts, in the calibrator's click-through order: `head`, `leftEye`, `rightEye`,
`neck`, `chest`, `abdomen`, `back`, `leftArm`, `rightArm`, `leftHand`, `rightHand`, `leftLeg`,
`rightLeg`, `nsys`. These same names are the vocabulary of the **Injury** condition in hotbars,
indicators, and hand icons.

### Skin doll art (`skin.toml`, GUI only)

| Key | Type | Default | What it does |
|---|---|---|---|
| `[injury_doll]` `base` | string | none | The body image. Relative to the skin folder, or a pool path, or absolute. |
| `[injury_doll.anchors]` | table | built-in | `part = [x, y]` as fractions of the base image. Written by the calibrator. |
| `[injury_doll.dots]` `wound_color` | string | `#e02020` | Solid wound dot color |
| `[injury_doll.dots]` `scar_color` | string | `#b8b8b8` | Scar ring color |
| `[injury_doll.dots]` `opacity` | float | `0.9` | Dot opacity, clamped 0–1 |
| `[injury_doll.dots]` `diameter` | float | `0.07` | Dot size as a fraction of the drawn doll height |

Any other table under `[injury_doll]` is a body part. A part table takes `hidden_when` (a
condition) plus per-severity image keys: `healthy`, `injury1`, `injury2`, `injury3`, `scar1`,
`scar2`, `scar3`. Those key names are exact — an unrecognized one is logged and ignored.

```toml
[injury_doll]
base = "doll/body.png"

[injury_doll.dots]
wound_color = "#e02020"
scar_color = "#b8b8b8"
diameter = 0.07

[injury_doll.leftArm]
healthy = "doll/arm_ok.png"
injury1 = "doll/arm_i1.png"
injury3 = "doll/arm_severed.png"

[injury_doll.leftHand]
hidden_when = { type = "injury", area = "leftArm", cmp = ">=", level = 3 }

[[injury_doll.variants]]
name = "downed"
when = { type = "indicator", id = "prone", active = true }

[injury_doll.variants.skin]
base = "doll/downed.png"
```

A variant's `skin` table has the same shape as `[injury_doll]` minus `variants` — variants do
not nest, and attempting it is a parse error. The **first** variant whose `when` matches wins,
in declaration order.

### Window TOML

```toml
[[windows]]
name = "injuries"
widget_type = "injury_doll"
title = "Injuries"
row = 0
col = 0
rows = 8
cols = 10
show_border = true
border_style = "rounded"
content_align = "center"
injury1_color = "#aa5500"
injury2_color = "#ff8800"
injury3_color = "#ff0000"
scar1_color = "#999999"
scar2_color = "#777777"
scar3_color = "#555555"
```

</details>
