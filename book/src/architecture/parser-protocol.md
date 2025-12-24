# Parser Protocol

VellumFE's XML streaming parser converts GemStone IV/DragonRealms server data into strongly-typed Rust events.

## Overview

The game server sends an XML-based protocol called "Wizard Front End" (WFE) or "Wrayth" protocol. The `XmlParser` maintains state across chunks and emits high-level `ParsedElement` values.

## Parser Architecture

```
Server XML Stream
       ↓
┌──────────────────────┐
│     XmlParser        │
│  ──────────────────  │
│  • Color stacks      │
│  • Stream tracking   │
│  • Link metadata     │
│  • Event matchers    │
└──────────────────────┘
       ↓
Vec<ParsedElement>
       ↓
AppCore (routes to widgets)
```

## ParsedElement Variants

The parser emits these typed events:

### Text Content

```rust
ParsedElement::Text {
    content: String,           // Decoded text content
    stream: String,            // Target stream: "main", "speech", etc.
    fg_color: Option<String>,  // Foreground color "#RRGGBB"
    bg_color: Option<String>,  // Background color
    bold: bool,                // Bold styling
    span_type: SpanType,       // Semantic type
    link_data: Option<LinkData>, // Clickable link metadata
}
```

### SpanType Enum

| SpanType | Source | Purpose |
|----------|--------|---------|
| `Normal` | Plain text | Default text |
| `Link` | `<a>` or `<d>` tags | Clickable game objects |
| `Monsterbold` | `<pushBold/>` | Monster/creature names |
| `Spell` | `<spell>` tags | Spell names |
| `Speech` | `<preset id="speech">` | Player dialogue |

### Game State Updates

```rust
// Prompt (command input ready)
ParsedElement::Prompt {
    time: String,    // Unix timestamp
    text: String,    // Prompt character (e.g., ">")
}

// Roundtime countdown
ParsedElement::RoundTime { value: u32 }   // Seconds

// Cast time countdown
ParsedElement::CastTime { value: u32 }    // Seconds

// Hand contents
ParsedElement::LeftHand { item: String, link: Option<LinkData> }
ParsedElement::RightHand { item: String, link: Option<LinkData> }
ParsedElement::SpellHand { spell: String }

// Progress bars (health, mana, etc.)
ParsedElement::ProgressBar {
    id: String,      // "health", "mana", "spirit", "stamina"
    value: u32,      // Current value
    max: u32,        // Maximum value
    text: String,    // Display text like "mana 407/407"
}

// Compass directions
ParsedElement::Compass {
    directions: Vec<String>,  // ["n", "e", "out", "up"]
}
```

### Stream Control

```rust
// Push new stream context
ParsedElement::StreamPush { id: String }

// Pop stream (return to main)
ParsedElement::StreamPop

// Clear stream contents
ParsedElement::ClearStream { id: String }

// Stream window metadata
ParsedElement::StreamWindow {
    id: String,
    subtitle: Option<String>,  // Room name for "room" stream
}
```

### Room Information

```rust
// Room ID for mapping
ParsedElement::RoomId { id: String }

// Room component (name, description, objects, players)
ParsedElement::Component {
    id: String,      // "room name", "room desc", "room objs", "room players"
    value: String,   // Component content
}
```

### Combat/Status

```rust
// Injury display
ParsedElement::InjuryImage {
    id: String,     // Body part: "head", "leftArm", "chest"
    name: String,   // Level: "Injury1"-"Injury3", "Scar1"-"Scar3"
}

// Status indicators
ParsedElement::StatusIndicator {
    id: String,     // "poisoned", "diseased", "bleeding", "stunned", "hidden"
    active: bool,   // true = active, false = cleared
}

// Active effects (spells, buffs, debuffs)
ParsedElement::ActiveEffect {
    category: String,  // "ActiveSpells", "Buffs", "Debuffs", "Cooldowns"
    id: String,
    value: u32,        // Progress percentage
    text: String,      // Effect name
    time: String,      // "HH:MM:SS"
}

ParsedElement::ClearActiveEffects { category: String }
```

