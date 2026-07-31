//! Settings editor: a registry-driven UI over every setting in
//! `config/registry.rs`, grouped by category. Edits buffer into per-key
//! drafts; Save applies only the CHANGED keys through the registry setters
//! and persists each one sparsely via `Config::save_single_setting` with a
//! per-key Global/Character scope choice.
//!
//! Bespoke (non-registry) sections kept alongside the generated rows:
//! GUI sizing (per-character GUI layout file, not config.toml), the Map
//! download button/status, skin management actions, the TTS test button,
//! and pronunciation substitutions (registry-exempt structured data).
//!
//! Widget-shaped settings moved to the Window Editor (editors/windows.rs),
//! next to the window they configure: the Vitals bar options and the
//! global Targets category.

use super::super::VellumGuiApp;
use crate::config::registry::{self, SettingDef, SettingKind, SettingScope, SettingValue};
use crate::config::Config;
use eframe::egui;
use std::collections::HashMap;

/// ComboBox entry meaning "no skin active" (draft value = empty string).
const NONE_SKIN: &str = "(none)";

/// ComboBox entry meaning "engine default voice" (draft value = empty string).
const NONE_VOICE: &str = "(engine default)";

/// Preferred category display order. Categories the registry grows later
/// (not listed here) render after these, alphabetically — nothing is ever
/// silently dropped.
const CATEGORY_ORDER: &[&str] = &[
    "Connection",
    "Appearance",
    "UI",
    "Terminal",
    "Selection",
    "Focus",
    "Sound",
    "Speech",
    "Highlights",
    "Targets",
    "Streams",
    "Logging",
    "Performance",
    "Web",
    "Map",
    "Travel",
];

/// One setting's edit buffer, shaped for the widget that edits it.
#[derive(Clone, Debug, PartialEq)]
enum DraftValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    /// Text, OptionalText, and Enum kinds (empty = unset for OptionalText).
    Text(String),
    /// List entries as a multiline buffer, one entry per line.
    List(String),
}

/// Build the draft for one setting from the live config value.
fn draft_from_config(def: &SettingDef, config: &Config) -> DraftValue {
    match (def.get)(config) {
        SettingValue::Bool(v) => DraftValue::Bool(v),
        SettingValue::Int(v) => DraftValue::Int(v),
        SettingValue::Float(v) => DraftValue::Float(v),
        SettingValue::Text(v) => DraftValue::Text(v),
        SettingValue::List(v) => DraftValue::List(v.join("\n")),
    }
}

/// Drafts for every registered setting.
fn drafts_from_config(config: &Config) -> HashMap<&'static str, DraftValue> {
    registry::registry()
        .iter()
        .map(|def| (def.key, draft_from_config(def, config)))
        .collect()
}

/// Multiline list buffer -> entries: split lines, trim, drop empties.
fn parse_list_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Convert an edit buffer back into the typed value the registry setter
/// expects. OptionalText trims (the setter treats empty as "unset"), so
/// change detection agrees with what a save would actually store.
fn value_from_draft(kind: &SettingKind, draft: &DraftValue) -> SettingValue {
    match draft {
        DraftValue::Bool(v) => SettingValue::Bool(*v),
        DraftValue::Int(v) => SettingValue::Int(*v),
        DraftValue::Float(v) => SettingValue::Float(*v),
        DraftValue::Text(v) => match kind {
            SettingKind::OptionalText => SettingValue::Text(v.trim().to_string()),
            _ => SettingValue::Text(v.clone()),
        },
        DraftValue::List(v) => SettingValue::List(parse_list_lines(v)),
    }
}

/// Keys whose draft differs from the live config value (sorted, because the
/// registry itself is key-sorted).
fn changed_keys(
    config: &Config,
    drafts: &HashMap<&'static str, DraftValue>,
) -> Vec<&'static str> {
    registry::registry()
        .iter()
        .filter_map(|def| {
            let draft = drafts.get(def.key)?;
            let desired = value_from_draft(&def.kind, draft);
            (desired != (def.get)(config)).then_some(def.key)
        })
        .collect()
}

/// Categories deliberately NOT rendered by this GUI Settings window: their
/// settings are edited in the Window Editor (editors/windows.rs) next to
/// the window they configure. Targets lives in the Window Editor's
/// "Targets (global)" section on any targets window. The TUI settings
/// editor still shows these categories.
const CATEGORIES_IN_WINDOW_EDITOR: &[&str] = &["Targets"];

/// Individual registry keys deliberately NOT rendered by this GUI Settings
/// window. `streams.drop_unsubscribed` is legacy: config load migrates its
/// entries into `[streams.routes]` and clears the list, so at runtime the
/// row would always be an empty list that a save immediately re-empties.
/// Per-stream routing is edited in the Streams panel instead. The registry
/// entry itself must stay (old config files load through it, and the TUI
/// settings editor is unaffected).
const HIDDEN_KEYS: &[&str] = &["streams.drop_unsubscribed"];

/// All categories present in the registry, in display order (minus those
/// the Window Editor owns in the GUI).
fn ordered_categories() -> Vec<&'static str> {
    let mut categories: Vec<&'static str> = CATEGORY_ORDER
        .iter()
        .copied()
        .filter(|cat| !CATEGORIES_IN_WINDOW_EDITOR.contains(cat))
        .filter(|cat| registry::registry().iter().any(|def| def.category == *cat))
        .collect();
    let mut extras: Vec<&'static str> = registry::registry()
        .iter()
        .map(|def| def.category)
        .filter(|cat| {
            !CATEGORY_ORDER.contains(cat) && !CATEGORIES_IN_WINDOW_EDITOR.contains(cat)
        })
        .collect();
    extras.sort_unstable();
    extras.dedup();
    categories.extend(extras);
    categories
}

