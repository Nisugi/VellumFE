//! Window and indicator template definitions.
//!
//! Contains the built-in window template catalog (`get_window_template`),
//! user-defined template stores persisted to TOML, and the category
//! groupings used by the add-window menus.

use super::*;
use crate::data::geometry::{Col, Height, Row, Width};

/// One condition-driven status icon state, mirroring `HandIconState`: while
/// its `when` holds, its icon/text/color override the template's static
/// defaults. First match wins across the `states` list. This is what lets a
/// status show one icon at rank 1 and another at rank 2 (e.g. an injury
/// threshold), or a different icon per game state generally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusIconState {
    pub when: super::Condition,
    /// GUI icon while the state holds (pool image / sheet cell /
    /// `IconRef::None` for no art). None = keep the resolved default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<crate::data::IconRef>,
    /// TUI text glyph while the state holds (the TUI renders no images).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Color override while the state holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Globally available indicator template definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndicatorTemplateEntry {
    /// Unique indicator id (case-preserved, e.g., "POISONED")
    pub id: String,
    /// Optional template name (defaults to lowercased id when omitted)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional display title
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Legacy text-glyph icon (TUI prefix / GUI fallback). Kept for
    /// back-compat; `icon_ref` takes precedence for GUI art.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Pickable ACTIVE GUI icon (pool image / sheet cell): shown when the
    /// indicator is active (game Y, or a matched condition). Resolved before
    /// the legacy `icon` string and the id-keyed skin/pictogram fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_ref: Option<crate::data::IconRef>,
    /// Pickable INACTIVE GUI icon: shown when the indicator is inactive (game
    /// N). None = show NO image while inactive (blank) — inactive art is
    /// opt-in, never a dimmed copy of the active icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive_icon_ref: Option<crate::data::IconRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_color: Option<String>,
    /// Condition-driven icon states (first match wins), overriding the static
    /// icon/colors above while a state holds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<StatusIconState>,
    /// Enabled flag; if false, this template is skipped on load
    #[serde(
        default = "default_template_enabled",
        skip_serializing_if = "is_enabled_default"
    )]
    pub enabled: bool,
}

impl IndicatorTemplateEntry {
    /// Key used for template lookup (stable even if id casing differs)
    pub fn key(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.id.to_lowercase())
    }

    /// Title shown to users; falls back to id
    pub fn title_or_id(&self) -> String {
        self.title.clone().unwrap_or_else(|| self.id.clone())
    }
}

fn default_template_enabled() -> bool {
    true
}

fn is_enabled_default(value: &bool) -> bool {
    *value
}

/// TOML file wrapper for indicator templates
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndicatorTemplateStore {
    #[serde(default)]
    pub indicators: Vec<IndicatorTemplateEntry>,
}

/// Generic window template definition stored globally
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowTemplateEntry {
    /// Template name (used as the template key)
    pub name: String,
    /// Widget type this template represents (e.g., "progress", "countdown", "text")
    pub widget_type: String,
    /// Full window definition to clone when instantiating
    pub window: WindowDef,
    /// Enabled flag; if false, this template is skipped on load
    #[serde(
        default = "default_template_enabled",
        skip_serializing_if = "is_enabled_default"
    )]
    pub enabled: bool,
}

/// TOML file wrapper for window templates
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowTemplateStore {
    #[serde(default)]
    pub templates: Vec<WindowTemplateEntry>,
}

/// THE ordered catalog: every built-in window key with its game gating
/// (redesign Phase 6 — one source of truth; formerly a 67-entry parallel
/// name list plus a separate `template_game_type` match). Order is the
/// catalog order the Phase 0 golden fixture pins. User presets from the
/// template stores layer on top in `list_window_templates`.
/// The built-in catalog as data: `defaults/globals/window_catalog.toml`,
/// embedded at build time.
///
/// Was a 78-arm `match` of struct literals (~1900 lines), now 80 entries
/// (two arms carried an alias). The entries are
/// `WindowDef`s in exactly the shape `layout.toml` already stores, so the
/// format needed no new machinery — and `tests/template_characterization.rs`
/// pins every one of them, which is what made the extraction provable rather
/// than hopeful.
///
/// Order and per-game gating deliberately stay in [`CATALOG`]: this file
/// answers "what IS a compass", never "which windows exist".
static WINDOW_CATALOG_TOML: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/defaults/globals/window_catalog.toml"));

