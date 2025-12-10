# Core-Data-Frontend Architecture

VellumFE strictly separates code into three layers. This design enables frontend independence and clean testing.

## Layer Overview

```
┌─────────────────────────────────────┐
│           FRONTEND                  │
│   • Renders widgets                 │
│   • Captures user input             │
│   • Converts events                 │
│   • NO business logic               │
├─────────────────────────────────────┤
│             CORE                    │
│   • Processes messages              │
│   • Handles commands                │
│   • Dispatches keybinds             │
│   • Routes data to state            │
├─────────────────────────────────────┤
│             DATA                    │
│   • Pure data structures            │
│   • No behavior                     │
│   • Owned by Core                   │
└─────────────────────────────────────┘
```

## Data Layer

Location: `src/data/`

The data layer contains pure data structures with no logic or rendering.

### UiState

```rust
pub struct UiState {
    pub windows: HashMap<String, WindowState>,
    pub widget_type_index: HashMap<WidgetType, Vec<String>>,  // Widget type cache
    pub focused_window: Option<String>,

    // === Input ===
    pub input_mode: InputMode,
    pub search_input: String,
    pub search_cursor: usize,

    // === Menus (3-level system) ===
    pub popup_menu: Option<PopupMenu>,      // Level 1: Main menu
    pub submenu: Option<PopupMenu>,         // Level 2: Category submenu
    pub nested_submenu: Option<PopupMenu>,  // Level 3: Deep submenu

    // === Display ===
    pub status_text: String,

    // === Mouse/Selection ===
    pub mouse_drag: Option<MouseDragState>,
    pub selection_state: Option<SelectionState>,
    pub selection_drag_start: Option<(u16, u16)>,
    pub link_drag_state: Option<LinkDragState>,
    pub pending_link_click: Option<PendingLinkClick>,
}
```

### GameState

```rust
pub struct GameState {
    // === Session ===
    pub connected: bool,
    pub character_name: Option<String>,

    // === Location ===
    pub room_id: Option<String>,
    pub room_name: Option<String>,
    pub exits: Vec<String>,

    // === Timing (All Unix timestamps) ===
    pub game_time: i64,                     // Authoritative game server time
    pub roundtime_end: Option<i64>,         // When roundtime expires
    pub casttime_end: Option<i64>,          // When casttime expires
    pub estimated_lag_ms: Option<i64>,      // System time - game time (ms)

    // === Character State ===
    pub vitals: Vitals,                     // Health, mana, stamina, spirit (0-100%)
    pub status: StatusInfo,                 // Position, buffs, debuffs
    pub left_hand: Option<String>,
    pub right_hand: Option<String>,
    pub spell: Option<String>,              // Preparing spell
    pub active_streams: HashMap<String, bool>,
    pub inventory: Vec<String>,
    pub active_effects: Vec<String>,
    pub compass_dirs: Vec<String>,

    // === Prompt & Commands ===
    pub last_prompt: String,                // Last prompt text (for echoes)
}

pub struct StatusInfo {
    pub standing: bool, pub kneeling: bool, pub sitting: bool, pub prone: bool,
    pub stunned: bool, pub bleeding: bool,
    pub hidden: bool, pub invisible: bool, pub webbed: bool,
    pub joined: bool, pub dead: bool,
}

pub struct Vitals {
    pub health: u8, pub mana: u8, pub stamina: u8, pub spirit: u8,  // All 0-100%
}
```

### WindowState

```rust
pub struct WindowState {
    pub name: String,
    pub widget_type: WidgetType,
    pub content: WindowContent,         // Content varies by widget type (enum)
    pub position: WindowPosition,       // X, Y, width, height
    pub visible: bool,
    pub focused: bool,
    pub content_align: Option<String>,
}

pub struct WindowPosition {
    pub x: u16, pub y: u16,             // Top-left corner
    pub width: u16, pub height: u16,    // Dimensions
}

/// Content is an enum discriminated by widget type
pub enum WindowContent {
    Text(TextContent),
    TabbedText(TabbedTextContent),
    Progress(ProgressData),
    Countdown(CountdownData),
    Compass(CompassData),
    InjuryDoll(InjuryDollData),
    Indicator(IndicatorData),
    Room(RoomContent),
    Inventory(TextContent),
    CommandInput,
    Hand,
    Spells(TextContent),
    ActiveEffects(ActiveEffectsContent),
    Targets,
    Players,
    Dashboard,
    Performance,
    Empty,  // Spacers or not-yet-implemented
}
```

