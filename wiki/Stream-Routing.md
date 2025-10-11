# Stream Routing Guide

This guide explains how stream routing works in vellum-fe, including what streams are, how they're routed to windows, and how to create custom routing configurations.

## Table of Contents

- [What Are Streams?](#what-are-streams)
- [Available Streams](#available-streams)
- [How Stream Routing Works](#how-stream-routing-works)
- [Default Stream Routing](#default-stream-routing)
- [Custom Stream Routing](#custom-stream-routing)
- [Multiple Streams to One Window](#multiple-streams-to-one-window)
- [One Stream to Multiple Windows](#one-stream-to-multiple-windows)
- [Advanced Examples](#advanced-examples)

---

## What Are Streams?

In GemStone IV, game output is divided into named **streams** based on the type of content. The game server uses XML tags (`<pushStream>` and `<popStream>`) to switch between streams.

**Example from game:**
```xml
You are standing in a forest.
<pushStream id="thoughts"/>You hear a thought: "Hello!"<popStream/>
You continue walking.
```

In this example:
1. "You are standing..." goes to the `main` stream
2. The thought goes to the `thoughts` stream
3. "You continue walking..." returns to the `main` stream

Profanity-rs routes each stream to one or more windows based on your configuration.

---

## Available Streams

Here's the complete list of streams used by GemStone IV:

| Stream | Description | Example Content |
|--------|-------------|-----------------|
| `main` | Default stream | Most game output, movement, combat |
| `thoughts` | PSInet thoughts | Thoughts from other players |
| `speech` | Speech in the room | NPCs and players speaking |
| `whisper` | Whispers to you | Private messages whispered to you |
| `familiar` | Familiar messages | Messages from your familiar |
| `room` | Room descriptions | Room names and descriptions |
| `logons` | Player arrivals | "Playername has connected" |
| `deaths` | Death messages | "Playername has died" |
| `arrivals` | Player arrivals to room | "Playername just arrived" |
| `ambients` | Ambient messages | Environmental flavor text |
| `announcements` | System announcements | Game-wide announcements |
| `loot` | Loot messages | Items found or picked up |

### Stream Characteristics

**High Volume:**
- `main` - Contains most game text
- `speech` - Can be busy in populated areas
- `thoughts` - Busy if using PSInet

**Low Volume:**
- `whisper` - Only direct whispers to you
- `deaths` - Occasional death announcements
- `logons` - Player connections (if enabled)

**Medium Volume:**
- `arrivals` - Depends on area traffic
- `loot` - Depends on hunting activity

---

## How Stream Routing Works

### XML Tags

The game server uses these XML tags to control streams:

```xml
<pushStream id='thoughts'/>
Text that goes to thoughts stream
<popStream/>
Text that goes back to previous stream
```

### Stream Stack

Profanity-rs maintains a **stream stack**:

1. Default stream is `main`
2. `<pushStream id='X'/>` pushes stream X onto the stack
3. `<popStream/>` pops the stack, returning to the previous stream
4. Text is always routed to the current top of the stack

### Window Mapping

Each window subscribes to one or more streams via the `streams` array in its configuration:

```toml
[[ui.windows]]
name = "thoughts"
widget_type = "text"
streams = ["thoughts"]  # Only show thoughts stream
# ...
```

When text arrives on a stream, vellum-fe checks which window(s) subscribe to that stream and routes the text accordingly.

---

## Default Stream Routing

Here's the default routing for built-in window templates:

| Window Template | Streams | Purpose |
|----------------|---------|---------|
| `main` | `["main"]` | Primary game output |
| `thoughts` | `["thoughts"]` | PSInet thoughts |
| `speech` | `["speech"]` | Room speech |
| `familiar` | `["familiar"]` | Familiar messages |
| `room` | `["room"]` | Room descriptions |
| `logons` | `["logons"]` | Player connections |
| `deaths` | `["deaths"]` | Death announcements |
| `arrivals` | `["arrivals"]` | Player arrivals |
| `ambients` | `["ambients"]` | Ambient messages |
| `announcements` | `["announcements"]` | System announcements |
| `loot` | `["loot"]` | Loot messages |

### Creating Default Windows

```
.createwindow main
.createwindow thoughts
.createwindow speech
.createwindow loot
```

---

## Custom Stream Routing

You can customize stream routing in two ways:

### 1. Using .customwindow Command

Create a custom window at runtime:

```
.customwindow mywindow stream1,stream2,stream3
```

**Examples:**

```
# Combine speech and whispers in one window
.customwindow chatter speech,whisper

# All communication in one window
.customwindow comms thoughts,speech,whisper

# All announcements in one window
.customwindow announcements logons,deaths,arrivals,announcements
```

### 2. Editing config.toml

Define custom stream routing in your config file:

```toml
[[ui.windows]]
name = "combat"
widget_type = "text"
streams = ["main", "death"]  # Combat and death messages
row = 0
col = 0
rows = 20
cols = 80
buffer_size = 5000
show_border = true
title = "Combat"
```

---

## Multiple Streams to One Window

You can route multiple streams to a single window by listing them in the `streams` array:

### Example: Communication Window

```toml
[[ui.windows]]
name = "communication"
widget_type = "text"
streams = ["thoughts", "speech", "whisper"]
row = 0
col = 80
rows = 30
cols = 60
buffer_size = 10000
show_border = true
border_style = "rounded"
title = "Communication"
```

This window will display:
- All PSInet thoughts
- All room speech
- All whispers to you

### Example: Social Window

```toml
[[ui.windows]]
name = "social"
widget_type = "text"
streams = ["logons", "deaths", "arrivals"]
row = 0
col = 100
rows = 20
cols = 40
buffer_size = 2000
show_border = true
title = "Social"
```

This window shows all player-related events.

---

## One Stream to Multiple Windows

**Important:** Each stream can only route to **one window** at a time. The last window defined with that stream in the config file "wins".

### Example: Stream Conflict

```toml
# Window 1
[[ui.windows]]
name = "window1"
streams = ["main"]
# ...

# Window 2
[[ui.windows]]
name = "window2"
streams = ["main", "thoughts"]  # main stream here
# ...
```

In this case, the `main` stream routes to `window2` because it's defined later.

### Workaround: Split Different Streams

If you want similar content in multiple windows, you'll need to split it by stream:

```toml
# Main combat
[[ui.windows]]
name = "combat"
streams = ["main"]
# ...

# Death log
[[ui.windows]]
name = "deaths"
streams = ["death"]
# ...
```

---

## Advanced Examples

### Minimal Layout (Main + Stats)

```toml
# Everything in one window
[[ui.windows]]
name = "main"
widget_type = "text"
streams = ["main", "thoughts", "speech", "whisper", "loot", "arrivals"]
row = 0
col = 0
rows = 28
cols = 140
buffer_size = 10000
show_border = true
title = "Game"
```

### Roleplay Layout

```toml
# Main game window
[[ui.windows]]
name = "main"
streams = ["main", "room", "ambients"]
# ...

# All communication separate
[[ui.windows]]
name = "comms"
streams = ["thoughts", "speech", "whisper"]
# ...

# Social events
[[ui.windows]]
name = "social"
streams = ["logons", "arrivals", "deaths", "announcements"]
# ...
```

### Combat Layout

```toml
# Combat window (main action)
[[ui.windows]]
name = "combat"
streams = ["main"]
# ...

# Loot tracking
[[ui.windows]]
name = "loot"
streams = ["loot"]
# ...

# Death log (keep track of kills)
[[ui.windows]]
name = "kills"
streams = ["death"]
# ...
```

### Hunting Layout with Minimal Distractions

```toml
# Main combat/movement
[[ui.windows]]
name = "main"
streams = ["main", "loot"]  # Combat + loot together
# ...

# Background chatter (can be ignored)
[[ui.windows]]
name = "background"
streams = ["thoughts", "speech", "arrivals", "logons", "deaths", "announcements"]
# ...
```

---

## Stream Routing Tips

### 1. Start Simple

Begin with default windows and add custom routing as needed:

```
.createwindow main
.createwindow thoughts
.createwindow loot
```

### 2. Group by Priority

Combine low-priority streams into one "background" window:

```
.customwindow background logons,deaths,announcements
```

### 3. Test Your Routing

Use the game to generate different types of messages and verify they appear in the correct windows.

### 4. Save Your Layout

Once you have routing configured:

```
.savelayout hunting
```

### 5. Multiple Layouts for Different Activities

Create different layouts for different activities:

```
.savelayout combat     # Combat-focused layout
.savelayout roleplay   # RP-focused layout
.savelayout trading    # Trading/shopping layout
```

Load them as needed:

```
.loadlayout combat
.loadlayout roleplay
```

---

## Troubleshooting Stream Routing

### Text Not Appearing in Window

**Check:**
1. Is the window subscribed to the correct stream?
2. Look in `config.toml` at the window's `streams` array
3. Use `.windows` to list active windows
4. Check if another window is receiving the stream (stream conflict)

**Solution:**
```toml
# Make sure the window includes the stream
[[ui.windows]]
name = "mywindow"
streams = ["main", "thoughts", "loot"]  # Add missing streams
```

### Duplicate Text in Multiple Windows

**Cause:** Multiple windows subscribed to the same stream

**Check:**
- Search `config.toml` for duplicate stream names
- Last window with the stream will receive it

**Solution:**
- Remove the stream from windows that shouldn't have it
- Or use different streams for different purposes

### Missing Streams

**Check:**
1. Verify the stream name is correct (case-sensitive)
2. Check the [Available Streams](#available-streams) list
3. Some streams may only appear with specific game settings

**Example:**
```toml
# Wrong (case matters!)
streams = ["Main", "Thoughts"]

# Correct
streams = ["main", "thoughts"]
```

---

## Related Commands

- `.customwindow <name> <streams>` - Create custom window with specific streams
- `.windows` - List all active windows
- `.deletewindow <name>` - Remove a window

See [Commands Reference](Commands-Reference.md) for details.

---

[← Previous: Configuration Guide](Configuration-Guide.md) | [Next: Mouse and Keyboard →](Mouse-and-Keyboard.md)
