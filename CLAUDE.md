# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

VellumFE is a modern client for GemStone IV (text-based MUD) built in Rust, GPL-3.0-or-later. A shared core drives four frontends: TUI (ratatui), GUI (egui), web (Despana, browser JS served by the Rust binary), and headless. Android/iOS build the library `--no-default-features` (core + parser + network + web + headless).

### Cargo features

`default = ["tui", "gui", "sound", "tts", "desktop", "gamepad"]`. The `vellum-fe` binary requires `tui,gui,desktop`. The feature split exists so the library builds for Android without them — when changing `core/`, `data/`, `parser/`, or `network.rs`, check `cargo test --no-default-features` still passes (CI gates this).

## Build Commands

```bash
# Standard build
cargo build

# Release build (optimized with LTO)
cargo build --release

# Check for errors without building
cargo check

# Run with debug logging
RUST_LOG=debug cargo run -- --port 8000

# Run all tests
cargo test

# Run a single test by name
cargo test test_name

# Run tests in a specific module
cargo test parser::

# Run tests with output shown
cargo test -- --nocapture

# Run with single thread (for tests with shared state)
cargo test -- --test-threads=1

# Feature-independent suite — gates the Android/web build (CI runs this)
cargo test --no-default-features

# Desktop launcher + web presentation without the TUI
cargo build --no-default-features --features gui,desktop
```

### Despana (web frontend) tests

Browser modules are plain ESM tested with Node 22's built-in runner — no npm
install, no package.json step:

```bash
node --test src/frontend/web/assets/despana/*.test.mjs

# End-to-end smoke test; needs Firefox + geckodriver on PATH
# (override with FIREFOX=/abs/path GECKODRIVER=/abs/path)
MOZ_HEADLESS=1 node --test src/frontend/web/assets/despana/browser-smoke.test.mjs
```

### Ignored / opt-in tests

`tests/chokepoints.rs` needs a real Lich map database and is `#[ignore]`d:

```bash
VELLUM_LICH_GAME_DIR=C:/Gemstone/Lich5/data/GSIV \
  cargo test --test chokepoints -- --ignored --nocapture
```

## Architecture

```
src/
├── main.rs           # CLI entry point (clap)
├── config.rs         # Configuration loading (TOML files)
├── parser.rs         # Wrayth XML protocol parser (facade; ≤1100 lines, enforced)
├── parser/           # Parser internals: text.rs (entities/tags/streams glue),
│                     # handlers.rs (tag handlers), dialogs.rs, links.rs,
│                     # builders.rs (in-flight multi-line captures), tests.rs
├── network.rs        # TCP/TLS connections (Lich proxy or direct eAccess)
│
├── core/             # Business logic layer (NO frontend imports)
│   ├── app_core/     # Main application state
│   │   ├── state.rs  # AppCore - central state manager
│   │   ├── layout.rs # Window layout management
│   │   └── commands.rs # Dot-command processing (.menu, .addwindow, etc.)
│   ├── messages.rs   # Message processing pipeline
│   └── input_router.rs # Input routing logic
│
├── data/             # Pure data structures (NO frontend imports)
│   ├── widget.rs     # Widget data types (TextSpan, ActiveEffect, etc.)
│   ├── ui_state.rs   # UI state (InputMode, PopupMenu, etc.)
│   ├── input.rs      # Input event types (KeyCode, KeyEvent, MouseEvent)
│   └── window.rs     # Window state
│
└── frontend/
    ├── mod.rs        # Frontend trait definition
    ├── common/       # Shared frontend types (Color, Rect, TextInput)
    ├── gui/          # egui desktop frontend (app.rs is a facade; see caps)
    ├── web/          # Despana: Rust server + assets/despana/*.mjs browser code
    ├── headless/     # No-render runtime (Android/iOS host the web UI over it)
    └── tui/          # Ratatui terminal UI
        ├── mod.rs    # TuiFrontend struct
        ├── input.rs  # Keyboard/mouse event handling
        ├── input_handlers.rs # Extracted input handler methods
        ├── widget_manager.rs # Widget cache synchronization
        └── [widgets] # progress_bar.rs, countdown.rs, compass.rs, etc.
```

