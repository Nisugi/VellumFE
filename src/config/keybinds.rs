use super::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyBindAction {
    Action(String),     // Just an action: "cursor_word_left"
    Macro(MacroAction), // A macro with text
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroAction {
    pub macro_text: String, // e.g., "sw\r" for southwest movement
}

impl KeyBindAction {
    /// Returns the type name of this keybind action
    pub fn type_name(&self) -> &'static str {
        match self {
            KeyBindAction::Action(_) => "Action",
            KeyBindAction::Macro(_) => "Macro",
        }
    }

    /// Returns the display value for this keybind action
    pub fn display_value(&self) -> String {
        match self {
            KeyBindAction::Action(a) => a.clone(),
            KeyBindAction::Macro(m) => m.macro_text.clone(),
        }
    }
}

/// Application keybinds that work across all modes or are mode-specific
/// These are checked in Layer 1 of the keybind dispatch system (before menu and game keybinds)
/// Note: Previously called GlobalKeybinds, renamed to avoid confusion with "global" folder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppKeybinds {
    /// Quit the application (default: "ctrl+c")
    #[serde(default = "default_quit_keybind")]
    pub quit: String,

    /// Start search mode (default: "ctrl+f")
    #[serde(default = "default_start_search_keybind")]
    pub start_search: String,

    /// Next search match - only works in Search mode (default: "ctrl+pagedown")
    #[serde(default = "default_next_search_match_keybind")]
    pub next_search_match: String,

    /// Previous search match - only works in Search mode (default: "ctrl+pageup")
    #[serde(default = "default_prev_search_match_keybind")]
    pub prev_search_match: String,

    /// Close priority windows (menus, browsers, forms) and exit modes (default: "esc")
    #[serde(default = "default_close_window_keybind")]
    pub close_window: String,
}

fn default_quit_keybind() -> String {
    "ctrl+c".to_string()
}

fn default_start_search_keybind() -> String {
    "ctrl+f".to_string()
}

fn default_next_search_match_keybind() -> String {
    "ctrl+pagedown".to_string()
}

fn default_prev_search_match_keybind() -> String {
    "ctrl+pageup".to_string()
}

fn default_close_window_keybind() -> String {
    "esc".to_string()
}

impl Default for AppKeybinds {
    fn default() -> Self {
        Self {
            quit: default_quit_keybind(),
            start_search: default_start_search_keybind(),
            next_search_match: default_next_search_match_keybind(),
            prev_search_match: default_prev_search_match_keybind(),
            close_window: default_close_window_keybind(),
        }
    }
}

/// Actions that can be bound to keys
#[derive(Debug, Clone, PartialEq)]
pub enum KeyAction {
    // Command input actions
    SendCommand,
    CursorLeft,
    CursorRight,
    CursorWordLeft,
    CursorWordRight,
    CursorHome,
    CursorEnd,
    CursorBackspace,
    CursorDelete,
    CursorDeleteWord,  // Delete from cursor to end of word
    CursorClearLine,   // Clear entire command line

    // History actions
    PreviousCommand,
    NextCommand,
    SendLastCommand,
    SendSecondLastCommand,

    // Window actions
    SwitchCurrentWindow,
    ScrollCurrentWindowUpOne,
    ScrollCurrentWindowDownOne,
    ScrollCurrentWindowUpPage,
    ScrollCurrentWindowDownPage,
    ScrollCurrentWindowHome,  // Scroll to top of window
    ScrollCurrentWindowEnd,   // Scroll to bottom of window

    // Search actions (already implemented)
    StartSearch,
    NextSearchMatch,
    PrevSearchMatch,
    ClearSearch,

    // Tab navigation (for TabbedText widgets)
    NextTab,           // Switch to next tab
    PrevTab,           // Switch to previous tab
    NextUnreadTab,     // Jump to next tab with unread messages

    // Clipboard actions
    Copy,              // Copy selected text to clipboard
    Paste,             // Paste from clipboard
    SelectAll,         // Select all text in command input

    // System toggles
    TogglePerformanceStats,  // Show/hide performance overlay
    ToggleSounds,            // Enable/disable sound system

    // TTS (Text-to-Speech) actions - Accessibility
    TtsNext,           // Next message (sequential, includes read)
    TtsPrevious,       // Previous message (sequential, includes read)
    TtsNextUnread,     // Skip to next unread message
    TtsStop,           // Stop current speech (keeps position)
    TtsMuteToggle,     // Toggle TTS mute on/off
    TtsIncreaseRate,   // Increase speech rate by 0.1
    TtsDecreaseRate,   // Decrease speech rate by 0.1
    TtsIncreaseVolume, // Increase volume by 0.1
    TtsDecreaseVolume, // Decrease volume by 0.1

    // Macro - send literal text
    SendMacro(String),
}

/// Keybinds for menu system (popups, browsers, forms, editors)
/// These are separate from game keybinds and only active when menus have focus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuKeybinds {
    // Navigation
    #[serde(default = "default_navigate_up")]
    pub navigate_up: String,
    #[serde(default = "default_navigate_down")]
    pub navigate_down: String,
    #[serde(default = "default_navigate_left")]
    pub navigate_left: String,
    #[serde(default = "default_navigate_right")]
    pub navigate_right: String,
    #[serde(default = "default_page_up")]
    pub page_up: String,
    #[serde(default = "default_page_down")]
    pub page_down: String,
    #[serde(default = "default_home")]
    pub home: String,
    #[serde(default = "default_end")]
    pub end: String,

    // Field Navigation
    #[serde(default = "default_next_field")]
    pub next_field: String,
    #[serde(default = "default_previous_field")]
    pub previous_field: String,

    // Actions
    #[serde(default = "default_select")]
    pub select: String,
    #[serde(default = "default_cancel")]
    pub cancel: String,
    #[serde(default = "default_save")]
    pub save: String,
    #[serde(default = "default_delete")]
    pub delete: String,

    // Text Editing (Clipboard)
    #[serde(default = "default_select_all")]
    pub select_all: String,
    #[serde(default = "default_copy")]
    pub copy: String,
    #[serde(default = "default_cut")]
    pub cut: String,
    #[serde(default = "default_paste")]
    pub paste: String,

    // Toggles/Cycling
    #[serde(default = "default_toggle")]
    pub toggle: String,
    #[serde(default = "default_toggle_filter")]
    pub toggle_filter: String,
    #[serde(default = "default_cycle_forward")]
    pub cycle_forward: String,
    #[serde(default = "default_cycle_backward")]
    pub cycle_backward: String,

    // Reordering (WindowEditor)
    #[serde(default = "default_move_up")]
    pub move_up: String,
    #[serde(default = "default_move_down")]
    pub move_down: String,

    // List Management (WindowEditor)
    #[serde(default = "default_add")]
    pub add: String,
    #[serde(default = "default_edit")]
    pub edit: String,
}