#[derive(Debug, Deserialize)]
struct WindowCatalogFile {
    templates: std::collections::HashMap<String, WindowDef>,
}

/// Parsed once; a built-in lookup is then a hash hit and a clone.
///
/// A malformed catalog is a build-time authoring error, not a runtime
/// condition a user can reach — the file is embedded, so if it parses in CI
/// it parses everywhere. Panicking with the parser's message beats degrading
/// to an empty catalog, which would silently offer the user no windows at
/// all. `built_in_catalog_parses` in the test module is the tripwire.
fn built_in_catalog() -> &'static std::collections::HashMap<String, WindowDef> {
    static CATALOG_DATA: std::sync::OnceLock<std::collections::HashMap<String, WindowDef>> =
        std::sync::OnceLock::new();
    CATALOG_DATA.get_or_init(|| {
        let parsed: WindowCatalogFile = toml::from_str(WINDOW_CATALOG_TOML)
            .unwrap_or_else(|e| panic!("defaults/globals/window_catalog.toml is malformed: {e}"));
        parsed.templates
    })
}

/// One built-in template by key, or `None` when the name is not a built-in
/// (a user preset or an unknown name — both handled by the caller).
fn built_in_template(name: &str) -> Option<WindowDef> {
    built_in_catalog().get(name).cloned()
}

pub const CATALOG: &[(&str, Option<GameType>)] = &[
    ("health", None),
    ("mana", None),
    ("stamina", None),
    ("spirit", None),
    ("concentration", Some(GameType::DR)),
    ("stance", None),
    ("progress_custom", None),
    ("dashboard", None),
    ("poisoned", None),
    ("bleeding", None),
    ("diseased", None),
    ("stunned", None),
    ("webbed", None),
    ("standing", None),
    ("kneeling", None),
    ("sitting", None),
    ("prone", None),
    ("hidden", None),
    ("invisible", None),
    ("joined", None),
    ("dead", None),
    ("main", None),
    ("thoughts", None),
    ("speech", None),
    ("bestiary", None),
    ("announcements", None),
    ("loot", None),
    ("death", None),
    ("logons", None),
    ("familiar", None),
    ("ambients", None),
    ("bounty", None),
    ("society", None),
    ("text_custom", None),
    ("chat", None),
    ("tabbedtext_custom", None),
    ("targets", None),
    ("creaturefield", Some(GameType::GS4)),
    ("players", None),
    ("items", None),
    ("entity_custom", None),
    ("roundtime", None),
    ("casttime", None),
    ("stuntime", None),
    ("aimtime", Some(GameType::GS4)),
    ("pulse", Some(GameType::GS4)),
    ("countdown_custom", None),
    ("left", None),
    ("right", None),
    ("spell", None),
    ("buffs", None),
    ("debuffs", None),
    ("cooldowns", None),
    ("active_spells", None),
    ("alert_timers", None),
    ("active_effects_custom", None),
    ("inventory", None),
    ("reserve", Some(GameType::GS4)),
    ("room", None),
    ("spells", None),
    ("missingspells", None),
    ("containers", Some(GameType::GS4)),
    ("bestiaryview", Some(GameType::GS4)),
    ("compass", None),
    ("map", None),
    ("injuries", None),
    ("quickbar", None),
    ("hotkeybar", None),
    ("spacer", None),
    ("perception", Some(GameType::DR)),
    ("experience", Some(GameType::DR)),
    ("gs4_experience", Some(GameType::GS4)),
    ("encum", None),
    ("minivitals", Some(GameType::GS4)),
    ("betrayer", Some(GameType::GS4)),
    ("multiaccount", None),
    ("quests", Some(GameType::GS4)),
];

