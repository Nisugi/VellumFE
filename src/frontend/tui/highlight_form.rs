//! Popup form for creating, editing, and validating highlight patterns.
//!
//! Mirrors the VellumFE workflow: regex pattern entry, optional colors/sounds,
//! and checkbox flags for rendering behavior.

use crate::config::{Config, HighlightPattern};
use crate::frontend::tui::crossterm_bridge;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Clear, Widget},
};
use regex::Regex;
use tui_textarea::TextArea;

// Keep popup geometry in one place so dragging + rendering stay in sync
const POPUP_WIDTH: u16 = 70;
const POPUP_HEIGHT: u16 = 32;

/// The nine screen anchors for the alert dropdown, in reading order with
/// their serde/display labels.
const ALERT_ANCHOR_OPTIONS: [(crate::config::AlertAnchor, &str); 9] = {
    use crate::config::AlertAnchor::*;
    [
        (TopLeft, "top-left"),
        (TopCenter, "top-center"),
        (TopRight, "top-right"),
        (CenterLeft, "center-left"),
        (Center, "center"),
        (CenterRight, "center-right"),
        (BottomLeft, "bottom-left"),
        (BottomCenter, "bottom-center"),
        (BottomRight, "bottom-right"),
    ]
};

/// Actions that can result from mouse interaction with the highlight form
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HighlightFormMouseAction {
    /// No special action, just drag or navigation
    None,
    /// User clicked Save button
    Save,
    /// User clicked Cancel button
    Cancel,
}

/// Form mode - Create new or Edit existing
#[derive(Debug, Clone, PartialEq)]
pub enum FormMode {
    Create,
    Edit(String), // Contains original highlight name
}

/// Result of form submission
#[derive(Debug, Clone)]
pub enum FormResult {
    Save {
        name: String,
        pattern: HighlightPattern,
        is_global: bool, // true = save to global/, false = save to character profile
    },
    Delete {
        name: String,
        is_global: bool, // true = delete from global/, false = delete from character profile
    },
    Cancel,
}

/// Highlight management form widget
pub struct HighlightFormWidget {
    // Text input fields (using tui-textarea)
    name: TextArea<'static>,
    pattern: TextArea<'static>,
    category: TextArea<'static>,
    fg_color: TextArea<'static>,
    bg_color: TextArea<'static>,
    sound: TextArea<'static>,
    sound_volume: TextArea<'static>,
    redirect_to: TextArea<'static>, // Stream name for redirect
    replace: TextArea<'static>,     // Replacement text for matched content

    // Checkbox states
    bold: bool,
    color_entire_line: bool,
    fast_parse: bool,
    /// Match without regard to case (both literal and regex rules).
    case_insensitive: bool,
    squelch: bool,
    silent_prompt: bool,

    // Form state
    focused_field: usize, // 0-14: text fields + checkboxes + dropdown
    status_message: String,
    pattern_error: Option<String>,
    mode: FormMode,

    // Sound dropdown
    sound_files: Vec<String>, // Available sound files (index 0 = "none", then actual files)
    sound_file_index: usize,  // Selected index in sound_files

    // Redirect dropdown (Off=0, Copy=1, Redirect-only=2)
    redirect_mode_index: usize,

    // Filter fields: restrict the highlight to one stream and/or window
    stream_filter: TextArea<'static>,
    window_filter: TextArea<'static>,

    // Custom-status actions: flip a named indicator status on match
    // (optionally auto-clearing after a duration) or turn one off.
    set_status: TextArea<'static>,
    status_duration: TextArea<'static>,
    clear_status: TextArea<'static>,

    /// The alert subfields this form does NOT edit (art, id, priority,
    /// offset, condition gate, re-arm, timer, cancels — the GUI's territory),
    /// preserved so saving here never wipes them. The fields below EDIT the
    /// rest; on save they merge over this remainder.
    alert: Option<crate::config::AlertSpec>,
    // Editable alert fields (indices 23-29): what the TUI's own renderer can
    // show — banner + colors, flash, anchor, plus duration/cooldown.
    alert_banner: TextArea<'static>,
    alert_banner_fg: TextArea<'static>,
    alert_banner_bg: TextArea<'static>,
    alert_flash: TextArea<'static>,
    /// Index into ALERT_ANCHOR_OPTIONS (dropdown, field 27).
    alert_anchor_index: usize,
    alert_duration: TextArea<'static>,
    alert_cooldown: TextArea<'static>,

    // Scope (Global vs Character)
    is_global: bool, // true = save to global/, false = save to character profile

    // Rumble pattern: a picklist like sound/redirect. `rumble_options` is the
    // selectable set ("(none)" at index 0, then RumbleConfig::pattern_names),
    // populated via set_rumble_options from the live controller config so the
    // TUI offers the same patterns as the GUI. `rumble` mirrors the selected
    // option and is what saves. Field focus index 19 (appended last so no
    // existing index shifts).
    rumble: Option<String>,
    rumble_options: Vec<String>,
    rumble_index: usize,

    // Popup position (for dragging)
    pub popup_x: u16,
    pub popup_y: u16,
    pub is_dragging: bool,
    pub drag_offset_x: u16,
    pub drag_offset_y: u16,
}

