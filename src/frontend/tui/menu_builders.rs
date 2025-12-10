//! Menu Builder Functions
//!
//! Constructs menu items for various popup menus in the TUI.

use crate::config;
use crate::core::AppCore;
use crate::data::ui_state::PopupMenuItem;
use crate::frontend::tui::settings_editor::{SettingItem, SettingValue};

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
pub fn build_settings_items(config: &config::Config) -> Vec<SettingItem> {
    let mut items = Vec::new();

    // Connection settings
    items.push(SettingItem {
        category: "Connection".to_string(),
        key: "connection.host".to_string(),
        display_name: "Host".to_string(),
        value: SettingValue::String(config.connection.host.clone()),
        description: Some("Game server hostname or IP address".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "Connection".to_string(),
        key: "connection.port".to_string(),
        display_name: "Port".to_string(),
        value: SettingValue::Number(config.connection.port as i64),
        description: Some("Game server port number".to_string()),
        editable: true,
        name_width: None,
    });

    if let Some(ref character) = config.connection.character {
        items.push(SettingItem {
            category: "Connection".to_string(),
            key: "connection.character".to_string(),
            display_name: "Character".to_string(),
            value: SettingValue::String(character.clone()),
            description: Some("Default character name".to_string()),
            editable: true,
            name_width: None,
        });
    }

    // UI settings
    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.buffer_size".to_string(),
        display_name: "Buffer Size".to_string(),
        value: SettingValue::Number(config.ui.buffer_size as i64),
        description: Some("Number of lines to keep in text window buffers".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.show_timestamps".to_string(),
        display_name: "Show Timestamps".to_string(),
        value: SettingValue::Boolean(config.ui.show_timestamps),
        description: Some("Display timestamps in text windows".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.border_style".to_string(),
        display_name: "Border Style".to_string(),
        value: SettingValue::Enum(
            config.ui.border_style.clone(),
            vec![
                "single".to_string(),
                "double".to_string(),
                "rounded".to_string(),
                "thick".to_string(),
                "none".to_string(),
            ],
        ),
        description: Some("Widget border style".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.countdown_icon".to_string(),
        display_name: "Countdown Icon".to_string(),
        value: SettingValue::String(config.ui.countdown_icon.clone()),
        description: Some("Unicode character for countdown blocks".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.poll_timeout_ms".to_string(),
        display_name: "Poll Timeout (ms)".to_string(),
        value: SettingValue::Number(config.ui.poll_timeout_ms as i64),
        description: Some("Event poll timeout - lower = higher FPS, higher CPU".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.startup_music".to_string(),
        display_name: "Startup Music".to_string(),
        value: SettingValue::Boolean(config.ui.startup_music),
        description: Some("Play music when connecting".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.startup_music_file".to_string(),
        display_name: "Startup Music File".to_string(),
        value: SettingValue::String(config.ui.startup_music_file.clone()),
        description: Some("Sound file to play on startup (without extension)".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.selection_enabled".to_string(),
        display_name: "Selection Enabled".to_string(),
        value: SettingValue::Boolean(config.ui.selection_enabled),
        description: Some("Enable text selection with mouse".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.selection_respect_window_boundaries".to_string(),
        display_name: "Selection Respects Windows".to_string(),
        value: SettingValue::Boolean(config.ui.selection_respect_window_boundaries),
        description: Some("Prevent selection from crossing window boundaries".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.drag_modifier_key".to_string(),
        display_name: "Drag Modifier Key".to_string(),
        value: SettingValue::Enum(
            config.ui.drag_modifier_key.clone(),
            vec!["ctrl".to_string(), "alt".to_string(), "shift".to_string()],
        ),
        description: Some("Modifier key required for drag and drop".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "UI".to_string(),
        key: "ui.min_command_length".to_string(),
        display_name: "Min Command Length".to_string(),
        value: SettingValue::Number(config.ui.min_command_length as i64),
        description: Some("Minimum command length to save to history".to_string()),
        editable: true,
        name_width: None,
    });

    // Sound settings
    items.push(SettingItem {
        category: "Sound".to_string(),
        key: "sound.enabled".to_string(),
        display_name: "Sound Enabled".to_string(),
        value: SettingValue::Boolean(config.sound.enabled),
        description: Some("Enable sound effects".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "Sound".to_string(),
        key: "sound.volume".to_string(),
        display_name: "Master Volume".to_string(),
        value: SettingValue::Float(config.sound.volume as f64),
        description: Some("Master volume (0.0 to 1.0)".to_string()),
        editable: true,
        name_width: None,
    });

    items.push(SettingItem {
        category: "Sound".to_string(),
        key: "sound.cooldown_ms".to_string(),
        display_name: "Sound Cooldown (ms)".to_string(),
        value: SettingValue::Number(config.sound.cooldown_ms as i64),
        description: Some("Cooldown between same sound plays".to_string()),
        editable: true,
        name_width: None,
    });

    // Theme settings
    items.push(SettingItem {
        category: "Theme".to_string(),
        key: "active_theme".to_string(),
        display_name: "Active Theme".to_string(),
        value: SettingValue::String(config.active_theme.clone()),
        description: Some("Currently active color theme".to_string()),
        editable: true,
        name_width: None,
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
