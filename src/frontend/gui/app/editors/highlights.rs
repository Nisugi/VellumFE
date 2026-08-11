//! Highlight browser + form: add/edit/delete highlight patterns through the
//! shared config layer (`Config::save_single_highlight` /
//! `delete_single_highlight`), hot-reloading the highlight engine via
//! `AppCore::reload_highlights` after every change.

use super::super::{theme, VellumGuiApp};
use crate::config::{Config, HighlightPattern, RedirectMode};
use eframe::egui;

pub(in super::super) struct HighlightEditorState {
    filter: String,
    form: Option<HighlightFormState>,
}

impl HighlightEditorState {
    fn new() -> Self {
        Self {
            filter: String::new(),
            form: None,
        }
    }
}

struct HighlightFormState {
    /// Some(name) when editing an existing highlight; None when adding.
    original_name: Option<String>,
    /// Scope the original lives in (delete-from scope on rename).
    original_is_global: bool,
    name: String,
    pattern: String,
    fg: String,
    bg: String,
    bold: bool,
    color_entire_line: bool,
    fast_parse: bool,
    sound: String,
    sound_volume: String,
    rumble: String,
    category: String,
    squelch: bool,
    silent_prompt: bool,
    redirect_to: String,
    redirect_copy: bool,
    replace: String,
    stream: String,
    window: String,
    set_status: String,
    status_duration: String,
    clear_status: String,

    // ---- Overlay alert ------------------------------------------------
    // Flattened into strings like every other field so the form stays one
    // uniform editing surface; `build_alert` reassembles the nested
    // AlertSpec on save and yields None when nothing was filled in.
    alert_banner: String,
    alert_banner_fg: String,
    alert_banner_bg: String,
    alert_art: String,
    alert_flash: String,
    alert_anchor: crate::config::AlertAnchor,
    alert_offset_x: String,
    alert_offset_y: String,
    alert_duration: String,
    alert_cooldown: String,
    /// Authored alert id, preserved so pack updates and cooldown state keep a
    /// stable identity across pattern edits.
    alert_id: String,
    /// Priority is carried but has no editor control yet (eviction is
    /// oldest-first in v1); kept so saving never drops an authored value.
    alert_priority: Option<i32>,
    /// Condition gate for condition-driven alerts. Carried through untouched:
    /// authoring a Condition tree needs the shared condition editor, which
    /// this form does not host yet. Preserving it is non-negotiable — a rule
    /// silently losing its `when` on an unrelated edit would turn a
    /// condition alert into one that never fires again.
    alert_when: Option<crate::config::Condition>,
    /// Re-arm seconds for the condition gate; edited alongside `when` when
    /// that UI lands, preserved until then.
    alert_rearm: Option<f32>,

    is_global: bool,
    error: Option<String>,
}

/// The nine screen anchors, in reading order so the picker matches the
/// mental image of a 3x3 grid.
const ALERT_ANCHORS: [crate::config::AlertAnchor; 9] = {
    use crate::config::AlertAnchor::*;
    [
        TopLeft,
        TopCenter,
        TopRight,
        CenterLeft,
        Center,
        CenterRight,
        BottomLeft,
        BottomCenter,
        BottomRight,
    ]
};

fn anchor_label(anchor: crate::config::AlertAnchor) -> &'static str {
    use crate::config::AlertAnchor::*;
    match anchor {
        TopLeft => "Top left",
        TopCenter => "Top center",
        TopRight => "Top right",
        CenterLeft => "Center left",
        Center => "Center",
        CenterRight => "Center right",
        BottomLeft => "Bottom left",
        BottomCenter => "Bottom center",
        BottomRight => "Bottom right",
    }
}

/// Pull one optional string out of a pattern's alert, or "" when the pattern
/// has no alert at all. Keeps `from_pattern` readable rather than repeating
/// the same `as_ref().and_then(...).unwrap_or_default()` chain a dozen times.
fn alert_str<F>(pattern: &HighlightPattern, get: F) -> String
where
    F: Fn(&crate::config::AlertSpec) -> Option<String>,
{
    pattern
        .alert
        .as_ref()
        .and_then(get)
        .unwrap_or_default()
}