// Default keybind functions
fn default_navigate_up() -> String {
    "Up".to_string()
}
fn default_navigate_down() -> String {
    "Down".to_string()
}
fn default_navigate_left() -> String {
    "Left".to_string()
}
fn default_navigate_right() -> String {
    "Right".to_string()
}
fn default_page_up() -> String {
    "PageUp".to_string()
}
fn default_page_down() -> String {
    "PageDown".to_string()
}
fn default_home() -> String {
    "Home".to_string()
}
fn default_end() -> String {
    "End".to_string()
}
fn default_next_field() -> String {
    "Tab".to_string()
}
fn default_previous_field() -> String {
    "Shift+Tab".to_string()
}
fn default_select() -> String {
    "Enter".to_string()
}
fn default_cancel() -> String {
    "Esc".to_string()
}
fn default_save() -> String {
    "Ctrl+s".to_string()
}
fn default_delete() -> String {
    "Delete".to_string()
}
fn default_select_all() -> String {
    "Ctrl+A".to_string()
}
fn default_copy() -> String {
    "Ctrl+C".to_string()
}
fn default_cut() -> String {
    "Ctrl+X".to_string()
}
fn default_paste() -> String {
    "Ctrl+V".to_string()
}
fn default_toggle() -> String {
    "Space".to_string()
}
fn default_toggle_filter() -> String {
    "F".to_string()
}
fn default_cycle_forward() -> String {
    "Right".to_string()
}
fn default_cycle_backward() -> String {
    "Left".to_string()
}
fn default_move_up() -> String {
    "Shift+Up".to_string()
}
fn default_move_down() -> String {
    "Shift+Down".to_string()
}
fn default_add() -> String {
    "A".to_string()
}
fn default_edit() -> String {
    "E".to_string()
}

impl Default for MenuKeybinds {
    fn default() -> Self {
        Self {
            navigate_up: default_navigate_up(),
            navigate_down: default_navigate_down(),
            navigate_left: default_navigate_left(),
            navigate_right: default_navigate_right(),
            page_up: default_page_up(),
            page_down: default_page_down(),
            home: default_home(),
            end: default_end(),
            next_field: default_next_field(),
            previous_field: default_previous_field(),
            select: default_select(),
            cancel: default_cancel(),
            save: default_save(),
            delete: default_delete(),
            select_all: default_select_all(),
            copy: default_copy(),
            cut: default_cut(),
            paste: default_paste(),
            toggle: default_toggle(),
            toggle_filter: default_toggle_filter(),
            cycle_forward: default_cycle_forward(),
            cycle_backward: default_cycle_backward(),
            move_up: default_move_up(),
            move_down: default_move_down(),
            add: default_add(),
            edit: default_edit(),
        }
    }
}

impl MenuKeybinds {
    /// Resolve a KeyEvent to a MenuAction based on the current context
    pub fn resolve_action(
        &self,
        key: &crate::frontend::common::KeyEvent,
        context: crate::core::menu_actions::ActionContext,
    ) -> crate::core::menu_actions::MenuAction {
        use crate::core::menu_actions::{key_event_to_string, ActionContext, MenuAction};

        let key_str = key_event_to_string(*key);
        let key_lower = key_str.to_lowercase();

        // DEBUG: Log what we're resolving
        tracing::debug!("🔍 resolve_action: key_str='{}', context={:?}", key_str, context);
        tracing::debug!("   Config values: navigate_up='{}', navigate_down='{}', select='{}', cancel='{}'",
                       self.navigate_up, self.navigate_down, self.select, self.cancel);

        // Special handling for BackTab (Shift+Tab)
        if matches!(key.code, KeyCode::BackTab)
            && (key_lower == self.previous_field.to_lowercase() || key_lower == "shift+tab") {
                return MenuAction::PreviousField;
            }

        // Context-specific bindings first (override general bindings)
        match context {
            ActionContext::Dropdown => {
                // In dropdown, Up/Down cycle through options instead of navigating
                if key_lower == self.navigate_up.to_lowercase() {
                    return MenuAction::NavigateUp; // Will be interpreted as cycle prev
                }
                if key_lower == self.navigate_down.to_lowercase() {
                    return MenuAction::NavigateDown; // Will be interpreted as cycle next
                }
            }
            ActionContext::TextInput => {
                // Clipboard operations only valid in text input
                if key_lower == self.select_all.to_lowercase() {
                    return MenuAction::SelectAll;
                }
                if key_lower == self.copy.to_lowercase() {
                    return MenuAction::Copy;
                }
                if key_lower == self.cut.to_lowercase() {
                    return MenuAction::Cut;
                }
                if key_lower == self.paste.to_lowercase() {
                    return MenuAction::Paste;
                }
            }
            _ => {}
        }

        // Global menu keybindings
        if key_lower == self.cancel.to_lowercase() {
            return MenuAction::Cancel;
        }
        if key_lower == self.save.to_lowercase() {
            return MenuAction::Save;
        }
        if key_lower == self.select.to_lowercase() {
            return MenuAction::Select;
        }
        if key_lower == self.delete.to_lowercase() {
            return MenuAction::Delete;
        }

        if key_lower == self.navigate_up.to_lowercase() {
            return MenuAction::NavigateUp;
        }
        if key_lower == self.navigate_down.to_lowercase() {
            return MenuAction::NavigateDown;
        }
        if key_lower == self.navigate_left.to_lowercase() {
            return MenuAction::NavigateLeft;
        }
        if key_lower == self.navigate_right.to_lowercase() {
            return MenuAction::NavigateRight;
        }
        if key_lower == self.page_up.to_lowercase() {
            return MenuAction::PageUp;
        }
        if key_lower == self.page_down.to_lowercase() {
            return MenuAction::PageDown;
        }
        if key_lower == self.home.to_lowercase() {
            return MenuAction::Home;
        }
        if key_lower == self.end.to_lowercase() {
            return MenuAction::End;
        }

        if key_lower == self.next_field.to_lowercase() {
            return MenuAction::NextField;
        }
        if key_lower == self.previous_field.to_lowercase() {
            return MenuAction::PreviousField;
        }

        if key_lower == self.toggle.to_lowercase() {
            return MenuAction::Toggle;
        }

        if key_lower == self.move_up.to_lowercase() {
            return MenuAction::MoveUp;
        }
        if key_lower == self.move_down.to_lowercase() {
            return MenuAction::MoveDown;
        }

        // Browser-only actions (don't trigger in forms where text input is needed)
        if matches!(context, ActionContext::Browser) {
            if key_lower == self.add.to_lowercase() {
                return MenuAction::Add;
            }
            if key_lower == self.edit.to_lowercase() {
                return MenuAction::Edit;
            }
            if key_lower == self.toggle_filter.to_lowercase() {
                return MenuAction::ToggleFilter;
            }
        }

        if key_lower == self.cycle_forward.to_lowercase() {
            return MenuAction::CycleForward;
        }
        if key_lower == self.cycle_backward.to_lowercase() {
            return MenuAction::CycleBackward;
        }

        // No matching keybind
        MenuAction::None
    }
}