impl Config {
    /// Get a window template by name
    /// Returns a WindowDef with default positioning that can be customized
    pub fn get_window_template(name: &str) -> Option<WindowDef> {
        // Create base defaults that all windows share
        let base_defaults = WindowBase {
            name: String::new(), // Will be overridden
            placement: TuiPlacement {
                row: Row::new(0),
                col: Col::new(0),
                rows: Height::new(10),
                cols: Width::new(40),
                min_rows: None,
                max_rows: None,
                min_cols: None,
                max_cols: None,
                visibility: crate::config::WindowVisibility::Shown,
                show_title: true,
                title_position: default_title_position(),
            },
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            title: None, // Will be overridden
            background_color: None,
            text_color: None,
            transparent_background: false,
            locked: false,
            binding: None,
            content_align: None,
            tts_speak: false,
            text_size: None,
            font_family: None,
        };
        // Prefer user-defined window templates (global store)
        if let Some(custom) = Self::get_custom_window_template(name) {
            return Some(custom);
        }

        // Prefer user-defined indicator templates (global store)
        if let Some(custom) = Self::get_custom_indicator_template(name, &base_defaults) {
            return Some(custom);
        }

        built_in_template(name)
    }

    /// Resolve a user-defined indicator template by name
    fn get_custom_indicator_template(name: &str, base_defaults: &WindowBase) -> Option<WindowDef> {
        let store = Self::load_indicator_template_store().ok()?;
        store
            .indicators
            .iter()
            .find(|tpl| tpl.enabled && tpl.key().eq_ignore_ascii_case(name))
            .map(|tpl| {
                let mut base = base_defaults.clone();
                base.name = tpl.key();
                base.title = Some(tpl.title_or_id());
                base.placement.rows = Height::new(1);
                base.placement.cols = Width::new(1);
                base.placement.min_rows = Some(1);
                base.placement.max_rows = Some(1);
                base.placement.min_cols = Some(1);
                base.placement.max_cols = Some(1);

                WindowDef::Indicator {
                    base,
                    data: IndicatorWidgetData {
                        icon: tpl.icon.clone(),
                        indicator_id: Some(tpl.id.clone()),
                        inactive_color: tpl.inactive_color.clone(),
                        active_color: tpl.active_color.clone(),
                        default_status: tpl.default_status.clone(),
                        default_color: tpl.default_color.clone(),
                    },
                }
            })
    }

    /// Resolve a user-defined window template by name (non-indicator)
    fn get_custom_window_template(name: &str) -> Option<WindowDef> {
        let store = Self::load_window_template_store().ok()?;
        store
            .templates
            .iter()
            .find(|tpl| tpl.enabled && tpl.name.eq_ignore_ascii_case(name))
            .map(|tpl| {
                // Ensure the stored window name matches the template name
                let mut window = tpl.window.clone();
                window.base_mut().name = tpl.name.clone();
                window
            })
    }

    /// Get list of all available window templates
    /// Returns all windows that can be added via .menu
    pub fn list_window_templates() -> Vec<String> {
        // Built-ins from THE catalog table, then the user preset stores
        // (the never-die layer) appended with case-insensitive dedup.
        let mut templates: Vec<String> = CATALOG.iter().map(|(name, _)| name.to_string()).collect();

        if let Ok(store) = Self::load_window_template_store() {
            for tpl in store.templates {
                if !tpl.enabled {
                    continue;
                }
                let key = tpl.name.to_lowercase();
                if !templates.iter().any(|t| t.to_lowercase() == key) {
                    templates.push(tpl.name);
                }
            }
        }

        if let Ok(store) = Self::load_indicator_template_store() {
            for tpl in store.indicators {
                let key = tpl.key();
                if !tpl.enabled {
                    continue;
                }
                if !templates.iter().any(|t| t.eq_ignore_ascii_case(&key)) {
                    templates.push(key);
                }
            }
        }

        templates
    }

