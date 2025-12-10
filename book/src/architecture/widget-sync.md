# Widget Synchronization

VellumFE uses generation-based change detection to efficiently sync data to frontend widgets without full comparisons.

## Overview

The sync system:
- Detects changes via generation counters (not content comparison)
- Syncs only changed data
- Handles incremental updates
- Supports full resyncs when needed

## Generation-Based Change Detection

### TextContent Generation

Every `TextContent` has a generation counter:

```rust
pub struct TextContent {
    pub lines: VecDeque<StyledLine>,
    pub scroll_offset: usize,
    pub max_lines: usize,
    pub title: String,
    pub generation: u64,  // Increments on every add_line()
}

impl TextContent {
    pub fn add_line(&mut self, line: StyledLine) {
        self.lines.push_back(line);

        // Trim oldest lines when over limit
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }

        self.generation += 1;  // Always increment
    }
}
```

### Sync Logic

```rust
// Get last synced generation for this window
let last_synced_gen = last_synced_generation.get(name).copied().unwrap_or(0);
let current_gen = text_content.generation;

// Only sync if generation changed
if current_gen > last_synced_gen {
    let gen_delta = (current_gen - last_synced_gen) as usize;

    // If delta > line count, need full resync (wrapped around or cleared)
    let needs_full_resync = gen_delta > text_content.lines.len();

    if needs_full_resync {
        text_window.clear();
    }

    // Add only new lines
    let lines_to_add = if needs_full_resync {
        text_content.lines.len()
    } else {
        gen_delta.min(text_content.lines.len())
    };

    let skip_count = text_content.lines.len().saturating_sub(lines_to_add);
    for line in text_content.lines.iter().skip(skip_count) {
        text_window.add_line(line.clone());
    }

    // Update synced generation
    last_synced_generation.insert(name.clone(), current_gen);
}
```

### Benefits

1. **O(1) change detection** - Compare numbers, not content
2. **Incremental updates** - Only new lines synced
3. **Automatic full resync** - Detects when buffer cleared
4. **Multi-window support** - Each window tracked independently

## Sync Architecture

### WidgetManager

The `WidgetManager` caches all frontend widgets and tracks sync state:

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

### Sync Functions

Each widget type has a dedicated sync function:

| Function | Widget Type | Data Source |
|----------|-------------|-------------|
| `sync_text_windows()` | Text windows | `WindowState.text_content` |
| `sync_tabbed_text_windows()` | Tabbed text | `WindowState.text_content` (per tab) |
| `sync_command_inputs()` | Command input | `UiState.command_input` |
| `sync_progress_bars()` | Progress bars | `GameState.vitals` |
| `sync_countdowns()` | Countdown timers | `GameState.roundtime` |
| `sync_compass_widgets()` | Compass | `GameState.exits` |
| `sync_hand_widgets()` | Hand display | `GameState.left_hand/right_hand` |
| `sync_indicator_widgets()` | Status indicators | `GameState.status` |
| `sync_injury_doll_widgets()` | Injury display | `GameState.injuries` |
| `sync_active_effects()` | Buffs/debuffs | `GameState.active_effects` |
| `sync_room_windows()` | Room description | `GameState.room_*` |
| `sync_dashboard_widgets()` | Dashboard | Multiple sources |
| `sync_inventory_windows()` | Inventory | `GameState.inventory` |
| `sync_spells_windows()` | Spells | `GameState.spells` |
| `sync_targets_widgets()` | Targets | Combat target list |
| `sync_players_widgets()` | Players | Players in room |
| `sync_spacer_widgets()` | Spacers | Layout placeholders |
| `sync_performance_widgets()` | Performance | `PerformanceStats` |

### Sync Pattern

Each sync function follows this pattern:

```rust
pub fn sync_text_windows(
    ui_state: &UiState,
    layout: &Layout,
    widget_manager: &mut WidgetManager,
    theme: &Theme,
) {
    // 1. Find windows of this type in layout
    for window_def in layout.windows.iter().filter(|w| w.widget_type == WidgetType::Text) {
        let name = &window_def.name;

        // 2. Get data from state
        let window_state = match ui_state.windows.get(name) {
            Some(ws) => ws,
            None => continue,
        };

        let text_content = match &window_state.text_content {
            Some(tc) => tc,
            None => continue,
        };

        // 3. Ensure widget exists in cache
        let text_window = widget_manager.text_windows
            .entry(name.clone())
            .or_insert_with(|| TextWindow::new(name.clone()));

        // 4. Apply configuration
        text_window.set_width(window_def.width);
        text_window.set_colors(theme.resolve_window_colors(window_def));

        // 5. Check generation for changes
        let last_gen = widget_manager.last_synced_generation
            .get(name).copied().unwrap_or(0);
        let current_gen = text_content.generation;

        if current_gen <= last_gen {
            continue;  // No changes
        }

        // 6. Sync content
        let delta = (current_gen - last_gen) as usize;
        let needs_full_resync = delta > text_content.lines.len();

        if needs_full_resync {
            text_window.clear();
        }

        let lines_to_add = if needs_full_resync {
            text_content.lines.len()
        } else {
            delta.min(text_content.lines.len())
        };

        let skip = text_content.lines.len().saturating_sub(lines_to_add);
        for line in text_content.lines.iter().skip(skip) {
            text_window.add_line(line.clone());
        }

        // 7. Update generation
        widget_manager.last_synced_generation.insert(name.clone(), current_gen);
    }
}
```