/// Buffered edit state, initialized from Config when the editor opens and
/// applied back per changed key on Save.
pub(in super::super) struct SettingsEditorState {
    /// Per-key edit buffers for every registered setting.
    drafts: HashMap<&'static str, DraftValue>,
    /// Per-key save scope for GlobalOrCharacter settings: true = global.
    /// Defaults to character (matching the old editor); not persisted.
    scopes: HashMap<&'static str, bool>,
    /// Per-key errors from the last Save attempt.
    errors: Vec<String>,
    theme_names: Vec<String>,
    skin_names: Vec<String>,
    /// Buffer for the "New skin" name field.
    new_skin_name: String,
    /// Inline error from the last "Create" attempt.
    skin_error: Option<String>,
    /// Voices the engine reported when the editor opened (empty when TTS
    /// is off or the platform doesn't enumerate).
    tts_voices: Vec<String>,
    /// Pronunciation substitutions (pattern, replacement) — registry-exempt
    /// structured data with bespoke rows.
    tts_subs: Vec<(String, String)>,
    /// GUI sizing settings; persisted in the per-character GUI layout file,
    /// not config.toml.
    gui_settings: crate::frontend::gui::persistence::GuiUiSettings,
}

impl SettingsEditorState {
    fn from_config(
        config: &Config,
        theme_names: Vec<String>,
        skin_names: Vec<String>,
        tts_voices: Vec<String>,
        gui_settings: crate::frontend::gui::persistence::GuiUiSettings,
    ) -> Self {
        Self {
            drafts: drafts_from_config(config),
            scopes: HashMap::new(),
            errors: Vec::new(),
            theme_names,
            skin_names,
            new_skin_name: String::new(),
            skin_error: None,
            tts_voices,
            tts_subs: config
                .tts
                .substitutions
                .iter()
                .map(|sub| (sub.pattern.clone(), sub.replacement.clone()))
                .collect(),
            gui_settings,
        }
    }

    /// Current draft text for a Text/OptionalText/Enum setting.
    fn text_draft(&self, key: &str) -> String {
        match self.drafts.get(key) {
            Some(DraftValue::Text(v)) => v.clone(),
            _ => String::new(),
        }
    }

    /// Current draft value for a Float setting.
    fn float_draft(&self, key: &str) -> f64 {
        match self.drafts.get(key) {
            Some(DraftValue::Float(v)) => *v,
            _ => 0.0,
        }
    }

    /// Generated rows for every registered setting in `category`:
    /// label | value widget | scope choice.
    fn render_category_grid(&mut self, ui: &mut egui::Ui, category: &str) {
        egui::Grid::new(format!("settings_grid_{category}"))
            .num_columns(3)
            .show(ui, |ui| {
                let defs: Vec<&'static SettingDef> = registry::registry()
                    .iter()
                    .filter(|def| def.category == category)
                    .filter(|def| !HIDDEN_KEYS.contains(&def.key))
                    .collect();
                for def in defs {
                    self.render_setting_row(ui, def);
                    ui.end_row();
                }
            });
    }

    fn render_setting_row(&mut self, ui: &mut egui::Ui, def: &'static SettingDef) {
        ui.label(def.label).on_hover_text(def.description);
        self.render_value_widget(ui, def);
        match def.scope {
            SettingScope::GlobalOrCharacter => {
                let is_global = self.scopes.entry(def.key).or_insert(false);
                egui::ComboBox::from_id_salt(("setting_scope", def.key))
                    .width(96.0)
                    .selected_text(if *is_global { "Global" } else { "Character" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(is_global, false, "Character");
                        ui.selectable_value(is_global, true, "Global");
                    })
                    .response
                    .on_hover_text("Where Save writes this setting");
            }
            SettingScope::CharacterOnly => {
                ui.weak("character").on_hover_text(
                    "This setting only makes sense per character and always \
                     saves to the character profile.",
                );
            }
        }
    }

