# Testing

Test patterns and practices for VellumFE development.

## Current Test Status

VellumFE has comprehensive test coverage:

| Test Type | Count | Description |
|-----------|-------|-------------|
| **Unit Tests** | 907 | In-module tests (`#[cfg(test)]`) |
| **Parser Integration** | 34 | XML parsing verification |
| **UI Integration** | 57 | End-to-end UI state tests |
| **Doc Tests** | 4 | Documentation examples |
| **Ignored** | 1 | Clipboard (requires system) |
| **Total** | **1,003** | All tests pass ✅ |

## Testing Philosophy

VellumFE testing prioritizes:

1. **Parser accuracy** - Correctly parse game protocol
2. **State consistency** - Updates propagate correctly
3. **Widget rendering** - Display matches data
4. **Configuration loading** - Files parse without errors
5. **Real-world validation** - Tests use actual game XML
6. **End-to-end flow** - XML → Parser → MessageProcessor → UiState

## Test Organization

```
vellum-fe/
├── src/
│   ├── parser.rs
│   │   └── #[cfg(test)] mod tests { ... }
│   ├── config.rs
│   │   └── #[cfg(test)] mod tests { ... }
│   ├── selection.rs
│   │   └── #[cfg(test)] mod tests { ... }
│   ├── clipboard.rs
│   │   └── #[cfg(test)] mod tests { ... }
│   └── ...
│
├── tests/                     # Integration tests
│   ├── parser_integration.rs  # 34 parser-level tests
│   ├── ui_integration.rs      # 57 end-to-end UI tests
│   └── fixtures/              # 45 real game XML fixtures
│       ├── session_start.xml
│       ├── vitals_indicators.xml
│       ├── room_navigation.xml
│       ├── combat_roundtime.xml
│       ├── buffs_progress.xml
│       ├── active_effects.xml
│       ├── injuries.xml
│       ├── text_routing.xml
│       └── ... (45 total fixtures)
│
└── Cargo.toml
```

## Unit Tests

### Basic Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name() {
        // Arrange
        let input = "test data";

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, expected_value);
    }
}
```

### Parser Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_room_name() {
        let mut parser = Parser::new();
        let input = "<roomName>Town Square</roomName>";

        let elements = parser.parse(input);

        assert_eq!(elements.len(), 1);
        assert!(matches!(
            &elements[0],
            ParsedElement::RoomName(name) if name == "Town Square"
        ));
    }

    #[test]
    fn test_parse_vitals() {
        let mut parser = Parser::new();
        let input = r#"<progressBar id="health" value="75"/>"#;

        let elements = parser.parse(input);

        assert_eq!(elements.len(), 1);
        if let ParsedElement::Vitals(data) = &elements[0] {
            assert_eq!(data.health, Some(75));
        } else {
            panic!("Expected Vitals element");
        }
    }

    #[test]
    fn test_parse_malformed_input() {
        let mut parser = Parser::new();
        let input = "<unclosed tag without closing";

        // Should not panic, may return empty or partial results
        let elements = parser.parse(input);
        // Parser should handle gracefully
    }

    #[test]
    fn test_parse_empty_input() {
        let mut parser = Parser::new();
        let elements = parser.parse("");
        assert!(elements.is_empty());
    }
}
```

