//! Colors editor: palette browser/form, UI colors, and spell color ranges in
//! one tabbed window. Persists through the shared ColorConfig layer and
//! hot-reloads via `AppCore::reload_colors`.

use super::super::{theme, VellumGuiApp};
use crate::config::{ColorConfig, PaletteColor, SpellColorRange};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorsTab {
    Palette,
    Ui,
    Spells,
}

pub(in super::super) struct ColorsEditorState {
    tab: ColorsTab,
    filter: String,
    palette_form: Option<PaletteFormState>,
    spell_form: Option<SpellFormState>,
    ui_buffer: Option<UiColorsBuffer>,
}

impl ColorsEditorState {
    fn new(tab: ColorsTab) -> Self {
        Self {
            tab,
            filter: String::new(),
            palette_form: None,
            spell_form: None,
            ui_buffer: None,
        }
    }
}

struct PaletteFormState {
    original_name: Option<String>,
    name: String,
    color: String,
    category: String,
    favorite: bool,
    slot: String,
    is_global: bool,
    error: Option<String>,
}

impl PaletteFormState {
    fn empty() -> Self {
        Self {
            original_name: None,
            name: String::new(),
            color: String::new(),
            category: String::new(),
            favorite: false,
            slot: String::new(),
            is_global: true,
            error: None,
        }
    }

    fn from_color(color: &PaletteColor) -> Self {
        Self {
            original_name: Some(color.name.clone()),
            name: color.name.clone(),
            color: color.color.clone(),
            category: color.category.clone(),
            favorite: color.favorite,
            slot: color.slot.map(|slot| slot.to_string()).unwrap_or_default(),
            is_global: true,
            error: None,
        }
    }

    fn build(&self) -> Result<PaletteColor, String> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("Name is required.".to_string());
        }
        if theme::resolve_color(&self.color).is_none() {
            return Err("Color must be a hex value like #ff8800 or a color name.".to_string());
        }
        let slot = match self.slot.trim() {
            "" => None,
            text => Some(
                text.parse::<u8>()
                    .map_err(|_| "Slot must be a number between 16 and 231.".to_string())?,
            ),
        };
        Ok(PaletteColor {
            name,
            color: self.color.trim().to_string(),
            category: self.category.trim().to_string(),
            favorite: self.favorite,
            slot,
        })
    }
}

struct SpellFormState {
    /// Index into spell_colors when editing; None when adding.
    original_index: Option<usize>,
    spells: String,
    bar_color: String,
    text_color: String,
    bg_color: String,
    error: Option<String>,
}

impl SpellFormState {
    fn empty() -> Self {
        Self {
            original_index: None,
            spells: String::new(),
            bar_color: String::new(),
            text_color: String::new(),
            bg_color: String::new(),
            error: None,
        }
    }

    fn from_range(index: usize, range: &SpellColorRange) -> Self {
        let style = range.style();
        Self {
            original_index: Some(index),
            spells: range
                .spells
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            bar_color: style.bar_color.unwrap_or_default(),
            text_color: style.text_color.unwrap_or_default(),
            bg_color: range.bg_color.clone().unwrap_or_default(),
            error: None,
        }
    }

    fn build(&self) -> Result<SpellColorRange, String> {
        fn opt(value: &str) -> Option<String> {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }

        let spells: Result<Vec<u32>, _> = self
            .spells
            .split([',', ' '])
            .filter(|part| !part.trim().is_empty())
            .map(|part| part.trim().parse::<u32>())
            .collect();
        let spells = spells.map_err(|_| "Spells must be numeric IDs (e.g. 101, 107).".to_string())?;
        if spells.is_empty() {
            return Err("At least one spell ID is required.".to_string());
        }
        Ok(SpellColorRange {
            spells,
            color: String::new(),
            bar_color: opt(&self.bar_color),
            text_color: opt(&self.text_color),
            bg_color: opt(&self.bg_color),
        })
    }
}

/// Live edit buffer for the global UI colors.
struct UiColorsBuffer {
    command_echo_color: String,
    border_color: String,
    focused_border_color: String,
    text_color: String,
    background_color: String,
    selection_bg_color: String,
    textarea_background: String,
}

