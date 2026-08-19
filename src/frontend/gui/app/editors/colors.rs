//! Colors editor: palette browser/form, UI colors, and spell color ranges in
//! one tabbed window. Persists through the shared ColorConfig layer and
//! hot-reloads via `AppCore::reload_colors`.

use super::super::{theme, VellumGuiApp};
use crate::config::{ColorConfig, PaletteColor, PresetColor, PromptColor, SpellColorRange};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorsTab {
    Palette,
    Ui,
    Presets,
    Prompts,
    Spells,
    Generate,
}

pub(in super::super) struct ColorsEditorState {
    tab: ColorsTab,
    filter: String,
    palette_form: Option<PaletteFormState>,
    spell_form: Option<SpellFormState>,
    ui_buffer: Option<UiColorsBuffer>,
    presets_buffer: Option<PresetsBuffer>,
    prompts_buffer: Option<PromptsBuffer>,
    generate: Option<HarmonyGenState>,
}

impl ColorsEditorState {
    fn new(tab: ColorsTab) -> Self {
        Self {
            tab,
            filter: String::new(),
            palette_form: None,
            spell_form: None,
            ui_buffer: None,
            presets_buffer: None,
            prompts_buffer: None,
            generate: None,
        }
    }
}

/// Live state for the Generate tab: a seed picked from the theme's swatch
/// strip (or a custom hex), scheme, and Low/Med/High constraint tiers. The
/// preview regenerates every frame from these values; nothing touches disk
/// until Apply.
struct HarmonyGenState {
    seed: String,
    custom_seed: String,
    use_custom: bool,
    scheme: crate::core::harmony::Scheme,
    variance: f64,
    min_contrast: f64,
    separation: f64,
    room_title_spread: f64,
    pins: std::collections::HashMap<String, String>,
    /// Role whose alternate-hue chips are open in the preview, if any.
    explore: Option<String>,
    skin_name: String,
    skin_panel: crate::core::harmony_skin::PanelSpec,
    skin_frame: crate::core::harmony_skin::FrameSpec,
    /// Cached preview textures keyed by a hash of everything they depend on.
    skin_preview: Option<(u64, Vec<egui::TextureHandle>)>,
}

impl HarmonyGenState {
    /// Start from the stored recipe when it was generated against the
    /// current theme background (so a saved look stays re-tunable); else
    /// defaults with the most vivid theme swatch pre-selected, so zero
    /// clicks still previews something decent.
    fn init(recipe: Option<crate::config::HarmonyRecipe>, swatches: &[String]) -> Self {
        use crate::core::harmony::{HarmonyParams, Scheme};
        let defaults = HarmonyParams::default();
        let first_swatch = swatches
            .first()
            .cloned()
            .unwrap_or_else(|| defaults.seed.clone());
        match recipe {
            Some(recipe) => {
                let seed_in_strip = swatches
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(&recipe.seed));
                Self {
                    seed: if seed_in_strip {
                        recipe.seed.clone()
                    } else {
                        first_swatch
                    },
                    custom_seed: recipe.seed.clone(),
                    use_custom: !seed_in_strip,
                    scheme: Scheme::parse(&recipe.scheme).unwrap_or(defaults.scheme),
                    variance: recipe.variance,
                    min_contrast: recipe.min_contrast,
                    separation: recipe.separation,
                    room_title_spread: recipe.room_title_spread,
                    pins: recipe.pins,
                    explore: None,
                    skin_name: "harmony".to_string(),
                    skin_panel: Default::default(),
                    skin_frame: Default::default(),
                    skin_preview: None,
                }
            }
            None => Self {
                seed: first_swatch,
                custom_seed: String::new(),
                use_custom: false,
                scheme: defaults.scheme,
                variance: defaults.variance,
                min_contrast: defaults.min_contrast,
                separation: defaults.separation,
                room_title_spread: defaults.room_title_spread,
                pins: std::collections::HashMap::new(),
                explore: None,
                skin_name: "harmony".to_string(),
                skin_panel: Default::default(),
                skin_frame: Default::default(),
                skin_preview: None,
            },
        }
    }

    /// The seed actually fed to the engine: the custom hex when selected and
    /// valid, else the picked swatch.
    fn effective_seed(&self) -> String {
        if self.use_custom {
            if crate::core::harmony::hex_to_lch(&self.custom_seed).is_some() {
                return self.custom_seed.clone();
            }
        }
        self.seed.clone()
    }

    fn params(&self, background: &str) -> crate::core::harmony::HarmonyParams {
        crate::core::harmony::HarmonyParams {
            seed: self.effective_seed(),
            background: background.to_string(),
            scheme: self.scheme,
            variance: self.variance,
            min_contrast: self.min_contrast,
            separation: self.separation,
            room_title_spread: self.room_title_spread,
            pins: self.pins.clone(),
        }
    }
}