### TextContent

```rust
pub struct TextContent {
    pub lines: VecDeque<StyledLine>,
    pub scroll_offset: usize,
    pub max_lines: usize,
    pub title: String,
    pub generation: u64,  // Change counter
}
```

### Input Modes

```rust
pub enum InputMode {
    // === Standard Modes ===
    Normal,              // Command input active
    Navigation,          // Vi-style navigation
    History,             // Scrolling command history
    Search,              // Text search mode
    Menu,                // Context menu open

    // === Browser Modes ===
    HighlightBrowser,
    KeybindBrowser,
    ColorPaletteBrowser,
    UIColorsBrowser,
    SpellColorsBrowser,
    ThemeBrowser,

    // === Form Modes (with text editing) ===
    HighlightForm,
    KeybindForm,
    ColorForm,
    SpellColorForm,
    ThemeEditor,
    SettingsEditor,

    // === Special Editors ===
    WindowEditor,
    IndicatorTemplateEditor,
}
```

## Core Layer

Location: `src/core/`

The core layer contains all business logic but no rendering code.

### AppCore

The central orchestrator:

```rust
pub struct AppCore {
    // === Configuration ===
    pub config: Config,
    pub layout: Layout,
    pub baseline_layout: Option<Layout>,     // Original layout for delta-based resizing

    // === State ===
    pub game_state: GameState,
    pub ui_state: UiState,

    // === Message Processing ===
    pub parser: XmlParser,
    pub message_processor: MessageProcessor,

    // === Stream Management ===
    pub current_stream: String,              // Where text is being routed
    pub discard_current_stream: bool,        // Skip if no window exists for stream
    pub stream_buffer: String,               // Buffer for multi-line content

    // === Timing ===
    pub server_time_offset: i64,             // server_time - local_time (milliseconds)

    // === Optional Features ===
    pub perf_stats: PerformanceStats,
    pub show_perf_stats: bool,
    pub sound_player: Option<SoundPlayer>,
    pub tts_manager: TtsManager,

    // === Menu System ===
    pub cmdlist: Option<CmdList>,
    pub menu_request_counter: u32,
    pub pending_menu_requests: HashMap<u32, PendingRequest>,
    pub menu_categories: HashMap<String, Vec<MenuItem>>,
    pub last_link_click_pos: Option<(u16, u16)>,

    // === Navigation State ===
    pub nav_room_id: Option<String>,
    pub lich_room_id: Option<String>,
    pub room_subtitle: Option<String>,
    pub room_components: HashMap<String, Vec<StyledLine>>,  // "room desc", "room objs", etc.
    pub current_room_component: Option<String>,
    pub room_window_dirty: bool,

    // === Runtime Flags ===
    pub running: bool,
    pub needs_render: bool,
    pub layout_modified_since_save: bool,
    pub save_reminder_shown: bool,
    pub base_layout_name: Option<String>,

    // === Keybind Runtime Cache ===
    pub keybind_map: HashMap<KeyEvent, KeyBindAction>,  // Fast O(1) keybind lookups
}
```

### AppCore Methods

```rust
impl AppCore {
    // Message processing
    pub fn process_server_message(&mut self, msg: &str) { ... }

    // User input
    pub fn handle_command(&mut self, cmd: &str) { ... }
    pub fn dispatch_keybind(&mut self, key: &KeyEvent) { ... }

    // State management
    pub fn get_window_state(&self, name: &str) -> Option<&WindowState> { ... }
    pub fn get_window_state_mut(&mut self, name: &str) -> Option<&mut WindowState> { ... }

    // Layout
    pub fn apply_layout(&mut self, layout: Layout) { ... }
}
```

### MessageProcessor

Routes parsed XML to appropriate state:

```rust
impl MessageProcessor {
    pub fn process(
        &mut self,
        element: ParsedElement,
        game_state: &mut GameState,
        ui_state: &mut UiState,
    ) {
        match element {
            ParsedElement::Text { content, stream, .. } => {
                // Route to appropriate window's text_content
            }
            ParsedElement::ProgressBar { id, value, max, .. } => {
                // Update vitals or progress widget
            }
            ParsedElement::Compass { directions } => {
                // Update game_state.exits
            }
            // ... handle all element types
        }
    }
}
```

