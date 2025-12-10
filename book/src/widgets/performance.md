# Performance Monitor

The performance widget displays real-time performance metrics for debugging and optimization.

## Overview

The performance widget shows:
- Frame rate (FPS)
- Frame timing statistics
- Network throughput
- Parser performance
- Memory usage
- Application uptime

## Configuration

```toml
[[windows]]
name = "performance"
type = "performance"

# Position and size
row = 0
col = 0
width = 35
height = 15

# Which metrics to display
show_fps = true
show_frame_times = true
show_render_times = true
show_ui_times = false
show_wrap_times = false
show_net = true
show_parse = true
show_events = false
show_memory = true
show_lines = true
show_uptime = true
show_jitter = false
show_frame_spikes = false
show_event_lag = false
show_memory_delta = false
```

## Metric Options

### Frame Metrics

| Option | Description |
|--------|-------------|
| `show_fps` | Frames per second |
| `show_frame_times` | Frame time min/avg/max |
| `show_jitter` | Frame time variance |
| `show_frame_spikes` | Count of slow frames (>33ms) |

### Render Metrics

| Option | Description |
|--------|-------------|
| `show_render_times` | Total render duration |
| `show_ui_times` | UI widget render time |
| `show_wrap_times` | Text wrapping time |

### Network Metrics

| Option | Description |
|--------|-------------|
| `show_net` | Bytes received/sent per second |

### Parser Metrics

| Option | Description |
|--------|-------------|
| `show_parse` | Parse time and chunks/sec |

### Event Metrics

| Option | Description |
|--------|-------------|
| `show_events` | Event processing time |
| `show_event_lag` | Time since last event |

### Memory Metrics

| Option | Description |
|--------|-------------|
| `show_memory` | Estimated memory usage |
| `show_lines` | Total lines buffered |
| `show_memory_delta` | Memory change rate |

### General

| Option | Description |
|--------|-------------|
| `show_uptime` | Application uptime |

## Display Example

```
┌─ Performance ──────────────────────┐
│ FPS: 60.0                          │
│ Frame: 0.8/1.2/2.1 ms              │
│ Render: 0.6 ms                     │
│                                    │
│ Net In: 1.2 KB/s                   │
│ Net Out: 0.1 KB/s                  │
│                                    │
│ Parse: 45 µs (12/s)                │
│                                    │
│ Memory: ~2.4 MB                    │
│ Lines: 12,345                      │
│                                    │
│ Uptime: 01:23:45                   │
└────────────────────────────────────┘
```

## Examples

### Full Metrics

```toml
[[windows]]
name = "performance"
type = "performance"
row = 0
col = 0
width = 35
height = 18
show_fps = true
show_frame_times = true
show_render_times = true
show_ui_times = true
show_wrap_times = true
show_net = true
show_parse = true
show_events = true
show_memory = true
show_lines = true
show_uptime = true
show_jitter = true
show_frame_spikes = true
```

### Minimal FPS

```toml
[[windows]]
name = "fps"
type = "performance"
row = 0
col = 0
width = 15
height = 1
show_fps = true
show_frame_times = false
show_render_times = false
show_net = false
show_parse = false
show_memory = false
show_lines = false
show_uptime = false
show_border = false
```

### Network Focus

```toml
[[windows]]
name = "network"
type = "performance"
row = 0
col = 100
width = 20
height = 5
show_fps = false
show_net = true
show_parse = true
show_memory = false
title = "Network"
```

### Memory Focus

```toml
[[windows]]
name = "memory"
type = "performance"
row = 5
col = 100
width = 20
height = 5
show_fps = false
show_net = false
show_memory = true
show_lines = true
show_memory_delta = true
title = "Memory"
```

## Metric Details

### FPS (Frames Per Second)

```
FPS: 60.0
```

Target is 60 FPS. Lower values indicate performance issues.

### Frame Times

```
Frame: 0.8/1.2/2.1 ms
```

Shows min/avg/max frame time over recent samples. Lower is better.

### Frame Jitter

```
Jitter: 0.3 ms
```

Standard deviation of frame times. Lower means smoother animation.

### Frame Spikes

```
Spikes: 2
```

Count of frames taking >33ms (below 30 FPS). Should be 0 or very low.

### Render Time

```
Render: 0.6 ms
```

Time spent rendering all widgets. Should be well under 16ms.

### Network

```
Net In: 1.2 KB/s
Net Out: 0.1 KB/s
```

Network throughput. High values during combat/activity are normal.

### Parse Time

```
Parse: 45 µs (12/s)
```

Average XML parse time and chunks processed per second.

### Memory

```
Memory: ~2.4 MB
Lines: 12,345
```

Estimated memory usage and total lines in all buffers.

### Uptime

```
Uptime: 01:23:45
```

Time since VellumFE started (HH:MM:SS).

## Performance Overhead

The performance widget itself has minimal overhead:
- Only enabled metrics are collected
- Collection can be disabled entirely

```toml
# In config.toml
[performance]
collect_metrics = false   # Disable all collection
```

## Troubleshooting Performance

### Low FPS

1. Reduce buffer sizes in text windows
2. Disable unused widgets
3. Reduce highlight pattern count
4. Use `fast_parse = true` for highlights

### High Memory

1. Reduce `buffer_size` on windows
2. Close unused tabs
3. Check for memory leaks (rising delta)

### Network Issues

1. Check connection stability
2. Monitor for packet loss
3. Consider bandwidth limitations

## See Also

- [Performance Optimization](../architecture/performance.md) - Optimization guide
- [Configuration](../configuration/config-toml.md) - Performance settings
- [Troubleshooting](../troubleshooting/performance-issues.md) - Performance fixes