### Interactive Elements

```rust
// Menu response (from INV SEARCH, etc.)
ParsedElement::MenuResponse {
    id: String,                            // Correlation ID
    coords: Vec<(String, Option<String>)>, // (coord, optional noun) pairs
}

// Pattern-matched events (stun, webbed, etc.)
ParsedElement::Event {
    event_type: String,    // "stun", "webbed", "prone"
    action: EventAction,   // Set, Clear, or Increment
    duration: u32,         // Seconds
}

// External URL launch
ParsedElement::LaunchURL { url: String }
```

## Link Data Structure

Clickable text carries metadata for game interaction:

```rust
pub struct LinkData {
    pub exist_id: String,        // Object ID or "_direct_"
    pub noun: String,            // Object noun or command
    pub text: String,            // Display text
    pub coord: Option<String>,   // Optional coord for direct commands
}
```

### GemStone IV Links (`<a>` tags)

```xml
<a exist="12345" noun="sword">a rusty sword</a>
```

- `exist_id`: "12345" (server object ID)
- `noun`: "sword" (for commands like "get sword")
- Clicking sends: `_INSPECT 12345` or contextual command

### DragonRealms Direct Commands (`<d>` tags)

```xml
<d cmd='get #8735861 in #8735860'>Some item</d>
```

- `exist_id`: "_direct_" (marker for direct command)
- `noun`: "get #8735861 in #8735860" (full command)
- Clicking sends the command directly

## Stream System

The server uses streams to route content to different windows:

| Stream | Purpose | Widget Type |
|--------|---------|-------------|
| `main` | Primary game output | Main text window |
| `speech` | Player dialogue | Speech tab/window |
| `thoughts` | ESP/telepathy | Thoughts tab/window |
| `inv` | Inventory listings | Inventory window |
| `room` | Room descriptions | Room info widget |
| `assess` | Combat assessment | Assessment window |
| `experience` | Skill/XP info | Experience window |
| `percWindow` | Perception checks | Perception window |
| `death` | Death messaging | Death/recovery window |
| `logons` | Login/logout notices | Arrivals window |
| `familiar` | Familiar messages | Familiar window |
| `group` | Group information | Group window |

### Stream Lifecycle

```xml
<!-- Server pushes to speech stream -->
<pushStream id='speech'/>
Soandso says, "Hello there!"
<popStream/>

<!-- Text returns to main stream -->
You nod to Soandso.
```

The parser tracks `current_stream` and emits `StreamPush`/`StreamPop` events.

## Color Stack System

The parser maintains multiple stacks for nested styling:

```rust
pub struct XmlParser {
    color_stack: Vec<ColorStyle>,   // <color> tags
    preset_stack: Vec<ColorStyle>,  // <preset> tags
    style_stack: Vec<ColorStyle>,   // <style> tags
    bold_stack: Vec<bool>,          // <pushBold>/<popBold>
}
```

### Priority (highest to lowest)

1. `color_stack` - Explicit `<color fg="..." bg="...">` tags
2. `preset_stack` - Named presets like `<preset id="speech">`
3. `style_stack` - Style IDs like `<style id="roomName">`

### XML Color Tags

```xml
<!-- Explicit color -->
<color fg='#FF0000' bg='#000000'>Red on black</color>

<!-- Named preset (from colors.toml) -->
<preset id='speech'>Someone says, "Hello"</preset>

<!-- Bold/monsterbold -->
<pushBold/>A goblin<popBold/> attacks you!

<!-- Style (from colors.toml) -->
<style id='roomName'>Town Square</style>
```

## Event Pattern Matching

The parser checks text against configurable patterns for game events:

```rust
pub struct EventPattern {
    pub pattern: String,           // Regex pattern
    pub event_type: String,        // "stun", "webbed", "prone"
    pub action: EventAction,       // Set, Clear, Increment
    pub duration: u32,             // Fixed duration (seconds)
    pub duration_capture: Option<usize>,  // Capture group for duration
    pub duration_multiplier: f32,  // Convert rounds to seconds
    pub enabled: bool,
}
```