## Frontend Layer

Location: `src/frontend/`

The frontend layer renders UI and captures input. It has no business logic.

### Frontend Trait

```rust
pub trait Frontend {
    fn poll_events(&mut self) -> Result<Vec<FrontendEvent>>;
    fn render(&mut self, app: &mut dyn Any) -> Result<()>;
    fn cleanup(&mut self) -> Result<()>;
    fn size(&self) -> (u16, u16);
}
```

### FrontendEvent

```rust
pub enum FrontendEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Paste(String),
    FocusGained,
    FocusLost,
}
```

### TUI Frontend

The terminal UI implementation:

```rust
pub struct TuiFrontend {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    widget_manager: WidgetManager,

    // === Menu System ===
    popup_menu: Option<PopupMenu>,           // Main menu (level 1)
    submenu: Option<PopupMenu>,              // Submenu (level 2)
    menu_categories: HashMap<String, Vec<MenuItem>>,

    // === Editors & Browsers ===
    pub window_editor: Option<WindowEditor>,
    pub indicator_template_editor: Option<IndicatorTemplateEditor>,
    pub highlight_browser: Option<HighlightBrowser>,
    pub highlight_form: Option<HighlightForm>,
    pub keybind_browser: Option<KeybindBrowser>,
    pub keybind_form: Option<KeybindForm>,
    pub color_palette_browser: Option<ColorPaletteBrowser>,
    pub color_form: Option<ColorForm>,
    pub uicolors_browser: Option<UIColorsBrowser>,
    pub spell_color_browser: Option<SpellColorBrowser>,
    pub spell_color_form: Option<SpellColorForm>,
    pub theme_browser: Option<ThemeBrowser>,
    pub theme_editor: Option<ThemeEditor>,
    pub settings_editor: Option<SettingsEditor>,

    // === Infrastructure ===
    resize_debouncer: ResizeDebouncer,       // 300ms debounce
    theme_cache: ThemeCache,                 // Avoid HashMap lookup per render
}

impl Frontend for TuiFrontend {
    fn poll_events(&mut self) -> Result<Vec<FrontendEvent>> {
        // Convert crossterm events to FrontendEvent
    }

    fn render(&mut self, app: &mut dyn Any) -> Result<()> {
        // 1. Sync all widget types to caches
        // 2. Render windows in stable order (sorted by name)
        // 3. Render popup menus (3 levels)
        // 4. Render active browsers/editors
    }
}
```

### Widget Manager

Caches and manages frontend widgets:

```rust
pub struct WidgetManager {
    // === Text-based widgets ===
    pub text_windows: HashMap<String, TextWindow>,
    pub tabbed_text_windows: HashMap<String, TabbedTextWindow>,
    pub command_inputs: HashMap<String, CommandInput>,
    pub room_windows: HashMap<String, RoomWindow>,
    pub inventory_windows: HashMap<String, InventoryWindow>,
    pub spells_windows: HashMap<String, SpellsWindow>,

    // === Status widgets ===
    pub progress_bars: HashMap<String, ProgressBar>,
    pub countdowns: HashMap<String, Countdown>,
    pub indicator_widgets: HashMap<String, Indicator>,
    pub dashboard_widgets: HashMap<String, Dashboard>,
    pub active_effects_windows: HashMap<String, ActiveEffects>,
    pub injury_doll_widgets: HashMap<String, InjuryDoll>,

    // === Navigation/Display widgets ===
    pub compass_widgets: HashMap<String, Compass>,
    pub hand_widgets: HashMap<String, Hand>,

    // === Entity widgets ===
    pub targets_widgets: HashMap<String, Targets>,
    pub players_widgets: HashMap<String, Players>,

    // === Utility widgets ===
    pub spacer_widgets: HashMap<String, Spacer>,
    pub performance_widgets: HashMap<String, PerformanceStatsWidget>,

    // === Generation tracking for incremental sync ===
    pub last_synced_generation: HashMap<String, u64>,
}
```

## Layer Communication

### Data Flow Pattern

