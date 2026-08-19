//! Per-tab render-setting resolution: layout-definition lookup plus the
//! effective font, text size, and word-wrap for a tab, and the one-time
//! migration of legacy per-tab settings onto shared layout defs.

use super::*;

impl VellumGuiApp {
    /// The shared layout definition backing a tab, when one exists.
    pub(super) fn layout_def_for_tab(&self, key: &TabKey) -> Option<&crate::config::WindowDef> {
        let window_name = &self.available_tabs.get(key)?.window_name;
        self.app_core
            .layout
            .windows
            .iter()
            .find(|window| window.name() == *window_name)
    }

    /// Mutate the shared layout def backing a tab and mark layout.toml
    /// modified (the same mechanism the Window Editor uses). Returns false
    /// (and changes nothing) when the tab has no layout definition.
    pub(super) fn with_layout_def_for_tab(
        &mut self,
        key: &TabKey,
        mutate: impl FnOnce(&mut crate::config::WindowDef),
    ) -> bool {
        let Some(window_name) = self
            .available_tabs
            .get(key)
            .map(|tab| tab.window_name.clone())
        else {
            return false;
        };
        let Some(def) = self
            .app_core
            .layout
            .windows
            .iter_mut()
            .find(|window| window.name() == window_name)
        else {
            return false;
        };
        mutate(def);
        self.app_core.schedule_layout_autosave();
        true
    }

    /// Per-window text size override: the shared layout def first, then the
    /// legacy per-tab GUI setting (pre-migration files).
    pub(super) fn text_size_override_for_tab(&self, key: &TabKey) -> Option<f32> {
        self.layout_def_for_tab(key)
            .and_then(|def| def.base().text_size)
            .or_else(|| self.tab_settings.get(key).and_then(|s| s.text_size))
    }

    /// Effective text size for a window: per-window override or the global size.
    pub(super) fn effective_text_size(&self, key: &TabKey) -> f32 {
        self.text_size_override_for_tab(key)
            .unwrap_or(self.ui_settings.text_size)
            .clamp(6.0, 72.0)
    }

    /// Per-window font: the shared layout def's family name first, then the
    /// legacy per-tab GUI setting (which can also be a custom font file).
    pub(super) fn font_ref_for_tab(&self, key: &TabKey) -> Option<FontRef> {
        if let Some(family) = self
            .layout_def_for_tab(key)
            .and_then(|def| def.base().font_family.clone())
        {
            return Some(FontRef::Named(family));
        }
        self.tab_settings
            .get(key)
            .map(|settings| settings.font_primary.clone())
    }

    /// Effective proportional font family for a window: the per-window font
    /// (registered as a named family during font setup) or egui's default.
    pub(super) fn effective_font_family(&self, key: &TabKey) -> egui::FontFamily {
        self.font_ref_for_tab(key)
            .as_ref()
            .and_then(theme::font_ref_key)
            .filter(|font_key| self.registered_font_families.contains(font_key))
            .map(|font_key| egui::FontFamily::Name(font_key.into()))
            .unwrap_or(egui::FontFamily::Proportional)
    }

    /// Word wrap stored on the shared layout def, for widget types whose def
    /// carries a `wordwrap` field. None = this def has no such field.
    pub(super) fn def_wordwrap_for_tab(&self, key: &TabKey) -> Option<bool> {
        match self.layout_def_for_tab(key)? {
            crate::config::WindowDef::Text { data, .. } => Some(data.wordwrap),
            crate::config::WindowDef::Inventory { data, .. }
            | crate::config::WindowDef::Reserve { data, .. } => Some(data.wordwrap),
            _ => None,
        }
    }

    /// Effective wrap for a window: the def's wordwrap when it has one, else
    /// the legacy per-tab GUI setting, else on.
    pub(super) fn effective_wrap_text(&self, key: &TabKey) -> bool {
        self.def_wordwrap_for_tab(key).unwrap_or_else(|| {
            self.tab_settings
                .get(key)
                .map(|settings| settings.wrap_text)
                .unwrap_or(true)
        })
    }