impl HighlightFormState {
    /// Reassemble the nested AlertSpec from the flat form fields. Returns
    /// `None` when no presentation was specified — an alert that shows
    /// nothing would silently burn a concurrent slot at runtime, so "empty
    /// form" must mean "no alert" rather than "invisible alert".
    fn build_alert(&self) -> Option<crate::config::AlertSpec> {
        let opt = |s: &str| {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let banner = opt(&self.alert_banner);
        let art = opt(&self.alert_art);
        let flash = opt(&self.alert_flash);
        // No presentation AND no condition gate means the user authored no
        // alert at all. But a rule carrying a `when` must survive even with
        // its presentation cleared: dropping the spec here would silently
        // discard a condition gate this form cannot yet re-author, turning a
        // working condition alert into one that never fires again.
        if banner.is_none()
            && art.is_none()
            && flash.is_none()
            && self.alert_when.is_none()
        {
            return None;
        }

        // An offset counts if EITHER axis was given; the other defaults to 0
        // so "nudge it down 20px" doesn't require typing a zero for x.
        let x = self.alert_offset_x.trim().parse::<f32>().ok();
        let y = self.alert_offset_y.trim().parse::<f32>().ok();
        let offset = (x.is_some() || y.is_some())
            .then(|| (x.unwrap_or(0.0), y.unwrap_or(0.0)));

        Some(crate::config::AlertSpec {
            id: opt(&self.alert_id),
            banner,
            banner_fg: opt(&self.alert_banner_fg),
            banner_bg: opt(&self.alert_banner_bg),
            art,
            flash,
            anchor: self.alert_anchor,
            offset,
            duration: self
                .alert_duration
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|v| *v > 0.0),
            cooldown: self
                .alert_cooldown
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|v| *v >= 0.0),
            priority: self.alert_priority,
            when: self.alert_when.clone(),
            rearm: self.alert_rearm,
        })
    }

    fn empty() -> Self {
        Self {
            original_name: None,
            original_is_global: true,
            name: String::new(),
            pattern: String::new(),
            fg: String::new(),
            bg: String::new(),
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            sound: String::new(),
            sound_volume: String::new(),
            rumble: String::new(),
            category: String::new(),
            squelch: false,
            silent_prompt: false,
            redirect_to: String::new(),
            redirect_copy: false,
            replace: String::new(),
            stream: String::new(),
            window: String::new(),
            set_status: String::new(),
            status_duration: String::new(),
            clear_status: String::new(),
            alert_banner: String::new(),
            alert_banner_fg: String::new(),
            alert_banner_bg: String::new(),
            alert_art: String::new(),
            alert_flash: String::new(),
            alert_anchor: crate::config::AlertAnchor::default(),
            alert_offset_x: String::new(),
            alert_offset_y: String::new(),
            alert_duration: String::new(),
            alert_cooldown: String::new(),
            alert_id: String::new(),
            alert_priority: None,
            alert_when: None,
            alert_rearm: None,
            is_global: true,
            error: None,
        }
    }

    fn from_pattern(name: &str, pattern: &HighlightPattern, is_global: bool) -> Self {
        Self {
            original_name: Some(name.to_string()),
            original_is_global: is_global,
            name: name.to_string(),
            pattern: pattern.pattern.clone(),
            fg: pattern.fg.clone().unwrap_or_default(),
            bg: pattern.bg.clone().unwrap_or_default(),
            bold: pattern.bold,
            color_entire_line: pattern.color_entire_line,
            fast_parse: pattern.fast_parse,
            sound: pattern.sound.clone().unwrap_or_default(),
            sound_volume: pattern
                .sound_volume
                .map(|volume| volume.to_string())
                .unwrap_or_default(),
            rumble: pattern.rumble.clone().unwrap_or_default(),
            category: pattern.category.clone().unwrap_or_default(),
            squelch: pattern.squelch,
            silent_prompt: pattern.silent_prompt,
            redirect_to: pattern.redirect_to.clone().unwrap_or_default(),
            redirect_copy: pattern.redirect_mode == RedirectMode::RedirectCopy,
            replace: pattern.replace.clone().unwrap_or_default(),
            stream: pattern.stream.clone().unwrap_or_default(),
            window: pattern.window.clone().unwrap_or_default(),
            set_status: pattern.set_status.clone().unwrap_or_default(),
            status_duration: pattern
                .status_duration
                .map(|secs| secs.to_string())
                .unwrap_or_default(),
            clear_status: pattern.clear_status.clone().unwrap_or_default(),
            alert_banner: alert_str(pattern, |a| a.banner.clone()),
            alert_banner_fg: alert_str(pattern, |a| a.banner_fg.clone()),
            alert_banner_bg: alert_str(pattern, |a| a.banner_bg.clone()),
            alert_art: alert_str(pattern, |a| a.art.clone()),
            alert_flash: alert_str(pattern, |a| a.flash.clone()),
            alert_anchor: pattern
                .alert
                .as_ref()
                .map(|a| a.anchor)
                .unwrap_or_default(),
            alert_offset_x: alert_str(pattern, |a| a.offset.map(|(x, _)| x.to_string())),
            alert_offset_y: alert_str(pattern, |a| a.offset.map(|(_, y)| y.to_string())),
            alert_duration: alert_str(pattern, |a| a.duration.map(|v| v.to_string())),
            alert_cooldown: alert_str(pattern, |a| a.cooldown.map(|v| v.to_string())),
            alert_id: alert_str(pattern, |a| a.id.clone()),
            alert_priority: pattern.alert.as_ref().and_then(|a| a.priority),
            alert_when: pattern.alert.as_ref().and_then(|a| a.when.clone()),
            alert_rearm: pattern.alert.as_ref().and_then(|a| a.rearm),
            is_global,
            error: None,
        }
    }

    fn build_pattern(&self) -> Result<(String, HighlightPattern), String> {
        fn opt(value: &str) -> Option<String> {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }

        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("Name is required.".to_string());
        }
        let pattern_text = self.pattern.trim().to_string();
        // A condition-driven alert is defined by having NO pattern — it fires
        // on a game-state transition, not a line of text. Requiring a pattern
        // unconditionally (as this did before alerts existed) makes that
        // entire rule shape unauthorable.
        if pattern_text.is_empty() && self.alert_when.is_none() {
            return Err(
                "Pattern is required (or add a condition to trigger on game state)."
                    .to_string(),
            );
        }
        // Only validate as a regex when there IS one. An empty pattern
        // compiles fine but matches every line, so it must reach the engine
        // as "no text trigger" rather than as a catch-all rule.
        if !self.fast_parse && !pattern_text.is_empty() {
            regex::Regex::new(&pattern_text).map_err(|err| format!("Invalid regex: {}", err))?;
        }
        let sound_volume = match self.sound_volume.trim() {
            "" => None,
            text => Some(
                text.parse::<f32>()
                    .map_err(|_| "Sound volume must be a number between 0 and 1.".to_string())
                    .map(|volume| volume.clamp(0.0, 1.0))?,
            ),
        };
        let status_duration = match self.status_duration.trim() {
            "" => None,
            text => Some(
                text.parse::<f32>()
                    .map_err(|_| "Status duration must be a number of seconds.".to_string())
                    .map(|secs| secs.max(0.0))?,
            ),
        };

        Ok((
            name,
            HighlightPattern {
                pattern: pattern_text,
                fg: opt(&self.fg),
                bg: opt(&self.bg),
                bold: self.bold,
                color_entire_line: self.color_entire_line,
                fast_parse: self.fast_parse,
                sound: opt(&self.sound),
                sound_volume,
                rumble: opt(&self.rumble),
                category: opt(&self.category),
                squelch: self.squelch,
                silent_prompt: self.silent_prompt,
                redirect_to: opt(&self.redirect_to),
                redirect_mode: if self.redirect_copy {
                    RedirectMode::RedirectCopy
                } else {
                    RedirectMode::RedirectOnly
                },
                replace: opt(&self.replace),
                stream: opt(&self.stream),
                window: opt(&self.window),
                set_status: opt(&self.set_status),
                status_duration,
                clear_status: opt(&self.clear_status),
                alert: self.build_alert(),
                compiled_regex: None,
            },
        ))
    }
}

