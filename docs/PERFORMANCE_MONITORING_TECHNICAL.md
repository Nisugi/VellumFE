# VellumFE Performance Monitoring - Technical Specification

## Overview

VellumFE implements a comprehensive real-time performance monitoring system designed to track, measure, and report on critical performance metrics across all major subsystems. This document provides technical specifications for developers, performance engineers, and system architects.

## Architecture

### Core Components

1. **Performance Statistics Tracker** (`src/performance.rs`)
   - Central telemetry collection point
   - Ring-buffer based metric storage for bounded memory usage
   - Real-time aggregation and statistical analysis
   - Thread-safe design for concurrent access patterns

2. **Performance Stats Widget** (`src/ui/performance_stats.rs`)
   - Real-time visualization layer
   - Ratatui-based TUI rendering
   - Color-coded metric display for at-a-glance assessment
   - Configurable border and styling options

3. **Integration Points**
   - Main event loop (`src/app.rs`)
   - XML parser (`src/parser.rs`)
   - Network layer (`src/network.rs`)
   - Window manager and text rendering subsystems

## Tracked Metrics

### 1. Frame Timing Metrics

**Purpose**: Monitor UI responsiveness and render performance

#### Frame Rate (FPS)
- **Collection**: Record timestamp delta between consecutive frames
- **Storage**: Circular buffer of 60 samples (1 second at 60fps)
- **Calculation**: `1.0 / avg(frame_durations)`
- **Units**: Frames per second
- **Target**: ≥30 FPS (smooth), ≥60 FPS (optimal)
- **API**: `PerformanceStats::fps()`

#### Frame Time
- **Average**: Mean duration of last N frames
- **Minimum**: Best-case frame render time (baseline)
- **Maximum**: Worst-case frame render time (identifies frame drops)
- **Units**: Milliseconds
- **Target**: ≤16.67ms (60fps), ≤33.33ms (30fps)
- **APIs**:
  - `avg_frame_time_ms()`
  - `min_frame_time_ms()`
  - `max_frame_time_ms()`

**Implementation Note**: Frame timing includes complete event loop iteration: event processing, UI render, terminal flush, and idle time.

### 2. Render Pipeline Metrics

**Purpose**: Identify bottlenecks in the rendering subsystem

#### Total Render Time
- **Measurement**: Wall-clock time from render start to terminal flush complete
- **Components**: UI widget rendering + text wrapping + buffer composition
- **Storage**: 60-sample circular buffer
- **Units**: Milliseconds
- **API**: `avg_render_time_ms()`, `max_render_time_ms()`

#### UI Widget Render Time
- **Measurement**: Time spent in Ratatui widget render calls
- **Scope**: Window borders, progress bars, countdown timers, text windows
- **Storage**: 60-sample circular buffer
- **Units**: Milliseconds
- **API**: `avg_ui_render_time_ms()`

#### Text Wrapping Time
- **Measurement**: Duration of text line wrapping operations
- **Scope**: Unicode-aware word wrapping with style preservation
- **Storage**: 60-sample circular buffer
- **Units**: Microseconds (high precision for optimization)
- **API**: `avg_text_wrap_time_us()`

**Optimization Target**: Text wrapping should be <100μs per operation to maintain 60fps under heavy text load.

### 3. Network Performance Metrics

**Purpose**: Monitor TCP connection health and data flow rates

#### Bytes Received
- **Collection**: Sum of bytes read from TCP socket
- **Aggregation**: Rolling 1-second window
- **Reset**: Counter resets every second for rate calculation
- **Units**: Bytes per second (displayed as KB/s)
- **API**: `bytes_received_per_sec()`
- **Recording**: `record_bytes_received(bytes: u64)`

#### Bytes Sent
- **Collection**: Sum of bytes written to TCP socket
- **Aggregation**: Rolling 1-second window
- **Reset**: Counter resets every second
- **Units**: Bytes per second (displayed as KB/s)
- **API**: `bytes_sent_per_sec()`
- **Recording**: `record_bytes_sent(bytes: u64)`

**Implementation Note**: Network stats currently track cumulative bytes but require integration into `src/network.rs` to capture actual TCP traffic. This is a TODO for production deployment.

