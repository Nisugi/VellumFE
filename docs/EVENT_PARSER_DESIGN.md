# Configurable Event Parser Design

## Problem Statement

Currently, stun detection and other text-based event parsing requires hardcoded logic in the parser. When Simutronics changes message formats or adds new status effects, users must wait for a new release. We need a **configurable event parser** that works like highlights but triggers game state updates instead of just coloring text.

## Goals

1. **User-configurable patterns** - Add new event patterns via config file
2. **No recompilation required** - Users can adapt to game changes immediately
3. **Reuse existing infrastructure** - Leverage regex patterns like highlights do
4. **Support multiple event types** - Stun, prone, webbed, hidden, invisible, etc.
5. **Extract values from text** - Parse duration/severity from messages

## Design Overview

### Event Pattern Structure

Similar to highlights, but with action/value extraction:

```toml
[[event_patterns]]
name = "stun_start"
pattern = "You are still stunned\\."
event_type = "stun"
action = "set"           # "set", "clear", "increment"
duration = 0             # 0 = use existing countdown

[[event_patterns]]
name = "stun_recovery"
pattern = "You recover from being stunned\\."
event_type = "stun"
action = "clear"

[[event_patterns]]
name = "web_trapped"
pattern = "You become entangled in a mass of sticky webbing!"
event_type = "webbed"
action = "set"
duration = 5             # Assume 5 seconds if not in XML

[[event_patterns]]
name = "web_escape"
pattern = "You manage to break free from the webbing\\."
event_type = "webbed"
action = "clear"
```

### Event Types

Initially support these common status effects:

**Countdown Timers** (require duration/timestamp):
- **stun** - Maps to `stun` countdown widget (shows remaining seconds)
- **custom_countdown** - User-defined countdown timers

**Indicators** (binary on/off state):
- **webbed** - Sets indicator state
- **prone** - Sets indicator state
- **kneeling** - Sets indicator state
- **sitting** - Sets indicator state
- **hidden** - Sets indicator state
- **invisible** - Sets indicator state
- **silenced** - Sets indicator state
- **custom_indicator** - User-defined indicators

**Important Distinction**:
- Countdown widgets need `duration` > 0 or they won't display anything
- Indicators just need `set`/`clear` actions (no duration needed)

### Value Extraction (Required for Stun!)

**Critical Discovery**: Stun does NOT come from XML tags - it's parsed from text messages!

The game sends messages like:
- "You are stunned for 3 rounds" → Need to extract "3" and multiply by 5 (1 round = 5 seconds)
- Special events (raise dead, Shadow Valley) → Hardcoded durations

Pattern with capture group and multiplier:

```toml
[[event_patterns]]
name = "stun_rounds"
pattern = "^\\s*You are stunned for ([0-9]+) rounds?"
event_type = "stun"
action = "set"
duration_capture = 1          # Regex capture group (1-based)
duration_multiplier = 5.0     # Convert rounds to seconds
enabled = true
```

This extracts "3" from "You are stunned for 3 rounds" and calculates 3 * 5 = 15 seconds.

## Implementation Plan

### Phase 1: Basic Event Patterns (Today)

**Config Structure** (`src/config.rs`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPattern {
    pub name: String,
    pub pattern: String,           // Regex pattern
    pub event_type: String,        // "stun", "webbed", "prone", etc.
    pub action: EventAction,       // set/clear/increment
    #[serde(default)]
    pub duration: u32,             // Duration in seconds (0 = don't change)
    #[serde(default)]
    pub duration_capture: Option<usize>,  // Regex capture group for duration (1-based)
    #[serde(default = "default_duration_multiplier")]
    pub duration_multiplier: f32,  // Multiply captured duration (e.g., 5.0 for rounds->seconds)
    #[serde(default = "default_enabled")]
    pub enabled: bool,             // Can disable without deleting
}

fn default_duration_multiplier() -> f32 { 1.0 }
fn default_enabled() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventAction {
    Set,       // Set state/timer (e.g., start stun countdown)
    Clear,     // Clear state/timer (e.g., recover from stun)
    Increment, // Add to existing value (future use)
}

impl Default for EventAction {
    fn default() -> Self {
        EventAction::Set
    }
}

// Add to Config struct:
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // ... existing fields ...
    #[serde(default)]
    pub event_patterns: HashMap<String, EventPattern>,
}
```

**Parser Integration** (`src/parser.rs`):

```rust
use regex::Regex;

pub struct XmlParser {
    // ... existing fields ...

    // Compile event patterns at init for performance
    event_matchers: Vec<(Regex, EventPattern)>,
}