impl VellumGuiApp {
    pub(in super::super) fn open_highlight_editor(&mut self, edit_name: Option<&str>) {
        // Reuse the open editor rather than rebuilding it (which would wipe
        // an unsaved form). A specific `edit_name` still repopulates the
        // form on the existing state; a bare open just raises the window.
        let mut state = self
            .highlight_editor
            .take()
            .unwrap_or_else(HighlightEditorState::new);
        match edit_name {
            Some("") | None => {}
            Some(name) => {
                if let Some(pattern) = self.app_core.config.highlights.get(name) {
                    let is_global = !self.highlight_is_character_override(name);
                    state.form = Some(HighlightFormState::from_pattern(name, pattern, is_global));
                } else {
                    self.app_core
                        .add_system_message(&format!("Highlight '{}' not found.", name));
                }
            }
        }
        self.highlight_editor = Some(state);
        self.raise_editor(egui::Id::new("gui_highlight_browser"));
    }

    pub(in super::super) fn open_highlight_form_new(&mut self) {
        let mut state = self
            .highlight_editor
            .take()
            .unwrap_or_else(HighlightEditorState::new);
        state.form = Some(HighlightFormState::empty());
        self.highlight_editor = Some(state);
    }

    fn highlight_is_character_override(&self, name: &str) -> bool {
        Config::load_character_highlights_only(self.app_core.config.character.as_deref())
            .map(|highlights| highlights.contains_key(name))
            .unwrap_or(false)
    }