/// One row of Low/Med/High-style tier buttons bound to an f64 parameter.
fn tier_row(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut f64, tiers: &[(&str, f64)]) {
    ui.horizontal(|ui| {
        ui.label(label);
        for (name, tier) in tiers {
            if ui
                .selectable_label((*value - tier).abs() < f64::EPSILON, *name)
                .clicked()
            {
                *value = *tier;
            }
        }
        ui.weak(hint);
    });
}

/// Fake game text rendered entirely in the generated colors, so the set is
/// judged the way it will actually be read - words on the theme background -
/// not as isolated swatches.
fn story_preview(
    app_theme: &crate::theme::AppTheme,
    result: &crate::core::harmony::HarmonyResult,
) -> egui::text::LayoutJob {
    let body = {
        let c = app_theme.text_primary;
        egui::Color32::from_rgb(c.r, c.g, c.b)
    };
    let role = |name: &str| -> egui::Color32 {
        result
            .color_for(name)
            .and_then(theme::resolve_color)
            .unwrap_or(body)
    };
    let prompt = |character: &str| -> egui::Color32 {
        result
            .prompts
            .iter()
            .find(|(c, _)| c == character)
            .and_then(|(_, hex)| theme::resolve_color(hex))
            .unwrap_or(body)
    };
    let plate = theme::resolve_color(&result.room_bg).unwrap_or(egui::Color32::BLACK);

    let font = egui::FontId::monospace(12.0);
    let mut job = egui::text::LayoutJob::default();
    let mut span = |text: &str, color: egui::Color32, background: egui::Color32| {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color,
                background,
                ..Default::default()
            },
        );
    };
    let none = egui::Color32::TRANSPARENT;

    span("[Seamist Hall, Central Lounge]", role("roomName"), plate);
    span("\nA circular ", body, none);
    span("bar", role("links"), none);
    span(" dominates the room, potted ", body, none);
    span("ferns", role("links"), none);
    span(" placed about it. A ", body, none);
    span("grizzled kobold", role("monsterbold"), none);
    span(" shuffles past.\nObvious exits: ", body, none);
    span("northeast", role("links"), none);
    span(", ", body, none);
    span("northwest", role("links"), none);
    span("\n>", prompt(">"), none);
    span("go archway", role("commands"), none);
    span("\n", body, none);
    span("Penelopia says,", role("speech"), none);
    span(
        " \"Selling a satyr will-o-wisp in a black acorn amulet.\"\n",
        body,
        none,
    );
    span("Someone whispers,", role("whisper"), none);
    span(" \"Meet me by the fountain.\"\n", body, none);
    span(
        "[General] Aravon: \"Anyone running the rift tonight?\"\n",
        role("thought"),
        none,
    );
    span(
        "Your familiar senses movement nearby.\n",
        role("familiar"),
        none,
    );
    span("You feel the favor of Voln upon you.\n", role("voln"), none);
    span("\u{25b8} kobold", role("target_indicator"), none);
    span("   ", body, none);
    span("Spirit Warding II  12:04", role("percWindow"), none);
    span("\nR", prompt("R"), none);
    span(" S", prompt("S"), none);
    span(" H", prompt("H"), none);
    span(" !", prompt("!"), none);
    span(" >", prompt(">"), none);
    span(
        "  (roundtime / stunned / hiding / bleeding / prompt)",
        body,
        none,
    );
    job
}

/// Clickable seed chip; returns true when clicked.
fn seed_chip(ui: &mut egui::Ui, hex: &str, selected: bool) -> bool {
    let Some(fill) = theme::resolve_color(hex) else {
        return false;
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 18.0), egui::Sense::click());
    ui.painter().rect_filled(rect, 3.0, fill);
    if selected {
        ui.painter().rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
            egui::StrokeKind::Outside,
        );
    }
    response.on_hover_text(hex).clicked()
}

struct PaletteFormState {
    original_name: Option<String>,
    name: String,
    color: String,
    category: String,
    favorite: bool,
    slot: String,
    is_global: bool,
    original_is_global: bool,
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
            original_is_global: true,
            error: None,
        }
    }

    fn from_color(color: &PaletteColor, is_global: bool) -> Self {
        Self {
            original_name: Some(color.name.clone()),
            name: color.name.clone(),
            color: color.color.clone(),
            category: color.category.clone(),
            favorite: color.favorite,
            slot: color.slot.map(|slot| slot.to_string()).unwrap_or_default(),
            is_global,
            original_is_global: is_global,
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
        let spells =
            spells.map_err(|_| "Spells must be numeric IDs (e.g. 101, 107).".to_string())?;
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
    system_message_color: String,
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
            system_message_color: ui.system_message_color.clone(),
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
        colors.ui.system_message_color = self.system_message_color.clone();
        colors.ui.border_color = self.border_color.clone();
        colors.ui.focused_border_color = self.focused_border_color.clone();
        colors.ui.text_color = self.text_color.clone();
        colors.ui.background_color = self.background_color.clone();
        colors.ui.selection_bg_color = self.selection_bg_color.clone();
        colors.ui.textarea_background = self.textarea_background.clone();
    }
}

