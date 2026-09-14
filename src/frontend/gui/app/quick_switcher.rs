//! Quick switcher (`.switcher`, default Ctrl+K): one box that filters
//! windows, dot-commands, saved layouts and settings sections by name.
//! Enter runs the highlighted row; Esc closes. Hidden windows are listed
//! too, so it doubles as "where did that window go".

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum SwitcherAction {
    /// Show (if hidden) and raise a window by name.
    Window(String),
    /// Send a dot-command as typed.
    Command(String),
    /// `.loadlayout <name> --keep-skin`.
    Layout(String),
    /// Open the Settings window at a section.
    Settings(String),
}

#[derive(Clone, Debug)]
pub(super) struct SwitcherItem {
    pub title: String,
    pub detail: String,
    pub kind: &'static str,
    pub action: SwitcherAction,
}

pub(super) struct QuickSwitcherState {
    pub query: String,
    pub selected: usize,
    pub needs_focus: bool,
}

/// Case-insensitive match score: exact prefix beats substring beats a
/// subsequence; `None` when the query does not match at all. Ties broken
/// by the caller on title length.
pub(super) fn match_score(query: &str, text: &str) -> Option<u32> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let text = text.to_ascii_lowercase();
    if text.starts_with(&query) {
        return Some(300);
    }
    if let Some(at) = text.find(&query) {
        // A match at a word start reads as intended more often.
        let word_start = at == 0 || !text.as_bytes()[at - 1].is_ascii_alphanumeric();
        return Some(if word_start { 220 } else { 200 });
    }
    let mut chars = text.chars();
    let mut gaps = 0u32;
    for needle in query.chars() {
        let mut found = false;
        for hay in chars.by_ref() {
            if hay == needle {
                found = true;
                break;
            }
            gaps += 1;
        }
        if !found {
            return None;
        }
    }
    Some(100u32.saturating_sub(gaps.min(90)))
}

impl VellumGuiApp {
    pub(super) fn open_quick_switcher(&mut self) {
        self.quick_switcher = Some(QuickSwitcherState {
            query: String::new(),
            selected: 0,
            needs_focus: true,
        });
    }