impl HighlightFormWidget {
    /// Scan ~/.vellum-fe/sounds/ for available sound files
    /// Returns: ["none", "file1.wav", "file2.wav", ...]
    fn load_sound_files() -> Vec<String> {
        let mut files = vec!["none".to_string()];

        if let Ok(sounds_dir) = Config::sounds_dir() {
            if let Ok(entries) = std::fs::read_dir(&sounds_dir) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() {
                            if let Some(name) = entry.file_name().to_str() {
                                // Skip README and other non-audio files
                                if !name.eq_ignore_ascii_case("README.md")
                                    && !name.eq_ignore_ascii_case(".gitkeep")
                                {
                                    files.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort the actual files (skip index 0 which is "none")
        if files.len() > 1 {
            files[1..].sort();
        }
        files
    }

    /// Create a new highlight form (Create mode)
    pub fn new() -> Self {
        let mut name = TextArea::default();
        name.set_cursor_line_style(Style::default());
        name.set_placeholder_text("e.g., swing_highlight");

        let mut pattern = TextArea::default();
        pattern.set_cursor_line_style(Style::default());
        pattern.set_placeholder_text("e.g., You swing.*");

        let mut category = TextArea::default();
        category.set_cursor_line_style(Style::default());
        category.set_placeholder_text("e.g., Combat, Loot, Spells");

        let mut fg_color = TextArea::default();
        fg_color.set_cursor_line_style(Style::default());
        fg_color.set_placeholder_text("#ff0000");

        let mut bg_color = TextArea::default();
        bg_color.set_cursor_line_style(Style::default());
        bg_color.set_placeholder_text("(optional)");

        let mut sound = TextArea::default();
        sound.set_cursor_line_style(Style::default());
        sound.set_placeholder_text("sword_swing.wav");

        let mut sound_volume = TextArea::default();
        sound_volume.set_cursor_line_style(Style::default());
        sound_volume.set_placeholder_text("0.0-1.0 (e.g., 0.8)");

        let mut redirect_to = TextArea::default();
        redirect_to.set_cursor_line_style(Style::default());
        redirect_to.set_placeholder_text("stream name (e.g., combat, speech)");

        let mut replace = TextArea::default();
        replace.set_cursor_line_style(Style::default());
        replace.set_placeholder_text("replacement text");

        let mut stream_filter = TextArea::default();
        stream_filter.set_cursor_line_style(Style::default());
        stream_filter.set_placeholder_text("only this stream (optional)");

        let mut window_filter = TextArea::default();
        window_filter.set_cursor_line_style(Style::default());
        window_filter.set_placeholder_text("only this window (optional)");

        let mut set_status = TextArea::default();
        set_status.set_cursor_line_style(Style::default());
        set_status.set_placeholder_text("status id to turn on (optional)");

        let mut status_duration = TextArea::default();
        status_duration.set_cursor_line_style(Style::default());
        status_duration.set_placeholder_text("seconds until auto-clear (optional)");

        let mut clear_status = TextArea::default();
        clear_status.set_cursor_line_style(Style::default());
        clear_status.set_placeholder_text("status id to turn off (optional)");

        // Alert fields. One tiny helper keeps nine constructions readable.
        let text_field = |placeholder: &str| {
            let mut area = TextArea::default();
            area.set_cursor_line_style(Style::default());
            area.set_placeholder_text(placeholder);
            area
        };
        let alert_banner = text_field("overlay text (optional)");
        let alert_banner_fg = text_field("color");
        let alert_banner_bg = text_field("color");
        let alert_flash = text_field("edge flash color (optional)");
        let alert_duration = text_field("secs");
        let alert_cooldown = text_field("secs");

        Self {
            name,
            pattern,
            category,
            fg_color,
            bg_color,
            sound,
            sound_volume,
            redirect_to,
            replace,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            case_insensitive: false,
            squelch: false,
            silent_prompt: false,
            focused_field: 0,
            status_message: "Ready".to_string(),
            pattern_error: None,
            mode: FormMode::Create,
            sound_files: Self::load_sound_files(),
            sound_file_index: 0,    // Default to "none"
            redirect_mode_index: 0, // Default to "Off"
            stream_filter,
            window_filter,
            set_status,
            status_duration,
            clear_status,
            alert: None,
            alert_banner,
            alert_banner_fg,
            alert_banner_bg,
            alert_flash,
            alert_anchor_index: 4, // Center, the schema default
            alert_duration,
            alert_cooldown,
            is_global: true, // Default to global scope
            rumble: None,
            rumble_options: vec!["(none)".to_string()],
            rumble_index: 0,
            popup_x: 0,
            popup_y: 0,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    /// Create form in Edit mode with existing highlight
    pub fn new_edit(name: String, pattern: &HighlightPattern) -> Self {
        let mut form = Self::new();
        form.mode = FormMode::Edit(name.clone());

        // Load existing values
        form.name = TextArea::from([name.clone()]);
        form.name.set_cursor_line_style(Style::default());

        form.pattern = TextArea::from([pattern.pattern.clone()]);
        form.pattern.set_cursor_line_style(Style::default());

        if let Some(ref cat) = pattern.category {
            form.category = TextArea::from([cat.clone()]);
            form.category.set_cursor_line_style(Style::default());
        }

        if let Some(ref fg) = pattern.fg {
            form.fg_color = TextArea::from([fg.clone()]);
            form.fg_color.set_cursor_line_style(Style::default());
        }

        if let Some(ref bg) = pattern.bg {
            form.bg_color = TextArea::from([bg.clone()]);
            form.bg_color.set_cursor_line_style(Style::default());
        }

        if let Some(ref sound_file) = pattern.sound {
            form.sound = TextArea::from([sound_file.clone()]);
            form.sound.set_cursor_line_style(Style::default());

            // Find the index of this sound file in the dropdown
            if let Some(idx) = form.sound_files.iter().position(|s| s == sound_file) {
                form.sound_file_index = idx;
            }
        }

        if let Some(volume) = pattern.sound_volume {
            form.sound_volume = TextArea::from([volume.to_string()]);
            form.sound_volume.set_cursor_line_style(Style::default());
        }

        form.rumble = pattern.rumble.clone();

        if let Some(ref replace) = pattern.replace {
            form.replace = TextArea::from([replace.clone()]);
            form.replace.set_cursor_line_style(Style::default());
        }

        form.bold = pattern.bold;
        form.color_entire_line = pattern.color_entire_line;
        form.fast_parse = pattern.fast_parse;
        form.case_insensitive = pattern.case_insensitive;
        form.squelch = pattern.squelch;
        form.silent_prompt = pattern.silent_prompt;

        // Load redirect settings
        if let Some(ref redirect_stream) = pattern.redirect_to {
            form.redirect_to = TextArea::from([redirect_stream.clone()]);
            form.redirect_to.set_cursor_line_style(Style::default());
        }

        // Set redirect mode index (0=Off, 1=Copy, 2=Redirect-only),
        // matching the dropdown display and the save-path mapping — the old
        // reversed mapping silently swapped Copy/Only on every edit round-trip
        form.redirect_mode_index = if pattern.redirect_to.is_none() {
            0 // Off
        } else {
            match pattern.redirect_mode {
                crate::config::RedirectMode::RedirectCopy => 1,
                crate::config::RedirectMode::RedirectOnly => 2,
            }
        };

        if let Some(ref stream) = pattern.stream {
            form.stream_filter = TextArea::from([stream.clone()]);
            form.stream_filter.set_cursor_line_style(Style::default());
        }
        if let Some(ref window) = pattern.window {
            form.window_filter = TextArea::from([window.clone()]);
            form.window_filter.set_cursor_line_style(Style::default());
        }
        if let Some(ref status) = pattern.set_status {
            form.set_status = TextArea::from([status.clone()]);
            form.set_status.set_cursor_line_style(Style::default());
        }
        if let Some(duration) = pattern.status_duration {
            form.status_duration = TextArea::from([duration.to_string()]);
            form.status_duration.set_cursor_line_style(Style::default());
        }
        if let Some(ref status) = pattern.clear_status {
            form.clear_status = TextArea::from([status.clone()]);
            form.clear_status.set_cursor_line_style(Style::default());
        }
        // Keep the whole spec as the merge base (see field doc), then load
        // the subfields this form edits.
        form.alert = pattern.alert.clone();
        if let Some(ref alert) = pattern.alert {
            let fill = |target: &mut TextArea<'static>, value: Option<&str>| {
                if let Some(value) = value {
                    *target = TextArea::from([value.to_string()]);
                    target.set_cursor_line_style(Style::default());
                }
            };
            fill(&mut form.alert_banner, alert.banner.as_deref());
            fill(&mut form.alert_banner_fg, alert.banner_fg.as_deref());
            fill(&mut form.alert_banner_bg, alert.banner_bg.as_deref());
            fill(&mut form.alert_flash, alert.flash.as_deref());
            fill(
                &mut form.alert_duration,
                alert.duration.map(|v| v.to_string()).as_deref(),
            );
            fill(
                &mut form.alert_cooldown,
                alert.cooldown.map(|v| v.to_string()).as_deref(),
            );
            form.alert_anchor_index = ALERT_ANCHOR_OPTIONS
                .iter()
                .position(|(anchor, _)| *anchor == alert.anchor)
                .unwrap_or(4);
        }

        form.status_message = "Editing highlight".to_string();
        form
    }

    /// Alias for new_edit - create form in Edit mode with existing highlight
    pub fn with_pattern(name: String, pattern: HighlightPattern) -> Self {
        Self::new_edit(name, &pattern)
    }

    /// Set the scope (global vs character) for this form
    pub fn set_scope(&mut self, is_global: bool) {
        self.is_global = is_global;
    }

    /// Get the current scope setting
    pub fn is_global(&self) -> bool {
        self.is_global
    }

    /// Provide the selectable rumble patterns (built-ins + user patterns from
    /// the controller config), with "(none)" prepended at index 0. Called
    /// right after construction from a context that has the live Config, so
    /// the TUI form offers the same set as the GUI. Re-syncs the dropdown
    /// index to the currently-loaded `rumble` value.
    pub fn set_rumble_options(&mut self, patterns: Vec<String>) {
        let mut options = vec!["(none)".to_string()];
        options.extend(patterns);
        self.rumble_options = options;
        // Point the index at the loaded rumble value if it's still offered;
        // if a rule references a pattern no longer defined, keep it selectable
        // by appending it so an edit doesn't silently drop it.
        self.rumble_index = match &self.rumble {
            None => 0,
            Some(name) => match self.rumble_options.iter().position(|o| o == name) {
                Some(idx) => idx,
                None => {
                    self.rumble_options.push(name.clone());
                    self.rumble_options.len() - 1
                }
            },
        };
    }

    /// Sync `rumble` from the current dropdown index (index 0 = "(none)").
    fn update_rumble_from_index(&mut self) {
        self.rumble = match self.rumble_options.get(self.rumble_index) {
            Some(opt) if opt != "(none)" => Some(opt.clone()),
            _ => None,
        };
    }

    /// Assemble the alert from the edited fields merged over the preserved
    /// remainder (condition gate, timer, cancels, art, id, priority, offset —
    /// the subfields this form does not host). Merging instead of replacing
    /// is what keeps a GUI-authored gate alive through a TUI edit.
    fn build_alert(&self) -> Option<crate::config::AlertSpec> {
        let text = |area: &TextArea<'static>| {
            let value = area.lines()[0].trim().to_string();
            (!value.is_empty()).then_some(value)
        };
        let mut spec = self.alert.clone().unwrap_or_default();
        spec.banner = text(&self.alert_banner);
        spec.banner_fg = text(&self.alert_banner_fg);
        spec.banner_bg = text(&self.alert_banner_bg);
        spec.flash = text(&self.alert_flash);
        spec.anchor = ALERT_ANCHOR_OPTIONS[self.alert_anchor_index.min(8)].0;
        spec.duration = text(&self.alert_duration)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| *v > 0.0);
        spec.cooldown = text(&self.alert_cooldown)
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| *v >= 0.0);

        // Nothing to show AND nothing preserved worth keeping: no alert at
        // all, same emptiness rule as the GUI form.
        let inert = spec.banner.is_none()
            && spec.art.is_none()
            && spec.flash.is_none()
            && spec.when.is_none()
            && spec.timer.is_none()
            && spec.cancels.is_empty();
        (!inert).then_some(spec)
    }

    /// Move focus to next field
    pub fn focus_next(&mut self) {
        // 0-30 (19 = rumble picklist, 20-22 = custom-status actions,
        // 23-29 = overlay alert; 27 = anchor picklist; 30 = ignore case).
        // New fields append at the end: the indices below are referenced by
        // the hand-placed cell renderer and the mouse row map, so inserting
        // in the middle would mean renumbering both.
        self.focused_field = (self.focused_field + 1) % 31;
    }

    /// Move focus to previous field
    pub fn focus_prev(&mut self) {
        self.focused_field = if self.focused_field == 0 {
            30
        } else {
            self.focused_field - 1
        };
    }

    /// Update sound field from current sound_file_index
    fn update_sound_from_index(&mut self) {
        if self.sound_files.is_empty() {
            return;
        }

        let selected = &self.sound_files[self.sound_file_index];
        if selected == "none" {
            // Clear the sound field
            self.sound = TextArea::default();
            self.sound.set_cursor_line_style(Style::default());
            self.sound.set_placeholder_text("sword_swing.wav");
        } else {
            // Set to selected file
            self.sound = TextArea::from([selected.clone()]);
            self.sound.set_cursor_line_style(Style::default());
        }
    }

    /// Handle key input for current focused field
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<FormResult> {
        // Note: Most keys are now routed via MenuAction in mod.rs:
        // - Tab/Shift+Tab → MenuAction::NextField/PreviousField
        // - Up/Down → MenuAction::NavigateUp/NavigateDown (for field navigation)
        // - Left/Right → MenuAction::CycleBackward/CycleForward (for dropdowns)
        // - Esc → MenuAction::Cancel
        // - Ctrl+A → MenuAction::SelectAll
        // - Ctrl+C/X/V → MenuAction::Copy/Cut/Paste
        // - Space/Enter → MenuAction::Toggle/Select (for checkboxes)
        // - Ctrl+S → MenuAction::Save
        // - Ctrl+D → MenuAction::Delete (handled via handle_action)

        match key.code {
            _ => {
                // Pass key to appropriate text field
                // Convert KeyEvent for tui-textarea compatibility
                let rt_key = crate::frontend::tui::textarea_bridge::to_textarea_event(key);

                let handled = match self.focused_field {
                    0 => self.name.input(rt_key),
                    1 => {
                        let result = self.pattern.input(rt_key);
                        self.validate_pattern();
                        result
                    }
                    2 => self.category.input(rt_key),
                    3 => self.fg_color.input(rt_key),
                    4 => self.bg_color.input(rt_key),
                    5 => self.sound.input(rt_key),
                    6 => self.sound_volume.input(rt_key),
                    7 => self.replace.input(rt_key),
                    8 => self.redirect_to.input(rt_key),
                    _ => false,
                };

                // Log if not handled for debugging
                if !handled {
                    tracing::debug!("Key not handled by TextArea: {:?}", key);
                }

                None
            }
        }
    }

    /// Handle MenuAction (called from mod.rs input routing)
    pub fn handle_action(
        &mut self,
        action: crate::core::menu_actions::MenuAction,
    ) -> Option<FormResult> {
        use crate::core::menu_actions::MenuAction;

        match action {
            MenuAction::NavigateUp => {
                // Up/Down now navigate fields (replaced old Up/Down navigation)
                self.focus_prev();
                None
            }
            MenuAction::NavigateDown => {
                // Up/Down now navigate fields
                self.focus_next();
                None
            }
            MenuAction::CycleBackward => {
                // Left arrow - cycle dropdown backward
                if self.focused_field == 5 {
                    // Sound dropdown
                    if self.sound_file_index > 0 {
                        self.sound_file_index -= 1;
                        self.update_sound_from_index();
                    }
                } else if self.focused_field == 9 {
                    // Redirect mode dropdown
                    if self.redirect_mode_index > 0 {
                        self.redirect_mode_index -= 1;
                    }
                } else if self.focused_field == 19 {
                    // Rumble dropdown
                    if self.rumble_index > 0 {
                        self.rumble_index -= 1;
                        self.update_rumble_from_index();
                    }
                } else if self.focused_field == 27 {
                    // Alert anchor dropdown
                    if self.alert_anchor_index > 0 {
                        self.alert_anchor_index -= 1;
                    }
                }
                None
            }
            MenuAction::CycleForward => {
                // Right arrow - cycle dropdown forward
                if self.focused_field == 5 {
                    // Sound dropdown
                    if !self.sound_files.is_empty()
                        && self.sound_file_index + 1 < self.sound_files.len()
                    {
                        self.sound_file_index += 1;
                        self.update_sound_from_index();
                    }
                } else if self.focused_field == 9 {
                    // Redirect mode dropdown
                    if self.redirect_mode_index < 2 {
                        self.redirect_mode_index += 1;
                    }
                } else if self.focused_field == 19 {
                    // Rumble dropdown
                    if self.rumble_index + 1 < self.rumble_options.len() {
                        self.rumble_index += 1;
                        self.update_rumble_from_index();
                    }
                } else if self.focused_field == 27 {
                    // Alert anchor dropdown
                    if self.alert_anchor_index + 1 < ALERT_ANCHOR_OPTIONS.len() {
                        self.alert_anchor_index += 1;
                    }
                }
                None
            }
            MenuAction::Select | MenuAction::Toggle => {
                // Enter/Space - toggle checkboxes or cycle dropdowns
                match self.focused_field {
                    5 => {
                        // Sound dropdown: cycle forward
                        if !self.sound_files.is_empty()
                            && self.sound_file_index + 1 < self.sound_files.len()
                        {
                            self.sound_file_index += 1;
                            self.update_sound_from_index();
                        } else if !self.sound_files.is_empty() {
                            self.sound_file_index = 0;
                            self.update_sound_from_index();
                        }
                    }
                    9 => {
                        // Redirect mode dropdown: cycle Off -> Copy -> Redirect -> Off
                        self.redirect_mode_index = (self.redirect_mode_index + 1) % 3;
                    }
                    10 => self.bold = !self.bold,
                    11 => self.color_entire_line = !self.color_entire_line,
                    12 => self.fast_parse = !self.fast_parse,
                    30 => self.case_insensitive = !self.case_insensitive,
                    13 => self.squelch = !self.squelch,
                    14 => self.silent_prompt = !self.silent_prompt,
                    15 => self.is_global = true, // Select "Global" scope
                    16 => self.is_global = false, // Select "Character" scope
                    19 => {
                        // Rumble dropdown: cycle (none) -> patterns -> (none)
                        if !self.rumble_options.is_empty() {
                            self.rumble_index = (self.rumble_index + 1) % self.rumble_options.len();
                            self.update_rumble_from_index();
                        }
                    }
                    27 => {
                        // Alert anchor dropdown: cycle through the 9-grid.
                        self.alert_anchor_index =
                            (self.alert_anchor_index + 1) % ALERT_ANCHOR_OPTIONS.len();
                    }
                    _ => {}
                }
                None
            }
            MenuAction::Save => {
                // Ctrl+S - save the form
                self.save_internal()
            }
            MenuAction::Delete => {
                // Treat Delete as a dismiss (no-op) for this form
                Some(FormResult::Cancel)
            }
            _ => None,
        }
    }

    /// Validate regex pattern
    fn validate_pattern(&mut self) {
        let pattern_text = self.pattern.lines()[0].as_str();
        if pattern_text.is_empty() {
            self.pattern_error = None;
            return;
        }

        match Regex::new(pattern_text) {
            Ok(_) => {
                self.pattern_error = None;
                self.status_message = "Pattern valid".to_string();
            }
            Err(e) => {
                self.pattern_error = Some(format!("Invalid regex: {}", e));
                self.status_message = "Invalid pattern!".to_string();
            }
        }
    }

    /// Internal save logic (called by Saveable trait implementation)
    fn save_internal(&self) -> Option<FormResult> {
        // Validate required fields
        let name = self.name.lines()[0].as_str().trim();
        if name.is_empty() {
            // Can't save without name
            return None;
        }

        let pattern_text = self.pattern.lines()[0].as_str().trim();
        if pattern_text.is_empty() {
            return None;
        }

        // Check pattern is valid
        if self.pattern_error.is_some() {
            return None;
        }

        // Build HighlightPattern
        let fg = {
            let fg_text = self.fg_color.lines()[0].as_str().trim();
            if fg_text.is_empty() {
                None
            } else {
                Some(fg_text.to_string())
            }
        };

        let bg = {
            let bg_text = self.bg_color.lines()[0].as_str().trim();
            if bg_text.is_empty() {
                None
            } else {
                Some(bg_text.to_string())
            }
        };

        let sound = {
            let sound_text = self.sound.lines()[0].as_str().trim();
            if sound_text.is_empty() {
                None
            } else {
                Some(sound_text.to_string())
            }
        };

        let sound_volume = {
            let vol_text = self.sound_volume.lines()[0].as_str().trim();
            if vol_text.is_empty() {
                None
            } else {
                vol_text.parse::<f32>().ok()
            }
        };

        let category = {
            let cat_text = self.category.lines()[0].as_str().trim();
            if cat_text.is_empty() {
                None
            } else {
                Some(cat_text.to_string())
            }
        };

        // Parse redirect settings
        let redirect_to = {
            let redirect_text = self.redirect_to.lines()[0].as_str().trim();
            if redirect_text.is_empty() || self.redirect_mode_index == 0 {
                None // Off mode or empty stream
            } else {
                Some(redirect_text.to_string())
            }
        };

        let redirect_mode = match self.redirect_mode_index {
            1 => crate::config::RedirectMode::RedirectCopy, // Copy = send to both
            2 => crate::config::RedirectMode::RedirectOnly, // Redirect = redirect only
            _ => crate::config::RedirectMode::default(), // Off (shouldn't be used as redirect_to will be None)
        };

        let replace = {
            let text = self.replace.lines()[0].as_str().trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        };

        let pattern = HighlightPattern {
            pattern: pattern_text.to_string(),
            category,
            fg,
            bg,
            bold: self.bold,
            color_entire_line: self.color_entire_line,
            fast_parse: self.fast_parse,
            case_insensitive: self.case_insensitive,
            squelch: self.squelch,
            silent_prompt: self.silent_prompt,
            sound,
            sound_volume,
            rumble: self.rumble.clone(),
            redirect_to,
            redirect_mode,
            replace,
            stream: {
                let text = self.stream_filter.lines()[0].as_str().trim();
                (!text.is_empty()).then(|| text.to_string())
            },
            window: {
                let text = self.window_filter.lines()[0].as_str().trim();
                (!text.is_empty()).then(|| text.to_string())
            },
            // These used to be hardcoded None, which silently WIPED
            // GUI-authored status actions on any TUI edit; the form owns
            // them now.
            set_status: {
                let text = self.set_status.lines()[0].as_str().trim();
                (!text.is_empty()).then(|| text.to_string())
            },
            status_duration: {
                let text = self.status_duration.lines()[0].as_str().trim();
                if text.is_empty() {
                    None
                } else {
                    text.parse::<f32>().ok().filter(|v| *v >= 0.0)
                }
            },
            clear_status: {
                let text = self.clear_status.lines()[0].as_str().trim();
                (!text.is_empty()).then(|| text.to_string())
            },
            alert: self.build_alert(),
            compiled_regex: None, // Will be compiled when config is loaded
        };

        Some(FormResult::Save {
            name: name.to_string(),
            pattern,
            is_global: self.is_global,
        })
    }

    /// Render the form as a draggable popup
    pub fn render(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        let width = POPUP_WIDTH;
        let height = POPUP_HEIGHT;

        // Center popup initially
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(width)) / 2;
            self.popup_y = (area.height.saturating_sub(height)) / 2;
        }

        let x = self.popup_x;
        let y = self.popup_y;

        // Clear the popup area to prevent bleed-through
        let popup_area = Rect {
            x,
            y,
            width,
            height,
        };
        Clear.render(popup_area, buf);

        // Draw black background
        for row in 0..height {
            for col in 0..width {
                if x + col < area.width && y + row < area.height {
                    buf[(x + col, y + row)]
                        .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                }
            }
        }

        // Draw cyan border
        self.draw_border(x, y, width, height, buf, theme);

        // Title (left-aligned)
        let title = match &self.mode {
            FormMode::Create => " Add Highlight ",
            FormMode::Edit(_) => " Edit Highlight ",
        };
        let buf_width = area.width;
        for (i, ch) in title.chars().enumerate() {
            let title_x = x + 1 + i as u16;
            if title_x < (x + width) && title_x < buf_width && y < area.height {
                buf[(title_x, y)]
                    .set_char(ch)
                    .set_fg(crossterm_bridge::to_ratatui_color(theme.browser_title))
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
            }
        }

        // Render fields
        self.render_fields(x, y, width, height, buf, config, theme);

        // Footer (matches mockup)
        let mut footer = "└─[Ctrl+S: Save]─[Esc: Back]".to_string();
        let footer_len = footer.chars().count() as u16;
        let fill_len = width.saturating_sub(footer_len + 1); // leave room for closing corner
        footer.push_str(&"─".repeat(fill_len as usize));
        footer.push('┘');
        let footer_y = y + height - 1;
        let footer_x = x;
        for (i, ch) in footer.chars().enumerate() {
            let fx = footer_x + i as u16;
            if fx >= x + width || fx >= buf_width || footer_y >= area.height {
                break;
            }
            buf[(fx, footer_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
    }

    fn draw_border(
        &self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let border_style =
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label));
        let buf_width = buf.area().width;
        let buf_height = buf.area().height;

        // Top border
        if x < buf_width && y < buf_height {
            buf[(x, y)].set_char('┌').set_style(border_style);
        }
        for col in 1..width - 1 {
            if x + col < buf_width && y < buf_height {
                buf[(x + col, y)].set_char('─').set_style(border_style);
            }
        }
        if x + width - 1 < buf_width && y < buf_height {
            buf[(x + width - 1, y)]
                .set_char('┐')
                .set_style(border_style);
        }

        // Side borders
        for row in 1..height - 1 {
            if x < buf_width && y + row < buf_height {
                buf[(x, y + row)].set_char('│').set_style(border_style);
            }
            if x + width - 1 < buf_width && y + row < buf_height {
                buf[(x + width - 1, y + row)]
                    .set_char('│')
                    .set_style(border_style);
            }
        }

        // Bottom border
        if x < buf_width && y + height - 1 < buf_height {
            buf[(x, y + height - 1)]
                .set_char('└')
                .set_style(border_style);
        }
        for col in 1..width - 1 {
            if x + col < buf_width && y + height - 1 < buf_height {
                buf[(x + col, y + height - 1)]
                    .set_char('─')
                    .set_style(border_style);
            }
        }
        if x + width - 1 < buf_width && y + height - 1 < buf_height {
            buf[(x + width - 1, y + height - 1)]
                .set_char('┘')
                .set_style(border_style);
        }
    }

    /// Render all form fields
    fn render_fields(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        _height: u16,
        buf: &mut Buffer,
        config: &crate::config::Config,
        theme: &crate::theme::AppTheme,
    ) {
        let mut current_y = y + 2; // Start below title bar
        let label_width = 16; // Enough for "Background:"
        let input_start = x + 2 + label_width;
        let input_width = width.saturating_sub((input_start - x) + 2);

        // Parse textarea background color from config
        // If "-" is specified, use Color::Reset (terminal default), otherwise parse hex
        let default_bg = crossterm_bridge::to_ratatui_color(theme.browser_background);
        let txtbg = if config.colors.ui.textarea_background == "-" {
            default_bg
        } else if let Ok(color) = Self::parse_hex_color(&config.colors.ui.textarea_background) {
            color
        } else {
            default_bg
        };

        let focused_field = self.focused_field;

        // Field 0: Name
        Self::render_text_row(
            focused_field,
            0,
            "Name:",
            &mut self.name,
            "monster_kill",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        // Field 1: Pattern
        Self::render_text_row(
            focused_field,
            1,
            "Pattern:",
            &mut self.pattern,
            "You swing.*at",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        // Field 2: Category
        Self::render_text_row(
            focused_field,
            2,
            "Category:",
            &mut self.category,
            "Combat",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        // Field 3: Foreground (10 char + 1 space + 2 char preview)
        {
            let fg_text = self.fg_color.lines()[0].clone();
            Self::render_color_row_internal(
                focused_field,
                3,
                "Foreground:",
                &mut self.fg_color,
                "#ff0000",
                x + 2,
                current_y,
                input_start,
                input_width,
                txtbg,
                buf,
                theme,
            );
            // Color preview (with bounds checking)
            let buf_width = buf.area().width;
            let buf_height = buf.area().height;
            if input_width >= 2 {
                let preview_x = input_start + input_width.saturating_sub(2);
                if preview_x < buf_width && current_y < buf_height {
                    buf[(preview_x, current_y)]
                        .set_char(' ')
                        .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                    if !fg_text.is_empty() {
                        if let Some(color) = self.parse_and_resolve_color(&fg_text, config) {
                            buf[(preview_x, current_y)].set_char(' ').set_bg(color);
                            if preview_x + 1 < buf_width && preview_x + 1 < x + width - 1 {
                                buf[(preview_x + 1, current_y)].set_char(' ').set_bg(color);
                            }
                        }
                    }
                }
            }
        }
        current_y += 1;

        // Field 4: Background (10 char + 1 space + 2 char preview)
        {
            let bg_text = self.bg_color.lines()[0].clone();
            Self::render_color_row_internal(
                focused_field,
                4,
                "Background:",
                &mut self.bg_color,
                "optional",
                x + 2,
                current_y,
                input_start,
                input_width,
                txtbg,
                buf,
                theme,
            );
            // Color preview (with bounds checking)
            let buf_width = buf.area().width;
            let buf_height = buf.area().height;
            if input_width >= 2 {
                let preview_x = input_start + input_width.saturating_sub(2);
                if preview_x < buf_width && current_y < buf_height {
                    buf[(preview_x, current_y)]
                        .set_char(' ')
                        .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                    if !bg_text.is_empty() {
                        if let Some(color) = self.parse_and_resolve_color(&bg_text, config) {
                            buf[(preview_x, current_y)].set_char(' ').set_bg(color);
                            if preview_x + 1 < buf_width && preview_x + 1 < x + width - 1 {
                                buf[(preview_x + 1, current_y)].set_char(' ').set_bg(color);
                            }
                        }
                    }
                }
            }
        }
        current_y += 1;

        // Field 5: Sound (dropdown)
        self.render_sound_dropdown(x + 2, current_y, input_start, input_width, buf, theme);
        current_y += 1;

        // Field 6: Volume
        Self::render_text_row(
            focused_field,
            6,
            "Volume:",
            &mut self.sound_volume,
            "0.8",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        // Field 7: Replace
        Self::render_text_row(
            focused_field,
            7,
            "Replace:",
            &mut self.replace,
            "replacement text",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        // Field 8: Redirect To (stream name)
        Self::render_text_row(
            focused_field,
            8,
            "Redirect To:",
            &mut self.redirect_to,
            "stream name",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        // Field 9: Redirect Mode (dropdown)
        self.render_redirect_mode_dropdown(x + 2, current_y, input_start, input_width, buf, theme);
        current_y += 2;

        // Checkboxes (Fields 10-13)
        buf[(x + 2, current_y)]
            .set_char('[')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 10 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.bold { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 10 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 10 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let bold_label = " Bold";
        for (i, ch) in bold_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(
                    if self.focused_field == 10 {
                        theme.form_label_focused
                    } else {
                        theme.form_label
                    },
                ))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
        current_y += 1;

        buf[(x + 2, current_y)]
            .set_char('[')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 11 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.color_entire_line { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 11 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 11 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let cel_label = " Color entire line";
        for (i, ch) in cel_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(
                    if self.focused_field == 11 {
                        theme.form_label_focused
                    } else {
                        theme.form_label
                    },
                ))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
        current_y += 1;

        buf[(x + 2, current_y)]
            .set_char('[')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 12 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.fast_parse { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 12 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 12 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let fp_label = " Fast parse";
        for (i, ch) in fp_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(
                    if self.focused_field == 12 {
                        theme.form_label_focused
                    } else {
                        theme.form_label
                    },
                ))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Field 30: Ignore case — shares the Fast parse ROW (to its right)
        // rather than taking a new one, so every row index below here (and
        // the mouse map that mirrors them) stays exactly as it was.
        {
            let ci_x = x + 5 + fp_label.chars().count() as u16 + 3;
            let focused = self.focused_field == 30;
            let color = crossterm_bridge::to_ratatui_color(if focused {
                theme.form_label_focused
            } else {
                theme.form_label
            });
            let bg = crossterm_bridge::to_ratatui_color(theme.browser_background);
            let cells = [
                ('[', 0u16),
                (if self.case_insensitive { '✓' } else { ' ' }, 1),
                (']', 2),
            ];
            for (ch, offset) in cells {
                buf[(ci_x + offset, current_y)]
                    .set_char(ch)
                    .set_fg(color)
                    .set_bg(bg);
            }
            for (i, ch) in " Ignore case".chars().enumerate() {
                buf[(ci_x + 3 + i as u16, current_y)]
                    .set_char(ch)
                    .set_fg(color)
                    .set_bg(bg);
            }
        }

        current_y += 1;

        // Field 10: Squelch checkbox
        buf[(x + 2, current_y)]
            .set_char('[')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 13 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.squelch { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 13 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 13 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let squelch_label = " Squelch (ignore line)";
        for (i, ch) in squelch_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(
                    if self.focused_field == 13 {
                        theme.form_label_focused
                    } else {
                        theme.form_label
                    },
                ))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        current_y += 1;

        // Field 14: Silent Prompt checkbox
        buf[(x + 2, current_y)]
            .set_char('[')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 14 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.silent_prompt { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 14 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 14 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let silent_label = " Silent Prompt (suppress prompt)";
        for (i, ch) in silent_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(
                    if self.focused_field == 14 {
                        theme.form_label_focused
                    } else {
                        theme.form_label
                    },
                ))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        current_y += 1;

        // Fields 15-16: Scope radio buttons (Global / Character)
        let scope_label = "Scope: ";
        for (i, ch) in scope_label.chars().enumerate() {
            buf[(x + 2 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(theme.form_label))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Field 15: Global radio button
        let global_start = x + 2 + scope_label.len() as u16;
        buf[(global_start, current_y)]
            .set_char('(')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 15 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(global_start + 1, current_y)]
            .set_char(if self.is_global { '●' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 15 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(global_start + 2, current_y)]
            .set_char(')')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 15 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let global_label = " Global  ";
        for (i, ch) in global_label.chars().enumerate() {
            buf[(global_start + 3 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(
                    if self.focused_field == 15 {
                        theme.form_label_focused
                    } else {
                        theme.form_label
                    },
                ))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Field 16: Character radio button
        let char_start = global_start + 3 + global_label.len() as u16;
        buf[(char_start, current_y)]
            .set_char('(')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 16 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(char_start + 1, current_y)]
            .set_char(if !self.is_global { '●' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 16 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(char_start + 2, current_y)]
            .set_char(')')
            .set_fg(crossterm_bridge::to_ratatui_color(
                if self.focused_field == 16 {
                    theme.form_label_focused
                } else {
                    theme.form_label
                },
            ))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let char_label = " Character";
        for (i, ch) in char_label.chars().enumerate() {
            buf[(char_start + 3 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(
                    if self.focused_field == 16 {
                        theme.form_label_focused
                    } else {
                        theme.form_label
                    },
                ))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
        current_y += 1;

        // Fields 17-18: optional stream/window filters
        Self::render_text_row(
            focused_field,
            17,
            "Stream:",
            &mut self.stream_filter,
            "optional",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        Self::render_text_row(
            focused_field,
            18,
            "Window:",
            &mut self.window_filter,
            "optional",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        // Field 19: Rumble pattern (dropdown)
        self.render_rumble_dropdown(x + 2, current_y, input_start, input_width, buf, theme);
        current_y += 1;

        // Fields 20-22: custom-status actions (set / duration / clear)
        Self::render_text_row(
            focused_field,
            20,
            "Set status:",
            &mut self.set_status,
            "optional",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        Self::render_text_row(
            focused_field,
            21,
            "Status secs:",
            &mut self.status_duration,
            "optional",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        Self::render_text_row(
            focused_field,
            22,
            "Clear status:",
            &mut self.clear_status,
            "optional",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        // ---- Overlay alert (fields 23-29) ---------------------------
        // Split rows: render_text_row takes explicit x/width, so two fields
        // share a line and the section costs 4 rows instead of 7.
        let half = input_width / 2;
        let right_label_x = input_start + half + 1;
        let right_input_x = right_label_x + 6;
        let right_width = (x + 2 + POPUP_WIDTH - 4).saturating_sub(right_input_x);

        Self::render_text_row(
            focused_field,
            23,
            "Alert banner:",
            &mut self.alert_banner,
            "optional",
            x + 2,
            current_y,
            input_start,
            input_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        Self::render_text_row(
            focused_field,
            24,
            "Banner fg:",
            &mut self.alert_banner_fg,
            "color",
            x + 2,
            current_y,
            input_start,
            half,
            txtbg,
            buf,
            theme,
        );
        Self::render_text_row(
            focused_field,
            25,
            "bg:",
            &mut self.alert_banner_bg,
            "color",
            right_label_x,
            current_y,
            right_input_x,
            right_width,
            txtbg,
            buf,
            theme,
        );
        current_y += 1;

        Self::render_text_row(
            focused_field,
            26,
            "Alert flash:",
            &mut self.alert_flash,
            "color",
            x + 2,
            current_y,
            input_start,
            half,
            txtbg,
            buf,
            theme,
        );
        // Anchor dropdown shares the flash row (field 27).
        {
            let focused = focused_field == 27;
            let label_color = crossterm_bridge::to_ratatui_color(if focused {
                theme.form_label_focused
            } else {
                theme.form_label
            });
            let label = "at:";
            for (i, ch) in label.chars().enumerate() {
                buf[(right_label_x + i as u16, current_y)]
                    .set_char(ch)
                    .set_fg(label_color)
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
            }
            let value = format!(
                "{} {} {}",
                if focused { "<" } else { " " },
                ALERT_ANCHOR_OPTIONS[self.alert_anchor_index.min(8)].1,
                if focused { ">" } else { " " },
            );
            for (i, ch) in value.chars().enumerate() {
                let cx = right_input_x + i as u16;
                if cx >= x + POPUP_WIDTH - 1 {
                    break;
                }
                buf[(cx, current_y)]
                    .set_char(ch)
                    .set_fg(label_color)
                    .set_bg(txtbg);
            }
        }
        current_y += 1;

        Self::render_text_row(
            focused_field,
            28,
            "Alert secs:",
            &mut self.alert_duration,
            "secs",
            x + 2,
            current_y,
            input_start,
            half,
            txtbg,
            buf,
            theme,
        );
        Self::render_text_row(
            focused_field,
            29,
            "cool:",
            &mut self.alert_cooldown,
            "secs",
            right_label_x,
            current_y,
            right_input_x,
            right_width,
            txtbg,
            buf,
            theme,
        );
    }

    fn render_text_row(
        focused_field: usize,
        field_id: usize,
        label: &str,
        textarea: &mut TextArea,
        _hint: &str,
        x: u16,
        y: u16,
        input_x: u16,
        input_width: u16,
        bg: Color,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let focused = focused_field == field_id;
        let label_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else {
            theme.form_label
        });

        // Render label
        for (i, ch) in label.chars().enumerate() {
            buf[(x + i as u16, y)]
                .set_char(ch)
                .set_fg(label_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Create rect for the TextArea widget
        let textarea_rect = Rect {
            x: input_x,
            y,
            width: input_width,
            height: 1,
        };

        // Set block style for the textarea (no border, just background)
        let block = ratatui::widgets::Block::default().style(Style::default().bg(bg));

        textarea.set_block(block);

        // Set text style
        textarea.set_style(
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                .bg(bg),
        );

        // Render the TextArea widget - it handles cursor positioning and scrolling automatically
        textarea.render(textarea_rect, buf);
    }

    fn render_color_row_internal(
        focused_field: usize,
        field_id: usize,
        label: &str,
        textarea: &mut TextArea,
        _hint: &str,
        x: u16,
        y: u16,
        input_x: u16,
        input_width: u16,
        bg: Color,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let focused = focused_field == field_id;
        let label_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else {
            theme.form_label
        });

        // Render label
        for (i, ch) in label.chars().enumerate() {
            buf[(x + i as u16, y)]
                .set_char(ch)
                .set_fg(label_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Create rect for the TextArea widget (fill toward border)
        let textarea_rect = Rect {
            x: input_x,
            y,
            width: input_width.max(10),
            height: 1,
        };

        // Set block style for the textarea (no border, just background)
        let block = ratatui::widgets::Block::default().style(Style::default().bg(bg));

        textarea.set_block(block);

        // Set text style
        textarea.set_style(
            Style::default()
                .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                .bg(bg),
        );

        // Render the TextArea widget
        textarea.render(textarea_rect, buf);
    }

    fn render_sound_dropdown(
        &self,
        x: u16,
        y: u16,
        input_x: u16,
        available_width: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let focused = self.focused_field == 5;
        let label_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else {
            theme.form_label
        });

        // Render label
        let label = "Sound:";
        for (i, ch) in label.chars().enumerate() {
            buf[(x + i as u16, y)]
                .set_char(ch)
                .set_fg(label_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Get current value from dropdown index
        let current_value =
            if !self.sound_files.is_empty() && self.sound_file_index < self.sound_files.len() {
                &self.sound_files[self.sound_file_index]
            } else {
                "none"
            };

        // Render current value (highlight if focused, no background)
        let value_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else {
            theme.text_disabled
        });
        for (i, ch) in current_value
            .chars()
            .enumerate()
            .take(available_width as usize)
        {
            buf[(input_x + i as u16, y)]
                .set_char(ch)
                .set_fg(value_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
    }

    fn render_redirect_mode_dropdown(
        &self,
        x: u16,
        y: u16,
        input_x: u16,
        available_width: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let focused = self.focused_field == 9;
        let label_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else {
            theme.form_label
        });

        // Render label
        let label = "Redirect Mode:";
        for (i, ch) in label.chars().enumerate() {
            buf[(x + i as u16, y)]
                .set_char(ch)
                .set_fg(label_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        // Get current value from dropdown index (0=Off, 1=Copy, 2=Redirect-only)
        let current_value = match self.redirect_mode_index {
            0 => "Off",
            1 => "Copy",
            2 => "Redirect",
            _ => "Off",
        };

        // Render current value (highlight if focused, no background)
        let value_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else {
            theme.text_disabled
        });
        for (i, ch) in current_value
            .chars()
            .enumerate()
            .take(available_width as usize)
        {
            buf[(input_x + i as u16, y)]
                .set_char(ch)
                .set_fg(value_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
    }

    fn render_rumble_dropdown(
        &self,
        x: u16,
        y: u16,
        input_x: u16,
        available_width: u16,
        buf: &mut Buffer,
        theme: &crate::theme::AppTheme,
    ) {
        let focused = self.focused_field == 19;
        let label_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else {
            theme.form_label
        });

        let label = "Rumble:";
        for (i, ch) in label.chars().enumerate() {
            buf[(x + i as u16, y)]
                .set_char(ch)
                .set_fg(label_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        let current_value = self
            .rumble_options
            .get(self.rumble_index)
            .map(|s| s.as_str())
            .unwrap_or("(none)");

        let value_color = crossterm_bridge::to_ratatui_color(if focused {
            theme.form_label_focused
        } else {
            theme.text_disabled
        });
        for (i, ch) in current_value
            .chars()
            .enumerate()
            .take(available_width as usize)
        {
            buf[(input_x + i as u16, y)]
                .set_char(ch)
                .set_fg(value_color)
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
    }

    fn parse_and_resolve_color(
        &self,
        color_text: &str,
        config: &crate::config::Config,
    ) -> Option<Color> {
        let trimmed = color_text.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Try resolving through config
        if let Some(hex) = config.resolve_color(trimmed) {
            return Self::parse_hex_color(&hex).ok();
        }

        // Try parsing directly as hex
        Self::parse_hex_color(trimmed).ok()
    }

    /// Parse hex color string (#RRGGBB)
    fn parse_hex_color(hex: &str) -> Result<Color, ()> {
        if !hex.starts_with('#') || hex.len() != 7 {
            return Err(());
        }

        let r = u8::from_str_radix(&hex[1..3], 16).map_err(|_| ())?;
        let g = u8::from_str_radix(&hex[3..5], 16).map_err(|_| ())?;
        let b = u8::from_str_radix(&hex[5..7], 16).map_err(|_| ())?;

        Ok(Color::Rgb(r, g, b))
    }

    /// Handle mouse events for the popup
    pub fn handle_mouse(
        &mut self,
        col: u16,
        row: u16,
        pressed: bool,
        terminal_area: Rect,
    ) -> HighlightFormMouseAction {
        let popup_width = POPUP_WIDTH;
        let popup_height = POPUP_HEIGHT;

        // Check if click is on title bar (top border, excluding corners)
        let on_title_bar =
            row == self.popup_y && col > self.popup_x && col < self.popup_x + popup_width - 1;

        if pressed && on_title_bar && !self.is_dragging {
            // Start dragging
            self.is_dragging = true;
            self.drag_offset_x = col.saturating_sub(self.popup_x);
            self.drag_offset_y = row.saturating_sub(self.popup_y);
            return HighlightFormMouseAction::None;
        }

        if self.is_dragging {
            if pressed {
                // Continue dragging - clamp to keep popup fully within terminal bounds
                let new_x = col.saturating_sub(self.drag_offset_x);
                let new_y = row.saturating_sub(self.drag_offset_y);
                // Ensure popup stays within terminal area
                let max_x = terminal_area.width.saturating_sub(popup_width);
                let max_y = terminal_area.height.saturating_sub(popup_height);
                self.popup_x = new_x.min(max_x);
                self.popup_y = new_y.min(max_y);
                return HighlightFormMouseAction::None;
            } else {
                // Mouse released
                self.is_dragging = false;
                return HighlightFormMouseAction::None;
            }
        }

        // Only process clicks (pressed), not releases
        if !pressed {
            return HighlightFormMouseAction::None;
        }

        // Check if click is inside the popup
        let inside_popup = col >= self.popup_x
            && col < self.popup_x + popup_width
            && row > self.popup_y
            && row < self.popup_y + popup_height;

        if !inside_popup {
            return HighlightFormMouseAction::None;
        }

        // Field layout (from render_fields - starts at y+2):
        // y+2: Name (field 0)
        // y+3: Pattern (field 1)
        // y+4: Category (field 2)
        // y+5: FG Color (field 3)
        // y+6: BG Color (field 4)
        // y+7: Sound File dropdown (field 5)
        // y+8: Volume (field 6)
        // y+9: Replace (field 7)
        // y+10: Redirect To (field 8)
        // y+11: Redirect Mode dropdown (field 9)
        // y+13: Bold(10)
        // y+14: Color entire line(11)
        // y+15: Fast parse(12)
        // y+16: Squelch(13)
        // y+17: Silent Prompt(14)
        // y+18: Scope (fields 15/16)
        // y+19: Stream filter (field 17)
        // y+20: Window filter (field 18)

        let field_y = self.popup_y + 2; // Fields start at y+2 in render_fields

        // Check field clicks based on row (matching render_fields layout)
        // Each text field is 1 row, redirect mode has a +2 gap before checkboxes
        if row == field_y {
            self.focused_field = 0; // Name
            return HighlightFormMouseAction::None;
        } else if row == field_y + 1 {
            self.focused_field = 1; // Pattern
            return HighlightFormMouseAction::None;
        } else if row == field_y + 2 {
            self.focused_field = 2; // Category
            return HighlightFormMouseAction::None;
        } else if row == field_y + 3 {
            self.focused_field = 3; // FG Color
            return HighlightFormMouseAction::None;
        } else if row == field_y + 4 {
            self.focused_field = 4; // BG Color
            return HighlightFormMouseAction::None;
        } else if row == field_y + 5 {
            self.focused_field = 5; // Sound File dropdown
            return HighlightFormMouseAction::None;
        } else if row == field_y + 6 {
            self.focused_field = 6; // Volume
            return HighlightFormMouseAction::None;
        } else if row == field_y + 7 {
            self.focused_field = 7; // Replace
            return HighlightFormMouseAction::None;
        } else if row == field_y + 8 {
            self.focused_field = 8; // Redirect To
            return HighlightFormMouseAction::None;
        } else if row == field_y + 9 {
            self.focused_field = 9; // Redirect Mode dropdown
            return HighlightFormMouseAction::None;
        } else if row == field_y + 11 {
            // Bold checkbox (after +2 gap from redirect mode)
            self.focused_field = 10;
            self.bold = !self.bold;
            return HighlightFormMouseAction::None;
        } else if row == field_y + 12 {
            // Color entire line checkbox
            self.focused_field = 11;
            self.color_entire_line = !self.color_entire_line;
            return HighlightFormMouseAction::None;
        } else if row == field_y + 13 {
            // This row carries TWO checkboxes: Fast parse on the left and
            // Ignore case to its right (see render_fields). Split on the
            // column where the second one starts — " Fast parse" is 11 wide
            // from x+5, plus the 3-cell gap.
            let ignore_case_x = self.popup_x + 5 + 11 + 3;
            if col >= ignore_case_x {
                self.focused_field = 30;
                self.case_insensitive = !self.case_insensitive;
            } else {
                self.focused_field = 12;
                self.fast_parse = !self.fast_parse;
            }
            return HighlightFormMouseAction::None;
        } else if row == field_y + 14 {
            // Squelch checkbox
            self.focused_field = 13;
            self.squelch = !self.squelch;
            return HighlightFormMouseAction::None;
        } else if row == field_y + 15 {
            // Silent Prompt checkbox
            self.focused_field = 14;
            self.silent_prompt = !self.silent_prompt;
            return HighlightFormMouseAction::None;
        } else if row == field_y + 16 {
            // Scope row (Global/Character radio)
            let rel_x = col.saturating_sub(self.popup_x + 9); // "Scope: " is 7 chars + 2 margin
            if rel_x < 12 {
                self.focused_field = 15;
                self.is_global = true;
            } else {
                self.focused_field = 16;
                self.is_global = false;
            }
            return HighlightFormMouseAction::None;
        } else if row == field_y + 17 {
            self.focused_field = 17; // Stream filter
            return HighlightFormMouseAction::None;
        } else if row == field_y + 18 {
            self.focused_field = 18; // Window filter
            return HighlightFormMouseAction::None;
        }

        // Check footer for Save/Back buttons (last row of popup)
        let footer_y = self.popup_y + popup_height - 1;
        if row == footer_y {
            let rel_x = col.saturating_sub(self.popup_x);
            // Footer: "└─[Ctrl+S: Save]─[Esc: Back]..."
            // Save is at ~3-15, Back is at ~18-26
            if rel_x >= 3 && rel_x <= 15 {
                return HighlightFormMouseAction::Save;
            } else if rel_x >= 18 && rel_x <= 26 {
                return HighlightFormMouseAction::Cancel;
            }
        }

        HighlightFormMouseAction::None
    }
}

// Trait implementations for HighlightFormWidget
use super::widget_traits::{Cyclable, FieldNavigable, TextEditable, Toggleable};
use anyhow::Result;

impl TextEditable for HighlightFormWidget {
    fn get_focused_field(&self) -> Option<&TextArea<'static>> {
        match self.focused_field {
            0 => Some(&self.name),
            1 => Some(&self.pattern),
            2 => Some(&self.category),
            3 => Some(&self.fg_color),
            4 => Some(&self.bg_color),
            5 => Some(&self.sound),
            6 => Some(&self.sound_volume),
            7 => Some(&self.replace),
            8 => Some(&self.redirect_to),
            17 => Some(&self.stream_filter),
            18 => Some(&self.window_filter),
            20 => Some(&self.set_status),
            21 => Some(&self.status_duration),
            22 => Some(&self.clear_status),
            23 => Some(&self.alert_banner),
            24 => Some(&self.alert_banner_fg),
            25 => Some(&self.alert_banner_bg),
            26 => Some(&self.alert_flash),
            28 => Some(&self.alert_duration),
            29 => Some(&self.alert_cooldown),
            _ => None,
        }
    }

    fn get_focused_field_mut(&mut self) -> Option<&mut TextArea<'static>> {
        match self.focused_field {
            0 => Some(&mut self.name),
            1 => Some(&mut self.pattern),
            2 => Some(&mut self.category),
            3 => Some(&mut self.fg_color),
            4 => Some(&mut self.bg_color),
            5 => Some(&mut self.sound),
            6 => Some(&mut self.sound_volume),
            7 => Some(&mut self.replace),
            8 => Some(&mut self.redirect_to),
            17 => Some(&mut self.stream_filter),
            18 => Some(&mut self.window_filter),
            20 => Some(&mut self.set_status),
            21 => Some(&mut self.status_duration),
            22 => Some(&mut self.clear_status),
            23 => Some(&mut self.alert_banner),
            24 => Some(&mut self.alert_banner_fg),
            25 => Some(&mut self.alert_banner_bg),
            26 => Some(&mut self.alert_flash),
            28 => Some(&mut self.alert_duration),
            29 => Some(&mut self.alert_cooldown),
            _ => None,
        }
    }
}

impl FieldNavigable for HighlightFormWidget {
    fn next_field(&mut self) {
        self.focus_next();
    }

    fn previous_field(&mut self) {
        self.focus_prev();
    }

    fn field_count(&self) -> usize {
        14
    }

    fn current_field(&self) -> usize {
        self.focused_field
    }
}

// Implement Saveable trait for uniform form interface
impl super::widget_traits::Saveable for HighlightFormWidget {
    type SaveResult = FormResult;

    fn try_save(&mut self) -> Option<Self::SaveResult> {
        // Delegate to internal save logic
        self.save_internal()
    }
}

impl Toggleable for HighlightFormWidget {
    fn toggle_focused(&mut self) -> Option<bool> {
        match self.focused_field {
            7 => {
                self.bold = !self.bold;
                Some(self.bold)
            }
            8 => {
                self.color_entire_line = !self.color_entire_line;
                Some(self.color_entire_line)
            }
            9 => {
                self.fast_parse = !self.fast_parse;
                Some(self.fast_parse)
            }
            _ => None,
        }
    }
}

impl Cyclable for HighlightFormWidget {
    fn cycle_forward(&mut self) {
        if self.focused_field == 5
            && !self.sound_files.is_empty()
            && self.sound_file_index + 1 < self.sound_files.len()
        {
            self.sound_file_index += 1;
            self.update_sound_from_index();
        }
    }

    fn cycle_backward(&mut self) {
        if self.focused_field == 5 && self.sound_file_index > 0 {
            self.sound_file_index -= 1;
            self.update_sound_from_index();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HighlightPattern, RedirectMode};

    fn pattern_with_filters() -> HighlightPattern {
        HighlightPattern {
            pattern: "You swing".to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            case_insensitive: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: Some("alerts".to_string()),
            redirect_mode: RedirectMode::RedirectOnly,
            replace: None,
            stream: Some("combat".to_string()),
            window: Some("combat_win".to_string()),
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        }
    }

    #[test]
    fn edit_round_trip_preserves_stream_and_window_filters() {
        let form = HighlightFormWidget::new_edit("test".to_string(), &pattern_with_filters());
        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected Save result");
        };
        assert_eq!(pattern.stream.as_deref(), Some("combat"));
        assert_eq!(pattern.window.as_deref(), Some("combat_win"));
    }

    /// Regression guard: save_internal used to hardcode the custom-status
    /// fields to None, silently wiping GUI-authored status actions on any
    /// TUI edit. They round-trip (and parse) now.
    #[test]
    fn edit_round_trip_preserves_status_actions() {
        let mut source = pattern_with_filters();
        source.set_status = Some("hunted".to_string());
        source.status_duration = Some(12.5);
        source.clear_status = Some("resting".to_string());
        let form = HighlightFormWidget::new_edit("test".to_string(), &source);
        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected Save result");
        };
        assert_eq!(pattern.set_status.as_deref(), Some("hunted"));
        assert_eq!(pattern.status_duration, Some(12.5));
        assert_eq!(pattern.clear_status.as_deref(), Some("resting"));

        // A junk duration clears rather than erroring the save.
        let mut source = pattern_with_filters();
        source.status_duration = Some(1.0);
        let mut form = HighlightFormWidget::new_edit("test".to_string(), &source);
        form.status_duration = TextArea::from(["not-a-number"]);
        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected Save result");
        };
        assert_eq!(pattern.status_duration, None);
    }

    /// The TUI form can now EDIT rumble (previously it only carried the value
    /// through untouched). Loading a rule with a rumble pattern points the
    /// dropdown at it, and cycling to a different pattern saves that choice.
    #[test]
    fn tui_form_can_edit_rumble() {
        let mut pattern = pattern_with_filters();
        pattern.rumble = Some("long".to_string());
        let mut form = HighlightFormWidget::new_edit("test".to_string(), &pattern);
        form.set_rumble_options(vec![
            "short".to_string(),
            "long".to_string(),
            "double".to_string(),
        ]);
        // Loaded value selected: options = [(none), short, long, double] → idx 2.
        assert_eq!(form.rumble.as_deref(), Some("long"));
        assert_eq!(form.rumble_index, 2);

        // Cycle the dropdown forward (long → double) and confirm it saves.
        form.focused_field = 19;
        form.rumble_index += 1;
        form.update_rumble_from_index();
        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected Save result");
        };
        assert_eq!(pattern.rumble.as_deref(), Some("double"));
    }

    /// A rumble pattern that's no longer defined in the controller config
    /// stays selectable (appended) so editing an unrelated field doesn't drop
    /// it — the same "don't clobber unoffered values" rule as the keybind form.
    #[test]
    fn tui_form_preserves_unknown_rumble_pattern() {
        let mut pattern = pattern_with_filters();
        pattern.rumble = Some("custom-retired".to_string());
        let mut form = HighlightFormWidget::new_edit("test".to_string(), &pattern);
        form.set_rumble_options(vec!["short".to_string(), "long".to_string()]);
        // Unknown pattern was appended and selected, not lost.
        assert_eq!(form.rumble.as_deref(), Some("custom-retired"));
        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected Save result");
        };
        assert_eq!(pattern.rumble.as_deref(), Some("custom-retired"));
    }

    #[test]
    fn new_highlight_saves_typed_filters() {
        let mut form = HighlightFormWidget::new();
        form.name = TextArea::from(["combat_hl"]);
        form.pattern = TextArea::from(["You swing"]);
        form.stream_filter = TextArea::from(["combat"]);
        form.window_filter = TextArea::from(["combat_win"]);
        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected Save result");
        };
        assert_eq!(pattern.stream.as_deref(), Some("combat"));
        assert_eq!(pattern.window.as_deref(), Some("combat_win"));
    }

    #[test]
    fn edit_round_trip_preserves_redirect_mode() {
        // RedirectOnly in, RedirectOnly out (the old index mapping was
        // reversed between new_edit and save, flipping Copy <-> Only)
        let form = HighlightFormWidget::new_edit("test".to_string(), &pattern_with_filters());
        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected Save result");
        };
        assert_eq!(pattern.redirect_mode, RedirectMode::RedirectOnly);
        assert_eq!(pattern.redirect_to.as_deref(), Some("alerts"));

        let mut copy_pattern = pattern_with_filters();
        copy_pattern.redirect_mode = RedirectMode::RedirectCopy;
        let form = HighlightFormWidget::new_edit("test".to_string(), &copy_pattern);
        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected Save result");
        };
        assert_eq!(pattern.redirect_mode, RedirectMode::RedirectCopy);
    }
    #[test]
    fn tui_form_authors_the_alert_fields_its_renderer_shows() {
        let mut form = HighlightFormWidget::new();
        form.name = TextArea::from(["stun".to_string()]);
        form.pattern = TextArea::from(["You are stunned".to_string()]);
        form.alert_banner = TextArea::from(["STUNNED".to_string()]);
        form.alert_banner_fg = TextArea::from(["#ff0000".to_string()]);
        form.alert_flash = TextArea::from(["#aa0000".to_string()]);
        form.alert_anchor_index = 1; // top-center
        form.alert_duration = TextArea::from(["6".to_string()]);
        form.alert_cooldown = TextArea::from(["3".to_string()]);

        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected save");
        };
        let alert = pattern.alert.expect("alert authored");
        assert_eq!(alert.banner.as_deref(), Some("STUNNED"));
        assert_eq!(alert.banner_fg.as_deref(), Some("#ff0000"));
        assert_eq!(alert.flash.as_deref(), Some("#aa0000"));
        assert_eq!(alert.anchor, crate::config::AlertAnchor::TopCenter);
        assert_eq!(alert.duration, Some(6.0));
        assert_eq!(alert.cooldown, Some(3.0));
    }

    #[test]
    fn tui_edit_preserves_the_alert_subfields_it_cannot_author() {
        // The merge rule: banner edits here must not wipe a GUI-authored
        // condition gate or timer — the status-actions wipe bug, prevented.
        let mut source = pattern_with_filters();
        source.alert = Some(crate::config::AlertSpec {
            banner: Some("OLD".to_string()),
            when: Some(crate::config::Condition::RtActive),
            rearm: Some(5.0),
            timer: Some(crate::config::AlertTimer {
                id: Some("t".to_string()),
                label: "Timer".to_string(),
                duration: 12.0,
                color: None,
            }),
            cancels: vec!["other".to_string()],
            art: Some("lightning".to_string()),
            ..Default::default()
        });

        let mut form = HighlightFormWidget::new_edit("rule".to_string(), &source);
        // Loaded for editing…
        assert_eq!(form.alert_banner.lines()[0], "OLD");
        // …edit only the banner and save.
        form.alert_banner = TextArea::from(["NEW".to_string()]);

        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected save");
        };
        let alert = pattern.alert.expect("alert survives");
        assert_eq!(alert.banner.as_deref(), Some("NEW"), "edit applied");
        assert!(alert.when.is_some(), "condition gate preserved");
        assert_eq!(alert.rearm, Some(5.0), "rearm preserved");
        assert!(alert.timer.is_some(), "timer preserved");
        assert_eq!(alert.cancels, vec!["other"], "cancels preserved");
        assert_eq!(alert.art.as_deref(), Some("lightning"), "art preserved");
    }

    #[test]
    fn clearing_every_alert_field_on_a_plain_rule_yields_no_alert() {
        // Empty alert fields on a rule with no preserved gate/timer must
        // save as "no alert", not an inert empty spec.
        let mut form = HighlightFormWidget::new();
        form.name = TextArea::from(["plain".to_string()]);
        form.pattern = TextArea::from(["hello".to_string()]);
        let Some(FormResult::Save { pattern, .. }) = form.save_internal() else {
            panic!("expected save");
        };
        assert!(pattern.alert.is_none());
    }
}