**Expected Values**:
- Normal gameplay: 1-5 KB/s incoming
- Combat/heavy scrolling: 10-50 KB/s incoming
- Outgoing: <1 KB/s (commands only)

### 4. XML Parser Metrics

**Purpose**: Measure parsing efficiency and throughput

#### Parse Time
- **Measurement**: Duration of `XmlParser::parse_line()` call
- **Storage**: 60-sample circular buffer
- **Units**: Microseconds
- **API**: `avg_parse_time_us()`
- **Recording**: `record_parse(duration: Duration)`

**Target**: <50μs per line parse operation

#### Chunks Parsed Per Second
- **Definition**: Number of text lines processed by parser
- **Aggregation**: Rolling 1-second window
- **Reset**: Counter resets every second
- **Units**: Chunks/second
- **API**: `chunks_per_sec()`

**Expected Values**:
- Idle: 0-10 chunks/s
- Active gameplay: 50-200 chunks/s
- Combat spam: 500+ chunks/s

#### Elements Parsed Per Second
- **Definition**: Number of XML elements extracted (tags, text nodes)
- **Aggregation**: Rolling 1-second window
- **Reset**: Counter resets every second
- **Units**: Elements/second
- **API**: `elements_per_sec()`
- **Recording**: `record_elements_parsed(count: u64)`

**Ratio Analysis**: Elements per chunk indicates XML tag density
- Sparse text: 1-2 elements/chunk
- Rich formatting: 5-10 elements/chunk
- Complex dialogs: 20+ elements/chunk

### 5. Event Processing Metrics

**Purpose**: Measure responsiveness to user input and server events

#### Event Process Time
- **Measurement**: Duration from event receipt to processing completion
- **Scope**: Keyboard, mouse, and server message events
- **Storage**: 100-sample circular buffer (2x frame buffer for high-resolution)
- **Units**: Microseconds
- **API**:
  - `avg_event_process_time_us()`
  - `max_event_process_time_us()`
- **Recording**: `record_event_process_time(duration: Duration)`

**Target**: Average <100μs, maximum <5ms for responsive feel

#### Events Processed (Lifetime)
- **Definition**: Total count of events since application start
- **Scope**: All event types (keyboard, mouse, network)
- **Units**: Count
- **API**: `total_events_processed()`

**Use Case**: Debugging event storms, calculating average event rate

### 6. Memory Utilization Metrics

**Purpose**: Monitor resource consumption and prevent memory leaks

#### Total Lines Buffered
- **Definition**: Sum of text lines across all window buffers
- **Update**: Polled from window manager during render cycle
- **Units**: Line count
- **API**: `total_lines_buffered()`
- **Recording**: `update_memory_stats(total_lines: usize, window_count: usize)`

**Expected Values**: 10,000-50,000 lines (depends on buffer_size config)

#### Active Window Count
- **Definition**: Number of instantiated windows
- **Update**: Polled from window manager
- **Units**: Count
- **API**: `active_window_count()`

**Typical Values**: 10-30 windows

#### Estimated Memory Usage
- **Calculation**: `total_lines * 200 bytes / (1024 * 1024)` MB
- **Rationale**: Conservative estimate includes:
  - Line content (~50 bytes average)
  - Style metadata (~20 bytes per span)
  - Vec overhead (~24 bytes per vec)
  - Heap allocator overhead (~2x multiplier)
- **Units**: Megabytes
- **API**: `estimated_memory_mb()`

**Warning Thresholds**:
- <50 MB: Normal
- 50-100 MB: Heavy usage
- >100 MB: Investigate buffer retention policies

### 7. Application Lifecycle Metrics

#### Uptime
- **Measurement**: Duration since `PerformanceStats::new()` call
- **Precision**: Instant-based (nanosecond resolution)
- **Units**: Duration / formatted HH:MM:SS
- **API**:
  - `uptime() -> Duration`
  - `uptime_formatted() -> String`

**Use Case**: Session length tracking, long-running stability testing

## Data Structures

### Circular Buffer Implementation

All time-series metrics use `VecDeque<T>` as bounded circular buffers:

```rust
self.frame_times.push_back(duration);
if self.frame_times.len() > self.max_frame_samples {
    self.frame_times.pop_front();
}
```