impl UiColorsBuffer {
    fn from_config(colors: &ColorConfig) -> Self {
        let ui = &colors.ui;
        Self {
            command_echo_color: ui.command_echo_color.clone(),
            border_color: ui.border_color.clone(),
            focused_border_color: ui.focused_border_color.clone(),
            text_color: ui.text_color.clone(),
            background_color: ui.background_color.clone(),
            selection_bg_color: ui.selection_bg_color.clone(),
            textarea_background: ui.textarea_background.clone(),
        }
    }

    fn apply(&self, colors: &mut ColorConfig) {
        colors.ui.command_echo_color = self.command_echo_color.clone();
        colors.ui.border_color = self.border_color.clone();
        colors.ui.focused_border_color = self.focused_border_color.clone();
        colors.ui.text_color = self.text_color.clone();
        colors.ui.background_color = self.background_color.clone();
        colors.ui.selection_bg_color = self.selection_bg_color.clone();
        colors.ui.textarea_background = self.textarea_background.clone();
    }
}

/// Hex/name text field with a live swatch and an egui color picker.
fn color_field(ui: &mut egui::Ui, value: &mut String) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(110.0));
        if let Some(color) = theme::resolve_color(value) {
            let mut rgb = [color.r(), color.g(), color.b()];
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                *value = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            }
        } else if !value.trim().is_empty() {
            ui.weak("?");
        }
    });
}

impl VellumGuiApp {
    pub(in super::super) fn open_colors_editor(&mut self) {
        self.colors_editor = Some(ColorsEditorState::new(ColorsTab::Palette));
    }

    pub(in super::super) fn open_ui_colors_editor(&mut self) {
        self.colors_editor = Some(ColorsEditorState::new(ColorsTab::Ui));
    }

    pub(in super::super) fn open_spell_colors_editor(&mut self) {
        self.colors_editor = Some(ColorsEditorState::new(ColorsTab::Spells));
    }

    pub(in super::super) fn open_palette_form_new(&mut self) {
        let mut state = self
            .colors_editor
            .take()
            .unwrap_or_else(|| ColorsEditorState::new(ColorsTab::Palette));
        state.tab = ColorsTab::Palette;
        state.palette_form = Some(PaletteFormState::empty());
        self.colors_editor = Some(state);
    }

    pub(in super::super) fn open_spell_form_new(&mut self) {
        let mut state = self
            .colors_editor
            .take()
            .unwrap_or_else(|| ColorsEditorState::new(ColorsTab::Spells));
        state.tab = ColorsTab::Spells;
        state.spell_form = Some(SpellFormState::empty());
        self.colors_editor = Some(state);
    }

    fn persist_color_config(&mut self) {
        let character = self.app_core.config.character.clone();
        if let Err(err) = self.app_core.config.colors.save(character.as_deref()) {
            self.app_core
                .add_system_message(&format!("Failed to save colors: {}", err));
        }
        self.app_core.reload_colors();
    }

    pub(in super::super) fn render_colors_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.colors_editor.take() else {
            return;
        };