    /// Get the game type requirement for a template
    /// Returns None if template is available for all games
    pub fn template_game_type(name: &str) -> Option<GameType> {
        // Phase 6: gating lives in THE catalog table (user presets and
        // unknown names gate to None, available for both games).
        CATALOG
            .iter()
            .find(|(key, _)| *key == name)
            .and_then(|(_, game)| *game)
    }

    // Phase 6: the dialog/stream id-maps (dialog_id_to_template,
    // id_has_widget_template, stream_id_to_template) moved into
    // core::view_resolver as the dedicated-view tables.

    /// List window templates filtered by game type
    pub fn list_window_templates_for_game(game: Option<GameType>) -> Vec<String> {
        Self::list_window_templates()
            .into_iter()
            .filter(|name| match Self::template_game_type(name) {
                None => true, // Available for all games
                Some(required_game) => game == Some(required_game),
            })
            .collect()
    }

    /// Return all indicator templates (built-in + user-defined), deduplicated by id
    pub fn list_indicator_templates() -> Vec<IndicatorTemplateEntry> {
        let mut templates = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for name in Self::list_window_templates() {
            if let Some(WindowDef::Indicator { base, data }) = Self::get_window_template(&name) {
                // Skip legacy placeholder
                if base.name == "indicator_custom" {
                    continue;
                }

                let id = data
                    .indicator_id
                    .clone()
                    .unwrap_or_else(|| base.name.clone());
                let key = id.to_lowercase();
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);

                templates.push(IndicatorTemplateEntry {
                    id,
                    name: Some(base.name),
                    title: base.title.clone(),
                    icon: data.icon,
                    icon_ref: None,
                    inactive_icon_ref: None,
                    inactive_color: data.inactive_color,
                    active_color: data.active_color,
                    default_status: data.default_status,
                    default_color: data.default_color,
                    states: Vec::new(),
                    enabled: true,
                });
            }
        }

