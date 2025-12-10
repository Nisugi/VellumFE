# Performance Issues

Diagnosing and fixing slow, laggy, or resource-intensive behavior.

## Symptoms

| Issue | Likely Cause | Quick Fix |
|-------|--------------|-----------|
| Slow scrolling | Large scrollback | Reduce `scrollback` |
| Input delay | Render bottleneck | Lower `render_rate` |
| High CPU | Busy loop, bad pattern | Check highlights |
| High memory | Scrollback accumulation | Limit buffer sizes |
| Startup slow | Config parsing | Simplify config |

## Performance Configuration

### Optimal Settings

```toml
[performance]
render_rate = 60          # FPS target (30-120)
batch_updates = true      # Combine rapid updates
lazy_render = true        # Skip unchanged regions
stream_buffer_size = 50000

# Widget defaults
[defaults.text]
scrollback = 2000         # Lines to keep
auto_scroll = true
```

### Low-Resource Settings

For older systems or constrained environments:

```toml
[performance]
render_rate = 30          # Lower FPS
batch_updates = true
lazy_render = true
stream_buffer_size = 20000

[defaults.text]
scrollback = 500          # Minimal scrollback
```

## Scroll Performance

### Slow Scrolling

**Symptom**: Page Up/Down is sluggish, mouse scroll lags

**Causes**:
1. Too much scrollback content
2. Complex text styling
3. Pattern matching on scroll

**Solutions**:

1. **Reduce scrollback**:
   ```toml
   [[widgets]]
   type = "text"
   scrollback = 1000  # Down from 5000+
   ```

2. **Simplify highlights**:
   ```toml
   # Avoid overly complex patterns
   # Bad - matches everything
   pattern = ".*"

   # Better - specific pattern
   pattern = "\\*\\* .+ \\*\\*"
   ```

3. **Use fast patterns**:
   ```toml
   [[highlights]]
   pattern = "stunned"
   fast_parse = true  # Use Aho-Corasick
   ```

### Scroll Buffer Memory

**Symptom**: Memory usage grows over time

**Solution**:
```toml
[[widgets]]
type = "text"
scrollback = 2000
scrollback_limit_action = "trim"  # Remove old content
```

## Input Latency

### Typing Delay

**Symptom**: Characters appear slowly after typing

**Causes**:
1. Render blocking input
2. Network latency
3. Pattern processing

**Solutions**:

1. **Priority input processing**:
   ```toml
   [input]
   priority = "high"
   buffer_size = 100
   ```

2. **Reduce render rate during input**:
   ```toml
   [performance]
   input_render_rate = 30  # Lower when typing
   render_rate = 60        # Normal rate
   ```

3. **Check network**:
   ```bash
   # Test latency to game server
   ping -c 10 gs4.play.net
   ```

### Command Delay

**Symptom**: Commands take long to send

**Solution**:
1. Check Lich processing time
2. Disable unnecessary Lich scripts
3. Try direct mode for comparison

## CPU Usage

### High CPU at Idle

**Symptom**: CPU high even when nothing happening

**Causes**:
1. Busy render loop
2. Bad regex pattern
3. Timer spinning

**Diagnosis**:
```bash
# Profile VellumFE
top -p $(pgrep vellum-fe)

# Check per-thread
htop -p $(pgrep vellum-fe)
```

**Solutions**:

1. **Enable lazy rendering**:
   ```toml
   [performance]
   lazy_render = true
   idle_timeout = 100  # ms before sleep
   ```

2. **Check patterns**:
   ```toml
   # Avoid catastrophic backtracking
   # Bad
   pattern = "(a+)+"

   # Good
   pattern = "a+"
   ```

3. **Reduce timers**:
   ```toml
   [[widgets]]
   type = "countdown"
   update_rate = 100  # ms, not too fast
   ```

### CPU Spikes During Activity

**Symptom**: CPU spikes during combat/busy scenes

**Solutions**:

1. **Batch updates**:
   ```toml
   [performance]
   batch_updates = true
   batch_threshold = 10  # Combine N updates
   ```

