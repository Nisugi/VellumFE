# PROJECT KNOWLEDGE BASE

**Generated:** 2025-12-29T19:53:00Z
**Commit:** backup-perception-changes
**Branch:** main

## OVERVIEW

VellumFE is a modern terminal client for GemStone IV (MUD) built in Rust with dual frontend architecture (TUI + future GUI). Features comprehensive widget system, direct eAccess authentication, and embedded configuration.

## STRUCTURE

```
{project-root}/
├── src/               # Main source code
│   ├── core/         # Business logic layer (NO frontend imports)
│   ├── data/         # Pure data structures
│   ├── frontend/      # Frontend abstraction trait
│   │   ├── tui/      # Ratatui terminal UI (67 widget files)
│   │   └── common/   # Shared frontend types
│   ├── config/       # Configuration system
│   ├── network/       # TCP/TLS connections
│   ├── parser/       # XML protocol parser
│   └── window_position/ # Cross-platform window management
├── tests/             # Integration tests with XML fixtures
├── defaults/          # Embedded default configurations
├── book/              # mdBook documentation
├── .github/workflows/  # CI/CD pipelines
└── .claude/          # Development workflow docs
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add new widget type | `src/frontend/tui/` | Create render function, register in config.rs |
| Modify game state | `src/core/app_core/state.rs` | Central AppCore state management |
| Update protocol | `src/parser.rs` | Wrayth XML protocol handling |
| Configuration changes | `src/config.rs` | TOML-based system with validation |
| Network layer | `src/network.rs` | Direct eAccess + Lich proxy support |
| TUI rendering | `src/frontend/tui/` | Ratatui widgets and layout management |

## CODE MAP

| Symbol | Type | Location | Refs | Role |
|--------|------|----------|-------|------|
| AppCore | Struct | `src/core/app_core/state.rs` | High | Central orchestrator for game state, UI state, and subsystems |
| MessageProcessor | Struct | `src/core/messages.rs` | High | Routes XML messages to appropriate state updates |
| GameState | Struct | `src/core/app_core/state.rs` | High | Game session data (connection, character, room, vitals) |
| NetworkManager | Struct | `src/network.rs` | Medium | Handles TCP/TLS connections and authentication |
| Frontend | Trait | `src/frontend/mod.rs` | Medium | Abstraction for TUI/GUI implementations |
| WidgetManager | Struct | `src/frontend/tui/widget_manager.rs` | Medium | Widget cache synchronization and rendering coordination |
| Parser | Struct | `src/parser.rs` | Medium | XML parser for Wrayth protocol with stream routing |

### Module Dependencies

```
main.rs ──imports──> core/AppCore
   │                    │
   ├─imports──> config/     <──imports──> data/
   │                    │
   ├─imports──> network/    <──imports──> core/
   │                    │
   ├─imports──> parser.rs    ──imports──> data/
   │                    │
   └─imports──> frontend/tui/
                         │
                         └─imports──> data/
```

## CONVENTIONS

- **Core layer isolation**: `src/core/` has NO frontend imports - architectural rule enforced
- **Data layer purity**: `src/data/` contains only pure structs, no I/O or rendering
- **Frontend trait pattern**: All frontends implement `Frontend` trait with `poll_events()`, `render()`, `cleanup()`
- **Async networking**: Uses tokio for TCP/TLS with non-blocking I/O
- **Configuration embedded**: Default configs embedded via `include_dir` crate, fallback to `~/.vellum-fe/`
- **Error handling**: Uses `anyhow::Result` throughout, explicit error propagation

## ANTI-PATTERNS (THIS PROJECT)

- **Core contamination**: Frontend imports in `src/core/` are forbidden - violates architecture
- **Unsafe Windows code**: Window positioning uses unsafe Win32 APIs (justified, documented)
- **Legacy color handling**: Old color system in config.rs with manual parsing (new system exists)
- **XML parsing regex**: Avoid regex for XML structure - use proper parser in `parser.rs`

## UNIQUE STYLES

- **Widget data/rendering split**: Data in `src/data/widget.rs`, rendering in `src/frontend/tui/*.rs`
- **Stream-based text routing**: Messages routed to windows based on XML stream tags
- **Dual connectivity**: Supports both Lich proxy and direct eAccess authentication
- **Platform abstraction**: Window position persistence via trait for Windows/Linux/macOS
- **Comprehensive testing**: 1,003 tests with XML fixtures for integration coverage

## COMMANDS

```bash
# Build
cargo build
cargo build --release

# Test  
cargo test
cargo test -- --nocapture

# Development
RUST_LOG=debug cargo run -- --port 8000

# Migration
cargo run -- migrate-layout --src old/path --out new/path

# Configuration validation
cargo run -- validate-layout --layout path.toml
```

## NOTES

- **Window position persistence**: Saves/restore terminal position per character (newly implemented)
- **Direct eAccess**: Uses challenge-response authentication with password obfuscation
- **Lich proxy support**: Requires authentication key provided by Lich as %key%
- **Sound system**: Optional rodio dependency with TTS support
- **Documentation**: Built with mdBook, deployed to GitHub Pages
- **Cross-platform builds**: CI produces Windows binaries, macOS universal binaries, Linux tarballs