### Configuration Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();

        assert_eq!(config.connection.port, 8000);
        assert!(config.connection.auto_reconnect);
    }

    #[test]
    fn test_load_valid_config() {
        let toml = r#"
            [connection]
            host = "127.0.0.1"
            port = 8001

            [[widgets]]
            type = "text"
            name = "main"
            x = 0
            y = 0
            width = 100
            height = 100
        "#;

        let config: Config = toml::from_str(toml).unwrap();

        assert_eq!(config.connection.port, 8001);
        assert_eq!(config.widgets.len(), 1);
    }

    #[test]
    fn test_invalid_config() {
        let toml = "invalid toml [[[";

        let result: Result<Config, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        config.widgets.push(WidgetConfig {
            width: 0,  // Invalid
            height: 0, // Invalid
            ..Default::default()
        });

        let result = config.validate();
        assert!(result.is_err());
    }
}
```

### Widget Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_creation() {
        let config = TextWidgetConfig {
            name: "test".into(),
            width: 80,
            height: 24,
            ..Default::default()
        };

        let widget = TextWidget::new(config);

        assert_eq!(widget.name(), "test");
        assert!(widget.can_focus());
    }

    #[test]
    fn test_widget_sync() {
        let mut widget = TextWidget::new(Default::default());
        let mut state = AppState::new();

        // Initial sync
        state.set_generation(1);
        assert!(widget.sync(&state));
        assert_eq!(widget.last_generation(), 1);

        // No change
        assert!(!widget.sync(&state));

        // After state change
        state.add_text("Hello");
        state.set_generation(2);
        assert!(widget.sync(&state));
        assert_eq!(widget.last_generation(), 2);
    }

    #[test]
    fn test_scrollback_limit() {
        let config = TextWidgetConfig {
            scrollback: 100,
            ..Default::default()
        };
        let mut widget = TextWidget::new(config);

        // Add more lines than scrollback limit
        for i in 0..200 {
            widget.add_line(format!("Line {}", i));
        }

        assert!(widget.line_count() <= 100);
    }
}
```

## Integration Tests

VellumFE has **91 integration tests** across two test files using **45 real game XML fixtures**.

### Parser Integration (`parser_integration.rs` - 34 tests)

Tests XML parsing at the parser level:

| Category | Tests | Description |
|----------|-------|-------------|
| Session Start | 7 | Mode, player ID, stream windows |
| Vitals & Indicators | 5 | Health, mana, hands, status |
| Room Navigation | 6 | Compass, exits, room components |
| Combat | 5 | Roundtime, casttime, combat flow |
| Edge Cases | 6 | Empty input, malformed XML, unicode |
| Performance | 2 | Parsing speed benchmarks |
| Parser State | 3 | Stream tracking, state reusability |

### UI Integration (`ui_integration.rs` - 57 tests)

End-to-end tests: XML → Parser → MessageProcessor → UiState:

| Category | Tests | Description |
|----------|-------|-------------|
| Progress Bars | 12 | Vitals, stance, mindstate, custom IDs |
| Countdowns | 4 | Roundtime, casttime timers |
| Active Effects | 8 | Buffs, debuffs, cooldowns, spells |
| Indicators | 6 | Status indicators, dashboard icons |
| Room & Navigation | 5 | Room components, compass, subtitle |
| Text Routing | 8 | Stream routing, tabbed windows |
| Hands & Inventory | 6 | Left/right hands, spell hand, inventory |
| Entity Counts | 4 | Player count, target count |
| Edge Cases | 4 | Clear events, unknown streams |

### Test Fixtures (45 files)

Located in `tests/fixtures/`:

| Category | Fixtures | Content |
|----------|----------|---------|
| **Session** | `session_start.xml`, `player_counts.xml` | Login, player data |
| **Vitals** | `vitals_indicators.xml`, `progress_*.xml` | Health, mana, custom bars |
| **Combat** | `combat_*.xml`, `roundtime_*.xml` | Combat, RT/CT, targets |
| **Effects** | `active_effects*.xml`, `buffs_*.xml` | Buffs, debuffs, cooldowns |
| **Indicators** | `indicators_*.xml`, `icon_*.xml` | Status indicators |
| **Room** | `room_*.xml`, `text_routing*.xml` | Room data, stream routing |
| **Injuries** | `injuries*.xml` | Body part injuries |
| **Hands** | `*_hand*.xml` | Equipment in hands |
| **Misc** | `spells_stream.xml`, `tabbed_*.xml` | Spells, tabbed windows |

### Example Integration Test

```rust
use vellum_fe::parser::{ParsedElement, XmlParser};

/// Helper to parse XML and collect all elements
fn parse_xml(xml: &str) -> Vec<ParsedElement> {
    let mut parser = XmlParser::new();
    let mut all_elements = Vec::new();
    for line in xml.lines() {
        all_elements.extend(parser.parse_line(line));
    }
    all_elements
}

#[test]
fn test_extracts_compass_directions() {
    let xml = include_str!("fixtures/room_navigation.xml");
    let elements = parse_xml(xml);

    let compass = elements.iter().find(|e| {
        matches!(e, ParsedElement::Compass { .. })
    });

    assert!(compass.is_some(), "Should find compass element");

    if let Some(ParsedElement::Compass { directions }) = compass {
        assert!(directions.contains(&"n".to_string()));
        assert!(directions.contains(&"s".to_string()));
    }
}
```