2. **Limit pattern matching**:
   ```toml
   [[highlights]]
   pattern = "complex pattern"
   max_matches_per_line = 10
   ```

3. **Use simpler widgets**:
   ```toml
   # Disable complex widgets during heavy activity
   [widgets.effects]
   auto_hide = true
   hide_threshold = 50  # Updates/second
   ```

## Memory Usage

### Memory Growth

**Symptom**: Memory usage increases continuously

**Causes**:
1. Unlimited scrollback
2. Stream buffer growth
3. Memory leak (rare)

**Solutions**:

1. **Limit all buffers**:
   ```toml
   [performance]
   max_total_memory = "500MB"

   [[widgets]]
   type = "text"
   scrollback = 2000
   ```

2. **Enable periodic cleanup**:
   ```toml
   [performance]
   cleanup_interval = 300  # seconds
   ```

3. **Monitor memory**:
   ```bash
   watch -n 5 'ps -o rss,vsz -p $(pgrep vellum-fe)'
   ```

### Large Scrollback

**Problem**: Each text window can accumulate megabytes

**Solution**:
```toml
# Global limit
[defaults.text]
scrollback = 1000

# Per-widget override only where needed
[[widgets]]
type = "text"
name = "main"
scrollback = 3000  # Main window can be larger
```

## Startup Performance

### Slow Launch

**Symptom**: Takes several seconds to start

**Causes**:
1. Config file parsing
2. Font loading
3. Network checks

**Solutions**:

1. **Simplify config**:
   - Split large configs into smaller files
   - Remove unused sections

2. **Skip network check**:
   ```bash
   vellum-fe --no-network-check
   ```

3. **Precompile patterns**:
   ```toml
   [highlights]
   precompile = true
   cache_file = "~/.vellum-fe/pattern_cache"
   ```

### Slow Reconnect

**Symptom**: Reconnection takes long

**Solution**:
```toml
[connection]
reconnect_delay = 1       # Start at 1 second
reconnect_max_delay = 30  # Cap at 30 seconds
reconnect_backoff = 1.5   # Exponential backoff
```

## Network Performance

### High Latency

**Symptom**: Actions feel delayed

**Diagnosis**:
```bash
# Check network latency
ping gs4.play.net
traceroute gs4.play.net
```

**Solutions**:

1. **Enable Nagle's algorithm** (if many small packets):
   ```toml
   [network]
   tcp_nodelay = false
   ```

2. **Or disable for lower latency**:
   ```toml
   [network]
   tcp_nodelay = true
   ```

3. **Buffer commands**:
   ```toml
   [input]
   send_delay = 50  # ms between commands
   ```

### Bandwidth Issues

**Symptom**: Connection drops during heavy traffic

**Solution**:
```toml
[network]
receive_buffer = 65536
send_buffer = 32768
```

## Profiling

### Built-in Performance Widget

```toml
[[widgets]]
type = "performance"
name = "perf"
x = 0
y = 95
width = 100
height = 5
show = ["fps", "memory", "cpu", "network"]
```

### Enable Timing Logs

```toml
[logging]
level = "debug"

[debug]
log_render_time = true
log_parse_time = true
log_pattern_time = true
```

### External Profiling

```bash
# Linux - perf
perf record -g vellum-fe
perf report

# macOS - Instruments
xcrun xctrace record --template "Time Profiler" --launch vellum-fe

# Cross-platform - flamegraph
cargo flamegraph --bin vellum-fe
```

## Quick Reference

### Performance Checklist

- [ ] Scrollback limited to reasonable size
- [ ] Lazy rendering enabled
- [ ] Batch updates enabled
- [ ] No catastrophic regex patterns
- [ ] Unused widgets disabled
- [ ] Network connection stable
- [ ] System resources adequate

### Key Settings Summary

```toml
[performance]
render_rate = 60
batch_updates = true
lazy_render = true
stream_buffer_size = 50000

[defaults.text]
scrollback = 2000

[logging]
level = "warn"  # Less logging = more performance
```

## See Also

- [Configuration](../configuration/README.md) - All settings
- [Architecture Performance](../architecture/performance.md) - Design details
- [Common Errors](./common-errors.md) - Error messages

