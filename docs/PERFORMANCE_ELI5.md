# VellumFE Performance Monitoring - The Simple Guide

## What Is This?

VellumFE has a built-in speedometer that tells you how fast it's running and if anything is slowing it down. Think of it like your car's dashboard - it shows you important numbers so you know if everything is working smoothly or if something needs attention.

## Why Should I Care?

**The Short Answer**: So your game doesn't lag or feel sluggish.

**The Longer Answer**: When you're playing GemStone IV, you want text to appear instantly when things happen. You want to be able to scroll, click, and type without delays. The performance monitor helps you (and developers) figure out why things might be slow and how to fix them.

## The Main Numbers Explained

### FPS (Frames Per Second)

**What it is**: How many times per second the screen updates.

**Simple version**: Higher = smoother. Think of it like a flip book - more pages flipped per second = smoother animation.

**Good numbers**:
- **60 FPS**: Silky smooth, feels amazing
- **30 FPS**: Acceptable, most people won't notice issues
- **Below 20 FPS**: Choppy, noticeable lag

**What it means for you**:
- If FPS is low, text will feel sluggish and scrolling will stutter
- Usually means you have too many windows open or your buffers are too big

**IMPORTANT - Common Misconception**:

❌ **Wrong thinking**: "If text only comes in every few seconds, my FPS will be low"

✓ **Right thinking**: "FPS measures screen redraws, which happen constantly regardless of text flow"

**Here's the deal**: Even when you're standing idle and no text is coming in, VellumFE should still maintain 30-60 FPS because:
- The program constantly checks for your keyboard/mouse input
- Countdown timers tick down every second (roundtime, casttime)
- Mouse movements trigger redraws
- The render loop runs independently of game data

Think of it like a security guard watching cameras. Even if nothing is happening on the cameras (no text coming in), the guard is still looking at the screens 60 times per second.

**Low FPS during idle gameplay = problem!** It means VellumFE's render loop itself is slow, not that text is sparse. The game server sends data in bursts when stuff happens, but VellumFE is always redrawing the screen in the background.

### Frame Time

**What it is**: How long it takes to draw everything on screen once.

**Simple version**: Lower = faster. This is like asking "how long does it take to paint one picture?"

**Good numbers**:
- **Under 17 milliseconds (ms)**: Perfect for 60 FPS
- **17-33 ms**: Still smooth
- **Over 33 ms**: Starting to feel laggy

**What it means for you**:
- High frame time = your computer is working really hard to draw stuff
- Usually caused by huge walls of text or too many windows

### Memory Usage

**What it is**: How much computer memory (RAM) VellumFE is using.

**Simple version**: Like how many papers you have stacked on your desk. More papers = more cluttered desk.

**Good numbers**:
- **Under 100 MB**: Normal usage
- **100-200 MB**: Heavy usage, but usually okay
- **Over 200 MB**: Too much, time to clean up

**What it means for you**:
- VellumFE keeps all your scrollback text in memory
- If you have 20 windows each storing 10,000 lines of text, that's a LOT of data
- Solution: Reduce buffer sizes or close windows you're not using

### Parse Time

**What it is**: How long it takes to understand the game's messages.

**Simple version**: When the game server sends text like `<color fg="#FF0000">You attack!</color>`, VellumFE has to figure out what that means. This is how long that takes.

**Good numbers**:
- **Under 50 microseconds (μs)**: Blazing fast
- **50-100 μs**: Still very good
- **Over 100 μs**: Might cause delays during text spam

**What it means for you**:
- High parse time = VellumFE is struggling to keep up with incoming text
- Usually only a problem during intense combat with lots of highlights

### Network Traffic

**What it is**: How much data is flowing in and out.

**Simple version**:
- **Bytes In**: Game server talking to you
- **Bytes Out**: You typing commands

**Good numbers**:
- **Normal gameplay**: 1-5 KB/s incoming
- **Combat/scrolling**: 10-50 KB/s incoming
- **Commands**: Usually under 1 KB/s outgoing

**What it means for you**:
- Helps identify if lag is from your internet or from VellumFE itself
- If bytes in is high but everything is smooth, VellumFE is doing great!

## How to See These Numbers