/// One editable preset row: a name plus fg/bg as free text (empty = None).
struct PresetRow {
    name: String,
    fg: String,
    bg: String,
}

/// Live edit buffer for named preset colors (`ColorConfig.presets`). Edited as
/// an ordered Vec so rows have stable identity while typing; applied back to
/// the map wholesale on Save (last write wins on duplicate names, flagged in
/// the UI before save).
struct PresetsBuffer {
    rows: Vec<PresetRow>,
}

impl PresetsBuffer {
    fn from_config(colors: &ColorConfig) -> Self {
        let mut rows: Vec<PresetRow> = colors
            .presets
            .iter()
            .map(|(name, p)| PresetRow {
                name: name.clone(),
                fg: p.fg.clone().unwrap_or_default(),
                bg: p.bg.clone().unwrap_or_default(),
            })
            .collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        Self { rows }
    }

    fn apply(&self, colors: &mut ColorConfig) {
        let mut map = std::collections::HashMap::new();
        for row in &self.rows {
            let name = row.name.trim();
            if name.is_empty() {
                continue; // unnamed rows are dropped
            }
            map.insert(
                name.to_string(),
                PresetColor {
                    fg: opt(&row.fg),
                    bg: opt(&row.bg),
                },
            );
        }
        colors.presets = map;
    }
}

/// One editable prompt row: the matched character plus fg/bg as free text.
struct PromptRow {
    character: String,
    fg: String,
    bg: String,
}

/// Live edit buffer for prompt colors (`ColorConfig.prompt_colors`).
struct PromptsBuffer {
    rows: Vec<PromptRow>,
}

impl PromptsBuffer {
    fn from_config(colors: &ColorConfig) -> Self {
        // Fold the legacy `color` field into fg up front so the editor only
        // ever presents/writes canonical fg + bg (legacy stays cleared).
        let rows = colors
            .prompt_colors
            .iter()
            .map(|p| PromptRow {
                character: p.character.clone(),
                fg: p.fg.clone().or_else(|| p.color.clone()).unwrap_or_default(),
                bg: p.bg.clone().unwrap_or_default(),
            })
            .collect();
        Self { rows }
    }

    fn apply(&self, colors: &mut ColorConfig) {
        colors.prompt_colors = self
            .rows
            .iter()
            .filter(|row| !row.character.trim().is_empty())
            .map(|row| PromptColor {
                character: row.character.trim().to_string(),
                fg: opt(&row.fg),
                bg: opt(&row.bg),
                color: None,
            })
            .collect();
    }
}

