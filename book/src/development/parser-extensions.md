# Parser Extensions

Guide to extending the XML protocol parser for new game elements.

## Overview

The parser converts raw game XML into structured data that widgets can display. Extending the parser allows VellumFE to:

- Recognize new game elements
- Extract additional data
- Support new game features
- Handle protocol changes

## Parser Architecture

### Data Flow

```
Raw Input → Tokenizer → Parser State Machine → ParsedElements
    │           │              │                    │
    │           │              │                    └─ Vec<ParsedElement>
    │           │              └─ Match patterns, build elements
    │           └─ Split into tags/text
    └─ Bytes from network
```

### Key Types

```rust
// Parser output - each element represents parsed game data
pub enum ParsedElement {
    Text(String),
    RoomName(String),
    RoomDesc(String),
    Prompt(PromptData),
    Vitals(VitalsData),
    Indicator(IndicatorData),
    Compass(CompassData),
    Stream { id: String, content: String },
    // ... many more
}

// Parser state tracks context
pub struct Parser {
    state: ParserState,
    buffer: String,
    current_stream: Option<String>,
    // ...
}

enum ParserState {
    Normal,
    InTag(String),
    InStream(String),
    // ...
}
```

## Step-by-Step: New Element

Let's add parsing for a hypothetical "spell_active" element.

### Step 1: Analyze the Protocol

First, understand the XML format:

```xml
<!-- Example game XML -->
<spell_active id="107" name="Spirit Shield" duration="300" circle="1"/>
<spell_expire id="107"/>
```

Document what data we need:
- Spell ID
- Spell name
- Duration (seconds)
- Circle

### Step 2: Define the ParsedElement Variant

Add to `src/parser.rs`:

```rust
pub enum ParsedElement {
    // ... existing variants ...

    /// Active spell notification
    SpellActive {
        id: u32,
        name: String,
        duration: u32,
        circle: u8,
    },

    /// Spell expiration notification
    SpellExpire {
        id: u32,
    },
}
```

### Step 3: Create Data Structures

```rust
#[derive(Debug, Clone)]
pub struct SpellActiveData {
    pub id: u32,
    pub name: String,
    pub duration: u32,
    pub circle: u8,
}

impl SpellActiveData {
    pub fn from_attributes(attrs: &HashMap<String, String>) -> Option<Self> {
        Some(Self {
            id: attrs.get("id")?.parse().ok()?,
            name: attrs.get("name")?.clone(),
            duration: attrs.get("duration")?.parse().ok()?,
            circle: attrs.get("circle")?.parse().ok()?,
        })
    }
}
```

### Step 4: Add Parsing Logic

In the parser's tag handling:

```rust
impl Parser {
    fn handle_tag(&mut self, tag: &str) -> Option<ParsedElement> {
        let (name, attrs) = self.parse_tag(tag)?;

        match name.as_str() {
            // ... existing tags ...

            "spell_active" => {
                let data = SpellActiveData::from_attributes(&attrs)?;
                Some(ParsedElement::SpellActive {
                    id: data.id,
                    name: data.name,
                    duration: data.duration,
                    circle: data.circle,
                })
            }

            "spell_expire" => {
                let id = attrs.get("id")?.parse().ok()?;
                Some(ParsedElement::SpellExpire { id })
            }

            _ => None,
        }
    }
}
```

### Step 5: Handle in Application State

In `src/core/app_core/state.rs`:

```rust
impl AppState {
    pub fn process_element(&mut self, element: ParsedElement) {
        match element {
            // ... existing handlers ...

            ParsedElement::SpellActive { id, name, duration, circle } => {
                self.active_spells.insert(id, ActiveSpell {
                    id,
                    name,
                    remaining: duration,
                    circle,
                });
                self.generation += 1;
            }

            ParsedElement::SpellExpire { id } => {
                self.active_spells.remove(&id);
                self.generation += 1;
            }

            // ...
        }
    }
}
```