    fn render_value_widget(&mut self, ui: &mut egui::Ui, def: &'static SettingDef) {
        let Some(draft) = self.drafts.get_mut(def.key) else {
            ui.weak("?");
            return;
        };

        // A few Text settings have a known value population; give them combo
        // boxes instead of free text (still the same draft + save path).
        match def.key {
            "active_theme" => {
                if let DraftValue::Text(value) = draft {
                    egui::ComboBox::from_id_salt(("setting", def.key))
                        .selected_text(value.clone())
                        .show_ui(ui, |ui| {
                            for name in &self.theme_names {
                                ui.selectable_value(value, name.clone(), name);
                            }
                        });
                }
                return;
            }
            "active_skin" => {
                if let DraftValue::Text(value) = draft {
                    let selected = if value.trim().is_empty() {
                        NONE_SKIN.to_string()
                    } else {
                        value.clone()
                    };
                    egui::ComboBox::from_id_salt(("setting", def.key))
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(value.trim().is_empty(), NONE_SKIN)
                                .clicked()
                            {
                                value.clear();
                            }
                            for name in &self.skin_names {
                                if ui.selectable_label(value == name, name).clicked() {
                                    *value = name.clone();
                                }
                            }
                        });
                }
                return;
            }
            "tts.voice" if !self.tts_voices.is_empty() => {
                if let DraftValue::Text(value) = draft {
                    let selected = if value.trim().is_empty() {
                        NONE_VOICE.to_string()
                    } else {
                        value.clone()
                    };
                    egui::ComboBox::from_id_salt(("setting", def.key))
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(value.trim().is_empty(), NONE_VOICE)
                                .clicked()
                            {
                                value.clear();
                            }
                            for name in &self.tts_voices {
                                if ui.selectable_label(value == name, name).clicked() {
                                    *value = name.clone();
                                }
                            }
                        });
                }
                return;
            }
            _ => {}
        }

        // Text inputs get a firm size: inside a Grid, egui's first-pass
        // sizing can collapse a TextEdit's desired_width to near zero and
        // the grid then remembers the squeezed column. add_sized wins.
        let input_height = ui.spacing().interact_size.y;
        match draft {
            DraftValue::Bool(value) => {
                ui.checkbox(value, "");
            }
            DraftValue::Int(value) => {
                if let SettingKind::Int { min, max } = def.kind {
                    ui.add(egui::DragValue::new(value).range(min..=max));
                } else {
                    ui.add(egui::DragValue::new(value));
                }
            }
            DraftValue::Float(value) => {
                if let SettingKind::Float { min, max } = def.kind {
                    ui.add(egui::Slider::new(value, min..=max));
                } else {
                    ui.add(egui::DragValue::new(value));
                }
            }
            DraftValue::Text(value) => match &def.kind {
                SettingKind::Enum { options } => {
                    egui::ComboBox::from_id_salt(("setting", def.key))
                        .selected_text(value.clone())
                        .show_ui(ui, |ui| {
                            for option in *options {
                                ui.selectable_value(value, option.to_string(), *option);
                            }
                        });
                }
                _ => {
                    let response = ui.add_sized(
                        [260.0, input_height],
                        egui::TextEdit::singleline(value).password(def.sensitive),
                    );
                    if matches!(def.kind, SettingKind::OptionalText) {
                        response.on_hover_text("empty = unset");
                    }
                }
            },
            DraftValue::List(value) => {
                ui.add_sized(
                    [260.0, input_height * 3.6],
                    egui::TextEdit::multiline(value).desired_rows(3),
                )
                .on_hover_text("one entry per line");
            }
        }
    }
}

/// One-line context for generated categories whose purpose isn't obvious
/// from the rows alone.
fn category_intro(category: &str) -> Option<&'static str> {
    match category {
        "Highlights" => Some(
            "Master switches for the highlight system: turn a whole feature off \
             without deleting any patterns. The patterns themselves live in the \
             Highlights editor.",
        ),
        "Focus" => Some(
            "TUI only: which windows Tab cycles through in the terminal \
             frontend. The GUI ignores these.",
        ),
        "Terminal" => Some("TUI only: how the terminal renders colors."),
        "Streams" => Some(
            "Game text streams that no window subscribes to go to the \
             fallback window. Per-stream routing (discard, main, or a \
             specific window) is edited in the Streams panel.",
        ),
        "Performance" => Some("Debug overlay (F3 in the TUI)."),
        _ => None,
    }
}