### Option 1: The Easy Way

1. Start VellumFE and connect to the game
2. Type this command: `.createwindow perf_stats`
3. A new window appears showing all the performance numbers updating in real-time

### Option 2: The Config File Way

Open your config file (`~/.vellum-fe/configs/default.toml`) and add:

```toml
[[ui.windows]]
name = "performance"
widget_type = "performance_stats"
row = 0
col = 100
rows = 25
cols = 25
show_border = true
title = "Performance"
```

Now the performance window will always be there when you start VellumFE.

## What Do I Do If Things Are Slow?

### Problem: Low FPS (under 30)

**What's happening**: Your screen is updating too slowly.

**Quick fixes**:
1. **Close unused windows**: Type `.windows` to see all windows, then `.deletewindow <name>` to close ones you don't need
2. **Reduce buffer sizes**: Each window stores text history. Edit your config and lower `buffer_size = 10000` to something smaller like `buffer_size = 5000`
3. **Make windows smaller**: Smaller windows = less text to draw = faster

**Real-life analogy**: Like having too many browser tabs open - close the ones you're not using.

### Problem: High Memory (over 150 MB)

**What's happening**: VellumFE is hoarding too much text in memory.

**Quick fixes**:
1. **Reduce buffer_size** in your config (see above)
2. **Clear old text**: Some window types let you clear their history
3. **Close tabs in tabbed windows**: Type `.removetab <window> <tab_name>`

**Real-life analogy**: Like your email inbox filling up - time to delete old emails.

### Problem: Choppy/Stuttering Even with Good FPS

**What's happening**: Occasional lag spikes.

**Quick fixes**:
1. **Check the "max frame time"** number - if it's way higher than average, you're getting spikes
2. **Avoid dragging/resizing windows during gameplay** - this is expensive
3. **Simplify highlight patterns** - complex regex patterns can cause stutters

**Real-life analogy**: Like driving smoothly then hitting a pothole - most of the time is fine, but occasional bumps.

## Advanced Features (For When You Get Comfortable)

### Logging Performance Over Time

Want to see how performance changes during a long gaming session?

Add to your config:
```toml
[performance]
enable_metrics_logging = true
metrics_log_interval = 10  # Log every 10 seconds
```

Now VellumFE saves performance snapshots to `~/.vellum-fe/metrics.csv`. Open it in Excel/Google Sheets to see graphs!

### Getting a Performance Report

Type `.perfreport` in the game and VellumFE will generate a detailed performance report at `~/.vellum-fe/performance_report.md`.

This report includes:
- Summary of current performance
- Assessment of what's good/bad
- Specific recommendations for improvement

### Sharing Performance Data with Developers

If you're reporting a bug or asking for help:

1. Type `.snapshot` - shows your current performance numbers
2. Copy/paste that info when asking for help
3. Or share the `performance_report.md` file

This helps developers understand what's happening on your system.

## Understanding the Performance Window Display

Here's what each line means:

```
FPS: 60.0                    ← Screen updates per second
Frame: 16.5ms (max: 22.1)    ← Time to draw screen (average and worst)
Render: 14.2ms (max: 20.0)   ← Time spent just drawing stuff
UI: 10.5ms                   ← Time drawing window borders/bars
Wrap: 45μs                   ← Time wrapping long lines of text

Net In: 3.2 KB/s             ← Data from game server
Net Out: 0.5 KB/s            ← Your commands going out

Parse: 35μs                  ← Time understanding game messages
Chunks/s: 87                 ← Lines of text per second
Elems/s: 234                 ← XML elements per second

Event: 125μs (max: 450)      ← Time processing your clicks/keys
Memory: 45.2 MB              ← RAM usage
Lines: 18234                 ← Total lines of text stored
Windows: 12                  ← Number of windows open

Uptime: 02:15:30             ← How long VellumFE has been running
```

## Quick Reference: What's Normal?

| Metric | Great | Good | Okay | Problem |
|--------|-------|------|------|---------|
| FPS | 60 | 45-60 | 30-45 | <30 |
| Frame Time | <17ms | 17-25ms | 25-33ms | >33ms |
| Memory | <50 MB | 50-100 MB | 100-150 MB | >150 MB |
| Parse Time | <50μs | 50-100μs | 100-200μs | >200μs |

