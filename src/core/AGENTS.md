# PROJECT KNOWLEDGE BASE

**Generated:** 2025-12-29T19:53:00Z
**Commit:** backup-perception-changes
**Branch:** main

## OVERVIEW

Pure business logic layer - handles game state, message processing, UI interactions, and command routing without frontend dependencies.

## STRUCTURE

```
src/core/
├── app_core/          # Core application orchestrator (AppCore)
│   ├── state.rs      # AppCore struct, constructors, process_server_data
│   ├── state/        # AppCore impl submodules: windows, window_lifecycle,
│   │                 #   menus, persistence, travel_ticks, remote, focus
│   ├── commands.rs   # Dot command parsing and execution
│   ├── keybinds.rs   # Keybind mapping and action handling
│   └── layout.rs     # Layout management and window positioning
├── messages.rs        # MessageProcessor facade (struct, config, tests)
├── messages/          # MessageProcessor impl submodules: element (XML
│                      #   dispatch), component, flush_line, buffers, routing
├── state.rs           # GameState (vitals, status, inventory, room data)
├── highlight_engine.rs  # Text highlighting with Aho-Corasick optimization
├── input_router.rs    # Menu input routing based on context
├── menu_actions.rs    # Shared action vocabulary for UI widgets
├── bounty_parser.rs   # Compact bounty text formatting
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add game state | `src/core/state.rs` | Central GameState struct with vitals, status, inventory, room data |
| Process XML messages | `src/core/messages/element.rs` | `process_element` dispatches every ParsedElement variant |
| Update vitals/hands | `src/core/messages/element.rs` | Updates GameState from progress bars, hands, status indicators |
| Handle dialog data | `src/core/messages/component.rs` | dialogData ingest, shown-dialog reflection (routing in `messages/routing.rs`) |
| Flush text lines | `src/core/messages/flush_line.rs` | Highlights, squelch, redirects, sorter, TTS per completed line |
| Apply highlights | `src/core/highlight_engine.rs` | CoreHighlightEngine with fast Aho-Corasick matching |
| Parse bounty text | `src/core/bounty_parser.rs` | Transforms verbose bounty into compact format |
| Route input actions | `src/core/input_router.rs` | Maps key events to actions based on context |
| Handle menu widgets | `src/core/menu_actions.rs` | Defines MenuAction enum for consistent widget behavior |
| Execute keybinds | `src/core/app_core/keybinds.rs` | Converts keybinds to actions and executes them |
| Manage layouts | `src/core/app_core/layout.rs` | Handles window positioning, loading, and proportional resizing |
| Execute commands | `src/core/app_core/commands.rs` | Handles all dot commands (.quit, .help, .savelayout, etc.) |

## CONVENTIONS

- **Core isolation**: NO frontend imports allowed - maintains clean architecture
- **State mutation pattern**: State updates flow through MessageProcessor → GameState → UI state
- **Event-driven**: XML elements trigger discrete state updates rather than polling
- **Performance optimization**: Aho-Corasick for fast highlight pattern matching

## ANTI-PATTERNS

- **Core contamination**: Frontend imports in src/core violate architectural separation
- **Direct state mutation**: Never modify UI state directly - use proper message routing
- **Bypass message processor**: Don't manually trigger state updates - route through MessageProcessor