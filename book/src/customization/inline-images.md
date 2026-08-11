# Inline Images

VellumFE can float real images into text, with the text wrapping around them.
Images are display-only and driven by Lich scripts through XML tags the game
itself never sends.

## Installing art

Drop image files into:

```
~/.vellum-fe/global/images/inline/
```

The filename (without extension) is the name you reference. `sunset.png`
becomes `src='sunset'`.

| Format | Extension | Animated |
|--------|-----------|----------|
| PNG    | `.png`    | if it is an animated PNG (APNG) |
| APNG   | `.apng`   | yes |
| GIF    | `.gif`    | yes |
| WebP   | `.webp`   | yes, if the file is animated |
| JPEG   | `.jpg` / `.jpeg` | no |

Names may contain letters, digits, `_`, `+`, and `-` only, and are
case-insensitive. Restart or run `.reload` to pick up new files.

**Sizing tip:** the art is drawn a few text rows tall, so a 512×512 export
looks identical to a 4000×4000 one and loads far faster.

## The tag

```xml
<vellumImg src='sunset' rows='4' align='left'/>
```

| Attribute | Default | Meaning |
|-----------|---------|---------|
| `src` | *(required)* | Image name. Never a path. |
| `rows` | `1` | Height in text rows. Width follows the art's aspect ratio. |
| `align` | `left` | Which side the image floats on: `left` or `right`. |

`rows` is a request, not a guarantee, and the two window kinds size the
picture differently — on purpose:

- **Text windows** (story, thoughts, custom, tabs): the picture follows the
  height of the text that wraps it. One short line beside it gives a small
  picture; a paragraph grows it toward `rows`. A standalone image with no
  text after it renders at exactly `rows`. Press-and-hold always shows the
  art at full size, so a small float loses nothing.
- **The room window**: the picture renders at `rows`, shrinking only with
  the window itself (its height, and a cap of about half its width).

In both, aspect ratio is always preserved, the ceiling is 8 rows, and a
window too narrow to wrap beside at all puts the image on its own rows with
the text below.

## Putting an image in the room window

GemStone declares a `sprite` component on every room change and never puts
anything in it — the official client shows no room-window images at all.
VellumFE uses that unused slot for room art:

```ruby
_respond "<compDef id='sprite'><vellumImg src='sunset' rows='4' align='left'/></compDef>"
```

`<component id='sprite'>…</component>` works too.

### Art plus the room description

To float art beside real room prose, put the image at the front of the
description component:

```ruby
_respond <<~'XML'.strip
  <clearStream id='room'/><pushStream id='room'/><compDef id='room desc'><vellumImg src='sunset' rows='4' align='left'/>Blue and red arrows painted upon the smooth planks of the dock point to the northeast, while a solid green arrow points to the southeast.  A pair of sturdy steps leads down to a finger pier with five docking slips.</compDef><compDef id='room exits'>Obvious paths: <d>northeast</d>, <d>southeast</d>, <d>west</d></compDef><popStream id='room'/>
XML
```

The description wraps beside the image and rejoins the full width once it
clears the image's bottom edge.

> **Ruby quoting:** room XML is full of both `'` and `"`, and room names
> contain apostrophes ("Kraken's Fall"). A single-quoted heredoc
> (`<<~'XML'`) passes everything through literally with no escaping and no
> interpolation — far less painful than a quoted string.

### Art clears when you move

The game re-declares `sprite` (empty) on every room change, so script art
disappears when you walk. That is usually what you want — set it from a
script that reacts to the room you are in.

## Room art without a script

Instead of sending a tag every time, you can map art to rooms once and have
it appear whenever you walk in.

Stand in the room and run:

```
.roomimages set sunset
```

That maps the room you are standing in — you never type a room number. Then
turn the feature on:

```
.roomimages on
```

| Command | What it does |
|---------|--------------|
| `.roomimages` | State, how many rooms are mapped, and what this room shows |
| `.roomimages on` / `off` | Master switch |
| `.roomimages set <image>` | Map the current room (moves it if already mapped elsewhere) |
| `.roomimages clear` | Unmap the current room |
| `.roomimages list` | Every image and the rooms it covers |
| `.roomimages edit` | Open the editor |

### The editor

