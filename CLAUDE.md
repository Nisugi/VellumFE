# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

VellumFE is a modern terminal client for GemStone IV (text-based MUD) built in Rust. It supports both TUI (ratatui) and GUI (egui, future) frontends with a shared core architecture.

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
```

## Architecture

```
src/
├── main.rs           # CLI entry point (clap)
├── config.rs         # Configuration loading (TOML files)
├── parser.rs         # Wrayth XML protocol parser
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
│   └── window.rs     # Window state
│
└── frontend/
    ├── mod.rs        # Frontend trait definition
    ├── common/       # Shared types (KeyCode, MouseEvent, color)
    └── tui/          # Ratatui terminal UI
        ├── mod.rs    # TuiFrontend struct
        ├── input.rs  # Keyboard/mouse event handling
        ├── input_handlers.rs # Extracted input handler methods
        ├── widget_manager.rs # Widget cache synchronization
        └── [widgets] # progress_bar.rs, countdown.rs, compass.rs, etc.
```

### Key Architectural Rules

1. **Core layer has NO frontend imports** - `core/` and `data/` modules must not import from `frontend/`
2. **Frontend reads from data layer** - Frontends render by reading `AppCore.ui_state` and `AppCore.game_state`
3. **Widget data vs rendering** - `data/widget.rs` defines data, `frontend/tui/*.rs` handles rendering

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
- `highlights.toml` - Text highlighting rules
- `colors.toml` - Color palette

Defaults embedded from `defaults/` directory via `include_dir` crate.

## Key Code Locations

| Task | Files |
|------|-------|
| Window definitions | `config.rs` (`get_window_template`, `list_window_templates`) |
| Layout positioning | `core/app_core/layout.rs` |
| Min/max window sizes | `widget_min_size()` in `layout.rs` |
| Menu building | `frontend/tui/menu_builders.rs`, `core/app_core/state.rs` (`build_*_menu`) |
| Menu actions | `frontend/tui/menu_actions.rs`, `core/menu_actions.rs` |
| Keyboard input | `frontend/tui/input.rs`, `input_handlers.rs` |
| Dot-commands | `core/app_core/commands.rs` |
| Color parsing | `frontend/tui/colors.rs` (`parse_color_to_ratatui`) |

## Direct eAccess Authentication

Direct mode connects to GemStone IV without Lich proxy. Implementation in `src/network.rs`:

1. **TLS Handshake**: `eaccess.play.net:7910` with SNI disabled, session caching disabled
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

Direct mode requires OpenSSL via vcpkg. On Windows set `VCPKG_ROOT`:
```bash
set VCPKG_ROOT=C:\path\to\vcpkg
```

## Troubleshooting

- **Authentication fails**: Check `~/.vellum-fe/vellum-fe.log`, delete `~/.vellum-fe/simu.pem` to re-download cert
- **Layout issues**: Check `widget_min_size()` in layout.rs for constraint conflicts
- **Menu items missing**: Check `get_visible_templates_by_category()` in config.rs for filtering logic