### Key Architectural Rules

`tests/architecture.rs` enforces these mechanically — run `cargo test --test architecture` after structural changes:

1. **Core layer has NO frontend imports** - `core/` and `data/` must not import from `frontend/`, nor reference `egui` (input event types live in `data/input.rs` for this reason)
2. **`core/` stays Android-safe** - no desktop-only APIs, so the `--no-default-features` build keeps working
3. **`gui/` must not reference `tui/`** - the two frontends are siblings, not layers
4. **Facade line caps** - split parents hold type definitions and dispatchers only; new methods go in the matching submodule. If a cap trips, **move code down, don't raise the cap**:
   `gui/app.rs` 3600 · `core/app_core/state.rs` 2100 · `tui/window_editor.rs` 1700 · `core/messages.rs` 800 · `gui/app/widgets.rs` 700 · `parser.rs` 1100 · `config.rs` 700
5. **Catalog access goes through the seams** - template enumeration, gating, and id-mapping are reachable only via `core::local_catalog` and `core::view_resolver` (or inside `config/`). The id-maps `dialog_id_to_template` / `stream_id_to_template` / `id_has_widget_template` are deleted; referencing them fails the test.
6. **Frontend reads from data layer** - Frontends render by reading `AppCore.ui_state` and `AppCore.game_state`
7. **Widget data vs rendering** - `data/widget.rs` defines data, `frontend/tui/*.rs` handles rendering

### Data Flow

```
Network (TCP) → Parser (XML) → Core (AppCore) → Data Layer → Frontend (TUI)
                                    ↑
                              User Input ←────────────────────┘
```

## Widget System

Widgets are defined in layout.toml and rendered based on type:

| Widget Type | File | Purpose |
|-------------|------|---------|
| `text` | text_window.rs | Scrollable text (main, thoughts, combat) |
| `tabbedtext` | tabbed_text_window.rs | Multi-tab text window |
| `progress` | progress_bar.rs | Health/mana/stamina bars |
| `countdown` | countdown.rs | RT/CT timers |
| `compass` | compass.rs | Navigation compass |
| `hand` | hand.rs | Left/right hand items |
| `indicator` | indicator.rs | Status indicators (kneeling, hidden) |
| `dashboard` | dashboard.rs | Character stats grid |
| `spacer` | spacer.rs | Layout spacing (1x1 minimum) |
| `hotkeybar` | hotkey_bar.rs | Command buttons with condition-driven styling + countdowns (hotbars.toml, `.hotbars` editor) |

### Adding New Widget Types

1. Create `frontend/tui/new_widget.rs` with render function
2. Add to `frontend/tui/mod.rs` exports
3. Register in `config.rs` widget templates (`list_window_templates`, `get_window_template`)
4. Add to `widget_min_size()` in `layout.rs` if special constraints needed

## Configuration System

Config files stored in `~/.vellum-fe/` (or `VELLUM_FE_DIR` env var):

- `config.toml` - Main settings (connection, UI options)
- `layout.toml` - Window positions and sizes
- `keybinds.toml` - Key bindings
- `controller.toml` - Gamepad binds, wheels, rumble, tuning (global base + per-character override)
- `hotbars.toml` - Hotkey bar definitions (bars of command buttons)
- `highlights.toml` - Text highlighting rules
- `colors.toml` - Color palette

Defaults embedded from `defaults/` directory via `include_dir` crate.

## Key Code Locations

| Task | Files |
|------|-------|
| Window definitions | `config.rs` (`get_window_template`, `list_window_templates`) |
| Layout positioning | `core/app_core/layout.rs` |
| Min/max window sizes | `widget_min_size()` in `layout.rs` |
| Menu building | `frontend/tui/menu_builders.rs`, `core/app_core/state/menus.rs` (`build_*_menu`) |
| Menu actions | `frontend/tui/menu_actions.rs`, `core/menu_actions.rs` |
| Keyboard input | `frontend/tui/input.rs`, `input_handlers.rs` |
| Dot-commands | `core/app_core/commands.rs` |
| Color parsing | `frontend/tui/colors.rs` (`parse_color_to_ratatui`) |

