# Performance Optimization

VellumFE is designed for smooth, low-latency gameplay with 60+ FPS rendering and sub-millisecond event processing.

## Performance Goals

- **60+ FPS** - Smooth terminal rendering
- **Sub-millisecond event processing** - Responsive input
- **Efficient memory usage** - Bounded buffers
- **Low network overhead** - Streaming parser

## Performance Telemetry

### PerformanceStats

The centralized telemetry collector:

```rust
pub struct PerformanceStats {
    // Frame timing
    frame_times: VecDeque<Duration>,
    last_frame_time: Instant,
    max_frame_samples: usize,      // Default: 60

    // Network stats
    bytes_received: u64,
    bytes_sent: u64,
    bytes_received_last_second: u64,
    bytes_sent_last_second: u64,

    // Parser stats
    parse_times: VecDeque<Duration>,
    chunks_parsed: u64,
    chunks_parsed_last_second: u64,

    // Render timing
    render_times: VecDeque<Duration>,
    ui_render_times: VecDeque<Duration>,
    text_wrap_times: VecDeque<Duration>,

    // Event processing
    event_process_times: VecDeque<Duration>,
    events_processed: u64,

    // Memory tracking
    total_lines_buffered: usize,
    active_window_count: usize,

    // Application uptime
    app_start_time: Instant,
}
```

### Recording Methods

```rust
// Frame timing
stats.record_frame();                    // Called each frame
stats.record_render_time(duration);      // Total render
stats.record_ui_render_time(duration);   // UI widgets
stats.record_text_wrap_time(duration);   // Text wrapping

// Network
stats.record_bytes_received(bytes);
stats.record_bytes_sent(bytes);

// Parser
stats.record_parse(duration);
stats.record_elements_parsed(count);

// Events
stats.record_event_process_time(duration);

// Memory
stats.update_memory_stats(total_lines, window_count);
```

### Available Metrics

| Metric | Method | Unit |
|--------|--------|------|
| FPS | `fps()` | frames/sec |
| Avg frame time | `avg_frame_time_ms()` | ms |
| Min frame time | `min_frame_time_ms()` | ms |
| Max frame time | `max_frame_time_ms()` | ms |
| Frame jitter | `frame_jitter_ms()` | ms (stddev) |
| Frame spikes | `frame_spike_count()` | count >33ms |
| Network in | `bytes_received_per_sec()` | bytes/sec |
| Network out | `bytes_sent_per_sec()` | bytes/sec |
| Parse time | `avg_parse_time_us()` | μs |
| Chunks/sec | `chunks_per_sec()` | count/sec |
| Render time | `avg_render_time_ms()` | ms |
| Lines buffered | `total_lines_buffered()` | count |
| Memory estimate | `estimated_memory_mb()` | MB |
| Uptime | `uptime()` | Duration |

## Generation-Based Change Detection

### Efficient Change Tracking

Widgets use generation counters instead of content comparison:

```rust
pub struct TextContent {
    pub lines: VecDeque<StyledLine>,
    pub generation: u64,  // Increments on every add_line()
}
```

### Sync Logic

```rust
let last_gen = last_synced_generation.get(name).unwrap_or(0);
let current_gen = text_content.generation;

// Only sync if generation changed - O(1) check
if current_gen > last_gen {
    let delta = (current_gen - last_gen) as usize;
    // Sync only new lines
}
```

### Benefits

1. **O(1) change detection** - Compare numbers, not content
2. **Incremental updates** - Only new lines synced
3. **Automatic full resync** - Detects when buffer cleared

## Text Wrapping Optimization

Text wrapping is expensive. VellumFE optimizes it through:

### Width-Based Invalidation

```rust
pub fn set_width(&mut self, width: u16) {
    if self.current_width != width {
        self.current_width = width;
        self.invalidate_wrap_cache();
    }
}
```

### Lazy Wrapping

Lines wrapped on-demand during render:

```rust
fn render_visible_lines(&self, visible_height: usize) -> Vec<WrappedLine> {
    // Only wrap lines that will be displayed
    let visible_range = self.calculate_visible_range(visible_height);
    self.lines
        .iter()
        .skip(visible_range.start)
        .take(visible_range.len())
        .flat_map(|line| self.wrap_line(line))
        .collect()
}
```

### Segment-Aware Wrapping

Preserves styling across word boundaries:

```rust
fn wrap_line(&self, line: &StyledLine) -> Vec<WrappedLine> {
    for segment in &line.segments {
        for word in segment.text.split_whitespace() {
            if current_width + word_width > self.max_width {
                // Emit current line, start new
            }
            // Add word with segment's style
        }
    }
}
```

