# Adding Browsers

Guide to creating new browser (popup window) types for VellumFE.

## Overview

Browsers are popup windows that provide extended functionality:

- Configuration editors
- Search interfaces
- Help dialogs
- Selection menus

They overlay the main interface temporarily and capture input focus.

## Browser Architecture

### Browser Lifecycle

```
Trigger → Creation → Render → Input Loop → Result → Cleanup
   │          │         │          │          │         │
   │          │         │          │          │         └─ Restore main UI
   │          │         │          │          └─ Return data or action
   │          │         │          └─ Handle keys until dismiss
   │          │         └─ Draw overlay on screen
   │          └─ Instantiate browser with context
   └─ Keybind or menu action
```

### Key Components

```rust
// Browser trait defines popup behavior
pub trait Browser {
    fn render(&self, frame: &mut Frame, area: Rect);
    fn handle_input(&mut self, key: KeyEvent) -> BrowserResult;
    fn title(&self) -> &str;
}

// Result of browser interaction
pub enum BrowserResult {
    Continue,           // Keep browser open
    Close,              // Close without action
    Action(BrowserAction),  // Close with action
}

pub enum BrowserAction {
    SelectItem(String),
    UpdateConfig(ConfigChange),
    ExecuteCommand(String),
    // ...
}
```

## Step-by-Step: New Browser

Let's create a "HighlightEditor" browser for editing highlight patterns.

### Step 1: Define Browser Structure

Create `src/frontend/tui/highlight_editor.rs`:

```rust
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct HighlightEditor {
    highlights: Vec<HighlightEntry>,
    selected: usize,
    editing: Option<EditField>,
    scroll_offset: usize,
}

struct HighlightEntry {
    pattern: String,
    foreground: String,
    background: Option<String>,
    enabled: bool,
}

enum EditField {
    Pattern,
    Foreground,
    Background,
}

impl HighlightEditor {
    pub fn new(highlights: Vec<HighlightConfig>) -> Self {
        let entries = highlights.into_iter()
            .map(|h| HighlightEntry {
                pattern: h.pattern,
                foreground: h.fg,
                background: h.bg,
                enabled: h.enabled,
            })
            .collect();

        Self {
            highlights: entries,
            selected: 0,
            editing: None,
            scroll_offset: 0,
        }
    }
}
```

### Step 2: Implement Browser Trait

```rust
impl Browser for HighlightEditor {
    fn title(&self) -> &str {
        "Highlight Editor"
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Calculate centered popup area
        let popup_area = centered_rect(80, 80, area);

        // Clear background
        frame.render_widget(Clear, popup_area);

        // Draw border
        let block = Block::default()
            .title(self.title())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Split into list and detail areas
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(inner);

        // Render highlight list
        self.render_list(frame, chunks[0]);

        // Render selected highlight details
        self.render_details(frame, chunks[1]);

        // Render help bar at bottom
        self.render_help(frame, popup_area);
    }

    fn handle_input(&mut self, key: KeyEvent) -> BrowserResult {
        match key.code {
            KeyCode::Esc => {
                if self.editing.is_some() {
                    self.editing = None;
                    BrowserResult::Continue
                } else {
                    BrowserResult::Close
                }
            }
            KeyCode::Enter => {
                if self.editing.is_some() {
                    self.commit_edit();
                    self.editing = None;
                } else {
                    self.start_edit();
                }
                BrowserResult::Continue
            }
            KeyCode::Up => {
                self.move_selection(-1);
                BrowserResult::Continue
            }
            KeyCode::Down => {
                self.move_selection(1);
                BrowserResult::Continue
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                BrowserResult::Action(BrowserAction::UpdateConfig(
                    self.build_config_change()
                ))
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.add_new_highlight();
                BrowserResult::Continue
            }
            KeyCode::Delete => {
                self.delete_selected();
                BrowserResult::Continue
            }
            _ => {
                if self.editing.is_some() {
                    self.handle_edit_key(key);
                }
                BrowserResult::Continue
            }
        }
    }
}
```

### Step 3: Implement Helper Methods