    fn save_highlight_from_form(&mut self, form: &HighlightFormState) -> Result<(), String> {
        let (name, pattern) = form.build_pattern()?;
        let character = self.app_core.config.character.clone();

        // Renamed or re-scoped: remove the old entry from its original scope.
        if let Some(original) = &form.original_name {
            if *original != name || form.original_is_global != form.is_global {
                if let Err(err) = Config::delete_single_highlight(
                    original,
                    form.original_is_global,
                    character.as_deref(),
                ) {
                    tracing::warn!("Failed to remove old highlight '{}': {}", original, err);
                }
            }
        }

        Config::save_single_highlight(&name, &pattern, form.is_global, character.as_deref())
            .map_err(|err| format!("Failed to save highlight: {}", err))?;
        self.app_core.reload_highlights();
        Ok(())
    }

    pub(in super::super) fn render_highlight_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.highlight_editor.take() else {
            return;
        };

        let mut open = true;
        let mut open_form: Option<HighlightFormState> = None;
        let mut delete_request: Option<String> = None;

        egui::Window::new("Highlights")
            .id(egui::Id::new("gui_highlight_browser"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(460.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut state.filter);
                    if ui.button("Add highlight").clicked() {
                        open_form = Some(HighlightFormState::empty());
                    }
                });
                ui.separator();

                let filter = state.filter.to_lowercase();
                let mut names: Vec<&String> = self
                    .app_core
                    .config
                    .highlights
                    .keys()
                    .filter(|name| filter.is_empty() || name.to_lowercase().contains(&filter))
                    .collect();
                names.sort();

