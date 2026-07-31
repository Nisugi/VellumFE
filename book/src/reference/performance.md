# Performance Monitor

VellumFE ships with a built-in performance monitor: a window of live
metrics about the client itself — how fast it draws, how much data the
game is sending, how hard it's working your CPU, and where the slow spots
are. When the client feels sluggish, this window turns "it stutters
sometimes" into an actual diagnosis.

## Opening it

- `.performance` (or `.perf`) toggles the monitor in any frontend.
- The `toggle_performance_stats` keybind action does the same thing.
- "Performance" is also in the window catalog (Windows menu), so you can
  add it as a permanent window like any other.
- `.performance dump` writes a full diagnostic report to a file — see
  [Diagnostic dumps](#diagnostic-dumps-performance-dump) below.

Each metric row can be shown or hidden in **Settings → Performance**, or
in the TUI by right-clicking the monitor window. When the monitor is
closed, none of the metrics are collected at all — it costs nothing while
you're not looking at it.

The TUI and GUI each show only the metrics **that frontend actually
measures**, so the two lists differ slightly. A metric you don't see in
one frontend isn't broken — it doesn't apply there.

## Reading the rows

Timing rows show three numbers: **average**, **p95**, and **max**. The
average is what a typical frame/operation costs. The **p95** ("95th
percentile") is the cost that 95% of samples beat — it answers *"how bad
is it when it's bad?"*. The max is the single worst recent sample. A low
average with a low p95 and one high max means a single freak spike —
usually ignorable. A low average with a **high p95** means one operation
in twenty is slow: that's a real, recurring stutter worth chasing.

Rows with thresholds change color when a value crosses them — yellow for
"worth a look", red for "there's your problem". Rows with a small graph
next to them (sparklines, toggleable in settings) show the last minute or
so of history: a flat line is calm, a ramp that keeps climbing is a leak
or a growing backlog, regular spikes point at something periodic.

## What each stat tells you

### Draws/s

How many times per second the window is actually redrawn, averaged over
the last five seconds. VellumFE redraws **on demand**, not at a fixed
frame rate — so near-zero while you're idle is normal and good (the
client is asleep). During scrolling combat expect roughly 10–60. If
draws/s is pegged high *while nothing is happening on screen*, something
is requesting repaints in a loop and burning CPU for nothing — check the
CPU row, and include a `.performance dump` if you report it.

### Render

What one repaint costs. In the **TUI** this is the whole terminal pass:
drawing every widget *plus* flushing the result to the terminal. In the
**GUI** it is the CPU cost of building and painting the frame, as
reported by the UI toolkit.

Healthy values are a few milliseconds. The row turns yellow when p95
passes 10 ms and red past 25 ms — at 25 ms per frame, dragging a window
visibly judders. If Render is red, look at **Windows** (below) to find
*which* window is expensive, and at the **spike log** to see *when* it
spikes and what was happening.

### Draw (TUI only)

The widget-drawing part of Render, measured before the terminal flush.
Comparing Draw against Render splits the blame: if Render is high but
Draw is low, the time is going into the terminal itself (flushing cells
to the emulator) — try a faster terminal emulator or a smaller window. If
Draw is high too, the cost is in VellumFE's widgets — usually one huge
text window; check **Windows**.

### Wrap (TUI only)

Time spent word-wrapping incoming text into window-width lines, in
microseconds. Normally double digits. It grows with very wide windows and
very long unbroken lines (walls of travel spam). High wrap times show up
as sluggishness *while text is arriving*, not while idle.

### Net

Bytes per second in and out. This is the game feed itself — during
normal play expect a few KB/s in and near-zero out. Its real value is
context: a Render or Parse spike that lines up with a burst on the Net
sparkline means the game sent something enormous (an inventory dump, a
busy room) and the client choked digesting it — which is a different
problem from a spike with *no* network burst behind it.

### Parse

What it costs to turn the game's XML protocol into client state:
per-chunk parse time (avg and p95, in microseconds), plus chunks and XML
elements processed per second. Parse time should stay well under a
millisecond. Elements/s is the best "how busy is the game feed" gauge —
idle rooms tick along in the tens, invasions can push thousands. If parse
p95 climbs into milliseconds, note what was on screen and grab a dump —
that's a protocol edge case worth reporting.

### Events

How long the client takes to process each unit of incoming work, and how
many are waiting. In the TUI, events are input events (keys, mouse); in
the GUI, each queued server message counts as one event. **Queue** is the
backlog right now; **peak** is the worst backlog since the monitor was
opened (peaks reset each time it opens, so the login flood doesn't pin
them forever).

Queue near zero means the client is keeping up. A queue that grows and
stays yellow/red (>10 / >50) means messages are arriving faster than they
are processed — the game will feel like it's lagging even though the
network is fine. That's a client-side problem: check CPU and Render.

### CPU

VellumFE's own CPU use, with the whole system's in parentheses. A few
percent is normal; the row warns past 30% and goes red past 70%. High
VellumFE CPU with low draws/s means something is spinning without
painting. High *system* CPU with normal VellumFE CPU means the problem is
elsewhere on the machine — the client can feel laggy because it isn't
getting scheduled, and no client-side setting will fix that.

### Memory

Real process memory: **RSS** (what the OS says the client actually
occupies) with virtual size in parentheses. Steady RSS after an hour of
play is what you want; the number depends on skins, fonts, and buffer
sizes, so absolute values vary. What matters is the *trend* — RSS that
climbs continuously without leveling off is a leak: note how long the
session ran, save a dump, and report it. Warns at 750 MB, red at 1.5 GB.

### Buffers

Total text lines held in scrollback across all windows, and how many
windows exist. This is the main thing you control that drives memory:
`buffer_size` in settings caps lines per window. Huge buffer counts with
high RSS is expected; high RSS with *small* buffers points at something
else (textures, leaks).

### Uptime

How long this session has been running — mostly context for the other
numbers ("RSS after 6 hours" means something different than after 6
minutes).

### Windows

The three most expensive windows to draw, by average cost over their last
30 draws. This is the "which of my 20 windows is the slow one" answer:
before blaming the client, check whether one window dominates — a huge
unwrapped text window, a map at high zoom, a WebUI panel. Windows that
haven't drawn in the last 5 seconds drop off the list, so it reflects
what's actually on screen.

### Spikes

The last 10 operations that blew past their threshold (renders over
10 ms, event processing over 10 ms, parses over 5 ms), each with a
timestamp and a snapshot of what the client was doing: network bytes that
second, XML elements parsed, and the event queue depth. This is the
single most useful row for diagnosing "it hitched a minute ago":

```
14:32:07  render    42.1 ms  18.2 KB, 312 elems, queue 27
```

reads as "at 14:32:07 a redraw took 42 ms while digesting an 18 KB burst
with 27 messages backed up" — i.e. the game sent something huge. A spike
with *no* bytes and *no* queue behind it is the client's own fault —
those are the ones worth reporting. The log clears each time the monitor
opens.

## Diagnostic dumps (`.performance dump`)

`.performance dump` writes everything — every metric the current frontend
measures (whether or not its row is visible), the full spike log, and
per-window costs — to a timestamped file:

```
~/.vellum-fe/perf-dump-20260731-143512.txt
```

In the GUI the dump also includes a **graphics internals** section:
allocated texture count and bytes (skins and images live here — a
runaway texture count means art is being re-uploaded instead of cached),
visible areas, and the DPI/zoom factors, which explain most "it looks
blurry/huge on my monitor" reports.

When reporting a performance problem on Discord, reproduce the hitch with
the monitor open, then `.performance dump` and attach the file — it
carries the whole diagnosis.
