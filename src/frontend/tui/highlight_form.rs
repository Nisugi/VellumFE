//! Popup form for creating, editing, and validating highlight patterns.
//!
//! Mirrors the VellumFE workflow: regex pattern entry, optional colors/sounds,
//! and checkbox flags for rendering behavior.

use crate::frontend::tui::crossterm_bridge;
use crate::config::{Config, HighlightPattern};
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
const POPUP_HEIGHT: u16 = 21;

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
    },
    Delete {
        name: String,
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

    // Redirect dropdown (Off=0, Only=1, Copy=2)
    redirect_mode_index: usize,

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
            squelch: false,
            silent_prompt: false,
            focused_field: 0,
            status_message: "Ready".to_string(),
            pattern_error: None,
            mode: FormMode::Create,
            sound_files: Self::load_sound_files(),
            sound_file_index: 0, // Default to "none"
            redirect_mode_index: 0, // Default to "Off"
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

        if let Some(ref replace) = pattern.replace {
            form.replace = TextArea::from([replace.clone()]);
            form.replace.set_cursor_line_style(Style::default());
        }

        form.bold = pattern.bold;
        form.color_entire_line = pattern.color_entire_line;
        form.fast_parse = pattern.fast_parse;
        form.squelch = pattern.squelch;
        form.silent_prompt = pattern.silent_prompt;

        // Load redirect settings
        if let Some(ref redirect_stream) = pattern.redirect_to {
            form.redirect_to = TextArea::from([redirect_stream.clone()]);
            form.redirect_to.set_cursor_line_style(Style::default());
        }

        // Set redirect mode index (0=Off, 1=Only, 2=Copy)
        form.redirect_mode_index = if pattern.redirect_to.is_none() {
            0 // Off
        } else {
            match pattern.redirect_mode {
                crate::config::RedirectMode::RedirectOnly => 1,
                crate::config::RedirectMode::RedirectCopy => 2,
            }
        };

        form.status_message = "Editing highlight".to_string();
        form
    }

    /// Alias for new_edit - create form in Edit mode with existing highlight
    pub fn with_pattern(name: String, pattern: HighlightPattern) -> Self {
        Self::new_edit(name, &pattern)
    }

    /// Move focus to next field
    pub fn focus_next(&mut self) {
        self.focused_field = (self.focused_field + 1) % 15; // 0-14
    }

    /// Move focus to previous field
    pub fn focus_prev(&mut self) {
        self.focused_field = if self.focused_field == 0 {
            14
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
    pub fn handle_action(&mut self, action: crate::core::menu_actions::MenuAction) -> Option<FormResult> {
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
                    13 => self.squelch = !self.squelch,
                    14 => self.silent_prompt = !self.silent_prompt,
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
            _ => None
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
            squelch: self.squelch,
            silent_prompt: self.silent_prompt,
            sound,
            sound_volume,
            redirect_to,
            redirect_mode,
            replace,
            stream: None, // TODO: Add UI for stream filtering
            window: None, // TODO: Add UI for window filtering
            compiled_regex: None, // Will be compiled when config is loaded
        };

        Some(FormResult::Save {
            name: name.to_string(),
            pattern,
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
                    buf[(x + col, y + row)].set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
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
        for (i, ch) in title.chars().enumerate() {
            if (x + 1 + i as u16) < (x + width) {
                buf[(x + 1 + i as u16, y)]
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
            if footer_x + i as u16 >= x + width {
                break;
            }
            buf[(footer_x + i as u16, footer_y)]
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
        let border_style = Style::default().fg(crossterm_bridge::to_ratatui_color(theme.form_label));

        // Top border
        buf[(x, y)].set_char('┌').set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y)].set_char('─').set_style(border_style);
        }
        buf[(x + width - 1, y)]
            .set_char('┐')
            .set_style(border_style);

        // Side borders
        for row in 1..height - 1 {
            buf[(x, y + row)].set_char('│').set_style(border_style);
            buf[(x + width - 1, y + row)]
                .set_char('│')
                .set_style(border_style);
        }

        // Bottom border
        buf[(x, y + height - 1)]
            .set_char('└')
            .set_style(border_style);
        for col in 1..width - 1 {
            buf[(x + col, y + height - 1)]
                .set_char('─')
                .set_style(border_style);
        }
        buf[(x + width - 1, y + height - 1)]
            .set_char('┘')
            .set_style(border_style);
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
            // Color preview
            if input_width >= 2 {
                let preview_x = input_start + input_width.saturating_sub(2);
                buf[(preview_x, current_y)]
                    .set_char(' ')
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                if !fg_text.is_empty() {
                    if let Some(color) = self.parse_and_resolve_color(&fg_text, config) {
                        buf[(preview_x, current_y)]
                            .set_char(' ')
                            .set_bg(color);
                        if preview_x + 1 < x + width - 1 {
                            buf[(preview_x + 1, current_y)]
                                .set_char(' ')
                                .set_bg(color);
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
            // Color preview
            if input_width >= 2 {
                let preview_x = input_start + input_width.saturating_sub(2);
                buf[(preview_x, current_y)]
                    .set_char(' ')
                    .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
                if !bg_text.is_empty() {
                    if let Some(color) = self.parse_and_resolve_color(&bg_text, config) {
                        buf[(preview_x, current_y)]
                            .set_char(' ')
                            .set_bg(color);
                        if preview_x + 1 < x + width - 1 {
                            buf[(preview_x + 1, current_y)]
                                .set_char(' ')
                                .set_bg(color);
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
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 10 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.bold { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 10 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 10 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let bold_label = " Bold";
        for (i, ch) in bold_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 10 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
        current_y += 1;

        buf[(x + 2, current_y)]
            .set_char('[')
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 11 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.color_entire_line { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 11 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 11 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let cel_label = " Color entire line";
        for (i, ch) in cel_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 11 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
        current_y += 1;

        buf[(x + 2, current_y)]
            .set_char('[')
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 12 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.fast_parse { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 12 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 12 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let fp_label = " Fast parse";
        for (i, ch) in fp_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 12 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        current_y += 1;

        // Field 10: Squelch checkbox
        buf[(x + 2, current_y)]
            .set_char('[')
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 13 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.squelch { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 13 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 13 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let squelch_label = " Squelch (ignore line)";
        for (i, ch) in squelch_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 13 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }

        current_y += 1;

        // Field 14: Silent Prompt checkbox
        buf[(x + 2, current_y)]
            .set_char('[')
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 14 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 3, current_y)]
            .set_char(if self.silent_prompt { '✓' } else { ' ' })
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 14 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        buf[(x + 4, current_y)]
            .set_char(']')
            .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 14 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
            .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        let silent_label = " Silent Prompt (suppress prompt)";
        for (i, ch) in silent_label.chars().enumerate() {
            buf[(x + 5 + i as u16, current_y)]
                .set_char(ch)
                .set_fg(crossterm_bridge::to_ratatui_color(if self.focused_field == 14 {
                theme.form_label_focused
            } else {
                theme.form_label
            }))
                .set_bg(crossterm_bridge::to_ratatui_color(theme.browser_background));
        }
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
        textarea.set_style(Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_primary)).bg(bg));

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
        textarea.set_style(Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_primary)).bg(bg));

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
        let label_color = crossterm_bridge::to_ratatui_color(if focused  {
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
        for (i, ch) in current_value.chars().enumerate().take(available_width as usize) {
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
        let label_color = crossterm_bridge::to_ratatui_color(if focused  {
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

        // Get current value from dropdown index (0=Off, 1=Only, 2=Copy)
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

    /// Handle mouse events for dragging
    pub fn handle_mouse(&mut self, col: u16, row: u16, pressed: bool, terminal_area: Rect) -> bool {
        let popup_width = POPUP_WIDTH;
        let popup_height = POPUP_HEIGHT;

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: popup_width.min(terminal_area.width.saturating_sub(self.popup_x)),
            height: popup_height.min(terminal_area.height.saturating_sub(self.popup_y)),
        };

        // Check if click is on title bar (top border, excluding corners)
        let on_title_bar =
            row == popup_area.y && col > popup_area.x && col < popup_area.x + popup_area.width - 1;

        if pressed {
            if on_title_bar && !self.is_dragging {
                // Start dragging
                self.is_dragging = true;
                self.drag_offset_x = col.saturating_sub(self.popup_x);
                self.drag_offset_y = row.saturating_sub(self.popup_y);
                return true;
            } else if self.is_dragging {
                // Continue dragging
                let new_x = col.saturating_sub(self.drag_offset_x);
                let new_y = row.saturating_sub(self.drag_offset_y);

                // Clamp to terminal bounds
                self.popup_x = new_x.min(terminal_area.width.saturating_sub(popup_width));
                self.popup_y = new_y.min(terminal_area.height.saturating_sub(popup_height));
                return true;
            }
        } else {
            // Mouse released
            if self.is_dragging {
                self.is_dragging = false;
                return true;
            }
        }

        false
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
        if self.focused_field == 5 && !self.sound_files.is_empty()
            && self.sound_file_index + 1 < self.sound_files.len() {
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