**Rationale**:
- O(1) push/pop at both ends
- Bounded memory (no unbounded growth)
- Cache-friendly sequential layout
- Efficient iterator support for aggregation

**Buffer Sizes**:
- Frame metrics: 60 samples (1 second @ 60fps)
- Render metrics: 60 samples
- Parse metrics: 60 samples
- Event metrics: 100 samples (higher resolution for latency analysis)

### Aggregation Algorithms

#### Average Calculation
```rust
let total: Duration = self.frame_times.iter().sum();
total.as_secs_f64() * 1000.0 / self.frame_times.len() as f64
```

**Complexity**: O(n) where n = buffer size (≤100)
**Performance**: <1μs for typical buffer sizes

#### Min/Max Calculation
```rust
self.frame_times.iter()
    .max()
    .map(|d| d.as_secs_f64() * 1000.0)
    .unwrap_or(0.0)
```

**Complexity**: O(n) linear scan
**Optimization**: Could maintain min/max heap for O(1) queries if needed

## Integration Guide

### Adding Performance Tracking to New Subsystems

#### Step 1: Add Metric Recording Call

```rust
use std::time::Instant;

pub fn my_expensive_operation(&mut self) {
    let start = Instant::now();

    // ... perform work ...

    let duration = start.elapsed();
    self.perf_stats.record_my_operation_time(duration);
}
```

#### Step 2: Add Metric Storage to PerformanceStats

```rust
// In src/performance.rs
pub struct PerformanceStats {
    // ... existing fields ...
    my_operation_times: VecDeque<Duration>,
    max_my_operation_samples: usize,
}
```

#### Step 3: Add Recording Method

```rust
impl PerformanceStats {
    pub fn record_my_operation_time(&mut self, duration: Duration) {
        self.my_operation_times.push_back(duration);
        if self.my_operation_times.len() > self.max_my_operation_samples {
            self.my_operation_times.pop_front();
        }
    }
}
```

#### Step 4: Add Getter Method

```rust
pub fn avg_my_operation_time_ms(&self) -> f64 {
    if self.my_operation_times.is_empty() {
        return 0.0;
    }
    let total: Duration = self.my_operation_times.iter().sum();
    total.as_secs_f64() * 1000.0 / self.my_operation_times.len() as f64
}
```

#### Step 5: Update Widget Display

```rust
// In src/ui/performance_stats.rs
Line::from(vec![
    Span::styled("My Op: ", Style::default().fg(Color::Cyan)),
    Span::styled(
        format!("{:.2}ms", stats.avg_my_operation_time_ms()),
        Style::default().fg(Color::White)
    ),
]),
```

### Best Practices