/// Trim a color text field to `Option<String>` (empty → None).
fn opt(value: &str) -> Option<String> {
    let t = value.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

use super::color_field;

impl VellumGuiApp {
    /// Open the colors editor on `tab`, or — if it is already open — switch
    /// it to that tab and raise it, preserving any unsaved buffer/form.
    fn open_or_focus_colors(&mut self, tab: ColorsTab) {
        match self.colors_editor.as_mut() {
            Some(state) => state.tab = tab,
            None => self.colors_editor = Some(ColorsEditorState::new(tab)),
        }
        self.raise_editor(egui::Id::new("gui_colors_editor"));
    }

    pub(in super::super) fn open_colors_editor(&mut self) {
        self.open_or_focus_colors(ColorsTab::Palette);
    }

    pub(in super::super) fn open_ui_colors_editor(&mut self) {
        self.open_or_focus_colors(ColorsTab::Ui);
    }

    pub(in super::super) fn open_spell_colors_editor(&mut self) {
        self.open_or_focus_colors(ColorsTab::Spells);
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
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(460.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.tab, ColorsTab::Palette, "Palette");
                    ui.selectable_value(&mut state.tab, ColorsTab::Ui, "UI Colors");
                    ui.selectable_value(&mut state.tab, ColorsTab::Presets, "Presets");
                    ui.selectable_value(&mut state.tab, ColorsTab::Prompts, "Prompts");
                    ui.selectable_value(&mut state.tab, ColorsTab::Spells, "Spell Colors");
                    ui.selectable_value(&mut state.tab, ColorsTab::Generate, "Generate");
                });
                ui.separator();
                match state.tab {
                    ColorsTab::Palette => self.render_palette_tab(ui, &mut state),
                    ColorsTab::Ui => self.render_ui_colors_tab(ui, &mut state),
                    ColorsTab::Presets => self.render_presets_tab(ui, &mut state),
                    ColorsTab::Prompts => self.render_prompts_tab(ui, &mut state),
                    ColorsTab::Spells => self.render_spell_colors_tab(ui, &mut state),
                    ColorsTab::Generate => self.render_generate_tab(ui, &mut state),
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

        // Names that live in the character-scoped colors.toml. Editing one of
        // these must stay character-scoped: hard-coding is_global=true wrote the
        // edit to the global file, which config load then replaces with the
        // (untouched) character list, so the change silently vanished.
        let character = self.app_core.config.character.clone();
        let character_scoped: std::collections::HashSet<String> =
            ColorConfig::load_character_colors_only(character.as_deref())
                .map(|c| c.color_palette.into_iter().map(|pc| pc.name).collect())
                .unwrap_or_default();

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
                        let is_character = character_scoped.contains(&color.name);
                        let scope = if is_character { "[C]" } else { "[G]" };
                        ui.label(egui::RichText::new(scope).weak().monospace())
                            .on_hover_text(if is_character {
                                "This character's override"
                            } else {
                                "Global (all characters)"
                            });
                        if ui.small_button("Edit").clicked() {
                            state.palette_form =
                                Some(PaletteFormState::from_color(color, !is_character));
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
            .order(egui::Order::Foreground)
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
                        // Renamed OR re-scoped: remove the old entry from its
                        // ORIGINAL scope, otherwise a global↔character flip
                        // leaves an orphaned duplicate behind.
                        if *original != color.name || form.original_is_global != form.is_global {
                            let _ = ColorConfig::delete_single_palette_color(
                                original,
                                form.original_is_global,
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

        egui::Grid::new("ui_colors_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Command echo");
                color_field(ui, &mut buffer.command_echo_color);
                ui.end_row();
                ui.label("System messages");
                color_field(ui, &mut buffer.system_message_color);
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

    fn render_presets_tab(&mut self, ui: &mut egui::Ui, state: &mut ColorsEditorState) {
        let buffer = state
            .presets_buffer
            .get_or_insert_with(|| PresetsBuffer::from_config(&self.app_core.config.colors));

        ui.label(
            "Named color presets. Games reference these by name; each maps to \
             a foreground and/or background color.",
        );
        if ui.button("Add preset").clicked() {
            buffer.rows.push(PresetRow {
                name: String::new(),
                fg: String::new(),
                bg: String::new(),
            });
        }
        ui.separator();

        let mut delete_row: Option<usize> = None;
        egui::ScrollArea::vertical()
            .id_salt("presets_scroll")
            .auto_shrink([false, false])
            .max_height(300.0)
            .show(ui, |ui| {
                egui::Grid::new("presets_grid")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Name");
                        ui.strong("Foreground");
                        ui.strong("Background");
                        ui.label("");
                        ui.end_row();
                        for (i, row) in buffer.rows.iter_mut().enumerate() {
                            ui.add(
                                egui::TextEdit::singleline(&mut row.name)
                                    .desired_width(120.0)
                                    .hint_text("preset name"),
                            );
                            color_field(ui, &mut row.fg);
                            color_field(ui, &mut row.bg);
                            if ui.small_button("Delete").clicked() {
                                delete_row = Some(i);
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(i) = delete_row {
            buffer.rows.remove(i);
        }

        // Duplicate names would collide on save (last wins) — warn first.
        let mut seen = std::collections::HashSet::new();
        let dup = buffer
            .rows
            .iter()
            .filter(|r| !r.name.trim().is_empty())
            .find(|r| !seen.insert(r.name.trim().to_string()));
        if let Some(r) = dup {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "Duplicate preset name '{}' — only the last is kept.",
                    r.name.trim()
                ),
            );
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                if let Some(buffer) = state.presets_buffer.take() {
                    buffer.apply(&mut self.app_core.config.colors);
                    self.persist_color_config();
                    self.app_core.add_system_message("Preset colors saved.");
                }
            }
            if ui.button("Reset").clicked() {
                state.presets_buffer = None;
            }
        });
    }

    fn render_prompts_tab(&mut self, ui: &mut egui::Ui, state: &mut ColorsEditorState) {
        let buffer = state
            .prompts_buffer
            .get_or_insert_with(|| PromptsBuffer::from_config(&self.app_core.config.colors));

        ui.label(
            "Per-character prompt colors. The character is the prompt glyph to \
             match (e.g. >, R, S).",
        );
        if ui.button("Add prompt color").clicked() {
            buffer.rows.push(PromptRow {
                character: String::new(),
                fg: String::new(),
                bg: String::new(),
            });
        }
        ui.separator();

        let mut delete_row: Option<usize> = None;
        egui::ScrollArea::vertical()
            .id_salt("prompts_scroll")
            .auto_shrink([false, false])
            .max_height(300.0)
            .show(ui, |ui| {
                egui::Grid::new("prompts_grid")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Character");
                        ui.strong("Foreground");
                        ui.strong("Background");
                        ui.label("");
                        ui.end_row();
                        for (i, row) in buffer.rows.iter_mut().enumerate() {
                            ui.add(
                                egui::TextEdit::singleline(&mut row.character)
                                    .desired_width(80.0)
                                    .hint_text("e.g. >"),
                            );
                            color_field(ui, &mut row.fg);
                            color_field(ui, &mut row.bg);
                            if ui.small_button("Delete").clicked() {
                                delete_row = Some(i);
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(i) = delete_row {
            buffer.rows.remove(i);
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                if let Some(buffer) = state.prompts_buffer.take() {
                    buffer.apply(&mut self.app_core.config.colors);
                    self.persist_color_config();
                    self.app_core.add_system_message("Prompt colors saved.");
                }
            }
            if ui.button("Reset").clicked() {
                state.prompts_buffer = None;
            }
        });
    }

    /// Generate tab: seed an entire readable preset set from the active
    /// theme. Clicking around only changes the live preview; Apply writes
    /// presets + palette entries (and the recipe) through the shared
    /// ColorConfig layer.
    fn render_generate_tab(&mut self, ui: &mut egui::Ui, state: &mut ColorsEditorState) {
        use crate::core::harmony::{self, Scheme};

        let app_theme = self.app_core.config.get_theme();
        let background = app_theme.background_primary.to_hex();
        let swatches = app_theme.seed_swatches();
        let stored_recipe = self
            .app_core
            .config
            .colors
            .harmony
            .clone()
            .filter(|r| r.background.eq_ignore_ascii_case(&background));

        let gen = state
            .generate
            .get_or_insert_with(|| HarmonyGenState::init(stored_recipe, &swatches));

        ui.label(format!(
            "Seed a full set of readable game-text colors from the '{}' theme.",
            app_theme.name
        ));
        ui.weak(
            "Every chip is a safe seed against this theme's background. \
             Click around - nothing is saved until Apply.",
        );
        ui.add_space(4.0);

        ui.horizontal_wrapped(|ui| {
            ui.label("Seed:");
            for hex in &swatches {
                let selected = !gen.use_custom && gen.seed.eq_ignore_ascii_case(hex);
                if seed_chip(ui, hex, selected) {
                    gen.seed = hex.clone();
                    gen.use_custom = false;
                }
            }
            if ui.selectable_label(gen.use_custom, "custom").clicked() {
                gen.use_custom = true;
            }
            let before = gen.custom_seed.clone();
            color_field(ui, &mut gen.custom_seed);
            if gen.custom_seed != before {
                gen.use_custom = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Scheme:");
            egui::ComboBox::from_id_salt("harmony_scheme")
                .selected_text(gen.scheme.name())
                .show_ui(ui, |ui| {
                    for scheme in Scheme::ALL {
                        ui.selectable_value(
                            &mut gen.scheme,
                            scheme,
                            format!("{} - {}", scheme.name(), scheme.description()),
                        );
                    }
                });
            ui.weak(gen.scheme.description());
        });

        tier_row(
            ui,
            "Variance:",
            "how far the scheme spreads its hues apart",
            &mut gen.variance,
            &[("Low", 0.7), ("Medium", 1.0), ("High", 1.4)],
        );
        tier_row(
            ui,
            "Contrast:",
            "how hard each color reads against the background",
            &mut gen.min_contrast,
            &[("Low", 3.0), ("Medium", 4.5), ("High", 7.0)],
        );
        tier_row(
            ui,
            "Separation:",
            "how far apart two colors are pushed from each other",
            &mut gen.separation,
            &[("Low", 0.04), ("Medium", 0.09), ("High", 0.15)],
        );
        tier_row(
            ui,
            "Room title:",
            "low reads as a subtle plate, high as a hard label",
            &mut gen.room_title_spread,
            &[("Low", 2.5), ("High", 7.0)],
        );
        ui.separator();

        // Live preview: regenerated every frame (the engine is trivially
        // cheap), rendered on the actual theme background.
        let params = gen.params(&background);
        let result = harmony::generate(&params);
        let bg_fill = theme::resolve_color(&background).unwrap_or(egui::Color32::BLACK);
        let mut apply_clicked = false;
        egui::ScrollArea::vertical()
            .id_salt("harmony_preview_scroll")
            .auto_shrink([false, true])
            .max_height(240.0)
            .show(ui, |ui| {
                egui::Frame::default()
                    .fill(bg_fill)
                    .inner_margin(egui::Margin::same(8))
                    .corner_radius(3.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(story_preview(&app_theme, &result));
                        ui.add_space(6.0);
                        ui.separator();
                        for (role, hex) in &result.colors {
                            let Some(color) = theme::resolve_color(hex) else {
                                continue;
                            };
                            ui.horizontal(|ui| {
                                // Clicking the swatch opens alternate hues
                                // for this role.
                                let exploring = gen.explore.as_deref() == Some(role.as_str());
                                if seed_chip(ui, hex, exploring) {
                                    gen.explore = if exploring { None } else { Some(role.clone()) };
                                }
                                let text = egui::RichText::new(format!("{role:<17}"))
                                    .monospace()
                                    .color(color);
                                let description = harmony::ROLES
                                    .iter()
                                    .find(|r| r.name == role)
                                    .map(|r| r.description)
                                    .unwrap_or("");
                                if role == "roomName" {
                                    let plate = theme::resolve_color(&result.room_bg)
                                        .unwrap_or(egui::Color32::BLACK);
                                    ui.label(text.background_color(plate))
                                        .on_hover_text(description);
                                } else {
                                    ui.label(text).on_hover_text(description);
                                }
                                ui.label(
                                    egui::RichText::new(hex.to_string())
                                        .monospace()
                                        .weak()
                                        .color(color),
                                );
                                let contrast = harmony::wcag_contrast(hex, &background);
                                let ok = contrast >= params.min_contrast - 0.35;
                                ui.label(
                                    egui::RichText::new(format!("{contrast:.1}:1"))
                                        .monospace()
                                        .color(if ok {
                                            egui::Color32::from_rgb(0x7f, 0xb8, 0xa4)
                                        } else {
                                            egui::Color32::from_rgb(0xc0, 0x56, 0x4a)
                                        }),
                                );
                                if gen.pins.contains_key(role)
                                    && ui
                                        .small_button("pinned")
                                        .on_hover_text("Click to unpin; rerolls with the set")
                                        .clicked()
                                {
                                    gen.pins.remove(role);
                                }
                            });
                            if gen.explore.as_deref() == Some(role.as_str()) {
                                ui.horizontal_wrapped(|ui| {
                                    ui.weak("other hues:");
                                    for variant in harmony::hue_variants(role, &params) {
                                        if seed_chip(ui, &variant, false) {
                                            // Choosing a hue pins the role so
                                            // regeneration keeps it and
                                            // re-harmonizes the rest.
                                            gen.pins.insert(role.clone(), variant);
                                            gen.explore = None;
                                        }
                                    }
                                });
                            }
                        }
                        // Prompt indicators: hue-anchored to their semantic
                        // bands (alarm stays warm), so no explorer or pins.
                        ui.add_space(4.0);
                        for (character, hex) in &result.prompts {
                            let Some(color) = theme::resolve_color(hex) else {
                                continue;
                            };
                            ui.horizontal(|ui| {
                                let _ = seed_chip(ui, hex, false);
                                let label = harmony::PROMPT_ROLES
                                    .iter()
                                    .find(|r| r.character == character)
                                    .map(|r| r.label)
                                    .unwrap_or("prompt");
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:<17}",
                                        format!("{label} '{character}'")
                                    ))
                                    .monospace()
                                    .color(color),
                                );
                                ui.label(
                                    egui::RichText::new(hex.to_string())
                                        .monospace()
                                        .weak()
                                        .color(color),
                                );
                                let contrast = harmony::wcag_contrast(hex, &background);
                                let ok = contrast >= params.min_contrast - 0.35;
                                ui.label(
                                    egui::RichText::new(format!("{contrast:.1}:1"))
                                        .monospace()
                                        .color(if ok {
                                            egui::Color32::from_rgb(0x7f, 0xb8, 0xa4)
                                        } else {
                                            egui::Color32::from_rgb(0xc0, 0x56, 0x4a)
                                        }),
                                );
                            });
                        }
                    });
            });

        // Matching skin: panel + frame images rendered from the same
        // harmony, so the chrome and the words belong together.
        let mut skin_write: Option<(
            String,
            crate::core::harmony_skin::SkinColors,
            crate::core::harmony_skin::PanelSpec,
            crate::core::harmony_skin::FrameSpec,
        )> = None;
        ui.separator();
        egui::CollapsingHeader::new("Matching skin")
            .default_open(false)
            .show(ui, |ui| {
                use crate::core::harmony_skin::{
                    render_frame, render_panel, GradientKind, SkinColors,
                };
                ui.weak(
                    "Panel and frame images colored from this harmony. Written to \
                     global/skins/<name>/; activate with .setskin <name>.",
                );
                ui.horizontal(|ui| {
                    ui.label("Gradient:");
                    egui::ComboBox::from_id_salt("harmony_skin_gradient")
                        .selected_text(gen.skin_panel.gradient.name())
                        .show_ui(ui, |ui| {
                            for kind in GradientKind::ALL {
                                ui.selectable_value(
                                    &mut gen.skin_panel.gradient,
                                    kind,
                                    kind.name(),
                                );
                            }
                        });
                });
                ui.add(
                    egui::Slider::new(&mut gen.skin_panel.fade_depth, 0.0..=60.0)
                        .text("fade depth"),
                );
                ui.add(
                    egui::Slider::new(&mut gen.skin_panel.vignette, 0.0..=90.0).text("vignette"),
                );
                ui.add(
                    egui::Slider::new(&mut gen.skin_panel.scanlines, 0.0..=40.0).text("scanlines"),
                );
                ui.add(egui::Slider::new(&mut gen.skin_frame.width, 0.0..=6.0).text("frame line"));
                ui.add(
                    egui::Slider::new(&mut gen.skin_frame.radius, 0.0..=14.0).text("corner radius"),
                );
                ui.add(
                    egui::Slider::new(&mut gen.skin_frame.stub, 0.0..=14.0).text("accent stubs"),
                );
                ui.add(egui::Slider::new(&mut gen.skin_frame.slice, 1.0..=15.0).text("slice"));
                if gen.skin_frame.slice_clips_corners() {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "slice is under radius + line - corners will clip",
                    );
                }

                let colors =
                    SkinColors::derive(&params.background, &params.seed, gen.skin_panel.fade_depth);
                // Regenerate the four preview textures only when an input
                // changed; uploading textures every frame would thrash.
                let hash = {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    format!("{:?}{:?}{:?}", colors, gen.skin_panel, gen.skin_frame)
                        .hash(&mut hasher);
                    hasher.finish()
                };
                if gen.skin_preview.as_ref().map(|(h, _)| *h) != Some(hash) {
                    let jobs: [(&str, u32, Vec<u8>); 4] = [
                        (
                            "harmony_skin_panel",
                            96,
                            render_panel(
                                96,
                                &colors.panel_top,
                                &colors.panel_bottom,
                                &gen.skin_panel,
                            ),
                        ),
                        (
                            "harmony_skin_deep",
                            96,
                            render_panel(
                                96,
                                &crate::core::harmony_skin::darken(
                                    &colors.panel_top,
                                    crate::core::harmony_skin::DEEP_OFFSET,
                                ),
                                &crate::core::harmony_skin::darken(
                                    &colors.panel_bottom,
                                    crate::core::harmony_skin::DEEP_OFFSET,
                                ),
                                &gen.skin_panel,
                            ),
                        ),
                        (
                            "harmony_skin_frame",
                            64,
                            render_frame(64, &colors.line, None, &gen.skin_frame),
                        ),
                        (
                            "harmony_skin_frame_accent",
                            64,
                            render_frame(64, &colors.line, Some(&colors.accent), &gen.skin_frame),
                        ),
                    ];
                    let textures = jobs
                        .into_iter()
                        .map(|(id, size, rgba)| {
                            ui.ctx().load_texture(
                                id,
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [size as usize, size as usize],
                                    &rgba,
                                ),
                                egui::TextureOptions::NEAREST,
                            )
                        })
                        .collect();
                    gen.skin_preview = Some((hash, textures));
                }
                if let Some((_, textures)) = &gen.skin_preview {
                    ui.horizontal(|ui| {
                        for texture in textures {
                            ui.image((texture.id(), egui::vec2(64.0, 64.0)));
                        }
                    });
                    ui.weak("panel - deep panel - frame - accented frame");
                }

                ui.horizontal(|ui| {
                    ui.label("Skin name:");
                    ui.add(egui::TextEdit::singleline(&mut gen.skin_name).desired_width(140.0));
                    if ui.button("Write skin files").clicked() {
                        skin_write = Some((
                            gen.skin_name.clone(),
                            colors.clone(),
                            gen.skin_panel.clone(),
                            gen.skin_frame.clone(),
                        ));
                    }
                });
            });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                apply_clicked = true;
            }
            if ui.button("Reset").clicked() {
                state.generate = None;
            }
            ui.weak("Apply rewrites the preset colors; the previous colors.toml is kept as .bak.");
        });

        if let Some((name, colors, panel, frame)) = skin_write {
            self.write_harmony_skin_files(&name, &params, &colors, &panel, &frame);
        }

        if apply_clicked {
            let recipe = crate::config::HarmonyRecipe {
                seed: params.seed.clone(),
                background: params.background.clone(),
                scheme: params.scheme.name().to_string(),
                variance: params.variance,
                min_contrast: params.min_contrast,
                separation: params.separation,
                room_title_spread: params.room_title_spread,
                pins: params.pins.clone(),
            };
            let character = self.app_core.config.character.clone();
            match ColorConfig::persist_generated_presets(
                &result.colors,
                &result.room_bg,
                &result.prompts,
                &recipe,
                character.as_deref(),
            ) {
                Ok(()) => {
                    self.app_core.reload_colors();
                    // Invalidate the Presets tab's edit buffer so it re-reads
                    // the freshly generated values instead of stale rows.
                    state.presets_buffer = None;
                    self.app_core.add_system_message(&format!(
                        "Harmony presets applied ({} from seed {}).",
                        params.scheme.name(),
                        params.seed
                    ));
                }
                Err(err) => {
                    self.app_core
                        .add_system_message(&format!("Failed to apply harmony presets: {}", err));
                }
            }
        }
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
                self.app_core
                    .add_system_message("Spell color range deleted.");
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
            .order(egui::Order::Foreground)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_buffer_round_trips_and_drops_blanks() {
        // Start from an empty preset map so the assertion isn't perturbed by
        // the built-in defaults ColorConfig::default() ships.
        let mut colors = ColorConfig::default();
        colors.presets.clear();
        colors.presets.insert(
            "combat".to_string(),
            PresetColor {
                fg: Some("#ff0000".into()),
                bg: None,
            },
        );
        let mut buf = PresetsBuffer::from_config(&colors);
        assert_eq!(buf.rows.len(), 1);
        // Add a bg, and add a blank-named row that must be dropped on save.
        buf.rows[0].bg = "#111111".into();
        buf.rows.push(PresetRow {
            name: "  ".into(),
            fg: "#abc".into(),
            bg: String::new(),
        });

        let mut out = ColorConfig::default();
        buf.apply(&mut out);
        assert_eq!(out.presets.len(), 1, "blank-named row dropped, combat kept");
        let p = out.presets.get("combat").expect("combat preset kept");
        assert_eq!(p.fg.as_deref(), Some("#ff0000"));
        assert_eq!(p.bg.as_deref(), Some("#111111"));
    }

    #[test]
    fn presets_duplicate_name_last_wins() {
        let mut colors = ColorConfig::default();
        let mut buf = PresetsBuffer {
            rows: vec![
                PresetRow {
                    name: "x".into(),
                    fg: "#111111".into(),
                    bg: String::new(),
                },
                PresetRow {
                    name: "x".into(),
                    fg: "#222222".into(),
                    bg: String::new(),
                },
            ],
        };
        buf.rows[0].name = "x".into(); // ensure identical
        let mut out = ColorConfig::default();
        buf.apply(&mut out);
        assert_eq!(out.presets.len(), 1);
        assert_eq!(
            out.presets["x"].fg.as_deref(),
            Some("#222222"),
            "last row wins"
        );
    }

    #[test]
    fn prompts_buffer_folds_legacy_color_into_fg() {
        let mut colors = ColorConfig::default();
        colors.prompt_colors = vec![PromptColor {
            character: ">".into(),
            fg: None,
            bg: None,
            color: Some("#00ff00".into()), // legacy-only value
        }];
        let buf = PromptsBuffer::from_config(&colors);
        assert_eq!(buf.rows[0].fg, "#00ff00", "legacy color surfaces as fg");

        let mut out = ColorConfig::default();
        buf.apply(&mut out);
        assert_eq!(out.prompt_colors.len(), 1);
        assert_eq!(out.prompt_colors[0].fg.as_deref(), Some("#00ff00"));
        assert!(
            out.prompt_colors[0].color.is_none(),
            "legacy field cleared on save"
        );
    }

    #[test]
    fn prompts_buffer_drops_blank_character_rows() {
        let buf = PromptsBuffer {
            rows: vec![
                PromptRow {
                    character: ">".into(),
                    fg: "#fff".into(),
                    bg: String::new(),
                },
                PromptRow {
                    character: "   ".into(),
                    fg: "#000".into(),
                    bg: String::new(),
                },
            ],
        };
        let mut out = ColorConfig::default();
        buf.apply(&mut out);
        assert_eq!(out.prompt_colors.len(), 1, "blank-character row dropped");
        assert_eq!(out.prompt_colors[0].character, ">");
    }
}
