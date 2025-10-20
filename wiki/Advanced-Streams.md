# Advanced Stream Routing

This guide covers VellumFE's stream system in depth, including how game output is routed, stream management, and advanced routing patterns.

## What are Streams?

Streams are named channels for game output. The game server sends XML tags to push/pop streams, directing where text should appear.

**Example XML:**
```xml
<pushStream id="speech"/>
Adventurer says, "Hello!"
<popStream/>
```

This pushes the "speech" stream, sends text, then pops back to the previous stream.

## Stream Stack

VellumFE maintains a stream stack:

1. **Current stream** - Top of stack, where text is currently routed
2. **Stack** - Previous streams, restored on pop

**Example flow:**
```
Initial:     [main]
pushStream:  [main, speech]         ← "speech" is current
text:        → routed to "speech"
popStream:   [main]                 ← back to "main"
```

## Common Streams

### Core Streams

- **main** - Main game output
- **room** - Room descriptions
- **speech** - Player speech
- **thoughts** - Character thoughts (ESP, etc.)
- **whisper** - Whispers to/from you

### Social Streams

- **logons** - Login/logout notifications
- **deaths** - Death messages
- **arrivals** - Player arrivals
- **ambients** - Ambient actions (emotes, etc.)

### Game System Streams

- **familiar** - Familiar messages (watching others)
- **announcements** - Game-wide announcements
- **loot** - Loot messages
- **combat** - Combat messages (varies by game)

### Special Streams

- **status** - Status indicators
- **guild** - Guild-specific channels
- **group** - Group chat

## Window Stream Subscription

Windows subscribe to streams via the `streams` array:

```toml
[[ui.windows]]
name = "main"
streams = ["main"]              # Only main stream

[[ui.windows]]
name = "social"
streams = ["speech", "thoughts", "whisper"]  # Multiple streams
```

### Multiple Streams, One Window

Route related streams to one window:

```toml
[[ui.windows]]
name = "chat"
streams = ["speech", "thoughts", "whisper", "logons"]
```

All four streams appear in the "chat" window.

### One Stream, Multiple Windows

You can route the same stream to multiple windows (though unusual):

```toml
[[ui.windows]]
name = "main1"
streams = ["main"]

[[ui.windows]]
name = "main2"
streams = ["main"]
```

Both windows show the same content.

### No Subscribers

If no window subscribes to a stream, text is **discarded**:

```
Game pushes "rare_stream"
No window subscribes to "rare_stream"
→ Text is discarded
```

**Solution:** Create window for that stream:
```bash
.customwindow newwin rare_stream
```

## Stream Routing Patterns

### Pattern 1: Dedicated Windows

Each stream has its own window:

```toml
[[ui.windows]]
name = "main"
streams = ["main"]

[[ui.windows]]
name = "speech"
streams = ["speech"]

[[ui.windows]]
name = "thoughts"
streams = ["thoughts"]
```

**Advantages:**
- Clear separation
- Easy to ignore specific streams

**Disadvantages:**
- Many windows
- Screen space usage

### Pattern 2: Grouped Windows

Related streams grouped together:

```toml
[[ui.windows]]
name = "main"
streams = ["main", "room"]

[[ui.windows]]
name = "social"
streams = ["speech", "thoughts", "whisper"]

[[ui.windows]]
name = "system"
streams = ["logons", "deaths", "announcements"]
```

**Advantages:**
- Fewer windows
- Related content together

**Disadvantages:**
- Can't separately scroll/highlight different streams

### Pattern 3: Tabbed Windows

Use tabs for related streams:

```toml
[[ui.windows]]
name = "chat"
widget_type = "tabbed"
tab_bar_position = "top"

[[ui.windows.tabs]]
name = "Speech"
stream = "speech"

[[ui.windows.tabs]]
name = "Thoughts"
stream = "thoughts"

[[ui.windows.tabs]]
name = "Whisper"
stream = "whisper"
```

**Advantages:**
- Separate buffers per stream
- Unread indicators
- Space-efficient

**Disadvantages:**
- Only one visible at a time

### Pattern 4: All-in-One

Everything in one window:

```toml
[[ui.windows]]
name = "everything"
streams = ["main", "room", "speech", "thoughts", "whisper", "logons", "deaths", "arrivals", "ambients"]
```

**Advantages:**
- Single scrollback
- Nothing missed

**Disadvantages:**
- Cluttered
- Hard to focus on specific content

## Stream Management

### Discovering Streams

To find what streams are being pushed:

1. **Check debug log:**
   ```bash
   cat ~/.vellum-fe/debug.log | grep "pushStream"
   ```

2. **Monitor game output:**
   - Watch for streams you don't recognize
   - Create windows as needed