                let row_count = names.len();
                // Character overrides (vs global) for the [C]/[G] row tags,
                // loaded once per render rather than per row.
                let char_highlights = Config::load_character_highlights_only(
                    self.app_core.config.character.as_deref(),
                )
                .unwrap_or_default();
                egui::ScrollArea::vertical()
                    .id_salt("highlight_browser_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for name in names {
                            let Some(pattern) = self.app_core.config.highlights.get(name) else {
                                continue;
                            };
                            ui.horizontal(|ui| {
                                let is_character = char_highlights.contains_key(name);
                                let scope = if is_character { "[C]" } else { "[G]" };
                                ui.label(egui::RichText::new(scope).weak().monospace())
                                    .on_hover_text(if is_character {
                                        "This character's override"
                                    } else {
                                        "Global (all characters)"
                                    });
                                if ui.small_button("Edit").clicked() {
                                    open_form = Some(HighlightFormState::from_pattern(
                                        name,
                                        pattern,
                                        !is_character,
                                    ));
                                }
                                if ui.small_button("Delete").clicked() {
                                    delete_request = Some(name.clone());
                                }
                                let mut sample = egui::RichText::new(name);
                                if let Some(fg) =
                                    pattern.fg.as_deref().and_then(theme::resolve_color)
                                {
                                    sample = sample.color(fg);
                                }
                                if let Some(bg) =
                                    pattern.bg.as_deref().and_then(theme::resolve_color)
                                {
                                    sample = sample.background_color(bg);
                                }
                                if pattern.bold {
                                    sample = sample.strong();
                                }
                                ui.label(sample);
                                if let Some(category) = &pattern.category {
                                    ui.weak(format!("[{}]", category));
                                }
                            });
                        }
                        if row_count == 0 {
                            ui.weak("No highlights match.");
                        }
                    });
            });

        if let Some(name) = delete_request {
            let character = self.app_core.config.character.clone();
            match Config::delete_highlight_everywhere(&name, character.as_deref()) {
                Ok(()) => {
                    self.app_core.reload_highlights();
                    self.app_core
                        .add_system_message(&format!("Highlight '{}' deleted.", name));
                }
                Err(err) => self
                    .app_core
                    .add_system_message(&format!("Failed to delete highlight: {}", err)),
            }
        }

        if let Some(form) = open_form {
            state.form = Some(form);
        }

        // Render the form on top of the browser when active.
        if let Some(mut form) = state.form.take() {
            // "(none)" + built-ins + user-defined patterns from the
            // controller editor's Rumble tab (shared source with the TUI form).
            let rumble_options: Vec<String> = self.app_core.config.controller_rumble.pattern_names();
            // Effect-name suggestions for the shared condition builder, built
            // the same way the hand-icons editor does.
            let condition_suggestions: std::collections::HashMap<&'static str, Vec<String>> =
                crate::config::EffectCategory::ALL
                    .iter()
                    .map(|category| {
                        (
                            category.state_key(),
                            self.app_core
                                .game_state
                                .effects
                                .get(category.state_key())
                                .map(|store| {
                                    store.effects.iter().map(|e| e.text.clone()).collect()
                                })
                                .unwrap_or_default(),
                        )
                    })
                    .collect();
            let mut form_open = true;
            let mut submitted = false;
            let mut cancelled = false;
            let title = if form.original_name.is_some() {
                "Edit Highlight"
            } else {
                "Add Highlight"
            };
            egui::Window::new(title)
                .id(egui::Id::new("gui_highlight_form"))
                .order(egui::Order::Foreground)
                .open(&mut form_open)
                .default_width(420.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("highlight_form_scroll")
                        .show(ui, |ui| {
                            egui::Grid::new("highlight_form_grid")
                                .num_columns(2)
                                .show(ui, |ui| {
                                    ui.label("Name");
                                    ui.text_edit_singleline(&mut form.name);
                                    ui.end_row();
                                    ui.label("Pattern");
                                    ui.text_edit_singleline(&mut form.pattern);
                                    ui.end_row();
                                    ui.label("Foreground");
                                    ui.horizontal(|ui| {
                                        theme::color_picker_swatch(ui, &mut form.fg);
                                        ui.text_edit_singleline(&mut form.fg)
                                            .on_hover_text(
                                                "Optional: type a color name or hex \
                                                 instead of using the picker.",
                                            );
                                    });
                                    ui.end_row();
                                    ui.label("Background");
                                    ui.horizontal(|ui| {
                                        theme::color_picker_swatch(ui, &mut form.bg);
                                        ui.text_edit_singleline(&mut form.bg)
                                            .on_hover_text(
                                                "Optional: type a color name or hex \
                                                 instead of using the picker.",
                                            );
                                    });
                                    ui.end_row();
                                    ui.label("Category");
                                    ui.text_edit_singleline(&mut form.category);
                                    ui.end_row();
                                    ui.label("Sound");
                                    ui.text_edit_singleline(&mut form.sound);
                                    ui.end_row();
                                    ui.label("Sound volume");
                                    ui.text_edit_singleline(&mut form.sound_volume);
                                    ui.end_row();
                                    ui.label("Rumble").on_hover_text(
                                        "Buzz the controller when this pattern \
                                         matches. Patterns are defined in the \
                                         Controller window's Rumble tab.",
                                    );
                                    egui::ComboBox::from_id_salt("highlight_form_rumble")
                                        .selected_text(if form.rumble.is_empty() {
                                            "(none)"
                                        } else {
                                            form.rumble.as_str()
                                        })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut form.rumble,
                                                String::new(),
                                                "(none)",
                                            );
                                            for option in &rumble_options {
                                                ui.selectable_value(
                                                    &mut form.rumble,
                                                    option.clone(),
                                                    option,
                                                );
                                            }
                                        });
                                    ui.end_row();
                                    ui.label("Redirect to");
                                    ui.text_edit_singleline(&mut form.redirect_to);
                                    ui.end_row();
                                    ui.label("Replace");
                                    ui.text_edit_singleline(&mut form.replace);
                                    ui.end_row();
                                    ui.label("Stream");
                                    ui.text_edit_singleline(&mut form.stream);
                                    ui.end_row();
                                    ui.label("Window");
                                    ui.text_edit_singleline(&mut form.window);
                                    ui.end_row();
                                    ui.label("Set status")
                                        .on_hover_text(
                                            "Custom status id to activate on match; indicator \
                                             and dashboard widgets with this id light up.",
                                        );
                                    ui.text_edit_singleline(&mut form.set_status);
                                    ui.end_row();
                                    ui.label("Status duration")
                                        .on_hover_text(
                                            "Seconds until the set status clears itself; empty \
                                             = stays on until a clear-status rule matches.",
                                        );
                                    ui.text_edit_singleline(&mut form.status_duration);
                                    ui.end_row();
                                    ui.label("Clear status")
                                        .on_hover_text(
                                            "Custom status id to deactivate on match.",
                                        );
                                    ui.text_edit_singleline(&mut form.clear_status);
                                    ui.end_row();
                                });

                            // Overlay alert: collapsed by default so the
                            // common case (a coloring rule) stays a short
                            // form, but always one click away.
                            ui.collapsing("Overlay alert", |ui| {
                                ui.label(
                                    "Raise an on-screen alert when this pattern matches. \
                                     Leave Banner, Art, and Flash all empty for no alert.",
                                );
                                egui::Grid::new("highlight_alert_grid")
                                    .num_columns(2)
                                    .spacing([8.0, 4.0])
                                    .show(ui, |ui| {
                                        ui.label("Banner").on_hover_text(
                                            "Text shown on screen. Supports $1, $2 capture \
                                             groups from the pattern.",
                                        );
                                        ui.text_edit_singleline(&mut form.alert_banner);
                                        ui.end_row();

                                        ui.label("Banner color");
                                        ui.text_edit_singleline(&mut form.alert_banner_fg);
                                        ui.end_row();

                                        ui.label("Banner background");
                                        ui.text_edit_singleline(&mut form.alert_banner_bg);
                                        ui.end_row();

                                        ui.label("Art").on_hover_text(
                                            "Image name from the image pool, played once. \
                                             Static images (PNG) work too.",
                                        );
                                        ui.text_edit_singleline(&mut form.alert_art);
                                        ui.end_row();

                                        ui.label("Flash").on_hover_text(
                                            "Screen-edge tint color. Scaled by the global \
                                             flash intensity setting.",
                                        );
                                        ui.text_edit_singleline(&mut form.alert_flash);
                                        ui.end_row();

                                        ui.label("Anchor");
                                        egui::ComboBox::from_id_salt("alert_anchor")
                                            .selected_text(anchor_label(form.alert_anchor))
                                            .show_ui(ui, |ui| {
                                                for anchor in ALERT_ANCHORS {
                                                    ui.selectable_value(
                                                        &mut form.alert_anchor,
                                                        anchor,
                                                        anchor_label(anchor),
                                                    );
                                                }
                                            });
                                        ui.end_row();

                                        ui.label("Offset X / Y").on_hover_text(
                                            "Pixel nudge from the anchor point.",
                                        );
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut form.alert_offset_x,
                                                )
                                                .desired_width(60.0),
                                            );
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut form.alert_offset_y,
                                                )
                                                .desired_width(60.0),
                                            );
                                        });
                                        ui.end_row();

                                        ui.label("Duration").on_hover_text(
                                            "Seconds on screen before it fades. Empty = 4s.",
                                        );
                                        ui.text_edit_singleline(&mut form.alert_duration);
                                        ui.end_row();

                                        ui.label("Cooldown").on_hover_text(
                                            "Minimum seconds between fires of this rule. \
                                             Empty = 3s. Keeps busy combat from spamming.",
                                        );
                                        ui.text_edit_singleline(&mut form.alert_cooldown);
                                        ui.end_row();

                                        ui.label("Alert id").on_hover_text(
                                            "Optional stable name for this alert. Keeps its \
                                             cooldown identity if you edit the pattern.",
                                        );
                                        ui.text_edit_singleline(&mut form.alert_id);
                                        ui.end_row();
                                    });

                                ui.separator();

                                // Condition gate. With a pattern, this is not
                                // yet consulted (phase 1 fires on the text
                                // match); with an EMPTY pattern the alert
                                // becomes condition-driven and fires on the
                                // moment the condition becomes true.
                                let mut gated = form.alert_when.is_some();
                                if ui
                                    .checkbox(&mut gated, "Trigger on a game-state condition")
                                    .on_hover_text(
                                        "Fires when the condition BECOMES true, not while it \
                                         stays true. Leave the Pattern field empty to make \
                                         this a condition-driven alert.",
                                    )
                                    .changed()
                                {
                                    form.alert_when = gated.then(|| {
                                        crate::config::Condition::All {
                                            conditions: Vec::new(),
                                        }
                                    });
                                }

                                if let Some(condition) = form.alert_when.as_mut() {
                                    if !form.pattern.trim().is_empty() {
                                        ui.colored_label(
                                            ui.visuals().warn_fg_color,
                                            "This rule has a pattern, so it fires on the text \
                                             match. Clear the Pattern field to make the \
                                             condition drive it.",
                                        );
                                    }
                                    super::hotbars::render_condition_group(
                                        ui,
                                        "highlight_alert_cond",
                                        condition,
                                        0,
                                        &condition_suggestions,
                                    );
                                    ui.horizontal(|ui| {
                                        ui.label("Re-arm after").on_hover_text(
                                            "Seconds the condition must stay FALSE before this \
                                             can fire again. Stops a value hovering on its \
                                             threshold from firing over and over. Empty = 3s.",
                                        );
                                        let mut rearm = form
                                            .alert_rearm
                                            .map(|v| v.to_string())
                                            .unwrap_or_default();
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(&mut rearm)
                                                    .desired_width(60.0),
                                            )
                                            .changed()
                                        {
                                            form.alert_rearm = rearm
                                                .trim()
                                                .parse::<f32>()
                                                .ok()
                                                .filter(|v| *v >= 0.0);
                                        }
                                        ui.label("seconds");
                                    });
                                }
                            });

                            ui.horizontal_wrapped(|ui| {
                                ui.checkbox(&mut form.bold, "Bold");
                                ui.checkbox(&mut form.color_entire_line, "Entire line");
                                ui.checkbox(&mut form.fast_parse, "Fast parse");
                                ui.checkbox(&mut form.squelch, "Squelch");
                                ui.checkbox(&mut form.silent_prompt, "Silent prompt");
                                ui.checkbox(&mut form.redirect_copy, "Redirect copies");
                                ui.checkbox(&mut form.is_global, "Global (all characters)");
                            });

                            if let Some(error) = &form.error {
                                ui.colored_label(ui.visuals().error_fg_color, error);
                            }

                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui.button("Save").clicked() {
                                    submitted = true;
                                }
                                if ui.button("Cancel").clicked() {
                                    cancelled = true;
                                }
                            });
                        });
                });

            if submitted {
                match self.save_highlight_from_form(&form) {
                    Ok(()) => {
                        self.app_core
                            .add_system_message(&format!("Highlight '{}' saved.", form.name.trim()));
                    }
                    Err(err) => {
                        form.error = Some(err);
                        state.form = Some(form);
                    }
                }
            } else if form_open && !cancelled {
                state.form = Some(form);
            }
        }

        if open {
            self.highlight_editor = Some(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form_with_alert() -> HighlightFormState {
        let mut form = HighlightFormState::empty();
        form.name = "stun-warning".to_string();
        form.pattern = "You are stunned".to_string();
        form.alert_banner = "STUNNED".to_string();
        form.alert_banner_fg = "#ff0000".to_string();
        form.alert_art = "lightning".to_string();
        form.alert_anchor = crate::config::AlertAnchor::TopCenter;
        form.alert_offset_x = "10".to_string();
        form.alert_offset_y = "-20".to_string();
        form.alert_duration = "6".to_string();
        form.alert_cooldown = "5".to_string();
        form.alert_id = "stun".to_string();
        form
    }

    #[test]
    fn alert_survives_a_full_form_round_trip() {
        let (_, pattern) = form_with_alert().build_pattern().expect("builds");
        let alert = pattern.alert.clone().expect("alert authored");
        assert_eq!(alert.banner.as_deref(), Some("STUNNED"));
        assert_eq!(alert.banner_fg.as_deref(), Some("#ff0000"));
        assert_eq!(alert.art.as_deref(), Some("lightning"));
        assert_eq!(alert.anchor, crate::config::AlertAnchor::TopCenter);
        assert_eq!(alert.offset, Some((10.0, -20.0)));
        assert_eq!(alert.duration, Some(6.0));
        assert_eq!(alert.cooldown, Some(5.0));
        assert_eq!(alert.id.as_deref(), Some("stun"));

        // Reload into a form and rebuild: nothing may be lost in the cycle,
        // since that silent drop is exactly what bit set_status once before.
        let reloaded = HighlightFormState::from_pattern("stun-warning", &pattern, true);
        let (_, again) = reloaded.build_pattern().expect("rebuilds");
        let alert2 = again.alert.expect("alert preserved");
        assert_eq!(alert2.banner, alert.banner);
        assert_eq!(alert2.anchor, alert.anchor);
        assert_eq!(alert2.offset, alert.offset);
        assert_eq!(alert2.duration, alert.duration);
        assert_eq!(alert2.cooldown, alert.cooldown);
        assert_eq!(alert2.id, alert.id);
    }

    #[test]
    fn condition_gate_survives_a_round_trip() {
        let mut form = HighlightFormState::empty();
        form.name = "low-hp".to_string();
        // Condition-driven alerts have no pattern.
        form.pattern = String::new();
        form.alert_banner = "LOW HEALTH".to_string();
        form.alert_when = Some(crate::config::Condition::Vital {
            vital: crate::config::VitalKind::Health,
            cmp: crate::config::Cmp::Lt,
            value: 30,
            unit: crate::config::VitalUnit::Percent,
        });
        form.alert_rearm = Some(5.0);

        let (_, pattern) = form.build_pattern().expect("builds");
        let alert = pattern.alert.clone().expect("alert");
        assert!(alert.when.is_some(), "gate authored");
        assert_eq!(alert.rearm, Some(5.0));

        let reloaded = HighlightFormState::from_pattern("low-hp", &pattern, true);
        let (_, again) = reloaded.build_pattern().expect("rebuilds");
        let alert2 = again.alert.expect("alert preserved");
        assert_eq!(
            format!("{:?}", alert2.when),
            format!("{:?}", alert.when),
            "the condition tree survives edit-save-reload"
        );
        assert_eq!(alert2.rearm, alert.rearm);
    }

    #[test]
    fn a_gate_alone_keeps_the_alert_alive_without_a_banner() {
        // Clearing the presentation of a condition alert must NOT discard its
        // gate — that would turn a working alert into one that never fires,
        // and this form could not re-author the gate to recover it.
        let mut form = HighlightFormState::empty();
        form.name = "gated".to_string();
        form.alert_when = Some(crate::config::Condition::RtActive);
        let (_, pattern) = form.build_pattern().expect("builds");
        assert!(
            pattern.alert.and_then(|a| a.when).is_some(),
            "gate preserved even with no banner/art/flash"
        );
    }

    #[test]
    fn empty_alert_fields_mean_no_alert() {
        let mut form = HighlightFormState::empty();
        form.name = "plain".to_string();
        form.pattern = "hello".to_string();
        let (_, pattern) = form.build_pattern().expect("builds");
        assert!(
            pattern.alert.is_none(),
            "a coloring-only rule must not carry an invisible alert"
        );
    }

    #[test]
    fn any_single_presentation_is_enough_to_author_an_alert() {
        for field in ["banner", "art", "flash"] {
            let mut form = HighlightFormState::empty();
            form.name = "x".to_string();
            form.pattern = "y".to_string();
            match field {
                "banner" => form.alert_banner = "hi".to_string(),
                "art" => form.alert_art = "boom".to_string(),
                _ => form.alert_flash = "#ff0000".to_string(),
            }
            let (_, pattern) = form.build_pattern().expect("builds");
            assert!(pattern.alert.is_some(), "{field} alone should author an alert");
        }
    }

    #[test]
    fn one_offset_axis_defaults_the_other_to_zero() {
        let mut form = HighlightFormState::empty();
        form.name = "x".to_string();
        form.pattern = "y".to_string();
        form.alert_banner = "hi".to_string();
        form.alert_offset_y = "30".to_string();
        let (_, pattern) = form.build_pattern().expect("builds");
        // Typing only a Y nudge must not require typing a zero for X.
        assert_eq!(pattern.alert.expect("alert").offset, Some((0.0, 30.0)));
    }

    #[test]
    fn garbage_numbers_fall_back_to_defaults_rather_than_failing_the_save() {
        let mut form = HighlightFormState::empty();
        form.name = "x".to_string();
        form.pattern = "y".to_string();
        form.alert_banner = "hi".to_string();
        form.alert_duration = "soon".to_string();
        form.alert_cooldown = "-5".to_string();
        let (_, pattern) = form.build_pattern().expect("still saves");
        let alert = pattern.alert.expect("alert");
        // Unparseable/invalid values become None so the runtime default
        // applies; the rest of the rule is not lost over a typo.
        assert_eq!(alert.duration, None);
        assert_eq!(alert.cooldown, None);
    }
}