/// Bespoke GUI sizing section (per-character GUI layout file, not
/// config.toml). The Vitals bar options that used to follow this grid moved
/// to the Window Editor (editors/windows.rs), rendered when editing the
/// vitals window itself.
fn render_gui_section(
    ui: &mut egui::Ui,
    gui_settings: &mut crate::frontend::gui::persistence::GuiUiSettings,
) {
    ui.label(
        "Sizing applies to the GUI only and is saved per character. \
         Ctrl+= / Ctrl+- / Ctrl+0 also adjust zoom anytime.",
    );
    egui::Grid::new("settings_gui_grid").num_columns(2).show(ui, |ui| {
        ui.label("UI zoom");
        ui.add(egui::Slider::new(&mut gui_settings.zoom_factor, 0.5..=3.0).step_by(0.05));
        ui.end_row();
        ui.label("Text size");
        ui.add(egui::Slider::new(&mut gui_settings.text_size, 8.0..=32.0).step_by(0.5));
        ui.end_row();
        ui.label("Title bar size");
        ui.add(egui::Slider::new(&mut gui_settings.title_font_size, 8.0..=40.0).step_by(0.5))
            .on_hover_text("Title text size in points.");
        ui.end_row();
        ui.label("Title bar height");
        ui.add(egui::Slider::new(&mut gui_settings.title_bar_height, 0.0..=32.0).step_by(1.0))
            .on_hover_text(
                "Exact title bar height for game windows; the title text \
                 keeps its own size and is vertically centered. \
                 0 = follow the title text size.",
            );
        ui.end_row();
        ui.label("Title alignment");
        egui::ComboBox::from_id_salt("settings_title_bar_align")
            .selected_text(match gui_settings.title_bar_align.as_str() {
                "left" => "Left",
                "right" => "Right",
                _ => "Center",
            })
            .show_ui(ui, |ui| {
                for (value, label) in
                    [("left", "Left"), ("center", "Center"), ("right", "Right")]
                {
                    if ui
                        .selectable_label(gui_settings.title_bar_align == value, label)
                        .clicked()
                    {
                        gui_settings.title_bar_align = value.to_string();
                    }
                }
            });
        ui.end_row();
        ui.label("Effect bar height");
        ui.add(egui::Slider::new(&mut gui_settings.effects_bar_height, 10.0..=60.0).step_by(1.0));
        ui.end_row();
        ui.label("Hand icon size");
        ui.add(egui::Slider::new(&mut gui_settings.hand_icon_size, 16.0..=48.0).step_by(1.0))
            .on_hover_text(
                "Size of the left/right/spell hand icons in points. \
                 Hand rows grow to fit.",
            );
        ui.end_row();
        ui.label("Density");
        ui.add(egui::Slider::new(&mut gui_settings.density, 0.5..=2.0).step_by(0.05))
            .on_hover_text(
                "Spacing and padding scale. Lower = denser \
                 (Wrayth-like), higher = more comfortable.",
            );
        ui.end_row();
        ui.label("Bar corners");
        ui.add(egui::Slider::new(&mut gui_settings.bar_corner_radius, 0.0..=12.0).step_by(0.5))
            .on_hover_text(
                "Corner radius for all progress bars. \
                 0 = square (Wrayth-style).",
            );
        ui.end_row();
        ui.label("Window corners");
        ui.add(egui::Slider::new(&mut gui_settings.window_corner_radius, 0.0..=12.0).step_by(0.5))
            .on_hover_text(
                "Corner radius for window frames. \
                 0 = square (Wrayth-style). Windows framed by skin \
                 border art always render square.",
            );
        ui.end_row();
        ui.label("Bar text contrast");
        ui.checkbox(&mut gui_settings.auto_contrast_bar_text, "Auto light/dark")
            .on_hover_text(
                "Switch bar text to light or dark when its \
                 configured color would be unreadable against \
                 the bar fill.",
            );
        ui.end_row();
        ui.label("Zone separators");
        {
            use crate::frontend::gui::persistence::ZoneSeparatorStyle;
            const STYLES: [(ZoneSeparatorStyle, &str); 3] = [
                (ZoneSeparatorStyle::Shown, "Shown"),
                (ZoneSeparatorStyle::Hover, "On hover"),
                (ZoneSeparatorStyle::Hidden, "Hidden"),
            ];
            let current_label = STYLES
                .iter()
                .find(|(style, _)| *style == gui_settings.zone_separators)
                .map(|(_, label)| *label)
                .unwrap_or("Shown");
            egui::ComboBox::from_id_salt("settings_zone_separators")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    for (style, label) in STYLES {
                        if ui
                            .selectable_label(gui_settings.zone_separators == style, label)
                            .clicked()
                        {
                            gui_settings.zone_separators = style;
                        }
                    }
                })
                .response
                .on_hover_text(
                    "Boundary lines between the header/footer/sidebar zones \
                     and the center. Hidden or hover-only keeps the edges \
                     draggable for resizing — handy when skin frames make \
                     the lines look out of place.",
                );
        }
        ui.end_row();
        ui.label("Window snapping");
        ui.checkbox(&mut gui_settings.snap_enabled, "Snap windows to edges")
            .on_hover_text(
                "Center-pane windows snap to each other and to the pane \
                 edges while you drag or resize them. Hold Shift during a \
                 drag to place a window freely.",
            );
        ui.end_row();
        if gui_settings.snap_enabled {
            ui.label("Snap distance");
            ui.add(egui::Slider::new(&mut gui_settings.snap_radius, 0.0..=24.0).step_by(1.0))
                .on_hover_text(
                    "How close an edge must get, in points, before it \
                     snaps. Trackpads and high-DPI displays usually want \
                     more than a mouse. 0 disables snapping.",
                );
            ui.end_row();
            ui.label("Snap targets");
            ui.horizontal(|ui| {
                ui.checkbox(&mut gui_settings.snap_to_siblings, "Other windows")
                    .on_hover_text(
                        "Butt two windows together or align them flush \
                         along the same edge.",
                    );
                ui.checkbox(&mut gui_settings.snap_to_bounds, "Pane edges");
                ui.checkbox(&mut gui_settings.snap_to_centers, "Pane center")
                    .on_hover_text(
                        "The pane's horizontal and vertical center lines. \
                         Off by default: center lines close to real edge \
                         targets make snapping feel jumpy.",
                    );
            });
            ui.end_row();
            ui.label("Snap grid");
            ui.add(egui::Slider::new(&mut gui_settings.snap_grid, 0.0..=48.0).step_by(4.0))
                .on_hover_text(
                    "Also snap edges to a grid of this pitch in points, \
                     anchored at the pane's top-left. While you drag, the \
                     grid shows as a faint overlay. 0 = no grid.",
                );
            ui.end_row();
            ui.label("Grid sizing");
            ui.add_enabled(
                gui_settings.snap_grid > 0.0,
                egui::Checkbox::new(
                    &mut gui_settings.snap_move_sizes_to_grid,
                    "Moving also sizes to grid",
                ),
            )
            .on_hover_text(
                "With a grid set, moving a window pulls each edge to its \
                 nearest grid line — the window resizes to fit the grid. \
                 Off = moving only repositions.",
            );
            ui.end_row();
            ui.label("Snap guides");
            ui.checkbox(&mut gui_settings.snap_show_guides, "Show alignment guides")
                .on_hover_text(
                    "Draw a dashed line with the matched coordinate while \
                     a snap is engaged.",
                );
            ui.end_row();
        }
    });
    ui.weak(
        "Vitals bar options (layout, height, text, bars shown) moved to the \
         vitals window's own editor: right-click the Vitals window and \
         choose Edit Window.",
    );
}