### Example: Stun Detection

```toml
[event_patterns.stun_start]
pattern = 'You are stunned!'
event_type = "stun"
action = "set"
duration = 5

[event_patterns.stun_with_rounds]
pattern = 'stunned for (\d+) rounds'
event_type = "stun"
action = "set"
duration_capture = 1
duration_multiplier = 5.0  # Rounds to seconds

[event_patterns.stun_end]
pattern = 'You are no longer stunned'
event_type = "stun"
action = "clear"
```

## XML Entity Decoding

The parser decodes standard XML entities:

| Entity | Character |
|--------|-----------|
| `&lt;` | `<` |
| `&gt;` | `>` |
| `&amp;` | `&` |
| `&quot;` | `"` |
| `&apos;` | `'` |

## Progress Bar Parsing

Progress bars contain both percentage and actual values:

```xml
<progressBar id='health' value='85' text='health 425/500' />
```

The parser extracts:
- `id`: "health"
- `value`: 425 (parsed from text, not the percentage)
- `max`: 500 (parsed from text)
- `text`: "health 425/500"

### Special Cases

- `mindState`: Text like "clear as a bell" (no numeric extraction)
- `encumlevel`: Encumbrance percentage
- `lblBPs`: Blood Points (Betrayer profession)

## Complete XML Tag Reference

### Handled Tags

| Tag | Purpose | ParsedElement |
|-----|---------|---------------|
| `<prompt>` | Command ready | `Prompt` |
| `<roundTime>` | RT countdown | `RoundTime` |
| `<castTime>` | Cast countdown | `CastTime` |
| `<left>` | Left hand item | `LeftHand` |
| `<right>` | Right hand item | `RightHand` |
| `<spell>` | Prepared spell | `SpellHand` |
| `<compass>` | Available exits | `Compass` |
| `<progressBar>` | Stat bars | `ProgressBar` |
| `<indicator>` | Status icons | `StatusIndicator` |
| `<dialogData>` | Dialog updates | Various |
| `<component>` | Room components | `Component` |
| `<pushStream>` | Stream switch | `StreamPush` |
| `<popStream>` | Stream return | `StreamPop` |
| `<clearStream>` | Clear window | `ClearStream` |
| `<streamWindow>` | Window metadata | `StreamWindow` |
| `<nav>` | Room ID | `RoomId` |
| `<a>` | Object link | `Text` with `LinkData` |
| `<d>` | Direct command | `Text` with `LinkData` |
| `<menu>` | Menu container | `MenuResponse` |
| `<mi>` | Menu item | Part of `MenuResponse` |
| `<preset>` | Named color | Color applied to `Text` |
| `<color>` | Explicit color | Color applied to `Text` |
| `<style>` | Style ID | Color applied to `Text` |
| `<pushBold>` | Start bold | Bold applied to `Text` |
| `<popBold>` | End bold | Bold removed |
| `<LaunchURL>` | External URL | `LaunchURL` |

### Ignored Tags

These tags are silently ignored:
- `<dropDownBox>`
- `<skin>`
- `<clearContainer>`
- `<container>`
- `<exposeContainer>`
- `<inv>` content (between open/close)

## Usage Example

```rust
use vellum_fe::parser::{XmlParser, ParsedElement};

// Create parser with presets and event patterns
let parser = XmlParser::with_presets(preset_list, event_patterns);

// Parse a line from the server
let elements = parser.parse_line("<pushBold/>A goblin<popBold/> attacks!");

// Route elements to appropriate widgets
for element in elements {
    match element {
        ParsedElement::Text { content, stream, bold, .. } => {
            // Route to text widget for `stream`
        }
        ParsedElement::RoundTime { value } => {
            // Update RT countdown widget
        }
        ParsedElement::Compass { directions } => {
            // Update compass widget
        }
        // ... handle other variants
    }
}
```

## See Also

- [Message Flow](./message-flow.md) - How parsed elements flow through the system
- [Theme System](./theme-system.md) - How presets are resolved to colors
- [Widget Sync](./widget-sync.md) - How parsed data reaches widgets