impl XmlParser {
    pub fn with_event_patterns(
        preset_list: Vec<(String, Option<String>, Option<String>)>,
        event_patterns: HashMap<String, EventPattern>,
    ) -> Self {
        let mut event_matchers = Vec::new();

        for (name, pattern) in event_patterns {
            if !pattern.enabled {
                continue;
            }

            match Regex::new(&pattern.pattern) {
                Ok(regex) => {
                    event_matchers.push((regex, pattern.clone()));
                }
                Err(e) => {
                    tracing::warn!(
                        "Invalid event pattern '{}': {}",
                        name,
                        e
                    );
                }
            }
        }

        Self {
            // ... existing initialization ...
            event_matchers,
        }
    }

    // Call this on every text element after creation
    fn check_event_patterns(&self, text: &str) -> Vec<ParsedElement> {
        let mut events = Vec::new();

        for (regex, pattern) in &self.event_matchers {
            if let Some(captures) = regex.captures(text) {
                let mut duration = pattern.duration;

                // Extract duration from capture group if specified
                if let Some(group_idx) = pattern.duration_capture {
                    if let Some(capture) = captures.get(group_idx) {
                        if let Ok(captured_value) = capture.as_str().parse::<f32>() {
                            // Apply multiplier (e.g., rounds to seconds)
                            duration = (captured_value * pattern.duration_multiplier) as u32;
                        }
                    }
                }

                tracing::debug!(
                    "Event pattern '{}' matched: '{}' (duration: {}s)",
                    pattern.name,
                    text,
                    duration
                );

                events.push(ParsedElement::Event {
                    event_type: pattern.event_type.clone(),
                    action: pattern.action.clone(),
                    duration,
                });
            }
        }

        events
    }

    fn create_text_element(&self, content: String) -> ParsedElement {
        // ... existing text element creation ...

        // NEW: Check for event patterns
        let events = self.check_event_patterns(&content);
        // Return events separately or embed in text element
        // (Implementation detail to be decided)

        ParsedElement::Text { /* ... */ }
    }
}
```

**ParsedElement Addition**:

```rust
#[derive(Debug, Clone)]
pub enum ParsedElement {
    // ... existing variants ...

    Event {
        event_type: String,
        action: EventAction,
        duration: u32,
    },
}
```

**App Handler** (`src/app.rs`):

```rust
impl App {
    fn handle_server_message(&mut self, msg: ServerMessage) {
        // ... existing message handling ...

        for element in elements {
            match element {
                // ... existing element handling ...

                ParsedElement::Event { event_type, action, duration } => {
                    self.handle_event(&event_type, action, duration);
                }
            }
        }
    }

    fn handle_event(&mut self, event_type: &str, action: EventAction, duration: u32) {
        match event_type {
            "stun" => {
                match action {
                    EventAction::Set => {
                        if duration > 0 {
                            // Find stuntime countdown widget and set it
                            if let Some(stun_window_idx) = self.window_manager
                                .find_window_by_name("stuntime")  // NOTE: Changed from "stun"
                            {
                                // Set countdown to current_time + duration
                                let end_time = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs() as u32 + duration;

                                self.window_manager.set_countdown(
                                    stun_window_idx,
                                    end_time
                                );
                            }
                        }
                    }
                    EventAction::Clear => {
                        // Clear stun countdown
                        if let Some(stun_window_idx) = self.window_manager
                            .find_window_by_name("stuntime")  // NOTE: Changed from "stun"
                        {
                            self.window_manager.set_countdown(stun_window_idx, 0);
                        }
                    }
                    _ => {}
                }
            }

            "webbed" | "prone" | "kneeling" | "sitting" => {
                // Handle indicator updates
                match action {
                    EventAction::Set => {
                        // Set indicator active
                        if let Some(indicator_idx) = self.window_manager
                            .find_indicator_by_id(event_type)
                        {
                            self.window_manager.set_indicator_state(
                                indicator_idx,
                                true
                            );
                        }
                    }
                    EventAction::Clear => {
                        // Clear indicator
                        if let Some(indicator_idx) = self.window_manager
                            .find_indicator_by_id(event_type)
                        {
                            self.window_manager.set_indicator_state(
                                indicator_idx,
                                false
                            );
                        }
                    }
                    _ => {}
                }
            }

            _ => {
                tracing::debug!("Unhandled event type: {}", event_type);
            }
        }
    }
}
```

### Phase 2: Value Extraction (Future)

Add capture group support:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPattern {
    // ... existing fields ...

    #[serde(default)]
    pub duration_capture: Option<usize>,  // Regex capture group for duration

    #[serde(default)]
    pub value_capture: Option<usize>,     // Regex capture group for generic value
}
```

Then in `check_event_patterns()`:

```rust
fn check_event_patterns(&self, text: &str) -> Vec<ParsedElement> {
    let mut events = Vec::new();

    for (regex, pattern) in &self.event_matchers {
        if let Some(captures) = regex.captures(text) {
            let mut duration = pattern.duration;

            // Extract duration from capture group if specified
            if let Some(group_idx) = pattern.duration_capture {
                if let Some(capture) = captures.get(group_idx) {
                    if let Ok(parsed_duration) = capture.as_str().parse::<u32>() {
                        duration = parsed_duration;
                    }
                }
            }

            events.push(ParsedElement::Event {
                event_type: pattern.event_type.clone(),
                action: pattern.action.clone(),
                duration,
            });
        }
    }

    events
}
```