### Step 6: Add Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_spell_active() {
        let mut parser = Parser::new();
        let input = r#"<spell_active id="107" name="Spirit Shield" duration="300" circle="1"/>"#;

        let elements = parser.parse(input);

        assert_eq!(elements.len(), 1);
        match &elements[0] {
            ParsedElement::SpellActive { id, name, duration, circle } => {
                assert_eq!(*id, 107);
                assert_eq!(name, "Spirit Shield");
                assert_eq!(*duration, 300);
                assert_eq!(*circle, 1);
            }
            _ => panic!("Expected SpellActive"),
        }
    }

    #[test]
    fn test_parse_spell_expire() {
        let mut parser = Parser::new();
        let input = r#"<spell_expire id="107"/>"#;

        let elements = parser.parse(input);

        assert_eq!(elements.len(), 1);
        match &elements[0] {
            ParsedElement::SpellExpire { id } => {
                assert_eq!(*id, 107);
            }
            _ => panic!("Expected SpellExpire"),
        }
    }

    #[test]
    fn test_missing_attributes() {
        let mut parser = Parser::new();
        // Missing required 'name' attribute
        let input = r#"<spell_active id="107" duration="300"/>"#;

        let elements = parser.parse(input);

        // Should not produce SpellActive (missing name)
        assert!(elements.iter().all(|e| !matches!(e, ParsedElement::SpellActive { .. })));
    }
}
```

## Common Patterns

### Self-Closing Tags

```rust
// <tag attr="value"/>
fn parse_self_closing(&mut self, tag: &str) -> Option<ParsedElement> {
    if !tag.ends_with('/') {
        return None;
    }
    let clean = tag.trim_end_matches('/');
    self.handle_tag(clean)
}
```

### Paired Tags with Content

```rust
// <tag>content</tag>
fn handle_open_tag(&mut self, name: &str, attrs: &HashMap<String, String>) {
    match name {
        "room_desc" => {
            self.state = ParserState::InRoomDesc;
            self.buffer.clear();
        }
        // ...
    }
}

fn handle_close_tag(&mut self, name: &str) -> Option<ParsedElement> {
    match name {
        "room_desc" => {
            let desc = std::mem::take(&mut self.buffer);
            self.state = ParserState::Normal;
            Some(ParsedElement::RoomDesc(desc))
        }
        // ...
    }
}
```

### Streaming Content

```rust
// <pushStream id="combat"/>...<popStream/>
fn handle_push_stream(&mut self, id: &str) {
    self.stream_stack.push(id.to_string());
    self.current_stream = Some(id.to_string());
}

fn handle_pop_stream(&mut self) {
    self.stream_stack.pop();
    self.current_stream = self.stream_stack.last().cloned();
}
```

### Attribute Extraction

```rust
fn parse_attributes(tag_content: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let re = regex::Regex::new(r#"(\w+)="([^"]*)""#).unwrap();

    for cap in re.captures_iter(tag_content) {
        attrs.insert(cap[1].to_string(), cap[2].to_string());
    }

    attrs
}
```

## Best Practices

### Graceful Degradation

```rust
// GOOD: Handle missing attributes
fn from_attributes(attrs: &HashMap<String, String>) -> Option<Self> {
    Some(Self {
        required: attrs.get("required")?.clone(),
        optional: attrs.get("optional").cloned(),  // Optional field
    })
}

// BAD: Panic on missing data
fn from_attributes(attrs: &HashMap<String, String>) -> Self {
    Self {
        required: attrs["required"].clone(),  // Panics if missing!
    }
}
```

### Preserve Unknown Elements

```rust
// Don't discard unrecognized tags
fn handle_unknown_tag(&mut self, tag: &str) -> ParsedElement {
    ParsedElement::Unknown {
        tag: tag.to_string(),
        raw: self.current_raw.clone(),
    }
}
```

### Performance

```rust
// GOOD: Compile regex once
lazy_static! {
    static ref ATTR_REGEX: Regex = Regex::new(r#"(\w+)="([^"]*)""#).unwrap();
}

// BAD: Compile regex every time
fn parse_attrs(s: &str) {
    let re = Regex::new(r#"(\w+)="([^"]*)""#).unwrap();  // Slow!
}
```

## Debugging Parser Issues

### Logging

```rust
fn handle_tag(&mut self, tag: &str) -> Option<ParsedElement> {
    log::trace!("Parsing tag: {}", tag);

    let result = self.parse_tag_internal(tag);

    if result.is_none() {
        log::debug!("Unhandled tag: {}", tag);
    }

    result
}
```

### Test with Real Data

Save actual game output for test fixtures:

```rust
#[test]
fn test_real_game_output() {
    let input = include_str!("../test_fixtures/combat_sample.xml");
    let mut parser = Parser::new();
    let elements = parser.parse(input);

    // Verify expected elements are present
    assert!(elements.iter().any(|e| matches!(e, ParsedElement::Combat { .. })));
}
```

## See Also

- [Parser Protocol](../architecture/parser-protocol.md) - Protocol details
- [Message Flow](../architecture/message-flow.md) - Data flow
- [Adding Widgets](./adding-widgets.md) - Use parsed data

