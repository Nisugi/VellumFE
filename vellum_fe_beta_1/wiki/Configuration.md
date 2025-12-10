# Configuration Guide

VellumFE stores all customization under `~/.vellum-fe/` (Windows: `%USERPROFILE%\.vellum-fe\`). Each character selected with `--character` gets its own subdirectory and inherits shared assets (layouts, sounds) when missing. This page explains each file and the TOML schema exposed by the code under `src/config.rs`.

## Directory Layout

```
~/.vellum-fe/
 ├── layouts/                 # Shared layouts (.savelayout writes here)
 ├── sounds/                  # User-provided audio (MP3/WAV/OGG/FLAC)
 ├── cmdlist1.xml             # Command link map copied from defaults
 └── <character>/             # Created per character (lowercased name)
      ├── config.toml
      ├── colors.toml
      ├── highlights.toml
      ├── keybinds.toml
      ├── history.txt
      └── debug.log
```

Missing files regenerate automatically using the embedded defaults shipped in `defaults/`.

## `config.toml`

Top-level sections correspond to the `Config` struct in `src/config.rs`.

### `[connection]`

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `host` | string | `"127.0.0.1"` | Lich host (use `"localhost"` or remote proxy address). |
| `port` | integer | `8001` in defaults, overridden by CLI port | Lich detached-mode port. |
| `character` | string | `null` | Populated at runtime; omitted on disk. |

### `[ui]`

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `buffer_size` | integer | `1000` | Lines kept per text window if layout does not override `buffer_size`. |
| `show_timestamps` | bool | `true` | Global toggle for `[HH:MM]` suffixes (windows can override). |
| `border_style` | string | `"single"` | Global default applied to new windows. |
| `countdown_icon` | string | `"?"` | Icon rendered in countdown widgets (per-window override available). |
| `poll_timeout_ms` | integer | `16` | Event loop poll period; lower = more responsive & CPU usage. |
| `startup_music` | bool | `true` | Plays default music on connect (`defaults/sounds`). |
| `startup_music_file` | string | `"wizard_music"` | Looked up in `sounds/` (extension resolved automatically). |
| `selection_enabled` | bool | `true` | Enables custom text selection (shift-drag uses terminal selection). |
| `selection_respect_window_boundaries` | bool | `true` | Prevents dragging selections across windows. |
| `drag_modifier_key` | string | `"ctrl"` | Modifier required for dragging windows (`ctrl`, `alt`, `shift`, `none`). |
| `min_command_length` | integer | `3` | Minimum length saved to command history. |
| `perf_stats_x/y/width/height` | integers | `0/0/35/23` | Dimensions of performance overlay when toggled. |

#### `[ui.layout]`

Describes the command input box placement. Fields map to `LayoutConfig`: `command_row`, `command_col`, `command_height`, `command_width`. `0` uses defaults (bottom full width).

### `[sound]`

Matches `SoundConfig`. Fields: `enabled` (bool), `volume` (float 0-1), `cooldown_ms` (per-sound throttle). The runtime `SoundPlayer` reads this on startup; updates via `.settings` save back to disk.

### `layout_mappings`

Optional array of tables mapping terminal size ranges to named layouts. Each entry:

```
[[layout_mappings]]
min_width = 120
max_width = 160
min_height = 35
max_height = 60
layout = "wide"
```

When VellumFE boots, it selects the first mapping whose range includes the current terminal size and loads the referenced layout from `layouts/`.

### `event_patterns`

`Config` can watch for text matches and inject `ParsedElement::Event` entries (stuns, prone, etc.). Each pattern table includes:

| Field | Type | Meaning |
| --- | --- | --- |
| `pattern` | string | Rust regex applied to plain text chunks. |
| `event_type` | string | Arbitrary identifier stored in `Event` elements. |
| `action` | `"set"`, `"clear"`, `"increment"` | Controls how the event is handled by the UI. |
| `duration` | integer | Default duration in seconds (used when no capture). |
| `duration_capture` | integer | Optional capture group index (1-based) containing a numeric quantity. |
| `duration_multiplier` | float | Converts captured value to seconds (rounds ×5 seconds, etc.). |
| `enabled` | bool | Quick toggle without removing the entry. |

The parser (`XmlParser::check_event_patterns`) evaluates these for every flushed text line and produces countdown updates accordingly.

## Layout Files (`layouts/*.toml`, `Layout` struct)

Layout files are serialized instances of `Layout` which contains `terminal_width`, `terminal_height`, `windows` (vector of `WindowDef`), and metadata for auto-save. Key fields on `WindowDef` include:

- **Identity & Streams**
  - `name`: Unique identifier referenced by commands.
  - `widget_type`: `"text"`, `"tabbed"`, `"progress"`, `"countdown"`, `"indicator"`, `"compass"`, `"injury_doll"`, `"hands"`, `"hand"`, `"dashboard"`, `"active_effects"`, `"targets"`, `"players"`, `"inventory"`, `"room"`, `"spacer"`, etc.
  - `streams`: Array of stream names (ignored when `tabs` is present).
- **Geometry**
  - `row`, `col`: Top-left position (absolute, zero-based).
  - `rows`, `cols`: Size in character cells.
  - `min_rows`, `max_rows`, `min_cols`, `max_cols`: Constraints honored during mouse resize and `.resize`.
  - `locked`: Prevents drag/resize when true.
- **Appearance**
  - `show_border`, `border_style`, `border_color`, `border_sides`.
  - `title`: Window title; `content_align`: `'top-left'`, `'top-right'`, `'bottom-left'`, `'bottom-right'`, `'center'`.
  - `background_color`, `bar_color`, `bar_background_color`, `text_color`.
  - `transparent_background`: Allows the terminal background to bleed through unfilled bars.
  - `show_timestamps`: Append localized time stamps to each line (overrides the global `[ui] show_timestamps` default).
- **Widget-Specific**
  - `tabs`: Array of `{ name, stream, show_timestamps }` entries for tabbed windows.
  - `tab_*_color`, `tab_bar_position`, `tab_unread_prefix`.
  - `progress_id`, `numbers_only`.
  - `countdown_id`, `countdown_icon`.
  - `indicator_colors`: Colors applied per indicator state.
  - `dashboard_layout`, `dashboard_indicators`, `dashboard_spacing`, `dashboard_hide_inactive`.
  - `visible_count`: For list-based widgets (players, targets, active effects) limiting rows.
  - `effect_category`, `effect_default_color`: Determines which active-effect bucket the window renders.
  - `hand_icon`: Glyph displayed before hand contents.
  - `compass_active_color`, `compass_inactive_color`.
  - `injury_*_color`, `scar*_color`: Custom palette for the injury doll.

`Layout::save(name, terminal_size, autogen)` records terminal dimensions when saving so `.resize` can scale proportionally from the same baseline.

## `colors.toml`

Holds all color-related settings (`ColorConfig`).

- `[presets]`: Named color presets referenced by highlights (`fg`, `bg` as `#RRGGBB` strings).
- `[[prompt_colors]]`: Map command prompt characters (e.g., `R`, `S`, `>`) to colors (fg/bg).
- `[ui]`: Global UI colors (background, text, border, focused border, selection background, textarea background).
- `[[spell_colors]]`: Each entry lists `spells` (array of IDs) and colors for `bar_color`, `text_color`, `bg_color`.
- `[[color_palette]]`: Shared color palette entries (name/category/color/favorite).

The settings editor resolves named colors through `Config::resolve_color`, allowing palette names to be used interchangeably with hex strings.

## `highlights.toml`

Each table key is a highlight name; values match `HighlightPattern`:

| Field | Type | Description |
| --- | --- | --- |
| `pattern` | string | Regex or plain text depending on `fast_parse`. |
| `category` | string | Grouping label used in the highlight browser. |
| `fg`, `bg` | optional string | Foreground/background colors (hex or palette name). |
| `bold` | bool | Toggles bold rendering. |
| `color_entire_line` | bool | Applies style to the whole line when matched. |
| `fast_parse` | bool | When true, pattern is treated as a literal and compiled into the Aho-Corasick automaton for high performance. |
| `sound` | optional string | File in `sounds/` triggered on match. |
| `volume` | optional float | Sound override volume (0–1). |

Changes persist immediately when you close the highlight editor or use `.testhighlight`.

## `keybinds.toml`

Keys map names like `"ctrl+e"` to either:

- `{ action = "cursor_word_left" }` – maps to a `KeyAction` in `config.rs`.
- `{ macro = { macro_text = "sw\r" } }` – sends literal text to the server.

Use `.addkeybind` / `.editkeybind` to manage entries; the code handles translation to `KeyCode`/`KeyModifiers` via `parse_key_string`.

## `cmdlist1.xml`

Extracted on first run to `~/.vellum-fe/cmdlist1.xml`. Parsed by `CmdList` to map existence IDs (`coord` attributes) to menu commands and display text. Clickable links use this data to mimic Wrayth context menus. You normally do not edit this file manually; replace it with updated copies from Lich assets if Simutronics ships changes.

## Sounds

Place audio assets in `~/.vellum-fe/sounds/`. The `SoundPlayer` looks up files by base name, automatically testing common extensions when a highlight references `sound = "my_alert"`. Use `.settings` → Sound to toggle globally or adjust volume/cooldown.

## Autosave & Priority

When `.resize` runs, VellumFE autosaves the scaled layout to `auto_<character>.toml`. Layout load order on start:

1. `auto_<character>.toml` (if present)
2. `<character>.toml`
3. Layout selected via `layout_mappings`
4. `default.toml`
5. Embedded fallback layout (`defaults/layouts/layout.toml`)

Keep this priority in mind when distributing layouts—renaming or deleting `auto_` files restores the manually saved baseline.
