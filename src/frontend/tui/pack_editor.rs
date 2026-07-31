//! TUI pack editor (.packs): guided export/import of `.vellumpack` files.
//!
//! A centered two-tab form. Export: pack name, optional destination
//! folder, part checkboxes. Import: cycle through packs dropped into
//! `~/.vellum-fe/imports/` (or type a path), preview what the pack
//! carries, pick parts, install. Actions are returned to the input
//! layer as [`PackEditorAction`]s; AppCore does the actual work through
//! the same `.uiexport`/`.uiimport` core paths.

use crate::core::uipack;
use crate::frontend::tui::crossterm_bridge::to_ratatui_color;
use crate::theme::AppTheme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PackTab {
    Export,
    Import,
}

/// What the input layer should do after a keypress.
pub enum PackEditorAction {
    None,
    Close,
    Export {
        name: String,
        parts: Vec<String>,
        dest: Option<PathBuf>,
    },
    Install {
        path: PathBuf,
        parts: Vec<String>,
    },
}

/// One focusable row of the active tab.
enum Row {
    NameField,
    DestField,
    PathField,
    PackPicker,
    Part(usize),
    ActionButton,
}

pub struct PackEditorWidget {
    tab: PackTab,
    focused: usize,
    // Export
    name: String,
    dest: String,
    export_parts: Vec<(&'static str, bool)>,
    // Import
    import_choices: Vec<String>,
    import_choice: Option<usize>,
    import_path: String,
    preview_for: Option<PathBuf>,
    preview_summary: Option<String>,
    preview_error: Option<String>,
    install_parts: Vec<(String, bool)>,
    base: PathBuf,
    status: String,
}

impl PackEditorWidget {
    pub fn new(base: PathBuf) -> Self {
        let import_choices = uipack::list_import_packs(&base);
        Self {
            tab: PackTab::Export,
            focused: 0,
            name: String::new(),
            dest: String::new(),
            export_parts: uipack::PARTS.iter().map(|p| (*p, true)).collect(),
            import_choices,
            import_choice: None,
            import_path: String::new(),
            preview_for: None,
            preview_summary: None,
            preview_error: None,
            install_parts: Vec::new(),
            base,
            status: String::new(),
        }
    }

    fn rows(&self) -> Vec<Row> {
        match self.tab {
            PackTab::Export => {
                let mut rows = vec![Row::NameField, Row::DestField];
                rows.extend((0..self.export_parts.len()).map(Row::Part));
                rows.push(Row::ActionButton);
                rows
            }
            PackTab::Import => {
                let mut rows = vec![Row::PackPicker, Row::PathField];
                rows.extend((0..self.install_parts.len()).map(Row::Part));
                rows.push(Row::ActionButton);
                rows
            }
        }
    }

    fn import_target(&self) -> Option<PathBuf> {
        let manual = self.import_path.trim();
        if !manual.is_empty() {
            return Some(PathBuf::from(manual));
        }
        self.import_choice
            .and_then(|i| self.import_choices.get(i))
            .map(|name| self.base.join("imports").join(format!("{name}.vellumpack")))
    }

    fn refresh_preview(&mut self) {
        let target = self.import_target();
        if target == self.preview_for {
            return;
        }
        self.preview_for = target.clone();
        self.preview_summary = None;
        self.preview_error = None;
        self.install_parts.clear();
        if let Some(path) = &target {
            match uipack::preview(path) {
                Ok(preview) => {
                    let mut summary = format!(
                        "VellumFE {} · {} file(s)",
                        preview.manifest.version,
                        preview.entries.len()
                    );
                    if let Some(skin) = &preview.manifest.skin {
                        summary.push_str(&format!(" · skin '{skin}'"));
                    }
                    if let Some(theme) = &preview.manifest.theme {
                        summary.push_str(&format!(" · theme '{theme}'"));
                    }
                    self.install_parts = preview
                        .manifest
                        .parts
                        .iter()
                        .map(|p| (p.clone(), true))
                        .collect();
                    self.preview_summary = Some(summary);
                }
                Err(err) => self.preview_error = Some(format!("{err:#}")),
            }
        }
    }