**Note**: μs = microseconds = one millionth of a second. Really tiny!

## Common Myths

**Myth**: "Higher memory usage means worse performance"
**Truth**: Not necessarily! Memory is meant to be used. It only matters if you're running out of RAM or if memory keeps growing forever (that's a leak).

**Myth**: "I need 60 FPS for a text game"
**Truth**: 30 FPS is perfectly fine for reading text. 60 FPS just makes scrolling and animations smoother.

**Myth**: "Network In/Out numbers should be small"
**Truth**: During busy combat, 50 KB/s incoming is normal! That's the game server telling you everything that's happening.

## When to Ignore Performance Warnings

You **don't** need to worry if:
- Memory slowly grows during first 5 minutes then stabilizes (that's normal caching)
- FPS drops to 30 when you're rapidly scrolling (scrolling is expensive)
- Frame time spikes when you resize a window (expected)
- Parse time is high during testing with `.highlight` commands (regex compilation is slow)

You **should** worry if:
- FPS stays below 20 constantly
- Memory keeps growing forever (check every hour)
- Frame time is always above 50ms
- VellumFE feels sluggish when doing normal things

## FAQ

**Q: Why does my FPS drop when I scroll?**
A: Scrolling means drawing lots of new text every frame. This is normal. It should recover quickly.

**Q: Is 100 MB of memory usage bad?**
A: Not really. Modern computers have gigabytes (1000s of MB) of RAM. 100 MB is less than 1% of an 8GB system.

**Q: What's the difference between Frame Time and Render Time?**
A: Frame Time includes everything (waiting for events, processing, drawing). Render Time is just the drawing part.

**Q: My Parse Time shows 0μs - is that broken?**
A: Probably means no data from server yet. Connect to the game and it should show real numbers.

**Q: Can I turn off performance monitoring to make things faster?**
A: The monitoring itself uses almost zero resources (maybe 0.1% CPU). It won't make a noticeable difference.

**Q: What's a "good" uptime?**
A: VellumFE should run for hours/days without issues. If you have to restart frequently, something's wrong - check the debug log or report a bug.

## Getting Help

If you're seeing performance issues:

1. **Generate a report**: Type `.perfreport`
2. **Check the debug log**: Located at `~/.vellum-fe/debug.log` (or `debug_<character>.log`)
3. **Report to developers**: Post issue at https://github.com/anthropics/vellum-fe/issues (or wherever VellumFE is hosted)

Include:
- Your performance report
- What you were doing when it got slow
- Your computer specs (Windows/Mac/Linux, RAM, CPU)
- How many windows you have open

## The Bottom Line

**For casual players**: Just keep an eye on FPS. If it's above 30, you're fine. If it drops below 20, close some windows.

**For power users**: Monitor Frame Time and Memory. Keep frame time under 33ms and memory under 100 MB for best experience.

**For people reporting bugs**: Run the performance window, type `.perfreport` when you see issues, and share that report with developers.

## Technical Terms Cheat Sheet

- **FPS**: Frames Per Second - screen updates per second
- **ms**: Milliseconds - one thousandth of a second (1/1000)
- **μs**: Microseconds - one millionth of a second (1/1,000,000)
- **MB**: Megabytes - unit of memory/storage (1,000,000 bytes)
- **KB/s**: Kilobytes per second - data transfer rate
- **Render**: Drawing stuff on screen
- **Parse**: Understanding game server messages
- **Buffer**: Stored text history in a window
- **Widget**: A UI component (window, progress bar, etc.)
- **Frame**: One complete screen update
- **Chunk**: One line of text from the game server
- **Element**: One piece of an XML message (like a color tag)

---

**Remember**: Performance monitoring is a tool to help you understand what's happening. Don't stress about the numbers unless things actually feel slow. Trust your experience first, numbers second!

---

**Document Version**: 1.0
**Last Updated**: 2025-01-12
**Audience**: End users and big-picture thinkers
**Related Docs**: [Technical Spec](./PERFORMANCE_MONITORING_TECHNICAL.md), [Implementation Guide](./PERFORMANCE_REPORTING_GUIDE.md)
