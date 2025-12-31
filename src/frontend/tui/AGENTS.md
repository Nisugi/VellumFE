# TUI FRONTEND KNOWLEDGE BASE

**Generated:** 2025-12-29T19:55:00Z
**Scope:** src/frontend/tui (67 widget files)

## OVERVIEW

Ratatui-based terminal frontend with 67 specialized widgets and comprehensive caching system.

## STRUCTURE

```
src/frontend/tui/
├── widget_manager.rs     # Central widget cache coordinator
├── runtime.rs            # Main event loop and rendering
├── frontend_impl.rs      # Frontend trait implementation
├── sync*.rs             # State synchronization utilities
├── widget_traits.rs     # Common widget behavior traits
├── [67 widget files]     # Individual widget implementations
└── [*_browser/*_form]   # Interactive configuration widgets
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add new widget | Create new file, register in WidgetManager | Follow existing widget patterns |
| Widget caching | widget_manager.rs | All widget instances cached by name |
| Input handling | input.rs, input_handlers.rs | Event routing and keybind processing |
| Theme rendering | theme_cache.rs | Performance optimization for theme lookups |
| State sync | sync.rs, sync_macros.rs | Core→widget state synchronization |

## CONVENTIONS

- **Widget file naming**: snake_case matching widget type (e.g., `progress_bar.rs`)
- **Cache management**: All widgets stored in WidgetManager HashMaps
- **Sync pattern**: Each widget has `sync_from_state()` method for Core→Widget updates
- **Editor widgets**: Browser/Form pairs for interactive configuration
- **Render isolation**: Widgets only render, never modify Core state directly

## ANTI-PATTERNS

- **Direct Core mutation**: Widgets must NEVER modify AppCore state
- **Widget cross-references**: Avoid direct widget-to-widget communication
- **Blocking I/O**: Never block in render or event handling
- **Theme lookups**: Use ThemeCache, avoid direct HashMap access every render

## COMMANDS

```bash
# Run with TUI debugging
RUST_LOG=debug cargo run

# Test specific widget
cargo test -- path::to::widget_file

# Performance profiling
cargo run --release -- --port 8000
```