    /// Handle a key; printable chars edit the focused text field.
    pub fn handle_key(
        &mut self,
        code: crate::data::input::KeyCode,
        _modifiers: crate::data::input::KeyModifiers,
    ) -> PackEditorAction {
        use crate::data::input::KeyCode;
        let rows = self.rows();
        let row = rows.get(self.focused);
        match code {
            KeyCode::Esc => return PackEditorAction::Close,
            KeyCode::Tab => {
                self.tab = match self.tab {
                    PackTab::Export => PackTab::Import,
                    PackTab::Import => PackTab::Export,
                };
                self.focused = 0;
                self.status.clear();
                if self.tab == PackTab::Import {
                    self.refresh_preview();
                }
            }
            KeyCode::Up => self.focused = self.focused.saturating_sub(1),
            KeyCode::Down => {
                self.focused = (self.focused + 1).min(rows.len().saturating_sub(1))
            }
            KeyCode::Left | KeyCode::Right => {
                if matches!(row, Some(Row::PackPicker)) && !self.import_choices.is_empty() {
                    let len = self.import_choices.len();
                    let current = self.import_choice.unwrap_or(0);
                    let next = match code {
                        KeyCode::Left => (current + len - 1) % len,
                        _ => match self.import_choice {
                            None => 0,
                            Some(i) => (i + 1) % len,
                        },
                    };
                    self.import_choice = Some(next);
                    self.import_path.clear();
                    self.refresh_preview();
                }
            }
            KeyCode::Enter | KeyCode::Char(' ')
                if matches!(row, Some(Row::Part(_)) | Some(Row::ActionButton))
                    // Space still types into text fields.
                    || matches!(code, KeyCode::Enter) =>
            {
                match row {
                    Some(Row::Part(i)) => {
                        let i = *i;
                        match self.tab {
                            PackTab::Export => {
                                if let Some((_, on)) = self.export_parts.get_mut(i) {
                                    *on = !*on;
                                }
                            }
                            PackTab::Import => {
                                if let Some((_, on)) = self.install_parts.get_mut(i) {
                                    *on = !*on;
                                }
                            }
                        }
                    }
                    Some(Row::ActionButton) => return self.activate(),
                    Some(Row::PackPicker) => {
                        // Enter on the picker rescans imports/.
                        self.import_choices = uipack::list_import_packs(&self.base);
                        self.import_choice = None;
                        self.refresh_preview();
                        self.status = format!(
                            "{} pack(s) in imports/",
                            self.import_choices.len()
                        );
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => match row {
                Some(Row::NameField) => {
                    self.name.pop();
                }
                Some(Row::DestField) => {
                    self.dest.pop();
                }
                Some(Row::PathField) => {
                    self.import_path.pop();
                    self.refresh_preview();
                }
                _ => {}
            },
            KeyCode::Char(c) => match row {
                Some(Row::NameField) => self.name.push(c),
                Some(Row::DestField) => self.dest.push(c),
                Some(Row::PathField) => {
                    self.import_path.push(c);
                    self.refresh_preview();
                }
                _ => {}
            },
            _ => {}
        }
        PackEditorAction::None
    }

    fn activate(&mut self) -> PackEditorAction {
        match self.tab {
            PackTab::Export => {
                let name = self.name.trim().to_string();
                if !uipack::is_valid_pack_name(&name) {
                    self.status =
                        "Names use letters, digits, '-' and '_' only.".to_string();
                    return PackEditorAction::None;
                }
                let parts: Vec<String> = self
                    .export_parts
                    .iter()
                    .filter(|(_, on)| *on)
                    .map(|(p, _)| p.to_string())
                    .collect();
                if parts.is_empty() {
                    self.status = "Select at least one part.".to_string();
                    return PackEditorAction::None;
                }
                let dest = {
                    let dest = self.dest.trim();
                    (!dest.is_empty()).then(|| PathBuf::from(dest))
                };
                PackEditorAction::Export { name, parts, dest }
            }
            PackTab::Import => {
                let Some(path) = self.import_target() else {
                    self.status = "Pick a pack or type a path.".to_string();
                    return PackEditorAction::None;
                };
                let parts: Vec<String> = self
                    .install_parts
                    .iter()
                    .filter(|(_, on)| *on)
                    .map(|(p, _)| p.clone())
                    .collect();
                if parts.is_empty() {
                    self.status = "Select at least one part.".to_string();
                    return PackEditorAction::None;
                }
                PackEditorAction::Install { path, parts }
            }
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, theme: &AppTheme) {
        if self.tab == PackTab::Import {
            self.refresh_preview();
        }
        let width = 62.min(area.width.saturating_sub(2)).max(30);
        let height = 24.min(area.height.saturating_sub(2)).max(12);
        let popup = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        Clear.render(popup, buf);

        let border = Style::default().fg(to_ratatui_color(theme.form_border));
        let label = Style::default().fg(to_ratatui_color(theme.form_label));
        let dim = Style::default().fg(to_ratatui_color(theme.text_secondary));
        let focus = Style::default()
            .fg(to_ratatui_color(theme.form_label_focused))
            .add_modifier(Modifier::REVERSED);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(" UI Packs (.packs) ");
        let inner = block.inner(popup);
        block.render(popup, buf);

        let rows = self.rows();
        let mut lines: Vec<Line> = Vec::new();

        // Tab bar
        let tab_style = |on: bool| if on { focus } else { dim };
        lines.push(Line::from(vec![
            Span::styled(" Export ", tab_style(self.tab == PackTab::Export)),
            Span::raw("  "),
            Span::styled(" Import ", tab_style(self.tab == PackTab::Import)),
            Span::styled("   (Tab switches)", dim),
        ]));
        lines.push(Line::from(""));

        let field = |focused: bool, name: &str, value: &str, hint: &str| {
            let shown = if value.is_empty() { hint } else { value };
            Line::from(vec![
                Span::styled(format!("{name:<9}"), label),
                Span::styled(
                    format!("[{shown}]"),
                    if focused { focus } else { dim },
                ),
            ])
        };

        for (i, row) in rows.iter().enumerate() {
            let is_focused = i == self.focused;
            match row {
                Row::NameField => lines.push(field(is_focused, "Name", &self.name, "my-ui")),
                Row::DestField => lines.push(field(
                    is_focused,
                    "Save to",
                    &self.dest,
                    "default: exports/",
                )),
                Row::PathField => lines.push(field(
                    is_focused,
                    "Path",
                    &self.import_path,
                    "or type a file path",
                )),
                Row::PackPicker => {
                    let shown = self
                        .import_choice
                        .and_then(|i| self.import_choices.get(i))
                        .cloned()
                        .unwrap_or_else(|| {
                            if self.import_choices.is_empty() {
                                "imports/ is empty".to_string()
                            } else {
                                "←/→ to choose".to_string()
                            }
                        });
                    lines.push(Line::from(vec![
                        Span::styled(format!("{:<9}", "Pack"), label),
                        Span::styled(
                            format!("< {shown} >"),
                            if is_focused { focus } else { dim },
                        ),
                        Span::styled("  (Enter rescans)", dim),
                    ]));
                }
                Row::Part(p) => {
                    let (id, on) = match self.tab {
                        PackTab::Export => {
                            let (id, on) = self.export_parts[*p];
                            (id.to_string(), on)
                        }
                        PackTab::Import => self.install_parts[*p].clone(),
                    };
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  [{}] {}",
                            if on { "x" } else { " " },
                            uipack::part_label(&id)
                        ),
                        if is_focused { focus } else { label },
                    )));
                }
                Row::ActionButton => {
                    lines.push(Line::from(""));
                    let text = match self.tab {
                        PackTab::Export => "[ Export pack ]",
                        PackTab::Import => "[ Install selected ]",
                    };
                    lines.push(Line::from(Span::styled(
                        text,
                        if is_focused { focus } else { label },
                    )));
                }
            }
        }

        if self.tab == PackTab::Import {
            lines.push(Line::from(""));
            if let Some(err) = &self.preview_error {
                lines.push(Line::from(Span::styled(err.clone(), dim)));
            } else if let Some(summary) = &self.preview_summary {
                lines.push(Line::from(Span::styled(summary.clone(), dim)));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("Drop packs into {}", self.base.join("imports").display()),
                    dim,
                )));
            }
        }
        if !self.status.is_empty() {
            lines.push(Line::from(Span::styled(self.status.clone(), label)));
        }
        lines.push(Line::from(Span::styled(
            "↑/↓ move · Enter/Space toggle · Esc close",
            dim,
        )));

        Paragraph::new(lines).render(inner, buf);
    }
}