## Special Cases

### Tabbed Text Windows

Tabbed windows track generation per tab:

```rust
pub struct TabbedTextSyncState {
    pub tab_generations: HashMap<String, u64>,  // Per-tab tracking
}

// In sync:
for tab in &tabbed_window.tabs {
    let tab_gen = sync_state.tab_generations.get(&tab.name).copied().unwrap_or(0);
    let current_gen = tab.text_content.generation;

    if current_gen > tab_gen {
        // Sync this tab
    }
}
```

### Progress Bars

Progress bars don't use generations - they directly copy values:

```rust
pub fn sync_progress_bars(
    game_state: &GameState,
    widget_manager: &mut WidgetManager,
) {
    // Direct sync - values are small, comparison is cheap
    if let Some(widget) = widget_manager.progress_bars.get_mut("health") {
        widget.set_value(game_state.vitals.health.0);
        widget.set_max(game_state.vitals.health.1);
    }
}
```

### Countdowns

Countdowns track their own state and update per-frame:

```rust
pub fn sync_countdowns(
    game_state: &GameState,
    widget_manager: &mut WidgetManager,
) {
    if let Some(widget) = widget_manager.countdowns.get_mut("roundtime") {
        let remaining = game_state.roundtime_end
            .map(|end| end.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::ZERO);
        widget.set_remaining(remaining);
    }
}
```

## Performance Optimizations

### Width-Based Invalidation

Text windows invalidate wrap cache only when width changes:

```rust
impl TextWindow {
    pub fn set_width(&mut self, width: u16) {
        if self.current_width != width {
            self.current_width = width;
            self.invalidate_wrap_cache();
        }
    }
}
```

### Lazy Line Wrapping

Lines are wrapped on-demand during render, not during sync:

```rust
fn render_visible_lines(&self, visible_height: usize) -> Vec<WrappedLine> {
    // Only wrap lines that will actually be displayed
    let visible_range = self.calculate_visible_range(visible_height);
    self.lines
        .iter()
        .skip(visible_range.start)
        .take(visible_range.len())
        .flat_map(|line| self.wrap_line(line))
        .collect()
}
```

### Dirty Tracking

Widgets track whether they need re-rendering:

```rust
impl TextWindow {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    fn add_line(&mut self, line: StyledLine) {
        // ...
        self.dirty = true;
    }
}
```

## Sync Timing

### When Sync Happens

Sync occurs every frame in the render loop:

```rust
// In TuiFrontend::render()
fn render(&mut self, app: &mut AppCore) -> Result<()> {
    // Sync all widget types
    sync_text_windows(&app.ui_state, &app.layout, &mut self.widget_manager, &app.theme);
    sync_tabbed_text_windows(&app.ui_state, &app.layout, &mut self.widget_manager, &app.theme);
    sync_progress_bars(&app.game_state, &mut self.widget_manager);
    sync_countdowns(&app.game_state, &mut self.widget_manager);
    sync_compass_widgets(&app.game_state, &mut self.widget_manager);
    // ... more sync functions

    // Then render
    self.terminal.draw(|frame| {
        // Render all widgets
    })?;

    Ok(())
}
```

### Sync Frequency

- **~60 times per second** - Once per frame
- **Generation checks are O(1)** - Very fast
- **Only changed widgets sync content** - Efficient

## Debugging Sync Issues

### Check Generation

```rust
// Debug: Print generation info
tracing::debug!(
    "Window '{}': gen={}, last_synced={}, lines={}",
    name,
    text_content.generation,
    last_synced_gen,
    text_content.lines.len()
);
```

### Check Widget Cache

```rust
// Debug: Check if widget exists
if widget_manager.text_windows.contains_key(name) {
    tracing::debug!("Widget '{}' exists in cache", name);
} else {
    tracing::warn!("Widget '{}' NOT in cache", name);
}
```

### Common Issues

| Symptom | Possible Cause | Solution |
|---------|----------------|----------|
| Content not appearing | Widget not in layout | Add to layout.toml |
| Content appears late | High generation delta | Check buffer_size |
| Content duplicated | Generation not tracking | Check add_line increments |
| Old content showing | Full resync needed | Clear and resync |

## See Also

- [Message Flow](./message-flow.md) - How data reaches sync
- [Performance](./performance.md) - Optimization details
- [Core-Data-Frontend](./core-data-frontend.md) - Architecture overview