        let mut open = true;
        egui::Window::new("Colors")
            .id(egui::Id::new("gui_colors_editor"))
            .open(&mut open)
            .default_width(460.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.tab, ColorsTab::Palette, "Palette");
                    ui.selectable_value(&mut state.tab, ColorsTab::Ui, "UI Colors");
                    ui.selectable_value(&mut state.tab, ColorsTab::Spells, "Spell Colors");
                });
                ui.separator();
                match state.tab {
                    ColorsTab::Palette => self.render_palette_tab(ui, &mut state),
                    ColorsTab::Ui => self.render_ui_colors_tab(ui, &mut state),
                    ColorsTab::Spells => self.render_spell_colors_tab(ui, &mut state),
                }
            });

        self.render_palette_form(ctx, &mut state);
        self.render_spell_form(ctx, &mut state);

        if open {
            self.colors_editor = Some(state);
        }
    }

    fn render_palette_tab(&mut self, ui: &mut egui::Ui, state: &mut ColorsEditorState) {
        let mut delete_request: Option<String> = None;
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut state.filter);
            if ui.button("Add color").clicked() {
                state.palette_form = Some(PaletteFormState::empty());
            }
        });

        let filter = state.filter.to_lowercase();
        let mut entries: Vec<PaletteColor> = self
            .app_core
            .config
            .colors
            .color_palette
            .iter()
            .filter(|color| {
                filter.is_empty()
                    || color.name.to_lowercase().contains(&filter)
                    || color.category.to_lowercase().contains(&filter)
            })
            .cloned()
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        egui::ScrollArea::vertical()
            .id_salt("palette_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for color in &entries {
                    ui.horizontal(|ui| {
                        if ui.small_button("Edit").clicked() {
                            state.palette_form = Some(PaletteFormState::from_color(color));
                        }
                        if ui.small_button("Delete").clicked() {
                            delete_request = Some(color.name.clone());
                        }
                        if let Some(swatch) = theme::resolve_color(&color.color) {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(18.0, 14.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, swatch);
                        }
                        let mut label = egui::RichText::new(&color.name);
                        if color.favorite {
                            label = label.strong();
                        }
                        ui.label(label);
                        ui.weak(&color.color);
                        if !color.category.is_empty() {
                            ui.weak(format!("[{}]", color.category));
                        }
                        if let Some(slot) = color.slot {
                            ui.weak(format!("slot {}", slot));
                        }
                    });
                }
                if entries.is_empty() {
                    ui.weak("No palette colors match.");
                }
            });

        if let Some(name) = delete_request {
            let character = self.app_core.config.character.clone();
            // Try character scope first, then global; ignore missing entries.
            let _ = ColorConfig::delete_single_palette_color(&name, false, character.as_deref());
            let _ = ColorConfig::delete_single_palette_color(&name, true, character.as_deref());
            self.app_core.reload_colors();
            self.app_core
                .add_system_message(&format!("Palette color '{}' deleted.", name));
        }
    }

    fn render_palette_form(&mut self, ctx: &egui::Context, state: &mut ColorsEditorState) {
        let Some(mut form) = state.palette_form.take() else {
            return;
        };
        let mut form_open = true;
        let mut submitted = false;
        let mut cancelled = false;
        let title = if form.original_name.is_some() {
            "Edit Palette Color"
        } else {
            "Add Palette Color"
        };
        egui::Window::new(title)
            .id(egui::Id::new("gui_palette_form"))
            .open(&mut form_open)
            .default_width(340.0)
            .show(ctx, |ui| {
                egui::Grid::new("palette_form_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut form.name);
                        ui.end_row();
                        ui.label("Color");
                        color_field(ui, &mut form.color);
                        ui.end_row();
                        ui.label("Category");
                        ui.text_edit_singleline(&mut form.category);
                        ui.end_row();
                        ui.label("Slot (16-231)");
                        ui.text_edit_singleline(&mut form.slot);
                        ui.end_row();
                    });
                ui.checkbox(&mut form.favorite, "Favorite");
                ui.checkbox(&mut form.is_global, "Global (all characters)");
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

        if submitted {
            match form.build() {
                Ok(color) => {
                    let character = self.app_core.config.character.clone();
                    if let Some(original) = &form.original_name {
                        if *original != color.name {
                            let _ = ColorConfig::delete_single_palette_color(
                                original,
                                form.is_global,
                                character.as_deref(),
                            );
                        }
                    }
                    match ColorConfig::save_single_palette_color(
                        &color,
                        form.is_global,
                        character.as_deref(),
                    ) {
                        Ok(()) => {
                            self.app_core.reload_colors();
                            self.app_core.add_system_message(&format!(
                                "Palette color '{}' saved.",
                                color.name
                            ));
                        }
                        Err(err) => {
                            form.error = Some(format!("Failed to save: {}", err));
                            state.palette_form = Some(form);
                        }
                    }
                }
                Err(err) => {
                    form.error = Some(err);
                    state.palette_form = Some(form);
                }
            }
        } else if form_open && !cancelled {
            state.palette_form = Some(form);
        }
    }

    fn render_ui_colors_tab(&mut self, ui: &mut egui::Ui, state: &mut ColorsEditorState) {
        let buffer = state
            .ui_buffer
            .get_or_insert_with(|| UiColorsBuffer::from_config(&self.app_core.config.colors));

        egui::Grid::new("ui_colors_grid").num_columns(2).show(ui, |ui| {
            ui.label("Command echo");
            color_field(ui, &mut buffer.command_echo_color);
            ui.end_row();
            ui.label("Border");
            color_field(ui, &mut buffer.border_color);
            ui.end_row();
            ui.label("Focused border");
            color_field(ui, &mut buffer.focused_border_color);
            ui.end_row();
            ui.label("Text");
            color_field(ui, &mut buffer.text_color);
            ui.end_row();
            ui.label("Background");
            color_field(ui, &mut buffer.background_color);
            ui.end_row();
            ui.label("Selection background");
            color_field(ui, &mut buffer.selection_bg_color);
            ui.end_row();
            ui.label("Textarea background");
            color_field(ui, &mut buffer.textarea_background);
            ui.end_row();
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                let buffer = state.ui_buffer.take();
                if let Some(buffer) = buffer {
                    buffer.apply(&mut self.app_core.config.colors);
                    self.persist_color_config();
                    self.app_core.add_system_message("UI colors saved.");
                }
            }
            if ui.button("Reset").clicked() {
                state.ui_buffer = None;
            }
        });
    }

    fn render_spell_colors_tab(&mut self, ui: &mut egui::Ui, state: &mut ColorsEditorState) {
        let mut delete_request: Option<usize> = None;
        if ui.button("Add spell color range").clicked() {
            state.spell_form = Some(SpellFormState::empty());
        }
        ui.separator();

        let ranges = self.app_core.config.colors.spell_colors.clone();
        egui::ScrollArea::vertical()
            .id_salt("spell_colors_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, range) in ranges.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("Edit").clicked() {
                            state.spell_form = Some(SpellFormState::from_range(index, range));
                        }
                        if ui.small_button("Delete").clicked() {
                            delete_request = Some(index);
                        }
                        let style = range.style();
                        if let Some(swatch) =
                            style.bar_color.as_deref().and_then(theme::resolve_color)
                        {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(18.0, 14.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, swatch);
                        }
                        let spells = range
                            .spells
                            .iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        ui.label(spells);
                    });
                }
                if ranges.is_empty() {
                    ui.weak("No spell color ranges configured.");
                }
            });

        if let Some(index) = delete_request {
            if index < self.app_core.config.colors.spell_colors.len() {
                self.app_core.config.colors.spell_colors.remove(index);
                self.persist_color_config();
                self.app_core.add_system_message("Spell color range deleted.");
            }
        }
    }

    fn render_spell_form(&mut self, ctx: &egui::Context, state: &mut ColorsEditorState) {
        let Some(mut form) = state.spell_form.take() else {
            return;
        };
        let mut form_open = true;
        let mut submitted = false;
        let mut cancelled = false;
        let title = if form.original_index.is_some() {
            "Edit Spell Colors"
        } else {
            "Add Spell Colors"
        };
        egui::Window::new(title)
            .id(egui::Id::new("gui_spell_color_form"))
            .open(&mut form_open)
            .default_width(340.0)
            .show(ctx, |ui| {
                egui::Grid::new("spell_form_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Spell IDs");
                        ui.text_edit_singleline(&mut form.spells);
                        ui.end_row();
                        ui.label("Bar color");
                        color_field(ui, &mut form.bar_color);
                        ui.end_row();
                        ui.label("Text color");
                        color_field(ui, &mut form.text_color);
                        ui.end_row();
                        ui.label("Background");
                        color_field(ui, &mut form.bg_color);
                        ui.end_row();
                    });
                ui.weak("Comma-separated spell IDs, e.g. 101, 107, 120.");
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

        if submitted {
            match form.build() {
                Ok(range) => {
                    let spell_colors = &mut self.app_core.config.colors.spell_colors;
                    match form.original_index {
                        Some(index) if index < spell_colors.len() => spell_colors[index] = range,
                        _ => spell_colors.push(range),
                    }
                    self.persist_color_config();
                    self.app_core.add_system_message("Spell colors saved.");
                }
                Err(err) => {
                    form.error = Some(err);
                    state.spell_form = Some(form);
                }
            }
        } else if form_open && !cancelled {
            state.spell_form = Some(form);
        }
    }
}