        templates.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
        templates
    }

    /// Load indicator templates from the global store file
    pub fn load_indicator_template_store() -> Result<IndicatorTemplateStore> {
        let path = Self::indicator_templates_path()?;
        if !path.exists() {
            return Ok(IndicatorTemplateStore::default());
        }

        let contents = fs::read_to_string(&path)
            .context(format!("Failed to read indicator templates at {:?}", path))?;
        let mut store: IndicatorTemplateStore = toml::from_str(&contents)
            .context(format!("Failed to parse indicator templates at {:?}", path))?;

        // Deduplicate by key (case-insensitive)
        let mut seen = std::collections::HashSet::new();
        store.indicators.retain(|tpl| {
            let key = tpl.key().to_lowercase();
            if seen.contains(&key) {
                false
            } else {
                seen.insert(key);
                true
            }
        });

        Ok(store)
    }

    /// Save indicator templates to the global store file
    pub fn save_indicator_template_store(store: &IndicatorTemplateStore) -> Result<()> {
        let path = Self::indicator_templates_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut sorted = store.clone();
        sorted
            .indicators
            .sort_by(|a, b| a.key().to_lowercase().cmp(&b.key().to_lowercase()));

        let contents =
            toml::to_string_pretty(&sorted).context("Failed to serialize indicator templates")?;
        write_atomic(&path, contents)
            .context(format!("Failed to write indicator templates to {:?}", path))?;
        Ok(())
    }

    /// Path to the shared indicator template store
    pub fn indicator_templates_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("indicator_templates.toml"))
    }

    /// Load window templates from the global store file
    pub fn load_window_template_store() -> Result<WindowTemplateStore> {
        let path = Self::window_templates_path()?;
        if !path.exists() {
            return Ok(WindowTemplateStore::default());
        }

        let contents = fs::read_to_string(&path)
            .context(format!("Failed to read window templates at {:?}", path))?;
        let mut store: WindowTemplateStore = toml::from_str(&contents)
            .context(format!("Failed to parse window templates at {:?}", path))?;

        // Deduplicate by name (case-insensitive) keeping first occurrence
        let mut seen = std::collections::HashSet::new();
        store.templates.retain(|tpl| {
            let key = tpl.name.to_lowercase();
            if seen.contains(&key) {
                false
            } else {
                seen.insert(key);
                true
            }
        });

        Ok(store)
    }

    /// Save window templates to the global store file
    pub fn save_window_template_store(store: &WindowTemplateStore) -> Result<()> {
        let path = Self::window_templates_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut sorted = store.clone();
        sorted
            .templates
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let contents =
            toml::to_string_pretty(&sorted).context("Failed to serialize window templates")?;
        write_atomic(&path, contents)
            .context(format!("Failed to write window templates to {:?}", path))?;
        Ok(())
    }

    /// Path to the shared window template store
    pub fn window_templates_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("window_templates.toml"))
    }

    /// Upsert a window definition into the global window template store
    /// Enabled is always true on save; users can disable manually in the TOML.
    pub fn upsert_window_template(window: &WindowDef) -> Result<()> {
        let mut store = Self::load_window_template_store().unwrap_or_default();
        let key = window.name().to_lowercase();

        store.templates.retain(|tpl| tpl.name.to_lowercase() != key);

        store.templates.push(WindowTemplateEntry {
            name: window.name().to_string(),
            widget_type: window.widget_type().to_string(),
            window: window.clone(),
            enabled: true,
        });

        Self::save_window_template_store(&store)
    }

    /// True if a global window template exists with this name (case-insensitive)
    pub fn window_template_exists(name: &str) -> bool {
        if let Ok(store) = Self::load_window_template_store() {
            return store
                .templates
                .iter()
                .any(|tpl| tpl.name.eq_ignore_ascii_case(name));
        }
        false
    }

    /// Get templates grouped by widget category
    pub fn get_templates_by_category() -> HashMap<WidgetCategory, Vec<String>> {
        let mut categories: HashMap<WidgetCategory, Vec<String>> = HashMap::new();

        for template_name in Self::list_window_templates() {
            if let Some(template) = Self::get_window_template(&template_name) {
                let category = WidgetCategory::from_widget_type(template.widget_type());
                categories.entry(category).or_default().push(template_name);
            }
        }

        categories
    }

    /// Get addable templates by category (excluding visible windows and wrong game type)
    pub fn get_addable_templates_by_category(
        layout: &crate::config::Layout,
        game_type: Option<GameType>,
    ) -> HashMap<WidgetCategory, Vec<String>> {
        let all_by_category = Self::get_templates_by_category();

        all_by_category
            .into_iter()
            .map(|(category, templates)| {
                let available: Vec<String> = templates
                    .into_iter()
                    .filter(|name| {
                        // Filter by game type first
                        match Self::template_game_type(name) {
                            None => true, // Available for all games
                            Some(required_game) => game_type == Some(required_game),
                        }
                    })
                    .filter(|name| {
                        // Then filter out already visible windows
                        !layout
                            .windows
                            .iter()
                            .any(|w| w.name() == *name && w.base().placement.visibility.is_shown())
                    })
                    .collect();
                (category, available)
            })
            .filter(|(category, templates)| {
                !templates.is_empty() || matches!(category, WidgetCategory::Status)
            })
            .collect()
    }

    /// Get visible windows by category (for Hide/Edit menus)
    /// Returns only categories that have visible windows (excludes essential windows like main/command_input for hide menu)
    pub fn get_visible_templates_by_category(
        layout: &crate::config::Layout,
        exclude_essential: bool,
    ) -> HashMap<WidgetCategory, Vec<String>> {
        Self::get_layout_templates_by_category(layout, exclude_essential, false)
    }

    /// Like `get_visible_templates_by_category`, but with `include_hidden`
    /// the filter is presence-in-layout rather than visibility — used by the
    /// edit-window picker so hidden windows stay reachable.
    pub fn get_layout_templates_by_category(
        layout: &crate::config::Layout,
        exclude_essential: bool,
        include_hidden: bool,
    ) -> HashMap<WidgetCategory, Vec<String>> {
        let all_by_category = Self::get_templates_by_category();
        let included = |w: &crate::config::WindowDef| {
            include_hidden || w.base().placement.visibility.is_shown()
        };

        let mut visible_by_category: HashMap<WidgetCategory, Vec<String>> = all_by_category
            .into_iter()
            .map(|(category, templates)| {
                let visible: Vec<String> = templates
                    .into_iter()
                    .filter(|name| {
                        // Skip essential windows for hide menu
                        if exclude_essential && (*name == "main" || *name == "command_input") {
                            return false;
                        }
                        // Include only windows present (and, unless
                        // include_hidden, visible) in the layout
                        layout
                            .windows
                            .iter()
                            .any(|w| w.name() == *name && included(w))
                    })
                    .collect();
                (category, visible)
            })
            .filter(|(category, templates)| {
                !templates.is_empty()
                    || (!exclude_essential && matches!(category, WidgetCategory::Status))
            })
            .collect();

        // Special-case command_input: always present, not addable, not hideable, but editable
        if !exclude_essential {
            if let Some(cmd) = layout
                .windows
                .iter()
                .find(|w| w.widget_type() == "command_input" && included(w))
            {
                visible_by_category
                    .entry(WidgetCategory::Other)
                    .or_default()
                    .push(cmd.name().to_string());
            }
        }

        // Special-case spacers: dynamically named (spacer_1, spacer_2, etc.), not in templates
        for spacer in layout
            .windows
            .iter()
            .filter(|w| w.widget_type() == "spacer" && included(w))
        {
            visible_by_category
                .entry(WidgetCategory::Other)
                .or_default()
                .push(spacer.name().to_string());
        }

        // Include custom windows (created via .addwindow) that aren't in templates
        // These have names like "custom-text-1", "custom-tabbedtext-2", etc.
        let all_templates: std::collections::HashSet<String> =
            Self::list_window_templates().into_iter().collect();
        for window in layout.windows.iter().filter(|w| included(w)) {
            let name = window.name().to_string();
            // Skip if already in templates or is essential window we're excluding
            if all_templates.contains(&name) {
                continue;
            }
            if exclude_essential && (name == "main" || name == "command_input") {
                continue;
            }
            // Skip spacers (already handled above) and command_input (handled above)
            if window.widget_type() == "spacer" || window.widget_type() == "command_input" {
                continue;
            }
            // Add custom window to appropriate category
            let category = WidgetCategory::from_widget_type(window.widget_type());
            let entry = visible_by_category.entry(category).or_default();
            if !entry.contains(&name) {
                entry.push(name);
            }
        }

        visible_by_category
    }

    /// Get list of visible windows in a layout
    pub fn list_visible_windows(layout: &crate::config::Layout) -> Vec<String> {
        layout
            .windows
            .iter()
            .filter(|w| w.base().placement.visibility.is_shown())
            .map(|w| w.name().to_string())
            .collect()
    }
}