    /// One-time migration: move legacy per-tab GUI appearance settings
    /// (text size, font, wrap) onto the shared layout window defs, where
    /// they now live. A value already present on a def wins; migrated
    /// TabSettings fields reset to their defaults either way, so a second
    /// run is a no-op. `FontRef::Custom` (a font file path, not a family
    /// name) stays in TabSettings and keeps working via the read fallback.
    /// Returns (layout_changed, gui_settings_changed).
    pub(super) fn migrate_tab_settings_to_layout(
        tab_settings: &mut HashMap<TabKey, TabSettings>,
        layout: &mut crate::config::Layout,
        window_name_of: impl Fn(&TabKey) -> Option<String>,
    ) -> (bool, bool) {
        use crate::config::WindowDef;

        let mut layout_changed = false;
        let mut gui_changed = false;
        for (key, settings) in tab_settings.iter_mut() {
            let Some(name) = window_name_of(key) else {
                continue;
            };
            let Some(def) = layout.windows.iter_mut().find(|w| w.name() == name) else {
                continue;
            };
            if let Some(size) = settings.text_size.take() {
                if def.base().text_size.is_none() {
                    def.base_mut().text_size = Some(size);
                    layout_changed = true;
                }
                gui_changed = true;
            }
            if matches!(settings.font_primary, FontRef::Named(_)) {
                let font = std::mem::take(&mut settings.font_primary);
                if let FontRef::Named(family) = font {
                    if def.base().font_family.is_none() {
                        def.base_mut().font_family = Some(family);
                        layout_changed = true;
                    }
                }
                gui_changed = true;
            }
            if !settings.wrap_text {
                // wrap_text=false is the only migratable signal (true is the
                // default); only defs with a wordwrap field can hold it.
                let target = match def {
                    WindowDef::Text { data, .. } => Some(&mut data.wordwrap),
                    WindowDef::Inventory { data, .. } | WindowDef::Reserve { data, .. } => {
                        Some(&mut data.wordwrap)
                    }
                    _ => None,
                };
                if let Some(wordwrap) = target {
                    if *wordwrap {
                        *wordwrap = false;
                        layout_changed = true;
                    }
                    settings.wrap_text = true;
                    gui_changed = true;
                }
            }
        }
        (layout_changed, gui_changed)
    }

    /// Resolve the sizing values a window's content renderer needs.
    pub(super) fn widget_render_settings(&self, key: &TabKey) -> WidgetRenderSettings {
        WidgetRenderSettings {
            text_size: self.effective_text_size(key),
            map_zoom: self.tab_settings.get(key).and_then(|s| s.map_zoom),
            font_family: self.effective_font_family(key),
            effects_bar_height: self.ui_settings.effects_bar_height.clamp(10.0, 60.0),
            bar_corner_radius: self.ui_settings.bar_corner_radius.clamp(0.0, 12.0),
            auto_contrast_bar_text: self.ui_settings.auto_contrast_bar_text,
            wrap_text: self.effective_wrap_text(key),
            vitals: self.ui_settings.vitals.clone(),
            background: self.available_tabs.get(key).and_then(|tab| {
                self.skin_state.background_for_with_override(
                    &tab.window_name,
                    self.tab_settings
                        .get(key)
                        .and_then(|settings| settings.background_image.as_deref())
                        .or(self.ui_settings.default_background.as_deref()),
                )
            }),
            skin_art: self.skin_state.widget_art(),
            creature_art: Some(self.skin_state.creature_art()),
            command_input_seed: self
                .available_tabs
                .get(key)
                .and_then(|tab| self.app_core.ui_state.windows.get(&tab.window_name))
                .filter(|window| window.widget_type == WidgetType::CommandInput)
                .map(|_| self.command_input.clone()),
            command_input_completion: self
                .available_tabs
                .get(key)
                .filter(|_| self.app_core.config.ui.history_suggestions)
                .and_then(|tab| self.app_core.ui_state.windows.get(&tab.window_name))
                .filter(|window| window.widget_type == WidgetType::CommandInput)
                .and_then(|_| {
                    crate::frontend::common::find_history_completion(
                        &self.command_input,
                        &self.command_history,
                    )
                }),
            command_input_drag_gutter: self
                .available_tabs
                .get(key)
                .and_then(|tab| self.app_core.ui_state.windows.get(&tab.window_name))
                .is_some_and(|window| window.widget_type == WidgetType::CommandInput)
                && self.title_bar_hidden(key),
            hand_icon_size: self.ui_settings.hand_icon_size.clamp(16.0, 48.0),
            gray_inactive_icons: self.ui_settings.status_icons.gray_inactive,
            gray_icon_overrides: self.ui_settings.status_icons.gray_overrides.clone(),
            doll_grayscale: self.ui_settings.doll_grayscale,
            effect_countdown_now: self
                .app_core
                .config
                .ui
                .effect_countdown
                .then(|| chrono::Utc::now().timestamp() + self.app_core.server_time_offset),
            multiaccount_peers: self.multiaccount_peers.clone(),
        }
    }
}
