# PROJECT KNOWLEDGE BASE

**Generated:** 2025-12-29T19:53:00Z
**Commit:** backup-perception-changes
**Branch:** main

## OVERVIEW

Pure business logic layer - handles game state, message processing, UI interactions, and command routing without frontend dependencies.

## STRUCTURE

```
src/core/
├── app_core/          # Core application orchestrator
│   ├── state.rs      # Central game state (GameState, vitals, room data)
│   ├── commands.rs   # Dot command parsing and execution
│   ├── keybinds.rs  # Keybind mapping and action handling
│   ├── layout.rs     # Layout management and window positioning
│   ├── highlight_engine.rs  # Text highlighting with Aho-Corasick optimization
│   ├── input_router.rs  # Menu input routing based on context
│   ├── menu_actions.rs  # Shared action vocabulary for UI widgets
│   ├── messages.rs        # XML message processing and state updates
│   ├── bounty_parser.rs  # Compact bounty text formatting
│   └── state.rs           # Game session state (alternative to app_core/state.rs)
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add game state | `src/core/state.rs` | Central GameState struct with vitals, status, inventory, room data |
| Process XML messages | `src/core/messages.rs` | MessageProcessor routes parsed XML to update state |
| Update vitals/hands | `src/core/messages.rs` | Updates GameState from progress bars, hands, status indicators |
| Handle dialog data | `src/core/messages.rs` | Processes `<dialog>` tags for popup management |
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