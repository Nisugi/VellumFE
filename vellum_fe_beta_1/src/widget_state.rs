use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use ratatui::style::Color;

/// Convert ratatui Color to hex string
pub fn color_to_hex(color: &Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        _ => "#ffffff".to_string(), // Default to white for non-RGB colors
    }
}

/// Convert hex string to ratatui Color
pub fn hex_to_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some(Color::Rgb(r, g, b))
}

/// Styled text segment for storing in state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyledSegment {
    pub text: String,
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
}

/// A single line of styled text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyledLine {
    pub segments: Vec<StyledSegment>,
}

/// Progress bar state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressState {
    pub current: i32,
    pub max: i32,
    pub percentage: u8,
    pub text: Option<String>,
}

/// Countdown timer state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountdownState {
    pub end_time: Option<i64>,
    pub label: String,
}

/// Hand state (left or right hand)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandState {
    pub text: String,
    pub exist: String,
    pub noun: String,
}

/// Indicator states
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndicatorStates {
    pub kneeling: bool,
    pub prone: bool,
    pub sitting: bool,
    pub standing: bool,
    pub stunned: bool,
    pub hidden: bool,
    pub invisible: bool,
    pub dead: bool,
    pub webbed: bool,
    pub joined: bool,
}

/// Compass state (exit directions)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompassState {
    pub north: bool,
    pub northeast: bool,
    pub east: bool,
    pub southeast: bool,
    pub south: bool,
    pub southwest: bool,
    pub west: bool,
    pub northwest: bool,
    pub up: bool,
    pub down: bool,
    pub out: bool,
}

/// Room components state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoomState {
    pub room_name: Option<String>,
    pub components: HashMap<String, Vec<StyledLine>>,
}

/// Complete widget state for the entire application
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WidgetState {
    /// Timestamp of when state was saved
    pub timestamp: Option<String>,

    /// Text window contents (last N lines)
    /// Key: window name, Value: vector of styled lines
    pub text_windows: HashMap<String, Vec<StyledLine>>,

    /// Progress bar states
    /// Key: window name, Value: progress state
    pub progress_bars: HashMap<String, ProgressState>,

    /// Countdown timer states
    /// Key: window name, Value: countdown state
    pub countdowns: HashMap<String, CountdownState>,

    /// Hand states
    pub left_hand: Option<HandState>,
    pub right_hand: Option<HandState>,

    /// Indicator states
    pub indicators: IndicatorStates,

    /// Compass state
    pub compass: CompassState,

    /// Room state
    pub room: RoomState,

    /// Spell window content
    pub spells: Vec<StyledLine>,
}

impl WidgetState {
    /// Create a new empty widget state
    pub fn new() -> Self {
        Self::default()
    }

    /// Save widget state to disk
    pub fn save(&self, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let toml_string = toml::to_string_pretty(self)?;
        fs::write(path, toml_string)?;

        tracing::info!("Widget state saved to {:?}", path);
        Ok(())
    }

    /// Load widget state from disk
    pub fn load(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        if !path.exists() {
            tracing::debug!("No widget state file at {:?}, starting fresh", path);
            return Ok(Self::new());
        }

        let content = fs::read_to_string(path)?;
        let state: WidgetState = toml::from_str(&content)?;

        tracing::info!("Widget state loaded from {:?}", path);
        Ok(state)
    }

    /// Add a styled line to a text window's state (with limit)
    pub fn add_text_window_line(&mut self, window_name: &str, line: StyledLine, max_lines: usize) {
        let lines = self.text_windows.entry(window_name.to_string()).or_insert_with(Vec::new);
        lines.push(line);

        // Keep only last N lines
        if lines.len() > max_lines {
            lines.drain(0..lines.len() - max_lines);
        }
    }

    /// Set progress bar state
    pub fn set_progress(&mut self, window_name: &str, current: i32, max: i32, percentage: u8, text: Option<String>) {
        self.progress_bars.insert(
            window_name.to_string(),
            ProgressState {
                current,
                max,
                percentage,
                text,
            },
        );
    }

    /// Set countdown state
    pub fn set_countdown(&mut self, window_name: &str, end_time: Option<i64>, label: String) {
        self.countdowns.insert(
            window_name.to_string(),
            CountdownState { end_time, label },
        );
    }

    /// Set hand state
    pub fn set_hand(&mut self, is_left: bool, text: String, exist: String, noun: String) {
        let hand_state = HandState { text, exist, noun };
        if is_left {
            self.left_hand = Some(hand_state);
        } else {
            self.right_hand = Some(hand_state);
        }
    }
}
