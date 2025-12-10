# Architecture Overview

VellumFE is built on a rigorous three-layer architecture designed for maintainability, testability, and future GUI support.

## Design Philosophy

VellumFE separates concerns into distinct layers:

1. **Data Layer** - Pure data structures, no rendering logic
2. **Core Layer** - Business logic, message processing
3. **Frontend Layer** - Rendering, user interaction

This separation enables:
- **Multiple frontends** - TUI now, GUI later, same logic
- **Testability** - Core logic without rendering dependencies
- **Maintainability** - Clear boundaries and responsibilities
- **Performance** - Efficient data flow and change detection

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      FRONTEND LAYER                             │
│  ┌─────────────────┐              ┌─────────────────┐           │
│  │   TUI Frontend  │              │   GUI Frontend  │           │
│  │   (ratatui)     │              │   (egui)        │           │
│  │                 │              │   [planned]     │           │
│  └────────┬────────┘              └────────┬────────┘           │
│           │                                │                    │
│           │    Frontend Trait Interface    │                    │
│           └────────────────┬───────────────┘                    │
├────────────────────────────┼────────────────────────────────────┤
│                      CORE LAYER                                 │
│  ┌─────────────────────────┴─────────────────────────┐          │
│  │                    AppCore                        │          │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐  │          │
│  │  │MessageProc. │ │  Commands   │ │  Keybinds   │  │          │
│  │  └─────────────┘ └─────────────┘ └─────────────┘  │          │
│  └───────────────────────┬───────────────────────────┘          │
├──────────────────────────┼──────────────────────────────────────┤
│                      DATA LAYER                                 │
│  ┌───────────────────────┴───────────────────────────┐          │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐  │          │
│  │  │  GameState  │ │   UiState   │ │ WindowState │  │          │
│  │  └─────────────┘ └─────────────┘ └─────────────┘  │          │
│  └───────────────────────────────────────────────────┘          │
└─────────────────────────────────────────────────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    │   Network Layer   │
                    │  (Tokio Async)    │
                    └───────────────────┘
```

## Module Organization

```
src/
├── main.rs              # Entry point, CLI parsing
├── lib.rs               # Library exports
│
├── core/                # Business logic
│   ├── mod.rs
│   ├── state.rs         # GameState
│   ├── messages.rs      # MessageProcessor
│   ├── input_router.rs  # Input dispatch
│   ├── menu_actions.rs  # Menu action handlers
│   └── app_core/
│       ├── mod.rs
│       ├── state.rs     # AppCore
│       ├── commands.rs  # Built-in commands
│       ├── keybinds.rs  # Keybind dispatch
│       └── layout.rs    # Layout management
│
├── data/                # Data structures
│   ├── mod.rs
│   ├── ui_state.rs      # UiState, InputMode
│   ├── widget.rs        # TextContent, ProgressData
│   └── window.rs        # WindowState, WidgetType
│
├── frontend/            # Rendering
│   ├── mod.rs           # Frontend trait
│   ├── events.rs        # FrontendEvent
│   ├── common/          # Shared types
│   │   ├── color.rs
│   │   ├── input.rs
│   │   ├── rect.rs
│   │   └── text_input.rs
│   ├── tui/             # Terminal UI
│   │   ├── mod.rs
│   │   ├── runtime.rs   # Event loop
│   │   ├── widget_manager.rs
│   │   ├── text_window.rs
│   │   ├── progress_bar.rs
│   │   └── ... (30+ widget files)
│   └── gui/             # Native GUI (planned)
│       └── mod.rs
│
├── config.rs            # Configuration
├── config/
│   ├── highlights.rs
│   └── keybinds.rs
│
├── parser.rs            # XML parser
├── cmdlist.rs           # Context menu data
├── network.rs           # Network connections
├── theme.rs             # Theme system
├── sound.rs             # Audio playback
├── tts/                 # Text-to-speech
├── clipboard.rs         # Clipboard access
├── selection.rs         # Text selection
└── performance.rs       # Performance tracking
```

## Layer Rules

### Import Restrictions

| Layer | Can Import From |
|-------|-----------------|
| Data | Nothing else |
| Core | Data only |
| Frontend | Data and Core |

```rust
// ✅ ALLOWED
// data/ imports nothing from core/ or frontend/
// core/ imports from data/
// frontend/ imports from data/ and core/

// ❌ FORBIDDEN
// data/ importing from core/ or frontend/
// core/ importing from frontend/
```

### Responsibilities

| Layer | Responsibilities |
|-------|------------------|
| **Data** | Define state structures, no behavior |
| **Core** | Process messages, handle commands, manage state |
| **Frontend** | Render widgets, capture input, convert events |

## Key Components

### AppCore

The central orchestrator containing all state and logic:

```rust
pub struct AppCore {
    pub config: Config,
    pub layout: Layout,
    pub game_state: GameState,
    pub ui_state: UiState,
    pub message_processor: MessageProcessor,
    pub parser: XmlParser,
}
```

### Frontend Trait

Interface for UI implementations:

```rust
pub trait Frontend {
    fn poll_events(&mut self) -> Result<Vec<FrontendEvent>>;
    fn render(&mut self, app: &mut dyn Any) -> Result<()>;
    fn cleanup(&mut self) -> Result<()>;
    fn size(&self) -> (u16, u16);
}
```

### Event Loop

The main loop coordinates all components:

1. Poll user input (non-blocking)
2. Process server messages
3. Update state
4. Render frame
5. Sleep to maintain frame rate

## Architecture Sections

This section covers:

- [Core-Data-Frontend](./core-data-frontend.md) - Three-layer architecture in depth
- [Message Flow](./message-flow.md) - Data flow through the system
- [Parser Protocol](./parser-protocol.md) - XML protocol handling
- [Widget Sync](./widget-sync.md) - Generation-based synchronization
- [Theme System](./theme-system.md) - Color resolution and theming
- [Browser Editors](./browser-editors.md) - Popup configuration system
- [Performance](./performance.md) - Optimization strategies

## Design Principles

1. **Separation of Concerns** - Each layer has clear responsibilities
2. **Frontend Independence** - Core logic works with any UI
3. **Event-Driven** - Asynchronous network, synchronous rendering
4. **State Centralization** - All state in Data layer
5. **Extensibility** - New widgets/frontends plug in cleanly

