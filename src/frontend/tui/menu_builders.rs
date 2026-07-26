//! Menu Builder Functions
//!
//! Constructs menu items for various popup menus in the TUI.

use crate::config;
use crate::core::AppCore;
use crate::data::ui_state::PopupMenuItem;
use crate::frontend::tui::settings_editor::{tui_value_for, SettingItem};

/// Build configuration submenu
pub fn build_config_submenu() -> Vec<PopupMenuItem> {
    vec![
        PopupMenuItem {
            text: "Layouts".to_string(),
            command: "menu:layouts".to_string(),
            disabled: false,
        },
        PopupMenuItem {
            text: "Highlights".to_string(),
            command: "action:highlights".to_string(),
            disabled: false,
        },
    ]
}

/// Build settings items from config
/// Uses merged config for values, character_config_exists to determine source
pub fn build_settings_items(config: &config::Config) -> Vec<SettingItem> {
    // Default: all settings come from global (merged config is used)
    build_settings_items_with_source(config, false)
}

/// Build settings items with source tracking, generated from the settings
/// registry so every registered setting is editable in the TUI.
///
/// If character_config_exists is true, scope-toggleable settings are marked
/// as character overrides. Character-only settings (connection identity,
/// pinned ports) are always character-scoped and cannot be toggled global.
pub fn build_settings_items_with_source(
    config: &config::Config,
    character_config_exists: bool,
) -> Vec<SettingItem> {
    use crate::config::registry::{self, SettingScope};

    let mut items: Vec<SettingItem> = registry::registry()
        .iter()
        .map(|def| {
            let character_only = def.scope == SettingScope::CharacterOnly;
            SettingItem {
                category: def.category.to_string(),
                key: def.key.to_string(),
                display_name: def.label.to_string(),
                value: tui_value_for(def, config),
                description: Some(def.description.to_string()),
                editable: true,
                name_width: None,
                // Character-only settings are ALWAYS character-scoped;
                // everything else starts global unless a character config
                // already overrides settings.
                is_global: if character_only {
                    false
                } else {
                    !character_config_exists
                },
                sensitive: def.sensitive,
            }
        })
        .collect();

    // Group items by category so the editor's section headers render each
    // category exactly once. Connection (identity) first, the rest
    // alphabetical, settings alphabetical by label within a category.
    items.sort_by(|a, b| {
        (a.category != "Connection", &a.category, &a.display_name).cmp(&(
            b.category != "Connection",
            &b.category,
            &b.display_name,
        ))
    });

    items
}

/// Build hide window menu (shows currently visible windows that can be hidden)
pub fn build_hidewindow_picker(app_core: &AppCore) -> Vec<PopupMenuItem> {
    let mut items = Vec::new();

    // Get all currently visible window names from ui_state (except main and command_input)
    let mut visible_names: Vec<String> = app_core
        .ui_state
        .windows
        .keys()
        .filter(|name| *name != "main" && *name != "command_input")
        .map(|name| name.to_string())
        .collect();

    // Sort alphabetically by display name
    visible_names.sort_by_key(|name| app_core.get_window_display_name(name));

    for name in visible_names {
        let display_name = app_core.get_window_display_name(&name);
        items.push(PopupMenuItem {
            text: display_name,
            command: format!("action:hidewindow:{}", name),
            disabled: false,
        });
    }

    // If no windows can be hidden
    if items.is_empty() {
        items.push(PopupMenuItem {
            text: "No windows to hide".to_string(),
            command: String::new(),
            disabled: true,
        });
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::registry::{self, SettingScope};

    /// The generated settings list must cover EVERY registry key exactly
    /// once, with non-empty categories and registry-sourced labels.
    #[test]
    fn settings_items_cover_every_registry_key_exactly_once() {
        let config = config::Config::default();
        let items = build_settings_items_with_source(&config, false);

        assert_eq!(
            items.len(),
            registry::registry().len(),
            "item count must match the registry"
        );
        for def in registry::registry() {
            let matching: Vec<_> = items.iter().filter(|item| item.key == def.key).collect();
            assert_eq!(
                matching.len(),
                1,
                "registry key {} appears {} times in the settings editor",
                def.key,
                matching.len()
            );
            let item = matching[0];
            assert!(!item.category.is_empty(), "{} has an empty category", def.key);
            assert_eq!(item.display_name, def.label, "{} label mismatch", def.key);
            assert_eq!(
                item.description.as_deref(),
                Some(def.description),
                "{} description mismatch",
                def.key
            );
            assert_eq!(item.sensitive, def.sensitive, "{} sensitive mismatch", def.key);
        }
    }

    /// Character-only settings (connection.*, web.pinned) always start as
    /// [C]; scope-toggleable settings follow character_config_exists.
    #[test]
    fn settings_items_scope_tracks_registry() {
        let config = config::Config::default();
        for character_config_exists in [false, true] {
            let items = build_settings_items_with_source(&config, character_config_exists);
            for item in &items {
                let def = registry::find(&item.key).expect("item key is registered");
                if def.scope == SettingScope::CharacterOnly {
                    assert!(!item.is_global, "{} must always be character-scoped", item.key);
                } else {
                    assert_eq!(
                        item.is_global, !character_config_exists,
                        "{} initial scope should track character config presence",
                        item.key
                    );
                }
            }
        }
    }
}