```rust
impl HighlightEditor {
    fn render_list(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.highlights
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .map(|(i, h)| {
                let style = if i == self.selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                let enabled = if h.enabled { "✓" } else { " " };
                let text = format!("[{}] {}", enabled, h.pattern);
                ListItem::new(text).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().title("Patterns").borders(Borders::ALL));

        frame.render_widget(list, area);
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        if let Some(highlight) = self.highlights.get(self.selected) {
            let details = vec![
                Line::from(vec![
                    Span::raw("Pattern: "),
                    Span::styled(&highlight.pattern, Style::default().fg(Color::Yellow)),
                ]),
                Line::from(vec![
                    Span::raw("FG: "),
                    Span::styled(&highlight.foreground, Style::default().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::raw("BG: "),
                    Span::raw(highlight.background.as_deref().unwrap_or("none")),
                ]),
            ];

            let paragraph = Paragraph::new(details)
                .block(Block::default().title("Details").borders(Borders::ALL));

            frame.render_widget(paragraph, area);
        }
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let help = " Enter: Edit | Ctrl+S: Save | Ctrl+N: New | Del: Delete | Esc: Close ";
        let help_area = Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        };
        let help_text = Paragraph::new(help)
            .style(Style::default().bg(Color::DarkGray));
        frame.render_widget(help_text, help_area);
    }

    fn move_selection(&mut self, delta: i32) {
        let new_idx = (self.selected as i32 + delta)
            .max(0)
            .min(self.highlights.len() as i32 - 1) as usize;
        self.selected = new_idx;
    }

    fn start_edit(&mut self) {
        self.editing = Some(EditField::Pattern);
    }

    fn commit_edit(&mut self) {
        // Apply edit changes
    }

    fn add_new_highlight(&mut self) {
        self.highlights.push(HighlightEntry {
            pattern: "new_pattern".into(),
            foreground: "white".into(),
            background: None,
            enabled: true,
        });
        self.selected = self.highlights.len() - 1;
    }

    fn delete_selected(&mut self) {
        if !self.highlights.is_empty() {
            self.highlights.remove(self.selected);
            if self.selected >= self.highlights.len() && self.selected > 0 {
                self.selected -= 1;
            }
        }
    }

    fn build_config_change(&self) -> ConfigChange {
        // Convert entries back to config format
        ConfigChange::Highlights(
            self.highlights.iter()
                .map(|h| HighlightConfig {
                    pattern: h.pattern.clone(),
                    fg: h.foreground.clone(),
                    bg: h.background.clone(),
                    enabled: h.enabled,
                })
                .collect()
        )
    }
}

// Helper function for centered popups
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
```

### Step 4: Register Browser

Add to browser registry:

```rust
pub enum BrowserType {
    Search,
    Help,
    LayoutEditor,
    HighlightEditor,  // New browser
}

pub fn open_browser(browser_type: BrowserType, context: BrowserContext) -> Box<dyn Browser> {
    match browser_type {
        BrowserType::Search => Box::new(SearchBrowser::new(context)),
        BrowserType::Help => Box::new(HelpBrowser::new()),
        BrowserType::LayoutEditor => Box::new(LayoutEditor::new(context)),
        BrowserType::HighlightEditor => Box::new(HighlightEditor::new(context.highlights)),
    }
}
```

### Step 5: Add Keybind

Allow users to open the browser:

```rust
// In keybind action handling
KeybindAction::OpenHighlightEditor => {
    self.active_browser = Some(open_browser(
        BrowserType::HighlightEditor,
        self.build_browser_context()
    ));
}
```

## Browser Best Practices

### Input Handling

```rust
// Always allow Escape to close
fn handle_input(&mut self, key: KeyEvent) -> BrowserResult {
    match key.code {
        KeyCode::Esc => BrowserResult::Close,
        // ... other handling
    }
}

// Provide keyboard help
fn render_help(&self) {
    // Always show available key commands
}
```

### State Management

```rust
// Keep original data for cancel
struct MyBrowser {
    original: Vec<Item>,  // For reverting
    modified: Vec<Item>,  // Working copy
}

fn handle_input(&mut self, key: KeyEvent) -> BrowserResult {
    match key.code {
        KeyCode::Esc => {
            // Discard changes
            BrowserResult::Close
        }
        KeyCode::Char('s') if ctrl => {
            // Save changes
            BrowserResult::Action(self.build_action())
        }
    }
}
```

### Visual Feedback

```rust
// Indicate edit mode
fn render(&self, frame: &mut Frame, area: Rect) {
    let title = if self.editing.is_some() {
        format!("{} [EDITING]", self.title())
    } else {
        self.title().to_string()
    };
    // ...
}

// Highlight modified fields
fn render_field(&self, value: &str, modified: bool) -> Span {
    let style = if modified {
        Style::default().fg(Color::Yellow)  // Modified indicator
    } else {
        Style::default()
    };
    Span::styled(value, style)
}
```

## Testing Browsers

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_escape_closes() {
        let mut browser = HighlightEditor::new(vec![]);
        let result = browser.handle_input(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(result, BrowserResult::Close));
    }

    #[test]
    fn test_selection_movement() {
        let mut browser = HighlightEditor::new(vec![
            HighlightConfig::default(),
            HighlightConfig::default(),
        ]);
        assert_eq!(browser.selected, 0);

        browser.handle_input(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(browser.selected, 1);

        browser.handle_input(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(browser.selected, 0);
    }
}
```

## See Also

- [Browser System Architecture](../architecture/browser-editors.md)
- [Adding Widgets](./adding-widgets.md)
- [Project Structure](./project-structure.md)