1. **Measure Before Optimizing**: Add instrumentation first, optimize based on data
2. **Minimize Measurement Overhead**: Use `Instant::now()` sparingly (it's fast but not free)
3. **Choose Appropriate Units**:
   - Milliseconds for frame/render operations (human perception scale)
   - Microseconds for fine-grained operations (parser, events)
   - Nanoseconds for micro-benchmarks only
4. **Aggregate Over Time**: Single-sample metrics are noisy; use circular buffers
5. **Consider Percentiles**: For latency-sensitive operations, track p95/p99 not just average

## Performance Targets

### Minimum Viable Performance (30 FPS)
- Frame time: ≤33.33ms average
- Parse time: ≤100μs per line
- Event latency: ≤500μs average
- Memory: ≤200 MB

### Optimal Performance (60 FPS)
- Frame time: ≤16.67ms average
- Parse time: ≤50μs per line
- Event latency: ≤100μs average
- Memory: ≤100 MB

### Degradation Thresholds

| Metric | Warning | Critical | Action |
|--------|---------|----------|--------|
| FPS | <45 | <25 | Reduce window count, decrease buffer sizes |
| Frame Time | >22ms | >40ms | Profile render pipeline |
| Parse Time | >100μs | >500μs | Optimize regex, reduce allocations |
| Memory | >150MB | >250MB | Reduce buffer_size config values |

## Debugging Performance Issues

### Frame Rate Drops

**Symptoms**: Low FPS, high max frame time

**Investigation**:
1. Check `max_frame_time_ms()` - identifies intermittent spikes
2. Compare `avg_frame_time_ms()` vs `avg_render_time_ms()` - determines if bottleneck is rendering or elsewhere
3. Check `avg_event_process_time_us()` and `max_event_process_time_us()` - rules out event processing bottleneck
4. Review `chunks_per_sec()` and `elements_per_sec()` - identifies data rate spikes

**Common Causes**:
- Text wrapping in large windows (check `avg_text_wrap_time_us()`)
- Too many windows with high buffer_size (check `total_lines_buffered()`)
- Mouse drag operations on maximized windows
- Highlight regex compilation storms

### High Memory Usage

**Symptoms**: `estimated_memory_mb()` growing over time

**Investigation**:
1. Check `total_lines_buffered()` - should plateau at buffer_size * window_count
2. Monitor over time - if continuously growing, memory leak suspected
3. Check `active_window_count()` - dynamically created windows not being cleaned up

**Solutions**:
- Reduce buffer_size in config (default 10000 lines)
- Delete unused windows with `.deletewindow`
- Implement periodic buffer trimming for inactive windows

### Network Lag

**Symptoms**: UI feels sluggish, commands don't send

**Investigation**:
1. Check `bytes_received_per_sec()` - confirms data is flowing
2. Check `bytes_sent_per_sec()` - confirms commands are being transmitted
3. Compare network activity to `chunks_per_sec()` - should be correlated

**Note**: Network byte counting is currently a stub; requires integration with `src/network.rs` TCP layer to capture actual socket traffic.

## Future Enhancements

### Planned Features

1. **Histogram Support**: P50/P95/P99 percentile tracking for latency metrics
2. **Metric Export**: JSON/Prometheus format for external monitoring
3. **Alert Thresholds**: Configurable warnings when metrics exceed thresholds
4. **Flame Graphs**: Integrated profiling for hotspot identification
5. **Network Integration**: Actual TCP byte counting in `LichConnection`
6. **Memory Profiling**: Heap allocation tracking with `tikv-jemallocator`

### Proposed Metrics

- **Highlight Match Time**: Aho-Corasick matching duration per line
- **Selection Render Time**: Text selection overlay rendering cost
- **Menu Lookup Time**: Context menu cmdlist coordinate lookup latency
- **Window Layout Time**: Window manager layout recalculation duration
- **Config Reload Time**: Hot-reload configuration parsing time

## Configuration

### Displaying the Performance Widget

The performance widget is not enabled by default. To show it:

#### Option 1: Create Window via Config

Add to `~/.vellum-fe/configs/default.toml`:

```toml
[[ui.windows]]
name = "perf"
widget_type = "performance_stats"
row = 0
col = 100
rows = 25
cols = 25
show_border = true
title = "Performance"
```

#### Option 2: Create Window via Dot Command

At runtime, type:
```
.createwindow perf_stats
```

### Adjusting Sample Sizes

Sample buffer sizes are hardcoded in `src/performance.rs::PerformanceStats::new()`:

```rust
max_frame_samples: 60,      // 1 second @ 60fps
max_render_samples: 60,     // 1 second of render ops
max_parse_samples: 60,      // 1 second of parse ops
max_event_samples: 100,     // Higher resolution for event latency
```

**Tradeoff**: Larger buffers = more accurate averages but higher memory overhead (~24 bytes per sample for Duration + VecDeque overhead).

## Performance Testing Methodology

### Benchmarking Procedure

1. **Baseline**: Start VellumFE connected to idle character
2. **Steady State**: Let run for 5 minutes to warmup JIT/caches
3. **Load Test**: Execute scripted combat scenario (e.g., attack dummy 100x)
4. **Measurement**: Record FPS, parse time, memory over 60-second window
5. **Analysis**: Compare against targets and previous versions

### Regression Detection

Monitor these key metrics across versions:
- `avg_frame_time_ms()` - should not increase >10% between releases
- `avg_parse_time_us()` - should not increase >20% between releases
- `estimated_memory_mb()` - should remain stable after 30-minute session

### Profiling Tools

#### Built-in Metrics
```bash
cargo run -- --character TestChar
# Create performance window with .createwindow perf_stats
# Observe real-time metrics
```

#### CPU Profiling
```bash
cargo install cargo-flamegraph
cargo flamegraph --bin vellum-fe
# Generate flamegraph.svg for hotspot analysis
```

#### Memory Profiling
```bash
cargo install cargo-instruments
cargo instruments --template Allocations --bin vellum-fe
# macOS only - generates detailed heap allocation trace
```

#### Linux perf
```bash
cargo build --release
perf record --call-graph dwarf ./target/release/vellum-fe
perf report
# Detailed CPU profiling on Linux
```

## API Reference

### Recording APIs

| Method | Parameters | Purpose |
|--------|------------|---------|
| `record_frame()` | None | Mark frame completion, calculate frame time |
| `record_bytes_received(bytes)` | `u64` | Accumulate incoming network bytes |
| `record_bytes_sent(bytes)` | `u64` | Accumulate outgoing network bytes |
| `record_parse(duration)` | `Duration` | Record XML parse operation time |
| `record_render_time(duration)` | `Duration` | Record total render time |
| `record_ui_render_time(duration)` | `Duration` | Record UI widget render time |
| `record_text_wrap_time(duration)` | `Duration` | Record text wrapping time |
| `record_event_process_time(duration)` | `Duration` | Record event processing time |
| `record_elements_parsed(count)` | `u64` | Accumulate parsed XML element count |
| `update_memory_stats(lines, windows)` | `usize, usize` | Update memory tracking |

### Query APIs

| Method | Returns | Units | Description |
|--------|---------|-------|-------------|
| `fps()` | `f64` | Hz | Average frame rate |
| `avg_frame_time_ms()` | `f64` | ms | Average frame duration |
| `min_frame_time_ms()` | `f64` | ms | Minimum frame duration |
| `max_frame_time_ms()` | `f64` | ms | Maximum frame duration |
| `bytes_received_per_sec()` | `u64` | bytes/s | Incoming network rate |
| `bytes_sent_per_sec()` | `u64` | bytes/s | Outgoing network rate |
| `avg_parse_time_us()` | `f64` | μs | Average parse time |
| `chunks_per_sec()` | `u64` | chunks/s | Lines parsed per second |
| `avg_render_time_ms()` | `f64` | ms | Average render time |
| `max_render_time_ms()` | `f64` | ms | Maximum render time |
| `avg_ui_render_time_ms()` | `f64` | ms | Average UI render time |
| `avg_text_wrap_time_us()` | `f64` | μs | Average text wrap time |
| `avg_event_process_time_us()` | `f64` | μs | Average event latency |
| `max_event_process_time_us()` | `f64` | μs | Maximum event latency |
| `total_events_processed()` | `u64` | count | Lifetime event count |
| `total_lines_buffered()` | `usize` | lines | Total buffered lines |
| `active_window_count()` | `usize` | count | Active window count |
| `elements_per_sec()` | `u64` | elem/s | XML elements parsed per second |
| `estimated_memory_mb()` | `f64` | MB | Estimated heap usage |
| `uptime()` | `Duration` | duration | Session duration |
| `uptime_formatted()` | `String` | HH:MM:SS | Formatted uptime |

## Troubleshooting

### Performance Widget Not Appearing

**Check**:
1. Window type is `performance_stats` (not `perf_stats`)
2. Window area is large enough (minimum 25x25 cells)
3. No other window overlapping at same coordinates

### Metrics Showing Zero

**Causes**:
1. **FPS = 0**: No frames rendered yet (wait 1 second)
2. **Network = 0**: Not integrated with TCP layer yet (expected)
3. **Parse time = 0**: No data received from server
4. **Memory = 0**: `update_memory_stats()` not being called

### High Frame Times Despite Low FPS

**Explanation**: Frame time is the average render duration, not the interval between frames. If event loop is blocking elsewhere (e.g., waiting for network), frame time will be low but FPS will also be low due to infrequent frame calls.

**Solution**: Instrument event loop itself to measure wait time vs work time.

---

**Document Version**: 1.0
**Last Updated**: 2025-01-12
**Maintained By**: VellumFE Development Team
**Related Docs**: [CLAUDE.md](../CLAUDE.md), [Performance ELI5](./PERFORMANCE_ELI5.md)