## Memory Management

### Line Buffer Limits

Each text window has a maximum:

```rust
pub struct TextContent {
    pub lines: VecDeque<StyledLine>,
    pub max_lines: usize,  // Default: 1000
}

impl TextContent {
    pub fn add_line(&mut self, line: StyledLine) {
        self.lines.push_back(line);

        // Trim oldest lines
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }

        self.generation += 1;
    }
}
```

### VecDeque Efficiency

`VecDeque` provides O(1) operations at both ends:

```rust
lines.push_back(new_line);  // O(1) add to end
lines.pop_front();           // O(1) remove from start
```

### Memory Estimation

```rust
pub fn estimated_memory_mb(&self) -> f64 {
    // ~200 bytes per line average
    let line_bytes = self.total_lines_buffered * 200;
    line_bytes as f64 / (1024.0 * 1024.0)
}
```

### Per-Window Configuration

```toml
[[windows]]
name = "main"
buffer_size = 2000  # More history for main

[[windows]]
name = "thoughts"
buffer_size = 500   # Less for secondary
```

## Render Optimization

### Dirty Tracking

Widgets track whether they need re-rendering:

```rust
impl TextWindow {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn add_line(&mut self, line: StyledLine) {
        self.dirty = true;
    }
}
```

### Double Buffering

Ratatui uses double-buffered rendering to minimize flicker.

### Partial Rendering

Only changed widgets are re-rendered when possible.

## Highlight Pattern Optimization

### Regex Caching

Patterns compiled once at load:

```rust
pub fn compile_highlight_patterns(highlights: &mut HashMap<String, HighlightPattern>) {
    for (name, pattern) in highlights.iter_mut() {
        if !pattern.fast_parse {
            pattern.compiled_regex = Some(regex::Regex::new(&pattern.pattern)?);
        }
    }
}
```

### Fast Parse Mode

For simple literals, Aho-Corasick is faster:

```toml
[highlights.names]
pattern = "Bob|Alice|Charlie"
fg = "#ffff00"
fast_parse = true  # Uses Aho-Corasick
```

### Early Exit

Skip highlight if text already styled:

```rust
if segment.span_type != SpanType::Normal {
    continue;  // Monsterbold, Link, etc. take priority
}
```

## Network Optimization

### Buffered I/O

```rust
let reader = BufReader::new(stream);
```

### Non-Blocking Polling

```rust
if crossterm::event::poll(Duration::from_millis(10))? {
    // Handle event
}
```

### Streaming Parser

Processes data as it arrives, not waiting for complete messages.

## Performance Widget

Display real-time metrics:

```toml
[[windows]]
name = "performance"
type = "performance"
show_fps = true
show_frame_times = true
show_render_times = true
show_net = true
show_parse = true
show_memory = true
show_lines = true
show_uptime = true
```

### Selective Collection

Enable only displayed metrics to reduce overhead:

```rust
stats.apply_enabled_from(&performance_widget_data);
```

## Profiling

### Built-in Metrics

Use the performance widget for real-time profiling.

### Tracing

```bash
RUST_LOG=vellum_fe=debug cargo run
```

### Release Build

Always profile with release builds:

```bash
cargo build --release
```

Release optimizations:
- Full optimization (`opt-level = 3`)
- Link-time optimization (`lto = true`)
- Single codegen unit

## Best Practices

### For Users

1. **Reduce buffer sizes** for secondary windows
2. **Use fast_parse** for simple highlight patterns
3. **Disable unused performance metrics**
4. **Keep highlight count reasonable** (<100 patterns)
5. **Use compact layouts** when possible

### For Developers

1. **Use generation counters** for change detection
2. **Avoid content comparison** - use flags or counters
3. **Batch updates** when possible
4. **Profile with release builds**
5. **Use VecDeque** for FIFO buffers
6. **Cache expensive computations**

## Performance Troubleshooting

### Low FPS

1. Reduce buffer sizes
2. Disable unused widgets
3. Reduce highlight patterns
4. Use `fast_parse = true`

### High Memory

1. Reduce `buffer_size` on windows
2. Close unused tabs
3. Check for memory leaks (rising delta)

### High Latency

1. Check network connection
2. Reduce highlight patterns
3. Simplify layout

### Frame Spikes

1. Check text wrapping (long lines)
2. Reduce highlight complexity
3. Profile with tracing

## See Also

- [Widget Sync](./widget-sync.md) - Generation-based sync details
- [Configuration](../configuration/config-toml.md) - Performance settings
- [Troubleshooting](../troubleshooting/performance-issues.md) - Performance fixes