impl KeyAction {
    pub fn from_str(action: &str) -> Option<Self> {
        match action {
            "send_command" => Some(Self::SendCommand),
            "cursor_left" => Some(Self::CursorLeft),
            "cursor_right" => Some(Self::CursorRight),
            "cursor_word_left" => Some(Self::CursorWordLeft),
            "cursor_word_right" => Some(Self::CursorWordRight),
            "cursor_home" => Some(Self::CursorHome),
            "cursor_end" => Some(Self::CursorEnd),
            "cursor_backspace" => Some(Self::CursorBackspace),
            "cursor_delete" => Some(Self::CursorDelete),
            "cursor_delete_word" => Some(Self::CursorDeleteWord),
            "cursor_clear_line" => Some(Self::CursorClearLine),
            "previous_command" => Some(Self::PreviousCommand),
            "next_command" => Some(Self::NextCommand),
            "send_last_command" => Some(Self::SendLastCommand),
            "send_second_last_command" => Some(Self::SendSecondLastCommand),
            "switch_current_window" => Some(Self::SwitchCurrentWindow),
            "scroll_current_window_up_one" => Some(Self::ScrollCurrentWindowUpOne),
            "scroll_current_window_down_one" => Some(Self::ScrollCurrentWindowDownOne),
            "scroll_current_window_up_page" => Some(Self::ScrollCurrentWindowUpPage),
            "scroll_current_window_down_page" => Some(Self::ScrollCurrentWindowDownPage),
            "scroll_current_window_home" => Some(Self::ScrollCurrentWindowHome),
            "scroll_current_window_end" => Some(Self::ScrollCurrentWindowEnd),
            "start_search" => Some(Self::StartSearch),
            "next_search_match" => Some(Self::NextSearchMatch),
            "prev_search_match" => Some(Self::PrevSearchMatch),
            "clear_search" => Some(Self::ClearSearch),
            "next_tab" => Some(Self::NextTab),
            "prev_tab" => Some(Self::PrevTab),
            "next_unread_tab" => Some(Self::NextUnreadTab),
            "copy" => Some(Self::Copy),
            "paste" => Some(Self::Paste),
            "select_all" => Some(Self::SelectAll),
            "toggle_performance_stats" => Some(Self::TogglePerformanceStats),
            "toggle_sounds" => Some(Self::ToggleSounds),
            "tts_next" => Some(Self::TtsNext),
            "tts_previous" => Some(Self::TtsPrevious),
            "tts_next_unread" => Some(Self::TtsNextUnread),
            "tts_stop" => Some(Self::TtsStop),
            "tts_pause_resume" => Some(Self::TtsStop), // Legacy support
            "tts_mute_toggle" => Some(Self::TtsMuteToggle),
            "tts_increase_rate" => Some(Self::TtsIncreaseRate),
            "tts_decrease_rate" => Some(Self::TtsDecreaseRate),
            "tts_increase_volume" => Some(Self::TtsIncreaseVolume),
            "tts_decrease_volume" => Some(Self::TtsDecreaseVolume),
            _ => None,
        }
    }
}

/// Parse a key string like "ctrl+f" or "num_1" into KeyCode and KeyModifiers
pub fn parse_key_string(key_str: &str) -> Option<(KeyCode, KeyModifiers)> {
    // Normalize to lowercase for consistent comparisons
    let key_str_lower = key_str.to_lowercase();
    let key_str = key_str_lower.as_str();

    // Special case: "num_+" contains a '+' but it's not a modifier separator
    // If the string is exactly a numpad key (no modifiers), handle it first
    if key_str.starts_with("num_")
        && !key_str.contains("shift+")
        && !key_str.contains("ctrl+")
        && !key_str.contains("alt+")
    {
        let key_code = match key_str {
            "num_0" => KeyCode::Keypad0,
            "num_1" => KeyCode::Keypad1,
            "num_2" => KeyCode::Keypad2,
            "num_3" => KeyCode::Keypad3,
            "num_4" => KeyCode::Keypad4,
            "num_5" => KeyCode::Keypad5,
            "num_6" => KeyCode::Keypad6,
            "num_7" => KeyCode::Keypad7,
            "num_8" => KeyCode::Keypad8,
            "num_9" => KeyCode::Keypad9,
            "num_." => KeyCode::KeypadPeriod,
            "num_+" => KeyCode::KeypadPlus,
            "num_-" => KeyCode::KeypadMinus,
            "num_*" => KeyCode::KeypadMultiply,
            "num_/" => KeyCode::KeypadDivide,
            _ => return None,
        };
        return Some((key_code, KeyModifiers::NONE));
    }

    // For keys with modifiers, we need to carefully parse
    // Split by + but be aware that num_+ contains a literal +
    let parts: Vec<&str> = key_str.split('+').collect();
    let mut modifiers = KeyModifiers::NONE;
    let mut key_part = key_str;

    // Parse modifiers
    if parts.len() > 1 {
        for part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "alt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                _ => return None,
            }
        }
        key_part = parts[parts.len() - 1];
    }

    // Parse the actual key
    let key_code = match key_part {
        // Special keys
        "enter" => KeyCode::Enter,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "tab" => KeyCode::Tab,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "page_up" | "pageup" => KeyCode::PageUp,
        "page_down" | "pagedown" => KeyCode::PageDown,

        // Numpad keys (when used with modifiers like shift+num_1)
        "num_0" => KeyCode::Keypad0,
        "num_1" => KeyCode::Keypad1,
        "num_2" => KeyCode::Keypad2,
        "num_3" => KeyCode::Keypad3,
        "num_4" => KeyCode::Keypad4,
        "num_5" => KeyCode::Keypad5,
        "num_6" => KeyCode::Keypad6,
        "num_7" => KeyCode::Keypad7,
        "num_8" => KeyCode::Keypad8,
        "num_9" => KeyCode::Keypad9,
        "num_." => KeyCode::KeypadPeriod,
        "num_+" => KeyCode::KeypadPlus,
        "num_-" => KeyCode::KeypadMinus,
        "num_*" => KeyCode::KeypadMultiply,
        "num_/" => KeyCode::KeypadDivide,

        // Function keys
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),

        // Single character
        s if s.len() == 1 => {
            let ch = s.chars().next().unwrap();
            KeyCode::Char(ch)
        }

        _ => return None,
    };

    Some((key_code, modifiers))
}