## Parser Invariants

The Wrayth XML parser (`src/parser.rs` + `src/parser/`) has behavior contracts
pinned by a golden snapshot; know them before touching it:

- **Golden snapshot workflow**: `tests/parser_characterization.rs` runs every
  fixture through the parser and compares against
  `tests/data/parser_golden.snap`. ANY behavior change must show up as a
  reviewed diff: regenerate with `UPDATE_PARSER_GOLDEN=1 cargo test --test
  parser_characterization`, review, commit the diff with the change.
  (Window templates have the same workflow: `UPDATE_GOLDEN=1 cargo test
  --test template_characterization`.)
- **Nothing is silently dropped**: unknown tag names render as literal text
  with a `warn!`; known-but-unhandled tags (sorted `KNOWN_WIRE_TAGS` set in
  `parser/text.rs`) swallow with a debug log. New wire tags must be added to
  the set.
- **Streams are a stack**: `pushStream` nests; a pop restores the enclosing
  stream via `ParsedElement::StreamResume` (re-route WITHOUT arrival side
  effects). `<prompt>` force-closes any stream still open (warn = eaten
  popStream upstream) and resets all style/bold/mono state.
- **Multi-line captures**: dialogData/openDialog/component/compDef/
  worldEvent/compass may close on a later line (`PairedCapture` in
  `parser/builders.rs`); prompt mid-capture discards with a warn; 256KiB cap.
  prompt/left/right/spell/inv are same-line-only by policy.
- **Mangled markup**: a `<` preceded by `$` is broken server escaping —
  rendered as literal text, never interpreted.
- **Entities**: 5 named + numeric (`&#nnn;`/`&#xhh;`), decoded exactly once
  (`decode_entities_stable` is for double-encoded display titles ONLY).
- **Facade limit**: `parser.rs` must stay ≤1100 lines (architecture test);
  new code goes in the `parser/` submodules.
- **Live instrumentation**: `grep -a "\[parser\]" ~/.vellum-fe/vellum-fe.log`
  after a session shows unknown tags, eaten popStreams, and torn captures.

## Direct eAccess Authentication

Direct mode connects to GemStone IV without Lich proxy. Implementation in `src/network.rs`:

1. **TLS Handshake**: `eaccess.play.net:7910` via `native-tls` (OS-native stack) with SNI disabled. The server only speaks TLS 1.2 with static-RSA key exchange (`AES128-GCM-SHA256`), so rustls cannot be used — it refuses to implement non-forward-secret suites.
2. **Challenge-Response**: Send "K", receive 32-byte hash key, obfuscate password: `((password[i] - 32) ^ hashkey[i]) + 32`
3. **Session**: Login payload `A\t{account}\t{encoded_password}\n`

**Critical**: The `send_line` function must send message + newline in a single TLS write (not two separate writes). This is essential for the protocol to work.

```bash
# Direct connection
vellum-fe --direct --account ACCOUNT --password PASS --game prime --character NAME

# Via Lich proxy
vellum-fe --port 8000 --character NAME
```

## Dependencies

TLS uses the OS-native stack via `native-tls` (SChannel on Windows, Security.framework on macOS) — no OpenSSL, Perl, or vcpkg needed there. Linux statically links a vendored OpenSSL (needs Perl, universal on Linux) so release binaries have no libssl runtime dependency.

## Troubleshooting

- **Authentication fails**: Check `~/.vellum-fe/vellum-fe.log`, delete `~/.vellum-fe/simu.pem` to re-download cert
- **Layout issues**: Check `widget_min_size()` in layout.rs for constraint conflicts
- **Menu items missing**: Check `get_visible_templates_by_category()` in config.rs for filtering logic
