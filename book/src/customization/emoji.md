# Emoji

VellumFE renders `:shortcode:`-style emoji in incoming game text, and lets you
add your own custom (Discord-server-style) emoji.

## Standard emoji

Any standard gemoji shortcode in game text is rendered as its emoji:

```
Someone thinks, "heading to town :grin:"
```

- In the **GUI**, emoji are drawn in full color (Twemoji artwork). Toggle color
  vs. monochrome with the `ui.color_emoji` setting.
- On the **phone/web** client, emoji use the system emoji font.
- In the **TUI**, emoji render as whatever Unicode glyph your terminal font
  provides.

Shortcode expansion is controlled by the `ui.emoji_shortcodes` setting
(default on).

## Custom emoji

Drop image files into `~/.vellum-fe/emoji/` and reference them as `:name:`,
just like Discord server emoji.

1. Create the directory `~/.vellum-fe/emoji/` if it does not exist.
2. Add an image named after the shortcode you want, e.g.
   `~/.vellum-fe/emoji/vibecat.png` for `:vibecat:`.
3. Restart, or run `.reload`, to pick up new files.

Supported formats:

| Format | Extension | Animated |
|--------|-----------|----------|
| PNG    | `.png`    | if it is an animated PNG (APNG) |
| APNG   | `.apng`   | yes |
| GIF    | `.gif`    | yes |
| WebP   | `.webp`   | yes, if the file is animated |

Animated GIF, WebP, and APNG all animate in the GUI and on the phone. Discord
serves its animated custom emoji as **animated WebP** — save one as
`~/.vellum-fe/emoji/<name>.webp` and it animates. (To grab one: right-click the
emoji in Discord → Copy Link, open the `…​.webp` URL in a browser, and save it.)

Naming rules: the filename (without extension) is the shortcode, and may
contain letters, digits, `_`, `+`, and `-` only. Names are case-insensitive
(`VibeCat.png` matches `:vibecat:` and `:VibeCat:`). If a custom emoji shares a
name with a standard one, **your custom emoji wins**.

### How each frontend renders custom emoji

- **GUI** — draws the image inline, animating APNG/GIF frames.
- **Phone/web** — draws the image inline via a private, token-gated endpoint;
  GIF/APNG animate natively in the browser.
- **TUI** — cannot render images, so it shows the literal `:name:` text as a
  readable fallback.

## Emoji are never sent to the game

GemStone IV is a roleplaying game, and sending emoji (or the shortcodes that
render as emoji) as speech, thoughts, whispers, or actions can get you warned
or banned. Emoji are a **display convenience on your side only**.

VellumFE strips emoji from every command before it reaches the game — whether
you type it, a hotbar sends it, or a Lich script sends it. This includes actual
emoji characters and any `:shortcode:` that resolves to a known emoji (standard
or custom). Ordinary text, unknown shortcodes, Lich script syntax, and things
like time strings (`12:30:45`) are left untouched. This protection is always on.

So a Lich script can safely send `:vibecat:` for VellumFE to display, and the
game will never see it.