    fn window_shown_in_core(&self, name: &str) -> bool {
        self.app_core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == name)
            .map(|w| w.base().visibility == crate::config::WindowVisibility::Shown)
            .unwrap_or(true)
    }

    /// Everything the switcher can reach, unfiltered.
    fn switcher_catalogue(&self) -> Vec<SwitcherItem> {
        let mut items = Vec::new();
        let mut windows: Vec<(&TabKey, &GuiTab)> = self.available_tabs.iter().collect();
        windows.sort_by(|a, b| a.1.id.title.cmp(&b.1.id.title));
        for (key, tab) in windows {
            let hidden = self.hidden_tabs.contains(key)
                || !self.window_shown_in_core(&tab.window_name);
            items.push(SwitcherItem {
                title: tab.id.title.clone(),
                detail: if hidden {
                    format!("{} · hidden — show and raise", tab.window_name)
                } else {
                    format!("{} · raise", tab.window_name)
                },
                kind: "window",
                action: SwitcherAction::Window(tab.window_name.clone()),
            });
        }
        for section in crate::core::app_core::command_help::COMMAND_HELP {
            for entry in section.entries {
                let name = entry.names[0];
                items.push(SwitcherItem {
                    title: format!(".{name}"),
                    detail: entry.blurb.to_string(),
                    kind: "command",
                    action: SwitcherAction::Command(format!(".{name}")),
                });
            }
        }
        for name in list_named_layouts() {
            items.push(SwitcherItem {
                title: name.clone(),
                detail: "saved layout · load, keeping your skin".to_string(),
                kind: "layout",
                action: SwitcherAction::Layout(name),
            });
        }
        for section in super::editors::settings_sections() {
            items.push(SwitcherItem {
                title: section.to_string(),
                detail: "settings section".to_string(),
                kind: "settings",
                action: SwitcherAction::Settings(section.to_string()),
            });
        }
        items
    }

    fn switcher_matches(&self, query: &str) -> Vec<SwitcherItem> {
        let mut scored: Vec<(u32, SwitcherItem)> = self
            .switcher_catalogue()
            .into_iter()
            .filter_map(|item| {
                let title = match_score(query, &item.title);
                // A detail hit ranks below any title hit.
                let detail = match_score(query, &item.detail).map(|s| s / 4);
                let score = match (title, detail) {
                    (Some(t), _) => t,
                    (None, Some(d)) if !query.trim().is_empty() => d,
                    _ => return None,
                };
                // Windows first when the query is empty so the box opens
                // as a window list.
                let kind_bonus = match item.kind {
                    "window" => 3,
                    "layout" => 2,
                    "settings" => 1,
                    _ => 0,
                };
                Some((score * 10 + kind_bonus, item))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.title.len().cmp(&b.1.title.len())));
        scored.into_iter().map(|(_, item)| item).take(14).collect()
    }

    pub(super) fn render_quick_switcher(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.quick_switcher.take() else {
            return;
        };
        let matches = self.switcher_matches(&state.query);
        if state.selected >= matches.len() {
            state.selected = matches.len().saturating_sub(1);
        }
        let (esc, enter, up, down) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
            )
        });
        if esc {
            return;
        }
        if down && !matches.is_empty() {
            state.selected = (state.selected + 1) % matches.len();
        }
        if up && !matches.is_empty() {
            state.selected = (state.selected + matches.len() - 1) % matches.len();
        }
        let mut chosen: Option<SwitcherAction> = None;
        if enter {
            chosen = matches.get(state.selected).map(|m| m.action.clone());
        }
        let screen = ctx.content_rect();
        let width = (screen.width() * 0.5).clamp(320.0, 640.0);
        let mut clicked_outside = false;
        let response = egui::Area::new(egui::Id::new("gui_quick_switcher"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, screen.height() * 0.12))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        let edit = egui::TextEdit::singleline(&mut state.query)
                            .hint_text("Window, command, layout or setting…")
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Heading);
                        let resp = ui.add(edit);
                        if state.needs_focus {
                            resp.request_focus();
                            state.needs_focus = false;
                        }
                        if resp.changed() {
                            state.selected = 0;
                        }
                        ui.add_space(6.0);
                        if matches.is_empty() {
                            ui.weak("No matches");
                        }
                        for (index, item) in matches.iter().enumerate() {
                            let selected = index == state.selected;
                            let row = ui
                                .horizontal(|ui| {
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 30.0),
                                        egui::Sense::click(),
                                    );
                                    if selected || resp.hovered() {
                                        let fill = if selected {
                                            ui.visuals().selection.bg_fill
                                        } else {
                                            ui.visuals().widgets.hovered.bg_fill
                                        };
                                        ui.painter().rect_filled(rect, 4.0, fill);
                                    }
                                    let painter = ui.painter();
                                    let font = egui::TextStyle::Body.resolve(ui.style());
                                    let small = egui::TextStyle::Small.resolve(ui.style());
                                    let text_color = ui.visuals().strong_text_color();
                                    let weak = ui.visuals().weak_text_color();
                                    painter.text(
                                        rect.left_center() + Vec2::new(8.0, -7.0),
                                        egui::Align2::LEFT_CENTER,
                                        &item.title,
                                        font,
                                        text_color,
                                    );
                                    painter.text(
                                        rect.left_center() + Vec2::new(8.0, 8.0),
                                        egui::Align2::LEFT_CENTER,
                                        &item.detail,
                                        small.clone(),
                                        weak,
                                    );
                                    painter.text(
                                        rect.right_center() - Vec2::new(8.0, 0.0),
                                        egui::Align2::RIGHT_CENTER,
                                        item.kind,
                                        small,
                                        weak,
                                    );
                                    resp
                                })
                                .inner;
                            if row.clicked() {
                                chosen = Some(item.action.clone());
                            }
                        }
                        ui.add_space(4.0);
                        ui.weak("↑↓ choose · Enter run · Esc close");
                    });
            });
        if ctx.input(|i| i.pointer.any_click())
            && !response.response.contains_pointer()
        {
            clicked_outside = true;
        }
        match chosen {
            Some(action) => self.run_switcher_action(action),
            None if !clicked_outside => self.quick_switcher = Some(state),
            None => {}
        }
    }

    fn run_switcher_action(&mut self, action: SwitcherAction) {
        match action {
            SwitcherAction::Window(name) => {
                let (w, h) = self.core_layout_size;
                if !self.window_shown_in_core(&name) {
                    self.app_core.set_known_window_shown(&name, true, w, h);
                }
                if let Some(key) = self.find_tab_key_by_name(&name) {
                    self.hidden_tabs.remove(&key);
                    self.pending_raise_tab = Some(key);
                }
                self.layout_dirty = true;
            }
            SwitcherAction::Command(command) => self.dispatch_command(command),
            SwitcherAction::Layout(name) => {
                self.dispatch_command(format!(".loadlayout {name} --keep-skin"))
            }
            SwitcherAction::Settings(section) => self.open_settings_editor_at(&section),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::match_score;

    #[test]
    fn prefix_beats_substring_beats_subsequence() {
        let prefix = match_score("sto", "story").unwrap();
        let sub = match_score("tor", "story").unwrap();
        let subseq = match_score("sry", "story").unwrap();
        assert!(prefix > sub && sub > subseq);
        assert_eq!(match_score("xyz", "story"), None);
        assert_eq!(match_score("", "anything"), Some(0));
        assert!(match_score("Lay", "loadlayout").is_some(), "case-insensitive");
    }
}