impl Config {
    /// Load common (global) keybinds that apply to all characters
    /// Returns: HashMap of global keybinds, or empty if file doesn't exist
    pub fn load_common_keybinds() -> Result<HashMap<String, KeyBindAction>> {
        let path = Self::common_keybinds_path()?;

        if !path.exists() {
            return Ok(HashMap::new());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common keybinds: {:?}", path))?;

        // Parse the entire TOML file to get the [user] section
        let toml_value: toml::Value = toml::from_str(&contents)
            .context("Failed to parse common keybinds TOML")?;

        // Extract [user] section if it exists
        if let Some(user_section) = toml_value.get("user") {
            let keybinds: HashMap<String, KeyBindAction> = user_section.clone().try_into()
                .context("Failed to parse [user] section from common keybinds")?;
            Ok(keybinds)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Load keybinds for a character, merging global + character-specific
    /// Character-specific keybinds override global ones with the same key
    pub fn load_keybinds(character: Option<&str>) -> Result<HashMap<String, KeyBindAction>> {
        // Start with global/common keybinds
        let mut keybinds = Self::load_common_keybinds()?;

        // Load character-specific keybinds
        let keybinds_path = Self::keybinds_path(character)?;

        if keybinds_path.exists() {
            let contents =
                fs::read_to_string(&keybinds_path).context("Failed to read keybinds.toml")?;

            // Parse the entire TOML file to get the [user] section
            let toml_value: toml::Value = toml::from_str(&contents)
                .context("Failed to parse keybinds.toml")?;

            // Extract [user] section if it exists
            if let Some(user_section) = toml_value.get("user") {
                let character_keybinds: HashMap<String, KeyBindAction> = user_section.clone().try_into()
                    .context("Failed to parse [user] section")?;
                // Character keybinds override global (HashMap::extend)
                keybinds.extend(character_keybinds);
            }
        } else if keybinds.is_empty() {
            // No global and no character keybinds - use embedded defaults
            keybinds = toml::from_str(DEFAULT_KEYBINDS).unwrap_or_else(|_| default_keybinds());
        }

        Ok(keybinds)
    }

    /// Load only character-specific keybinds (not merged with global)
    /// Returns: HashMap of character keybinds, or empty if file doesn't exist
    pub fn load_character_keybinds_only(character: Option<&str>) -> Result<HashMap<String, KeyBindAction>> {
        let keybinds_path = Self::keybinds_path(character)?;

        if !keybinds_path.exists() {
            return Ok(HashMap::new());
        }

        let contents = fs::read_to_string(&keybinds_path)
            .with_context(|| format!("Failed to read character keybinds: {:?}", keybinds_path))?;

        // Parse the entire TOML file to get the [user] section
        let toml_value: toml::Value = toml::from_str(&contents)
            .context("Failed to parse character keybinds TOML")?;

        // Extract [user] section if it exists
        if let Some(user_section) = toml_value.get("user") {
            let keybinds: HashMap<String, KeyBindAction> = user_section.clone().try_into()
                .context("Failed to parse [user] section from character keybinds")?;
            Ok(keybinds)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Save keybinds to keybinds.toml for a character
    pub(crate) fn save_keybinds(&self, character: Option<&str>) -> Result<()> {
        let keybinds_path = Self::keybinds_path(character)?;
        let contents =
            toml::to_string_pretty(&self.keybinds).context("Failed to serialize keybinds")?;
        fs::write(&keybinds_path, contents).context("Failed to write keybinds.toml")?;
        Ok(())
    }

    /// Save a single keybind to the appropriate file based on scope
    ///
    /// # Arguments
    /// * `key` - The key combo (e.g., "f5", "ctrl+e")
    /// * `action` - The keybind action
    /// * `is_global` - If true, save to global/keybinds.toml; if false, save to character profile
    /// * `character` - Character name (required if is_global is false)
    pub fn save_single_keybind(
        key: &str,
        action: &KeyBindAction,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        let path = if is_global {
            Self::common_keybinds_path()?
        } else {
            Self::keybinds_path(character)?
        };

        // Load existing content or create new
        let mut toml_table: toml::value::Table = if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read keybinds file: {:?}", path))?;
            toml::from_str(&contents).unwrap_or_else(|_| toml::value::Table::new())
        } else {
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {:?}", parent))?;
            }
            toml::value::Table::new()
        };

        // Get or create [user] section
        let user_section = toml_table
            .entry("user".to_string())
            .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));

        if let toml::Value::Table(user_table) = user_section {
            // Convert KeyBindAction to TOML value
            let action_value = match action {
                KeyBindAction::Action(a) => toml::Value::String(a.clone()),
                KeyBindAction::Macro(m) => {
                    let mut macro_table = toml::value::Table::new();
                    macro_table.insert(
                        "macro_text".to_string(),
                        toml::Value::String(m.macro_text.clone()),
                    );
                    toml::Value::Table(macro_table)
                }
            };
            user_table.insert(key.to_string(), action_value);
        }

        // Write back to file
        let contents = toml::to_string_pretty(&toml_table)
            .context("Failed to serialize keybinds")?;
        fs::write(&path, contents)
            .with_context(|| format!("Failed to write keybinds file: {:?}", path))?;

        tracing::info!(
            "Saved keybind '{}' to {} keybinds file: {:?}",
            key,
            if is_global { "global" } else { "character" },
            path
        );

        Ok(())
    }

    /// Delete a single keybind from the appropriate file based on scope
    ///
    /// # Arguments
    /// * `key` - The key combo to delete
    /// * `is_global` - If true, delete from global/keybinds.toml; if false, from character profile
    /// * `character` - Character name (required if is_global is false)
    pub fn delete_single_keybind(
        key: &str,
        is_global: bool,
        character: Option<&str>,
    ) -> Result<()> {
        let path = if is_global {
            Self::common_keybinds_path()?
        } else {
            Self::keybinds_path(character)?
        };

        if !path.exists() {
            tracing::warn!(
                "Cannot delete keybind '{}' - file does not exist: {:?}",
                key,
                path
            );
            return Ok(());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read keybinds file: {:?}", path))?;

        let mut toml_table: toml::value::Table = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse keybinds file: {:?}", path))?;

        // Get [user] section and remove the key
        if let Some(toml::Value::Table(user_table)) = toml_table.get_mut("user") {
            if user_table.remove(key).is_some() {
                // Write back to file
                let contents = toml::to_string_pretty(&toml_table)
                    .context("Failed to serialize keybinds")?;
                fs::write(&path, contents)
                    .with_context(|| format!("Failed to write keybinds file: {:?}", path))?;

                tracing::info!(
                    "Deleted keybind '{}' from {} keybinds file: {:?}",
                    key,
                    if is_global { "global" } else { "character" },
                    path
                );
            } else {
                tracing::warn!(
                    "Keybind '{}' not found in [user] section of {:?}",
                    key,
                    path
                );
            }
        } else {
            tracing::warn!(
                "No [user] section found in {:?} - cannot delete keybind '{}'",
                path,
                key
            );
        }

        Ok(())
    }

    /// Validate app keybinds and log warnings for any issues
    fn validate_app_keybinds(keybinds: &AppKeybinds) {
        // Check each critical global keybind
        if keybinds.quit.is_empty() {
            tracing::warn!("Global keybind 'quit' is empty - application may be difficult to exit");
        } else if parse_key_string(&keybinds.quit).is_none() {
            tracing::warn!("Global keybind 'quit' has invalid value: '{}' - using default 'ctrl+c'", keybinds.quit);
        }

        if keybinds.start_search.is_empty() {
            tracing::warn!("Global keybind 'start_search' is empty - search feature disabled");
        } else if parse_key_string(&keybinds.start_search).is_none() {
            tracing::warn!("Global keybind 'start_search' has invalid value: '{}'", keybinds.start_search);
        }

        if keybinds.close_window.is_empty() {
            tracing::warn!("Global keybind 'close_window' is empty - may not be able to close dialogs");
        } else if parse_key_string(&keybinds.close_window).is_none() {
            tracing::warn!("Global keybind 'close_window' has invalid value: '{}'", keybinds.close_window);
        }

        if keybinds.next_search_match.is_empty() {
            tracing::debug!("Global keybind 'next_search_match' is empty");
        } else if parse_key_string(&keybinds.next_search_match).is_none() {
            tracing::warn!("Global keybind 'next_search_match' has invalid value: '{}'", keybinds.next_search_match);
        }

        if keybinds.prev_search_match.is_empty() {
            tracing::debug!("Global keybind 'prev_search_match' is empty");
        } else if parse_key_string(&keybinds.prev_search_match).is_none() {
            tracing::warn!("Global keybind 'prev_search_match' has invalid value: '{}'", keybinds.prev_search_match);
        }
    }

    /// Load common (global) app keybinds from global/keybinds.toml [app] section
    /// Returns: AppKeybinds from global, or default if file doesn't exist
    fn load_common_app_keybinds() -> Result<AppKeybinds> {
        let path = Self::common_keybinds_path()?;

        if !path.exists() {
            return Ok(AppKeybinds::default());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common keybinds: {:?}", path))?;

        let toml_value: toml::Value = toml::from_str(&contents)
            .context("Failed to parse common keybinds TOML")?;

        // Try [app] section first
        if let Some(app_section) = toml_value.get("app") {
            let app_keybinds: AppKeybinds = app_section.clone().try_into()
                .context("Failed to parse [app] section from common keybinds")?;
            Ok(app_keybinds)
        } else if let Some(global_section) = toml_value.get("global") {
            // Backward compatibility
            tracing::warn!("Using deprecated [global] section in global keybinds.toml - please rename to [app]");
            let app_keybinds: AppKeybinds = global_section.clone().try_into()
                .context("Failed to parse [global] section from common keybinds")?;
            Ok(app_keybinds)
        } else {
            Ok(AppKeybinds::default())
        }
    }

    /// Load app keybinds, checking character file first, then global, then defaults
    /// For backward compatibility, also checks for deprecated [global] section
    pub fn load_app_keybinds(character: Option<&str>) -> Result<AppKeybinds> {
        // First, try character-specific keybinds
        let keybinds_path = Self::keybinds_path(character)?;

        if keybinds_path.exists() {
            let contents =
                fs::read_to_string(&keybinds_path).context("Failed to read keybinds.toml")?;

            let toml_value: toml::Value = toml::from_str(&contents)
                .context("Failed to parse keybinds.toml")?;

            // Check if character file has [app] or [global] section
            if let Some(app_section) = toml_value.get("app") {
                let app_keybinds: AppKeybinds = app_section.clone().try_into()
                    .context("Failed to parse [app] section")?;
                Self::validate_app_keybinds(&app_keybinds);
                return Ok(app_keybinds);
            } else if let Some(global_section) = toml_value.get("global") {
                tracing::warn!("Using deprecated [global] section in keybinds.toml - please rename to [app]");
                let app_keybinds: AppKeybinds = global_section.clone().try_into()
                    .context("Failed to parse [global] section")?;
                Self::validate_app_keybinds(&app_keybinds);
                return Ok(app_keybinds);
            }
            // Character file exists but has no [app] section - fall through to global
        }

        // Try global keybinds
        let app_keybinds = Self::load_common_app_keybinds()?;
        Self::validate_app_keybinds(&app_keybinds);
        Ok(app_keybinds)
    }

    /// Load common (global) menu keybinds from global/keybinds.toml [menu] section
    /// Returns: MenuKeybinds from global, or default if file doesn't exist
    fn load_common_menu_keybinds() -> Result<MenuKeybinds> {
        let path = Self::common_keybinds_path()?;

        if !path.exists() {
            return Ok(MenuKeybinds::default());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common keybinds: {:?}", path))?;

        let toml_value: toml::Value = toml::from_str(&contents)
            .context("Failed to parse common keybinds TOML")?;

        if let Some(menu_section) = toml_value.get("menu") {
            let menu_keybinds: MenuKeybinds = menu_section.clone().try_into()
                .context("Failed to parse [menu] section from common keybinds")?;
            Ok(menu_keybinds)
        } else {
            Ok(MenuKeybinds::default())
        }
    }

    /// Load menu keybinds, checking character file first, then global, then defaults
    pub fn load_menu_keybinds(character: Option<&str>) -> Result<MenuKeybinds> {
        tracing::debug!("load_menu_keybinds() called for character: {:?}", character);

        // First, try character-specific keybinds
        let keybinds_path = Self::keybinds_path(character)?;

        if keybinds_path.exists() {
            let contents =
                fs::read_to_string(&keybinds_path).context("Failed to read keybinds.toml")?;

            let toml_value: toml::Value = toml::from_str(&contents)
                .context("Failed to parse keybinds.toml")?;

            // Check if character file has [menu] section
            if let Some(menu_section) = toml_value.get("menu") {
                tracing::debug!("Found [menu] section in character keybinds");
                let menu_keybinds: MenuKeybinds = menu_section.clone().try_into()
                    .context("Failed to parse [menu] section")?;
                return Ok(menu_keybinds);
            }
            // Character file exists but has no [menu] section - fall through to global
        }

        // Try global keybinds
        Self::load_common_menu_keybinds()
    }
}

/// Get default keybindings (based on ProfanityFE defaults)
pub fn default_keybinds() -> HashMap<String, KeyBindAction> {
    let mut map = HashMap::new();

    // Basic command input
    map.insert(
        "enter".to_string(),
        KeyBindAction::Action("send_command".to_string()),
    );
    map.insert(
        "left".to_string(),
        KeyBindAction::Action("cursor_left".to_string()),
    );
    map.insert(
        "right".to_string(),
        KeyBindAction::Action("cursor_right".to_string()),
    );
    map.insert(
        "ctrl+left".to_string(),
        KeyBindAction::Action("cursor_word_left".to_string()),
    );
    map.insert(
        "ctrl+right".to_string(),
        KeyBindAction::Action("cursor_word_right".to_string()),
    );
    map.insert(
        "home".to_string(),
        KeyBindAction::Action("cursor_home".to_string()),
    );
    map.insert(
        "end".to_string(),
        KeyBindAction::Action("cursor_end".to_string()),
    );
    map.insert(
        "backspace".to_string(),
        KeyBindAction::Action("cursor_backspace".to_string()),
    );
    map.insert(
        "delete".to_string(),
        KeyBindAction::Action("cursor_delete".to_string()),
    );

    // Window management
    map.insert(
        "tab".to_string(),
        KeyBindAction::Action("switch_current_window".to_string()),
    );
    map.insert(
        "alt+page_up".to_string(),
        KeyBindAction::Action("scroll_current_window_up_one".to_string()),
    );
    map.insert(
        "alt+page_down".to_string(),
        KeyBindAction::Action("scroll_current_window_down_one".to_string()),
    );
    map.insert(
        "page_up".to_string(),
        KeyBindAction::Action("scroll_current_window_up_page".to_string()),
    );
    map.insert(
        "page_down".to_string(),
        KeyBindAction::Action("scroll_current_window_down_page".to_string()),
    );

    // Command history
    map.insert(
        "up".to_string(),
        KeyBindAction::Action("previous_command".to_string()),
    );
    map.insert(
        "down".to_string(),
        KeyBindAction::Action("next_command".to_string()),
    );

    // Search
    map.insert(
        "ctrl+f".to_string(),
        KeyBindAction::Action("start_search".to_string()),
    );
    map.insert(
        "ctrl+page_up".to_string(),
        KeyBindAction::Action("prev_search_match".to_string()),
    );
    map.insert(
        "ctrl+page_down".to_string(),
        KeyBindAction::Action("next_search_match".to_string()),
    );

    // Debug/Performance
    map.insert(
        "f12".to_string(),
        KeyBindAction::Action("toggle_performance_stats".to_string()),
    );

    // Numpad movement macros
    map.insert(
        "num_1".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "sw\r".to_string(),
        }),
    );
    map.insert(
        "num_2".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "s\r".to_string(),
        }),
    );
    map.insert(
        "num_3".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "se\r".to_string(),
        }),
    );
    map.insert(
        "num_4".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "w\r".to_string(),
        }),
    );
    map.insert(
        "num_5".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "out\r".to_string(),
        }),
    );
    map.insert(
        "num_6".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "e\r".to_string(),
        }),
    );
    map.insert(
        "num_7".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "nw\r".to_string(),
        }),
    );
    map.insert(
        "num_8".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "n\r".to_string(),
        }),
    );
    map.insert(
        "num_9".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "ne\r".to_string(),
        }),
    );
    map.insert(
        "num_0".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "down\r".to_string(),
        }),
    );
    map.insert(
        "num_.".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "up\r".to_string(),
        }),
    );
    map.insert(
        "num_+".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "look\r".to_string(),
        }),
    );
    map.insert(
        "num_-".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "info\r".to_string(),
        }),
    );
    map.insert(
        "num_*".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "exp\r".to_string(),
        }),
    );
    map.insert(
        "num_/".to_string(),
        KeyBindAction::Macro(MacroAction {
            macro_text: "health\r".to_string(),
        }),
    );

    // Note: Shift+numpad doesn't work on Windows - the OS doesn't report SHIFT modifier for numpad numeric keys
    // If you want peer keybinds, use alt+numpad or ctrl+numpad instead (those modifiers work with numpad)

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // parse_key_string - basic keys
    // ===========================================

    #[test]
    fn test_parse_key_string_single_char() {
        let result = parse_key_string("a");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Char('a'));
        assert!(mods.is_empty());
    }

    #[test]
    fn test_parse_key_string_uppercase_normalized() {
        let result = parse_key_string("A");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        // Normalized to lowercase
        assert_eq!(key, KeyCode::Char('a'));
    }

    #[test]
    fn test_parse_key_string_enter() {
        let result = parse_key_string("enter");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Enter);
        assert!(mods.is_empty());
    }

    #[test]
    fn test_parse_key_string_backspace() {
        let result = parse_key_string("backspace");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        assert_eq!(key, KeyCode::Backspace);
    }

    #[test]
    fn test_parse_key_string_delete() {
        let result = parse_key_string("delete");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        assert_eq!(key, KeyCode::Delete);
    }

    #[test]
    fn test_parse_key_string_tab() {
        let result = parse_key_string("tab");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        assert_eq!(key, KeyCode::Tab);
    }

    #[test]
    fn test_parse_key_string_escape() {
        assert!(parse_key_string("esc").is_some());
        assert!(parse_key_string("escape").is_some());
        let (key, _) = parse_key_string("esc").unwrap();
        assert_eq!(key, KeyCode::Esc);
    }

    #[test]
    fn test_parse_key_string_space() {
        let result = parse_key_string("space");
        assert!(result.is_some());
        let (key, _) = result.unwrap();
        assert_eq!(key, KeyCode::Char(' '));
    }

    // ===========================================
    // parse_key_string - arrow keys
    // ===========================================

    #[test]
    fn test_parse_key_string_arrows() {
        assert_eq!(parse_key_string("left").unwrap().0, KeyCode::Left);
        assert_eq!(parse_key_string("right").unwrap().0, KeyCode::Right);
        assert_eq!(parse_key_string("up").unwrap().0, KeyCode::Up);
        assert_eq!(parse_key_string("down").unwrap().0, KeyCode::Down);
    }

    #[test]
    fn test_parse_key_string_navigation() {
        assert_eq!(parse_key_string("home").unwrap().0, KeyCode::Home);
        assert_eq!(parse_key_string("end").unwrap().0, KeyCode::End);
    }

    #[test]
    fn test_parse_key_string_page_keys() {
        assert_eq!(parse_key_string("page_up").unwrap().0, KeyCode::PageUp);
        assert_eq!(parse_key_string("pageup").unwrap().0, KeyCode::PageUp);
        assert_eq!(parse_key_string("page_down").unwrap().0, KeyCode::PageDown);
        assert_eq!(parse_key_string("pagedown").unwrap().0, KeyCode::PageDown);
    }

    // ===========================================
    // parse_key_string - function keys
    // ===========================================

    #[test]
    fn test_parse_key_string_function_keys() {
        for i in 1..=12 {
            let key_str = format!("f{}", i);
            let result = parse_key_string(&key_str);
            assert!(result.is_some(), "F{} should parse", i);
            let (key, _) = result.unwrap();
            assert_eq!(key, KeyCode::F(i as u8));
        }
    }

    // ===========================================
    // parse_key_string - numpad keys
    // ===========================================

    #[test]
    fn test_parse_key_string_numpad_digits() {
        assert_eq!(parse_key_string("num_0").unwrap().0, KeyCode::Keypad0);
        assert_eq!(parse_key_string("num_1").unwrap().0, KeyCode::Keypad1);
        assert_eq!(parse_key_string("num_5").unwrap().0, KeyCode::Keypad5);
        assert_eq!(parse_key_string("num_9").unwrap().0, KeyCode::Keypad9);
    }

    #[test]
    fn test_parse_key_string_numpad_operators() {
        assert_eq!(parse_key_string("num_+").unwrap().0, KeyCode::KeypadPlus);
        assert_eq!(parse_key_string("num_-").unwrap().0, KeyCode::KeypadMinus);
        assert_eq!(parse_key_string("num_*").unwrap().0, KeyCode::KeypadMultiply);
        assert_eq!(parse_key_string("num_/").unwrap().0, KeyCode::KeypadDivide);
        assert_eq!(parse_key_string("num_.").unwrap().0, KeyCode::KeypadPeriod);
    }

    // ===========================================
    // parse_key_string - modifiers
    // ===========================================

    #[test]
    fn test_parse_key_string_ctrl_modifier() {
        let result = parse_key_string("ctrl+a");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Char('a'));
        assert!(mods.ctrl);
        assert!(!mods.shift);
        assert!(!mods.alt);
    }

    #[test]
    fn test_parse_key_string_alt_modifier() {
        let result = parse_key_string("alt+x");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Char('x'));
        assert!(mods.alt);
        assert!(!mods.ctrl);
    }

    #[test]
    fn test_parse_key_string_shift_modifier() {
        let result = parse_key_string("shift+tab");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Tab);
        assert!(mods.shift);
    }

    #[test]
    fn test_parse_key_string_control_alias() {
        let result = parse_key_string("control+c");
        assert!(result.is_some());
        let (_, mods) = result.unwrap();
        assert!(mods.ctrl);
    }

    #[test]
    fn test_parse_key_string_multiple_modifiers() {
        let result = parse_key_string("ctrl+shift+a");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::Char('a'));
        assert!(mods.ctrl);
        assert!(mods.shift);
        assert!(!mods.alt);
    }

    #[test]
    fn test_parse_key_string_all_modifiers() {
        let result = parse_key_string("ctrl+alt+shift+f5");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::F(5));
        assert!(mods.ctrl);
        assert!(mods.alt);
        assert!(mods.shift);
    }

    #[test]
    fn test_parse_key_string_modifier_with_special_key() {
        let result = parse_key_string("ctrl+page_up");
        assert!(result.is_some());
        let (key, mods) = result.unwrap();
        assert_eq!(key, KeyCode::PageUp);
        assert!(mods.ctrl);
    }

    // ===========================================
    // parse_key_string - case insensitivity
    // ===========================================

    #[test]
    fn test_parse_key_string_case_insensitive() {
        assert!(parse_key_string("CTRL+A").is_some());
        assert!(parse_key_string("Ctrl+A").is_some());
        assert!(parse_key_string("ENTER").is_some());
        assert!(parse_key_string("Enter").is_some());
    }

    // ===========================================
    // parse_key_string - invalid inputs
    // ===========================================

    #[test]
    fn test_parse_key_string_invalid() {
        assert!(parse_key_string("invalid_key").is_none());
        assert!(parse_key_string("").is_none());
        assert!(parse_key_string("ctrl+").is_none());
    }

    #[test]
    fn test_parse_key_string_invalid_modifier() {
        assert!(parse_key_string("meta+a").is_none());
        assert!(parse_key_string("super+a").is_none());
    }

    // ===========================================
    // KeyAction::from_str tests
    // ===========================================

    #[test]
    fn test_key_action_from_str_command_input() {
        assert_eq!(KeyAction::from_str("send_command"), Some(KeyAction::SendCommand));
        assert_eq!(KeyAction::from_str("cursor_left"), Some(KeyAction::CursorLeft));
        assert_eq!(KeyAction::from_str("cursor_right"), Some(KeyAction::CursorRight));
        assert_eq!(KeyAction::from_str("cursor_home"), Some(KeyAction::CursorHome));
        assert_eq!(KeyAction::from_str("cursor_end"), Some(KeyAction::CursorEnd));
        assert_eq!(KeyAction::from_str("cursor_backspace"), Some(KeyAction::CursorBackspace));
        assert_eq!(KeyAction::from_str("cursor_delete"), Some(KeyAction::CursorDelete));
    }

    #[test]
    fn test_key_action_from_str_word_movement() {
        assert_eq!(KeyAction::from_str("cursor_word_left"), Some(KeyAction::CursorWordLeft));
        assert_eq!(KeyAction::from_str("cursor_word_right"), Some(KeyAction::CursorWordRight));
        assert_eq!(KeyAction::from_str("cursor_delete_word"), Some(KeyAction::CursorDeleteWord));
        assert_eq!(KeyAction::from_str("cursor_clear_line"), Some(KeyAction::CursorClearLine));
    }

    #[test]
    fn test_key_action_from_str_history() {
        assert_eq!(KeyAction::from_str("previous_command"), Some(KeyAction::PreviousCommand));
        assert_eq!(KeyAction::from_str("next_command"), Some(KeyAction::NextCommand));
        assert_eq!(KeyAction::from_str("send_last_command"), Some(KeyAction::SendLastCommand));
        assert_eq!(KeyAction::from_str("send_second_last_command"), Some(KeyAction::SendSecondLastCommand));
    }

    #[test]
    fn test_key_action_from_str_window() {
        assert_eq!(KeyAction::from_str("switch_current_window"), Some(KeyAction::SwitchCurrentWindow));
        assert_eq!(KeyAction::from_str("scroll_current_window_up_one"), Some(KeyAction::ScrollCurrentWindowUpOne));
        assert_eq!(KeyAction::from_str("scroll_current_window_down_one"), Some(KeyAction::ScrollCurrentWindowDownOne));
        assert_eq!(KeyAction::from_str("scroll_current_window_up_page"), Some(KeyAction::ScrollCurrentWindowUpPage));
        assert_eq!(KeyAction::from_str("scroll_current_window_down_page"), Some(KeyAction::ScrollCurrentWindowDownPage));
        assert_eq!(KeyAction::from_str("scroll_current_window_home"), Some(KeyAction::ScrollCurrentWindowHome));
        assert_eq!(KeyAction::from_str("scroll_current_window_end"), Some(KeyAction::ScrollCurrentWindowEnd));
    }

    #[test]
    fn test_key_action_from_str_search() {
        assert_eq!(KeyAction::from_str("start_search"), Some(KeyAction::StartSearch));
        assert_eq!(KeyAction::from_str("next_search_match"), Some(KeyAction::NextSearchMatch));
        assert_eq!(KeyAction::from_str("prev_search_match"), Some(KeyAction::PrevSearchMatch));
        assert_eq!(KeyAction::from_str("clear_search"), Some(KeyAction::ClearSearch));
    }

    #[test]
    fn test_key_action_from_str_tabs() {
        assert_eq!(KeyAction::from_str("next_tab"), Some(KeyAction::NextTab));
        assert_eq!(KeyAction::from_str("prev_tab"), Some(KeyAction::PrevTab));
        assert_eq!(KeyAction::from_str("next_unread_tab"), Some(KeyAction::NextUnreadTab));
    }

    #[test]
    fn test_key_action_from_str_clipboard() {
        assert_eq!(KeyAction::from_str("copy"), Some(KeyAction::Copy));
        assert_eq!(KeyAction::from_str("paste"), Some(KeyAction::Paste));
        assert_eq!(KeyAction::from_str("select_all"), Some(KeyAction::SelectAll));
    }

    #[test]
    fn test_key_action_from_str_toggles() {
        assert_eq!(KeyAction::from_str("toggle_performance_stats"), Some(KeyAction::TogglePerformanceStats));
        assert_eq!(KeyAction::from_str("toggle_sounds"), Some(KeyAction::ToggleSounds));
    }

    #[test]
    fn test_key_action_from_str_tts() {
        assert_eq!(KeyAction::from_str("tts_next"), Some(KeyAction::TtsNext));
        assert_eq!(KeyAction::from_str("tts_previous"), Some(KeyAction::TtsPrevious));
        assert_eq!(KeyAction::from_str("tts_next_unread"), Some(KeyAction::TtsNextUnread));
        assert_eq!(KeyAction::from_str("tts_stop"), Some(KeyAction::TtsStop));
        assert_eq!(KeyAction::from_str("tts_mute_toggle"), Some(KeyAction::TtsMuteToggle));
        assert_eq!(KeyAction::from_str("tts_increase_rate"), Some(KeyAction::TtsIncreaseRate));
        assert_eq!(KeyAction::from_str("tts_decrease_rate"), Some(KeyAction::TtsDecreaseRate));
        assert_eq!(KeyAction::from_str("tts_increase_volume"), Some(KeyAction::TtsIncreaseVolume));
        assert_eq!(KeyAction::from_str("tts_decrease_volume"), Some(KeyAction::TtsDecreaseVolume));
    }

    #[test]
    fn test_key_action_from_str_legacy() {
        // Legacy alias
        assert_eq!(KeyAction::from_str("tts_pause_resume"), Some(KeyAction::TtsStop));
    }

    #[test]
    fn test_key_action_from_str_invalid() {
        assert_eq!(KeyAction::from_str("invalid_action"), None);
        assert_eq!(KeyAction::from_str(""), None);
        assert_eq!(KeyAction::from_str("SEND_COMMAND"), None); // Case sensitive
    }

    // ===========================================
    // AppKeybinds tests
    // ===========================================

    #[test]
    fn test_app_keybinds_default() {
        let keybinds = AppKeybinds::default();
        assert_eq!(keybinds.quit, "ctrl+c");
        assert_eq!(keybinds.start_search, "ctrl+f");
        assert_eq!(keybinds.next_search_match, "ctrl+pagedown");
        assert_eq!(keybinds.prev_search_match, "ctrl+pageup");
        assert_eq!(keybinds.close_window, "esc");
    }

    #[test]
    fn test_app_keybinds_clone() {
        let keybinds = AppKeybinds::default();
        let cloned = keybinds.clone();
        assert_eq!(cloned.quit, keybinds.quit);
        assert_eq!(cloned.start_search, keybinds.start_search);
    }

    // ===========================================
    // MenuKeybinds tests
    // ===========================================

    #[test]
    fn test_menu_keybinds_default() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.navigate_up, "Up");
        assert_eq!(keybinds.navigate_down, "Down");
        assert_eq!(keybinds.navigate_left, "Left");
        assert_eq!(keybinds.navigate_right, "Right");
        assert_eq!(keybinds.page_up, "PageUp");
        assert_eq!(keybinds.page_down, "PageDown");
        assert_eq!(keybinds.home, "Home");
        assert_eq!(keybinds.end, "End");
    }

    #[test]
    fn test_menu_keybinds_field_navigation() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.next_field, "Tab");
        assert_eq!(keybinds.previous_field, "Shift+Tab");
    }

    #[test]
    fn test_menu_keybinds_actions() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.select, "Enter");
        assert_eq!(keybinds.cancel, "Esc");
        assert_eq!(keybinds.save, "Ctrl+s");
        assert_eq!(keybinds.delete, "Delete");
    }

    #[test]
    fn test_menu_keybinds_clipboard() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.select_all, "Ctrl+A");
        assert_eq!(keybinds.copy, "Ctrl+C");
        assert_eq!(keybinds.cut, "Ctrl+X");
        assert_eq!(keybinds.paste, "Ctrl+V");
    }

    #[test]
    fn test_menu_keybinds_toggles() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.toggle, "Space");
        assert_eq!(keybinds.toggle_filter, "F");
    }

    #[test]
    fn test_menu_keybinds_reordering() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.move_up, "Shift+Up");
        assert_eq!(keybinds.move_down, "Shift+Down");
    }

    #[test]
    fn test_menu_keybinds_list_management() {
        let keybinds = MenuKeybinds::default();
        assert_eq!(keybinds.add, "A");
        assert_eq!(keybinds.edit, "E");
    }

    // ===========================================
    // default_keybinds tests
    // ===========================================

    #[test]
    fn test_default_keybinds_basic() {
        let keybinds = default_keybinds();
        assert!(keybinds.contains_key("enter"));
        assert!(keybinds.contains_key("left"));
        assert!(keybinds.contains_key("right"));
        assert!(keybinds.contains_key("backspace"));
    }

    #[test]
    fn test_default_keybinds_history() {
        let keybinds = default_keybinds();
        assert!(keybinds.contains_key("up"));
        assert!(keybinds.contains_key("down"));
    }

    #[test]
    fn test_default_keybinds_numpad() {
        let keybinds = default_keybinds();
        for i in 0..=9 {
            let key = format!("num_{}", i);
            assert!(keybinds.contains_key(&key), "Missing numpad key: {}", key);
        }
        assert!(keybinds.contains_key("num_+"));
        assert!(keybinds.contains_key("num_-"));
        assert!(keybinds.contains_key("num_*"));
        assert!(keybinds.contains_key("num_/"));
        assert!(keybinds.contains_key("num_."));
    }

    #[test]
    fn test_default_keybinds_numpad_movement() {
        let keybinds = default_keybinds();

        // Check numpad movement macros
        if let Some(KeyBindAction::Macro(m)) = keybinds.get("num_8") {
            assert_eq!(m.macro_text, "n\r"); // North
        } else {
            panic!("num_8 should be a Macro action");
        }

        if let Some(KeyBindAction::Macro(m)) = keybinds.get("num_2") {
            assert_eq!(m.macro_text, "s\r"); // South
        }
    }

    #[test]
    fn test_default_keybinds_search() {
        let keybinds = default_keybinds();
        assert!(keybinds.contains_key("ctrl+f"));
        assert!(keybinds.contains_key("ctrl+page_up"));
        assert!(keybinds.contains_key("ctrl+page_down"));
    }

    // ===========================================
    // KeyBindAction tests
    // ===========================================

    #[test]
    fn test_key_bind_action_action() {
        let action = KeyBindAction::Action("send_command".to_string());
        match action {
            KeyBindAction::Action(s) => assert_eq!(s, "send_command"),
            _ => panic!("Expected Action variant"),
        }
    }

    #[test]
    fn test_key_bind_action_macro() {
        let action = KeyBindAction::Macro(MacroAction {
            macro_text: "look\r".to_string(),
        });
        match action {
            KeyBindAction::Macro(m) => assert_eq!(m.macro_text, "look\r"),
            _ => panic!("Expected Macro variant"),
        }
    }

    #[test]
    fn test_macro_action_clone() {
        let macro_action = MacroAction {
            macro_text: "test\r".to_string(),
        };
        let cloned = macro_action.clone();
        assert_eq!(cloned.macro_text, macro_action.macro_text);
    }

    // ===========================================
    // KeyAction equality tests
    // ===========================================

    #[test]
    fn test_key_action_equality() {
        assert_eq!(KeyAction::SendCommand, KeyAction::SendCommand);
        assert_ne!(KeyAction::SendCommand, KeyAction::CursorLeft);
        assert_ne!(KeyAction::Copy, KeyAction::Paste);
    }

    #[test]
    fn test_key_action_send_macro_equality() {
        let macro1 = KeyAction::SendMacro("test".to_string());
        let macro2 = KeyAction::SendMacro("test".to_string());
        let macro3 = KeyAction::SendMacro("other".to_string());
        assert_eq!(macro1, macro2);
        assert_ne!(macro1, macro3);
    }

    #[test]
    fn test_key_action_clone() {
        let action = KeyAction::ScrollCurrentWindowUpPage;
        let cloned = action.clone();
        assert_eq!(action, cloned);
    }
}