```
Server Message
      ↓
[Network Layer] ──────→ Raw XML string
      ↓
[Parser]        ──────→ Vec<ParsedElement>
      ↓
[MessageProcessor] ───→ State updates
      ↓
[Data Layer]    ──────→ GameState, UiState, WindowState
      ↓
[Sync Functions] ─────→ Widget updates
      ↓
[Frontend]      ──────→ Rendered frame
```

### User Input Flow

```
User Input
      ↓
[Frontend] ───────────→ FrontendEvent
      ↓
[Input Router] ───────→ Keybind match?
      ↓
[AppCore] ────────────→ Execute action
      ↓
Either:
  • UI Action (state change)
  • Game Command (send to server)
```

## Strict Separation Benefits

### 1. Frontend Independence

Core and Data know nothing about rendering:

```rust
// In core/
// ❌ FORBIDDEN:
use ratatui::*;
use crossterm::*;

// ✅ ALLOWED:
use crate::data::*;
```

This enables adding a GUI frontend without modifying core logic.

### 2. Testability

Core logic can be tested without a terminal:

```rust
#[test]
fn test_message_processing() {
    let mut app = AppCore::new(test_config());

    // Simulate server message
    app.process_server_message("<pushBold/>A goblin<popBold/>");

    // Verify state without rendering
    let main_window = app.ui_state.windows.get("main").unwrap();
    assert!(main_window.text_content.as_ref().unwrap()
        .lines.back().unwrap().segments[0].bold);
}
```

### 3. Clear Boundaries

Each layer has explicit responsibilities:

| Question | Answer |
|----------|--------|
| Where do I add a new data field? | Data layer |
| Where do I process server XML? | Core layer (parser/message processor) |
| Where do I render a new widget? | Frontend layer |
| Where do I handle a keybind? | Core layer (commands/keybinds) |

### 4. Maintainability

Changes are localized:

- Change rendering style → Frontend only
- Change data format → Data + consumers
- Change business logic → Core only
- Add new widget type → All layers, but well-defined boundaries

## State Hierarchy

```
AppCore
├── config: Config                    # Configuration (immutable)
│   ├── connection: ConnectionConfig
│   ├── ui: UiConfig
│   ├── highlights: HashMap<String, HighlightPattern>
│   ├── keybinds: HashMap<String, KeyBindAction>
│   └── colors: ColorConfig
│
├── layout: Layout                    # Current window layout
│   └── windows: Vec<WindowDef>
│
├── game_state: GameState             # Game session state
│   ├── connected: bool
│   ├── character_name: Option<String>
│   ├── room_id, room_name: Option<String>
│   ├── exits: Vec<String>
│   ├── vitals: Vitals
│   └── status: StatusInfo
│
├── ui_state: UiState                 # UI interaction state
│   ├── windows: HashMap<String, WindowState>
│   ├── focused_window: Option<String>
│   ├── input_mode: InputMode
│   └── popup_menu: Option<PopupMenu>
│
└── message_processor: MessageProcessor
    ├── current_stream: String
    └── squelch_matcher: Option<AhoCorasick>
```

## Adding a New Feature

### Example: Add "Stance" Display

**1. Data Layer** - Add data structure:

```rust
// src/data/game_state.rs
pub struct GameState {
    // ... existing fields
    pub stance: Option<Stance>,
}

pub struct Stance {
    pub name: String,
    pub offensive: u8,
    pub defensive: u8,
}
```

**2. Core Layer** - Handle the XML:

```rust
// src/core/messages.rs
ParsedElement::Stance { name, offensive, defensive } => {
    game_state.stance = Some(Stance { name, offensive, defensive });
}
```

**3. Frontend Layer** - Render the widget:

```rust
// src/frontend/tui/stance_widget.rs
pub struct StanceWidget {
    stance: Option<Stance>,
}

impl Widget for &StanceWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Render stance display
    }
}
```

**4. Sync Function** - Connect data to widget:

```rust
// src/frontend/tui/sync.rs
pub fn sync_stance_widgets(
    game_state: &GameState,
    widget_manager: &mut WidgetManager,
) {
    if let Some(stance) = &game_state.stance {
        let widget = widget_manager.stance_widgets
            .entry("stance".to_string())
            .or_insert_with(StanceWidget::new);
        widget.update(stance);
    }
}
```

## See Also

- [Message Flow](./message-flow.md) - Detailed data flow
- [Widget Sync](./widget-sync.md) - Synchronization patterns
- [Performance](./performance.md) - Optimization strategies