#[cfg(test)]
mod dialog_template_mapping_tests {
    use super::*;

    // effect_dialog_ids_resolve_to_their_widget_templates moved to
    // core::view_resolver (Phase 6: the mapping lives there now).

    #[test]
    fn standard_status_indicators_all_have_templates() {
        // Every self-status the game reports (core::state::StatusInfo) must be a
        // first-class indicator template so its icon is user-customizable and a
        // combined indicator can drive it. Postures were previously absent,
        // leaving them as uncustomizable runtime-only dashboard cells.
        for name in [
            "poisoned",
            "bleeding",
            "diseased",
            "stunned",
            "webbed", // afflictions
            "standing",
            "kneeling",
            "sitting",
            "prone", // postures
            "hidden",
            "invisible",
            "joined",
            "dead", // presence
        ] {
            assert!(
                Config::get_window_template(name).is_some(),
                "indicator template '{name}' must exist"
            );
        }

        // And they surface in the customizable indicator-template list.
        let ids: std::collections::HashSet<String> = Config::list_indicator_templates()
            .into_iter()
            .map(|t| t.id.to_ascii_uppercase())
            .collect();
        for id in ["STANDING", "KNEELING", "SITTING", "PRONE", "DEAD", "HIDDEN"] {
            assert!(ids.contains(id), "indicator template list missing '{id}'");
        }
    }
}
