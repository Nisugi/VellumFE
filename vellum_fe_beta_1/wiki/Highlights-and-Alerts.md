# Highlights and Alerts

The highlight system combines fast literal matching, full regex support, audio triggers, and event pattern detection. This page explains how to manage highlights and how they interact with the parser and sound subsystem.

## Managing Highlights

- `.highlights` / `.listhl` lists all highlights grouped by category.
- `.addhighlight` opens the highlight form in create mode.
- `.edithighlight <name>` preloads an existing entry for editing.
- `.deletehighlight <name>` removes an entry and saves `highlights.toml`.
- `.testhighlight <name> <sample text>` runs a highlight against arbitrary text and prints whether it matches, which style rules apply, and why it may have failed.

### Highlight Form Fields

| Field | Description |
| --- | --- |
| **Name** | Unique identifier used in dot commands and `highlights.toml`. |
| **Pattern** | Regex or literal pattern. Regex syntax follows Rust’s `regex` crate. |
| **Category** | Free-form label for grouping inside the browser (e.g., “Combat”, “Spells”). |
| **Foreground / Background** | Accept either `#RRGGBB` strings or palette names from `colors.toml`. |
| **Bold** | Toggles bold styling. |
| **Color entire line** | When true, the whole line adopts the highlight style instead of the match span. |
| **Fast parse** | Treats the pattern as a literal string; the application adds it to the Aho-Corasick automaton for O(n) scanning across all windows. Ideal for short, high-frequency phrases. |
| **Sound file** | Base name (without extension) of an audio file in `~/.vellum-fe/sounds/`. |
| **Volume override** | Optional per-highlight volume (0.0–1.0). |

Saved highlights are cached inside `WindowManager::highlights` and re-evaluated every frame so changes apply instantly.

## Performance Details

- Literal highlights (`fast_parse = true`) are compiled into a shared `AhoCorasick` matcher inside `TextWindow`. This allows tens of thousands of patterns without runaway CPU usage.
- Regex highlights compile lazily and are cached per highlight (`highlight_regexes` vector).
- Both literal and regex matches run before lines are wrapped; the resulting style segments survive wrapping so multi-line output remains colored consistently.

## Sound Alerts

Sound playback lives in `src/sound.rs`:

- `SoundPlayer::new` creates a Rodio output stream and keeps a per-sound cooldown map.
- `SoundConfig` (from `config.toml`) sets `enabled`, `volume`, and cooldown (`cooldown_ms`). Toggle via `.settings` → Sound.
- Sounds must reside in `~/.vellum-fe/sounds/`. When a highlight references `sound = "alert"`, the player searches for `alert`, `alert.mp3`, `alert.wav`, `alert.ogg`, `alert.flac`.
- The player silently skips missing/undecodable files but logs warnings in `debug.log`.

`sound_cooldown_ms` prevents the same clip from triggering repeatedly in rapid succession (e.g., damage-over-time ticks).

## Event Patterns

Highlighting alone does not manage timers. The parser (`XmlParser::check_event_patterns`) executes the `event_patterns` defined in `config.toml`. Typical use cases:

- Set/cancel stun countdowns.
- Track prone or webbed durations.
- Trigger status indicators (`statusIndicator` elements).

Fields recap:

- `pattern`: Regex applied against plain text emitted by the server.
- `event_type`: Key used by `App` to decide which widget to update (e.g., `stun`).
- `action`: `set`, `clear`, or `increment`.
- `duration`, `duration_capture`, `duration_multiplier`: Determine how long the effect persists; capture groups enable parsing numeric values from text.
- `enabled`: Quick toggle; disabled patterns stay in the file without matching.

Parsed events flow through `App::handle_parsed_elements` to countdown widgets, dashboards, and indicators.

## Prompt and Preset Colors

Prompt colors (`[[prompt_colors]]` in `colors.toml`) allow fine-grained styling of the command prompt characters sent by the game (`R`, `S`, `>`). Highlights can reference presets defined in `[presets]`, enabling consistent colors between `colors.toml` and highlight rules.

## Troubleshooting Highlights

- Use `.testhighlight` frequently. It reports regex compilation errors, unmatched patterns, and the styling that would apply on success.
- If a highlight never fires, confirm the text stream routes to your window (check `.windows` to see names and `.customwindow` definitions).
- When highlight colors appear washed out, ensure `transparent_background` is `false` or set explicit backgrounds via palette entries.
- For audio, run `dir ~/.vellum-fe/sounds` and verify file names. Remember to restart VellumFE after adding new audio files—the sound directory is scanned on connect.

## Exporting & Sharing

Highlights live in `highlights.toml`. You can copy specific entries to share with friends. Remember to include any palette names you reference (e.g., share your `colors.toml` `presets` or instruct recipients to add them).