impl VellumGuiApp {
    pub(in super::super) fn open_settings_editor(&mut self) {
        if self.settings_editor.is_some() {
            self.raise_editor(egui::Id::new("gui_settings_editor"));
            return;
        }
        let mut theme_names: Vec<String> = crate::theme::ThemePresets::all_with_custom(
            self.app_core.config.character.as_deref(),
        )
        .into_keys()
        .collect();
        theme_names.sort();
        self.settings_editor = Some(SettingsEditorState::from_config(
            &self.app_core.config,
            theme_names,
            crate::config::skins::list_skins(),
            self.app_core.tts_manager.available_voices(),
            self.ui_settings.clone(),
        ));
    }

    pub(in super::super) fn render_settings_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.settings_editor.take() else {
            return;
        };

        let mut open = true;
        let mut saved = false;
        let mut cancelled = false;
        // Map download actions fire immediately (they're actions, not
        // settings); the repo text itself still applies on Save.
        let mut map_download_repo: Option<String> = None;
        let mut map_remove_clicked = false;
        // Calibration works against the *loaded* skin, so the button only
        // makes sense once the selection has been saved and its doll base
        // art actually loaded.
        let mut calibrate_doll_clicked = false;
        // Test button applies the panel's live values and speaks a sample
        // without waiting for Save.
        let mut tts_test_clicked = false;
        // `.data reload` action from the Data panel (re-resolves the shared
        // item database used by .foreach/.sorter).
        let mut data_reload_clicked = false;
        let can_calibrate_doll = self
            .skin_state
            .widget_art()
            .is_some_and(|art| art.doll_base.is_some());
        let updater_status = self.app_core.map_updater.status.clone();
        let updater_installed = self.app_core.map_updater.installed.clone();
        // Data-pack asset status (source tier + age), computed before the
        // UI closure borrows self immutably.
        let data_status_lines = crate::core::data_pack::status_lines(
            self.app_core.config.map.lich_dir.as_deref(),
        );
        let updater_in_flight = self.app_core.map_updater.in_flight();
        let saved_target_count = self.app_core.config.go2.saved.len();
        egui::Window::new("Settings")
            .id(egui::Id::new("gui_settings_editor"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_width(440.0)
            .collapsible(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("settings_editor_scroll")
                    .show(ui, |ui| {
                        for category in ordered_categories() {
                            match category {
                                "Connection" => {
                                    ui.collapsing("Connection", |ui| {
                                        ui.label(
                                            "Connection settings are always character-specific.",
                                        );
                                        state.render_category_grid(ui, "Connection");
                                    });
                                }
                                "Appearance" => {
                                    ui.collapsing("Appearance", |ui| {
                                        ui.label(
                                            "Skins add graphics (backgrounds, borders, widget \
                                             art) on top of the theme.",
                                        );
                                        state.render_category_grid(ui, "Appearance");
                                        ui.separator();
                                        ui.horizontal(|ui| {
                                            if ui
                                                .button("Open skins folder")
                                                .on_hover_text(
                                                    "Skins live in ~/.vellum-fe/global/skins/<name>/",
                                                )
                                                .clicked()
                                            {
                                                if let Ok(dir) = Config::skins_dir() {
                                                    let _ = std::fs::create_dir_all(&dir);
                                                    if let Err(err) = crate::platform::open_url(
                                                        &dir.to_string_lossy(),
                                                    ) {
                                                        tracing::warn!(
                                                            "Failed to open skins folder {}: {}",
                                                            dir.display(),
                                                            err
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut state.new_skin_name,
                                                )
                                                .hint_text("new skin name")
                                                .desired_width(140.0),
                                            );
                                            if ui
                                                .button("Create")
                                                .on_hover_text(
                                                    "Write a starter skin.toml (all sections \
                                                     commented out) and select it",
                                                )
                                                .clicked()
                                            {
                                                match crate::config::skins::write_scaffold(
                                                    &state.new_skin_name,
                                                ) {
                                                    Ok(_) => {
                                                        let name = state
                                                            .new_skin_name
                                                            .trim()
                                                            .to_string();
                                                        state.skin_names.push(name.clone());
                                                        state.skin_names.sort();
                                                        state.drafts.insert(
                                                            "active_skin",
                                                            DraftValue::Text(name),
                                                        );
                                                        state.new_skin_name.clear();
                                                        state.skin_error = None;
                                                    }
                                                    Err(err) => {
                                                        state.skin_error =
                                                            Some(err.to_string());
                                                    }
                                                }
                                            }
                                        });
                                        if let Some(error) = &state.skin_error {
                                            ui.colored_label(
                                                ui.visuals().error_fg_color,
                                                error,
                                            );
                                        }
                                        if ui
                                            .add_enabled(
                                                can_calibrate_doll,
                                                egui::Button::new(
                                                    "Calibrate injury doll\u{2026}",
                                                ),
                                            )
                                            .on_hover_text(
                                                "Click each body part on the skin's doll image \
                                                 to place its wound dot",
                                            )
                                            .on_disabled_hover_text(
                                                "Needs an active (saved) skin with base art \
                                                 under [injury_doll] in its skin.toml",
                                            )
                                            .clicked()
                                        {
                                            calibrate_doll_clicked = true;
                                        }
                                    });

                                    // The GUI sizing/vitals section lives in
                                    // the per-character GUI layout file, not
                                    // config.toml; keep it next to Appearance.
                                    ui.collapsing("GUI", |ui| {
                                        render_gui_section(ui, &mut state.gui_settings);
                                    });
                                }
                                "Speech" => {
                                    ui.collapsing("Speech", |ui| {
                                        ui.label(
                                            "Text-to-speech reads incoming lines aloud. Pick \
                                             which windows speak with the checkbox in each \
                                             window's editor; the speak toggles below cover \
                                             the classic three.",
                                        );
                                        state.render_category_grid(ui, "Speech");
                                        if state.tts_voices.is_empty() {
                                            ui.label(
                                                egui::RichText::new(
                                                    "enable TTS, save, and reopen to list voices",
                                                )
                                                .weak(),
                                            );
                                        }
                                        let tts_enabled = matches!(
                                            state.drafts.get("tts.enabled"),
                                            Some(DraftValue::Bool(true))
                                        );
                                        if ui
                                            .add_enabled(
                                                tts_enabled,
                                                egui::Button::new("Test voice"),
                                            )
                                            .on_hover_text(
                                                "Applies the values above and speaks a sample",
                                            )
                                            .clicked()
                                        {
                                            tts_test_clicked = true;
                                        }

                                        ui.separator();
                                        ui.label("Pronunciations (regex pattern -> spoken text)");
                                        let mut remove_sub: Option<usize> = None;
                                        for (index, (pattern, replacement)) in
                                            state.tts_subs.iter_mut().enumerate()
                                        {
                                            ui.horizontal(|ui| {
                                                let valid =
                                                    regex::Regex::new(pattern).is_ok();
                                                ui.add(
                                                    egui::TextEdit::singleline(pattern)
                                                        .hint_text("Wehnimer's")
                                                        .desired_width(140.0),
                                                );
                                                // ▸ not →: U+2192 is tofu in
                                                // the bundled fallback fonts.
                                                ui.label("▸");
                                                ui.add(
                                                    egui::TextEdit::singleline(replacement)
                                                        .hint_text("Wenimers")
                                                        .desired_width(140.0),
                                                );
                                                if !valid {
                                                    ui.colored_label(
                                                        ui.visuals().error_fg_color,
                                                        "invalid",
                                                    );
                                                }
                                                if ui.small_button("✕").clicked() {
                                                    remove_sub = Some(index);
                                                }
                                            });
                                        }
                                        if let Some(index) = remove_sub {
                                            state.tts_subs.remove(index);
                                        }
                                        if ui.button("Add pronunciation").clicked() {
                                            state.tts_subs.push((String::new(), String::new()));
                                        }
                                        ui.label(
                                            egui::RichText::new(
                                                "Queue navigation: Ctrl+Alt+Left/Right = \
                                                 previous/next, Ctrl+Alt+Up = next unread, \
                                                 Ctrl+Alt+Down = stop, F11 = mute. Rebind \
                                                 under Keybinds.",
                                            )
                                            .weak(),
                                        );
                                    });
                                }
                                "Map" => {
                                    ui.collapsing("Map", |ui| {
                                        ui.label(
                                            "Map data comes from a downloaded release or your \
                                             Lich install (data/<game>/map-<timestamp>.json); \
                                             downloaded data wins.",
                                        );
                                        state.render_category_grid(ui, "Map");
                                        let repo_draft = state.text_draft("map.mapdb_repo");
                                        ui.horizontal(|ui| {
                                            let can_download = !updater_in_flight
                                                && !repo_draft.trim().is_empty();
                                            let label = if updater_installed.is_some() {
                                                "Check for update"
                                            } else {
                                                "Download map data"
                                            };
                                            if ui
                                                .add_enabled(
                                                    can_download,
                                                    egui::Button::new(label),
                                                )
                                                .clicked()
                                            {
                                                map_download_repo =
                                                    Some(repo_draft.trim().to_string());
                                            }
                                            if updater_installed.is_some()
                                                && ui
                                                    .add_enabled(
                                                        !updater_in_flight,
                                                        egui::Button::new("Remove downloaded"),
                                                    )
                                                    .on_hover_text(
                                                        "Delete downloaded map data and fall \
                                                         back to the Lich folder",
                                                    )
                                                    .clicked()
                                            {
                                                map_remove_clicked = true;
                                            }
                                        });
                                        use crate::core::mapdb_update::UpdateStatus;
                                        let status_text = match &updater_status {
                                            UpdateStatus::Idle => match &updater_installed {
                                                Some(tag) => format!("Installed: {tag}"),
                                                None => "No downloaded map data".to_string(),
                                            },
                                            UpdateStatus::Checking => {
                                                "Checking for the latest release...".to_string()
                                            }
                                            UpdateStatus::Downloading {
                                                tag,
                                                received,
                                                total,
                                            } => match total {
                                                Some(total) => format!(
                                                    "Downloading {tag}: {:.1} / {:.1} MB",
                                                    *received as f64 / 1e6,
                                                    *total as f64 / 1e6
                                                ),
                                                None => format!(
                                                    "Downloading {tag}: {:.1} MB",
                                                    *received as f64 / 1e6
                                                ),
                                            },
                                            UpdateStatus::UpToDate { tag } => {
                                                format!("Up to date ({tag})")
                                            }
                                            UpdateStatus::Updated { tag } => {
                                                format!("Installed {tag}")
                                            }
                                            UpdateStatus::Failed(e) => {
                                                format!("Update failed: {e}")
                                            }
                                        };
                                        if matches!(updater_status, UpdateStatus::Failed(_)) {
                                            ui.colored_label(
                                                ui.visuals().error_fg_color,
                                                status_text,
                                            );
                                        } else {
                                            ui.label(egui::RichText::new(status_text).weak());
                                        }
                                    });
                                }
                                "Travel" => {
                                    ui.collapsing("Travel", |ui| {
                                        ui.label(
                                            "Native .go2 walks the map without Lich — travel \
                                             with .go2 <room|tag|name>, stop with .go2 stop.",
                                        );
                                        state.render_category_grid(ui, "Travel");
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Saved targets: {saved_target_count} (manage \
                                                 with .go2 save / .go2 targets)"
                                            ))
                                            .weak(),
                                        );
                                    });
                                }
                                other => {
                                    ui.collapsing(other, |ui| {
                                        if let Some(intro) = category_intro(other) {
                                            ui.label(intro);
                                        }
                                        state.render_category_grid(ui, other);
                                    });
                                }
                            }
                        }

                        // Data pack: shared game-data assets used by
                        // .foreach and .sorter. Action-only (no editable
                        // settings), so it lives outside the registry-driven
                        // category loop.
                        ui.collapsing("Data", |ui| {
                            ui.label(
                                "Item database for .foreach and .sorter \
                                 (gameobj-data.xml). Resolved from your Lich \
                                 folder if set, then a local copy, then the \
                                 bundled snapshot.",
                            );
                            for line in &data_status_lines {
                                ui.label(egui::RichText::new(line).weak());
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .button("Reload")
                                    .on_hover_text(
                                        "Re-resolve the sources now — e.g. after \
                                         ';repo download' in Lich",
                                    )
                                    .clicked()
                                {
                                    data_reload_clicked = true;
                                }
                                ui.label(
                                    egui::RichText::new(
                                        "Set the Lich folder under Map for the \
                                         freshest data.",
                                    )
                                    .weak(),
                                );
                            });
                        });

                        if !state.errors.is_empty() {
                            ui.separator();
                            for error in &state.errors {
                                ui.colored_label(ui.visuals().error_fg_color, error);
                            }
                        }

                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                saved = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancelled = true;
                            }
                        });
                    });
            });

        if let Some(repo) = map_download_repo {
            self.app_core.start_mapdb_download(&repo);
        }
        if map_remove_clicked {
            self.app_core.remove_downloaded_mapdb();
            self.app_core.add_system_message("Downloaded map data removed.");
        }
        if calibrate_doll_clicked {
            self.open_doll_calibration();
        }
        if data_reload_clicked {
            let types = self.app_core.reload_data_pack();
            self.app_core.add_system_message(&format!(
                "Data pack reloaded ({types} item types)."
            ));
        }
        if tts_test_clicked {
            let rate = state.float_draft("tts.rate") as f32;
            let volume = state.float_draft("tts.volume") as f32;
            let voice = state.text_draft("tts.voice");
            let tts = &mut self.app_core.tts_manager;
            tts.set_enabled(true);
            let _ = tts.set_rate(rate);
            let _ = tts.set_volume(volume);
            tts.set_voice_by_name(if voice.trim().is_empty() {
                None
            } else {
                Some(voice.trim().to_string())
            });
            if let Err(err) =
                tts.speak_text_now("A giant rat scampers out of the shadows. Roundtime, 5 seconds.")
            {
                self.app_core
                    .add_system_message(&format!("TTS test failed: {}", err));
            }
            // Voices may have just become enumerable (first engine init).
            if state.tts_voices.is_empty() {
                state.tts_voices = self.app_core.tts_manager.available_voices();
            }
        }

        if saved {
            let character = self.app_core.config.character.clone();
            let mut errors: Vec<String> = Vec::new();
            let mut applied: Vec<&'static str> = Vec::new();

            // Apply and persist only the keys the user actually changed —
            // one sparse scoped save per key, never a whole-config dump.
            for key in changed_keys(&self.app_core.config, &state.drafts) {
                let Some(def) = registry::find(key) else {
                    continue;
                };
                let value = value_from_draft(&def.kind, &state.drafts[key]);
                if let Err(err) = (def.set)(&mut self.app_core.config, &value) {
                    errors.push(format!("{key}: {err}"));
                    continue;
                }
                applied.push(key);
                let is_global = state.scopes.get(key).copied().unwrap_or(false);
                if let Err(err) = self.app_core.config.save_single_setting(
                    key,
                    is_global,
                    character.as_deref(),
                ) {
                    errors.push(format!("{key}: {err}"));
                }
            }

            // Pronunciation substitutions are registry-exempt structured
            // data (EXEMPT_PREFIXES); when they changed, persist them with
            // the sparse whole-config save they always used.
            let new_subs: Vec<crate::config::TtsSubstitution> = state
                .tts_subs
                .iter()
                .filter(|(pattern, _)| !pattern.trim().is_empty())
                .map(|(pattern, replacement)| crate::config::TtsSubstitution {
                    pattern: pattern.trim().to_string(),
                    replacement: replacement.clone(),
                })
                .collect();
            let subs_changed = new_subs != self.app_core.config.tts.substitutions;
            if subs_changed {
                self.app_core.config.tts.substitutions = new_subs;
                if let Err(err) = self.app_core.save_config() {
                    errors.push(format!("tts.substitutions: {err}"));
                }
            }

            // Side effects for the keys that just changed. Theme needs no
            // explicit hook: apply_theme_if_changed watches
            // config.active_theme every frame. The skin's home is the GUI
            // layout (ui_settings.active_skin); the registry draft wrote
            // the config mirror, so copy it over — apply_if_changed then
            // swaps the art next frame.
            if applied.iter().any(|key| *key == "active_skin") {
                self.ui_settings.active_skin = self.app_core.config.active_skin.clone();
                self.layout_dirty = true;
            }
            if applied
                .iter()
                .any(|key| key.starts_with("map.") || *key == "connection.game")
            {
                self.app_core.refresh_map_source();
            }
            if applied.iter().any(|key| key.starts_with("tts.")) || subs_changed {
                self.app_core.apply_tts_settings();
            }
            if applied.iter().any(|key| key.starts_with("sound.")) {
                self.app_core.apply_sound_settings();
            }

            // GUI sizing lives in the per-character layout file; force the
            // zoom/title-bar values to re-apply on the next frame. Vitals
            // options are edited in the Window Editor now, and the active
            // skin was routed above — preserve the live values for both
            // rather than restoring this editor's stale open-time snapshot.
            let vitals = self.ui_settings.vitals.clone();
            let active_skin = self.ui_settings.active_skin.clone();
            self.ui_settings = state.gui_settings.clone();
            self.ui_settings.vitals = vitals;
            self.ui_settings.active_skin = active_skin;
            self.zoom_applied = false;
            self.applied_title_font_size = None;
            self.applied_density = None;
            self.applied_window_corner_radius = None;
            self.layout_dirty = true;

            if errors.is_empty() {
                self.app_core.add_system_message("Settings saved.");
                self.settings_editor = None;
                return;
            }
            self.app_core.add_system_message(&format!(
                "Settings: {} value(s) failed to save.",
                errors.len()
            ));
            state.errors = errors;
            self.settings_editor = Some(state);
            return;
        }

        if open && !cancelled {
            self.settings_editor = Some(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_lines_parse_trims_and_drops_empties() {
        let parsed = parse_list_lines("  one \n\n two\nthree\n   ");
        assert_eq!(parsed, ["one", "two", "three"]);
        // Round trip: join -> parse is stable.
        assert_eq!(parse_list_lines(&parsed.join("\n")), parsed);
    }

    #[test]
    fn every_registered_setting_gets_a_draft() {
        let drafts = drafts_from_config(&Config::default());
        assert_eq!(drafts.len(), registry::registry().len());
    }

    #[test]
    fn untouched_drafts_report_no_changes() {
        let config = Config::default();
        let drafts = drafts_from_config(&config);
        assert_eq!(changed_keys(&config, &drafts), Vec::<&str>::new());
    }

    #[test]
    fn edited_drafts_report_exactly_their_keys() {
        let config = Config::default();
        let mut drafts = drafts_from_config(&config);
        drafts.insert(
            "ui.buffer_size",
            DraftValue::Int(config.ui.buffer_size as i64 + 17),
        );
        drafts.insert("sound.enabled", DraftValue::Bool(!config.sound.enabled));
        drafts.insert(
            "ui.focus.types",
            DraftValue::List("alpha\n beta \n".to_string()),
        );
        assert_eq!(
            changed_keys(&config, &drafts),
            ["sound.enabled", "ui.buffer_size", "ui.focus.types"]
        );
    }

    #[test]
    fn optional_text_whitespace_is_not_a_change() {
        // map.lich_dir defaults to None; whitespace-only input still means
        // "unset", so it must not count as an edit.
        let config = Config::default();
        let mut drafts = drafts_from_config(&config);
        drafts.insert("map.lich_dir", DraftValue::Text("   ".to_string()));
        assert_eq!(changed_keys(&config, &drafts), Vec::<&str>::new());
    }

    #[test]
    fn list_draft_round_trips_through_registry_setter() {
        let mut config = Config::default();
        let def = registry::find("tts.gags").expect("tts.gags registered");
        let draft = DraftValue::List(" spam \n\n ^You feel \n".to_string());
        let value = value_from_draft(&def.kind, &draft);
        (def.set)(&mut config, &value).expect("set tts.gags");
        assert_eq!(config.tts.gags, ["spam", "^You feel"]);
        // Reopening the editor reproduces an equivalent draft.
        assert_eq!(
            draft_from_config(def, &config),
            DraftValue::List("spam\n^You feel".to_string())
        );
    }

    #[test]
    fn ordered_categories_cover_the_whole_registry() {
        // Every category is rendered by this editor EXCEPT the ones the
        // Window Editor owns in the GUI (Targets, deliberately: its global
        // settings live in the "Targets (global)" section of any targets
        // window's editor).
        let categories = ordered_categories();
        for def in registry::registry() {
            if CATEGORIES_IN_WINDOW_EDITOR.contains(&def.category) {
                continue;
            }
            assert!(
                categories.contains(&def.category),
                "category {} missing from ordered_categories()",
                def.category
            );
        }
        assert!(
            !categories.contains(&"Targets"),
            "Targets is edited in the Window Editor, not the GUI Settings window"
        );
        // The exclusion list only names categories that actually exist.
        for excluded in CATEGORIES_IN_WINDOW_EDITOR {
            assert!(
                registry::registry().iter().any(|def| def.category == *excluded),
                "excluded category {excluded} is not in the registry"
            );
        }
        // No duplicates.
        let mut deduped = categories.clone();
        deduped.dedup();
        assert_eq!(categories, deduped);
    }
}