`.roomimages edit`, or **Editors → Room Images** in the GUI. One card per
image, since a single picture usually covers many rooms:

```
[thumb]  krakens_fall_pier          Rows [4]  Align [Left]
         7118245  Kraken's Fall, Third Pier          ✕
         7118250  Kraken's Fall, Second Pier         ✕
         [+ Add current room]
```

`Rows` and `Align` are per image, so every room sharing a picture is framed
the same way. **+ Add current room** is the quickest way to build a mapping:
walk around and click it in each room you want covered.

### Where the mappings live

`~/.vellum-fe/global/room_images.toml`, with an optional per-character file
at `~/.vellum-fe/profiles/<character>/room_images.toml`:

```toml
[[image]]
name = "krakens_fall_pier"
rooms = [7118245, 7118250, 7118251]
rows = 6
align = "left"
```

Rooms are keyed by the game's own room id. A room can belong to only one
image — mapping it somewhere else moves it rather than leaving a duplicate.

### Different art by time of day

Add variants to show a night version of the same view:

```toml
[[image]]
name = "krakens_fall_pier"
rooms = [7118245, 7118250]

  [[image.variant]]
  name = "krakens_fall_pier_night"
  when = { type = "time_of_day", phase = "night" }
```

The first variant whose condition matches wins; none matching falls back to
the entry's own `name`.

In the editor, each image card has an **+ Add variant** button and a row per
variant — pick the phase and the art from dropdowns. Conditions more complex
than time of day are shown read-only there and edited in the file.

Elanthian time runs on **US Eastern**, so VellumFE computes the phase from
your system clock — no `time` command, no waiting for the game to tell it,
and it is correct the moment you log in. The four phases are `dawn`, `day`,
`dusk`, and `night`.

> Sunrise and sunset in Elanthia follow Central more closely than Eastern,
> so the phase boundaries are approximate by design rather than a claim of
> exactness.

Variants use the same condition system as hotbar button states and injury
doll art, so they are not limited to time — a variant can key off effects,
injuries, vitals, or any combination. Time of day is also now available to
hotbars and the doll for the same reason.

Script art still wins: if a script sends a `sprite` for the room you are in,
that is what shows.

## GemStone's own room pictures

The game sends `<resource picture='N'/>` on every room change — the official
client uses it to show a picture beside the room name. The wire carries only
the number, so VellumFE fetches the art from play.net and caches it.

**This is off by default.** It sends requests to play.net, so it is an
explicit opt-in: run `.settings` and turn on **Game Room Pictures** under
Appearance.

Once on, walking into a room with a picture downloads it once and caches it
under `global/images/gs-art/`. The image appears on the next room change
after it arrives — fetching happens in the background so the game feed is
never held up.

Your own art still wins: install `images/inline/32.png` and rooms with
`picture='32'` use yours instead of the download. Most rooms send
`picture='0'`, meaning no picture at all, and ids with no art are remembered
so they are not requested again.

## Putting an image in the story window

Send the tag on its own:

```ruby
_respond "<vellumImg src='sunset' rows='4' align='left'/>"
```

The image floats and the following lines wrap beside it, rejoining the full
width once they clear its bottom edge — the same behaviour as the room
window. This works in every text window: story, thoughts, combat, custom
windows, and each tab of a tabbed window.

## Press and hold to enlarge

Press and hold an image to see it at full size; release to shrink it back.
Works with mouse and touch, in both the desktop GUI and the phone client.

## Where images work

| Frontend | Support |
|----------|---------|
| **GUI — room window** | Full: floats, wrapping, hold-to-enlarge |
| **GUI — text windows** | Full: floats, wrapping, hold-to-enlarge |
| **Phone / web** | Full: floats via CSS, hold-to-enlarge |
| **TUI** | `[img:name]` text fallback (terminals cannot draw images) |

An image name with no matching file also shows the `[img:name]` fallback, so
a missing file is visible rather than blank.

## Images are never sent to the game

These tags are display instructions your client interprets; they travel from
Lich to VellumFE only and never reach the game server. `src` is a **name, not
a path** — it is checked against a restricted alphabet and then resolved
through the client's own image list, so a script can name art you installed
but can never read another file on your disk.

See also: [Emoji](./emoji.md) for `:shortcode:`-sized inline pictures.