3. **Common GemStone IV streams:**
   - See [Common Streams](#common-streams) section above

### Creating Stream Windows

**From template:**
```bash
.createwindow speech
.createwindow thoughts
```

**Custom:**
```bash
.customwindow mywindow stream1,stream2,stream3
```

**Tabbed:**
```bash
.createtabbed mytabs Tab1:stream1,Tab2:stream2
```

### Listing Windows and Streams

```bash
.windows
```

Shows all windows and their subscribed streams.

## Advanced Routing Techniques

### Highlight-Based Routing

You can't route based on content (highlights don't change streams), but you can:

1. **Color differently:** Use highlights to color text from different streams
2. **Sound alerts:** Add sounds to important text
3. **Separate windows:** Route streams to different windows

**Example:**
```bash
# Route speech to dedicated window
.createwindow speech

# Add highlight to make your character's name stand out
.addhl
# Name: my_name
# Pattern: Nisugi
# FG Color: #ffff00
# Bold: true
```

### Conditional Display

Not directly supported, but you can approximate:

1. **Hide window:** Resize to 0 rows (effectively hidden)
2. **Show window:** Resize back to normal size

**Example workflow:**
- Combat layout: Show combat window, hide social
- Social layout: Show social window, hide combat

Use `.savelayout combat` and `.savelayout social` to save different layouts.

### Stream Filtering

Not currently supported. All text pushed to a stream is displayed in subscribed windows.

**Workaround:** Use Lich scripts to filter before text reaches VellumFE.

## Stream and Highlight Interaction

Highlights apply to **all text in subscribed windows**, regardless of stream:

```toml
[[ui.windows]]
name = "social"
streams = ["speech", "thoughts", "whisper"]
```

Highlights will match text from any of the three streams.

**To highlight specific streams only:**
You can't directly, but you can route streams to separate windows and apply highlights differently per window (not currently supported—highlights are global).

**Current limitation:** Highlights are global and apply to all windows.

## Stream Performance

### Buffer Management

Each window maintains its own buffer:

```toml
buffer_size = 10000  # Lines of scrollback
```

Large buffers use more memory but allow more scrollback.

**Recommendations:**
- **Main window:** 10,000 lines
- **Chat windows:** 5,000 lines
- **System windows:** 1,000 lines

### Stream Throughput

High-volume streams (main) can slow performance:

1. **Increase poll_timeout_ms:** Lower FPS, less CPU
2. **Reduce buffer_size:** Less memory, faster scrolling
3. **Simplify highlights:** Faster text processing

## Troubleshooting Streams

### Text Not Appearing

**Check window subscribes to stream:**
```bash
.windows
# Verify stream is in windows array
```

**Solution:** Add stream to window:
```bash
.editwindow windowname
# Add stream to streams list
```

Or create new window:
```bash
.customwindow newwin streamname
```

### Text in Wrong Window

**Check stream routing:**
Game may be pushing unexpected stream.

**Check debug log:**
```bash
cat ~/.vellum-fe/debug.log | grep "pushStream"
```

**Solution:** Adjust window stream subscriptions.

### Duplicate Text

**Check multiple windows subscribe:**
If two windows subscribe to the same stream, text appears in both.

**Solution:** Remove stream from one window.

### Missing Text

**Check stream has subscriber:**
If no window subscribes, text is discarded.

**Solution:** Create window for that stream.

## Stream Reference

### GemStone IV Streams

Complete list of known GemStone IV streams:

**Core:**
- `main` - Main game output
- `room` - Room descriptions
- `inv` - Inventory
- `container` - Container contents

**Communication:**
- `speech` - Player speech
- `thoughts` - ESP/thoughts
- `whisper` - Whispers
- `talk` - Talk command
- `ooc` - Out of character

**Social:**
- `logons` - Login/logout
- `deaths` - Death messages
- `arrivals` - Player arrivals
- `departures` - Player departures
- `ambients` - Ambient actions
- `emotes` - Emotes

**Combat:**
- `combat` - Combat messages
- `damage` - Damage messages

**Game Systems:**
- `familiar` - Familiar/watching
- `announcements` - Game announcements
- `loot` - Loot messages
- `experience` - Experience messages

**Guild/Profession:**
- `guild` - Guild channels
- `group` - Group chat

**Special:**
- `status` - Status indicators
- `prompt` - Game prompts

Note: Actual streams depend on game configuration and may vary.

## See Also

- [Windows and Layouts](Windows-and-Layouts.md) - Window configuration
- [Configuration](Configuration.md) - Config file format
- [Commands Reference](Commands.md) - Window management commands
- [Advanced XML](Advanced-XML.md) - XML protocol details