### Phase 3: Management UI (Future)

Similar to highlight management:

```
.addevent <name>      - Create new event pattern (interactive form)
.editevent <name>     - Edit existing event pattern
.removeevent <name>   - Delete event pattern
.listevents           - Show all event patterns
.testevent <name>     - Test pattern against recent text
```

## Default Event Patterns

Add to `defaults/config.toml`:

```toml
#==============================================================================
# Event Patterns
#==============================================================================
# Configurable patterns for detecting game events and updating UI state
# Similar to highlights, but triggers state changes instead of just coloring

# Stun Events (based on Profanity implementation)

# Primary stun detection - extracts rounds from message and converts to seconds
[event_patterns.stun_rounds]
pattern = "^\\s*You are stunned for ([0-9]+) rounds?"
event_type = "stun"
action = "set"
duration_capture = 1          # Capture group 1 = number of rounds
duration_multiplier = 5.0     # 1 round = 5 seconds
enabled = true

# Shadow Valley exit stun (hardcoded duration from Profanity)
[event_patterns.stun_shadow_valley]
pattern = "Just as you think the falling will never end, you crash through an ethereal barrier"
event_type = "stun"
action = "set"
duration = 16                 # Profanity uses 16.2s
enabled = true

# Raise dead stun (hardcoded duration from Profanity)
[event_patterns.stun_raise_dead]
pattern = "Your surroundings grow dim\\.\\.\\.you lapse into a state of awareness only"
event_type = "stun"
action = "set"
duration = 31                 # Profanity uses 30.6s
enabled = true

# Stun recovery message
[event_patterns.stun_recovery]
pattern = "You recover from being stunned\\."
event_type = "stun"
action = "clear"
enabled = true

# Web Events
[event_patterns.web_trapped]
pattern = "You become entangled in a mass of sticky webbing!"
event_type = "webbed"
action = "set"
duration = 5
enabled = true

[event_patterns.web_escape]
pattern = "You manage to break free from the webbing\\."
event_type = "webbed"
action = "clear"
enabled = true

# Prone Events
[event_patterns.knocked_down]
pattern = "You are knocked to the ground!"
event_type = "prone"
action = "set"
enabled = true

[event_patterns.stand_up]
pattern = "You stand back up\\."
event_type = "prone"
action = "clear"
enabled = true

# Hidden Events
[event_patterns.hide_success]
pattern = "You blend into the shadows\\."
event_type = "hidden"
action = "set"
enabled = true

[event_patterns.hide_broken]
pattern = "You come out of hiding\\."
event_type = "hidden"
action = "clear"
enabled = true
```

## Performance Considerations

**Concern**: Running regex on every text line could be expensive.

**Optimizations**:

1. **Compile regexes once at startup** - Store compiled `Regex` objects
2. **Early exit on empty event_patterns** - Skip entirely if no patterns configured
3. **Match on XML text elements only** - Don't check tags, attributes, etc.
4. **Limit pattern count** - Recommend <50 event patterns
5. **Use Aho-Corasick for common literal patterns** - Similar to highlight optimization

**Benchmark Target**: <10μs overhead per text line (acceptable given parser already runs at ~35μs avg)

## Benefits

1. **User empowerment** - Players can adapt to game changes immediately
2. **Community sharing** - Users can share event pattern configs
3. **Extensibility** - Foundation for future scripting/automation
4. **No maintainer bottleneck** - Don't need new release for every message change
5. **Character-specific** - Different patterns for different professions/situations

## Example Use Cases

**Rogue hiding detection**:
```toml
[event_patterns.hide_check]
pattern = "You attempt to blend into the surroundings\\."
event_type = "hide_attempt"
action = "set"
enabled = true
```

**Bard song tracking**:
```toml
[event_patterns.song_start]
pattern = "You begin to sing a song\\."
event_type = "singing"
action = "set"
duration = 60
enabled = true
```

**Monk stance changes**:
```toml
[event_patterns.stance_offensive]
pattern = "You move into an offensive stance\\."
event_type = "stance"
action = "set"
enabled = true
```

## Future Enhancements

- **Capture group extraction** - Parse values from messages
- **Multiple actions per pattern** - Set multiple states at once
- **Conditional patterns** - Only match in certain contexts
- **Pattern priority/ordering** - Control match order
- **Regex debugging UI** - Test patterns interactively
- **Pattern statistics** - Track match frequency
- **Scripting hooks** - Call Lich scripts on events (advanced)

---

**Status**: Design complete, ready for implementation
**Estimated Effort**: 2-3 hours for Phase 1 (basic functionality)
**Dependencies**: None - builds on existing parser/config infrastructure