### Creating New Fixtures

Capture real game output for testing:

```bash
# Game logs are stored by Lich at:
# ~/.lich5/logs/GSIV-CharName/YYYY/MM/

# Copy relevant XML sections to test fixtures:
cp ~/.lich5/logs/GSIV-Nisugi/2025/10/session.xml tests/fixtures/
```

### Using Fixtures

```rust
const COMBAT_FIXTURE: &str = include_str!("fixtures/combat_roundtime.xml");

#[test]
fn test_combat_parsing() {
    let elements = parse_xml(COMBAT_FIXTURE);

    // Find roundtime elements
    let roundtime = elements.iter().find(|e| {
        matches!(e, ParsedElement::RoundTime { .. })
    });

    assert!(roundtime.is_some());
}
```

## Running Tests

### All Tests

```bash
cargo test
```

### Specific Test

```bash
cargo test test_parse_room_name
```

### Specific Module

```bash
cargo test parser::tests
```

### With Output

```bash
cargo test -- --nocapture
```

### Verbose

```bash
cargo test -- --show-output
```

## Test Patterns

### Table-Driven Tests

```rust
#[test]
fn test_parse_indicators() {
    let cases = vec![
        ("<indicator id='IconHIDDEN' visible='y'/>", "hidden", true),
        ("<indicator id='IconHIDDEN' visible='n'/>", "hidden", false),
        ("<indicator id='IconSTUNNED' visible='y'/>", "stunned", true),
        ("<indicator id='IconPRONE' visible='y'/>", "prone", true),
    ];

    for (input, expected_name, expected_visible) in cases {
        let mut parser = Parser::new();
        let elements = parser.parse(input);

        if let ParsedElement::Indicator { name, visible } = &elements[0] {
            assert_eq!(name, expected_name, "Failed for input: {}", input);
            assert_eq!(*visible, expected_visible, "Failed for input: {}", input);
        } else {
            panic!("Expected Indicator for input: {}", input);
        }
    }
}
```

### Property-Based Testing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_scrollback_never_exceeds_limit(lines in 1..1000usize, limit in 1..500usize) {
        let mut widget = TextWidget::new(TextWidgetConfig {
            scrollback: limit,
            ..Default::default()
        });

        for i in 0..lines {
            widget.add_line(format!("Line {}", i));
        }

        prop_assert!(widget.line_count() <= limit);
    }
}
```

### Snapshot Testing

```rust
#[test]
fn test_render_snapshot() {
    let widget = ProgressWidget::new(Default::default());
    let mut state = AppState::new();
    state.vitals_mut().health = 75;

    widget.sync(&state);

    // Render to string buffer
    let rendered = widget.render_to_string();

    // Compare with stored snapshot
    insta::assert_snapshot!(rendered);
}
```

## Mocking

### Mock Network

```rust
struct MockConnection {
    responses: Vec<String>,
    index: usize,
}

impl MockConnection {
    fn new(responses: Vec<String>) -> Self {
        Self { responses, index: 0 }
    }
}

impl Connection for MockConnection {
    fn receive(&mut self) -> Result<String> {
        if self.index < self.responses.len() {
            let response = self.responses[self.index].clone();
            self.index += 1;
            Ok(response)
        } else {
            Err(Error::EndOfStream)
        }
    }
}

#[test]
fn test_with_mock_connection() {
    let mock = MockConnection::new(vec![
        "<roomName>Test Room</roomName>".into(),
    ]);

    let mut app = App::with_connection(mock);
    app.tick();

    assert_eq!(app.state().room_name(), Some("Test Room"));
}
```

## Continuous Integration

### GitHub Actions

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-action@stable
      - run: cargo test --all-features
```

### Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Run coverage
cargo tarpaulin --out Html
```

## See Also

- [Building](./building.md) - Build process
- [Contributing](./contributing.md) - Contribution guide
- [Project Structure](./project-structure.md) - Code organization

