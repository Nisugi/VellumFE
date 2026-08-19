//! Menu Builder Functions
//!
//! Constructs menu items for various popup menus in the TUI.

use crate::config;
use crate::config::StreamRoute;
use crate::core::messages::{route_for, RouteDecision};
use crate::core::AppCore;
use crate::data::ui_state::PopupMenuItem;
use crate::data::WindowContent;
use crate::frontend::tui::settings_editor::{tui_value_for, SettingItem};
use std::collections::BTreeMap;

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
        // GUI-scoped settings have no effect in the terminal; hide them.
        .filter(|def| def.frontend.includes_tui())
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

// ---- Streams routing menus (.streams) ------------------------------------

/// Human label for a stream's EFFECTIVE destination, mirroring the GUI
/// Streams panel: subscriber window ("name", "name +2"), else the
/// `[streams.routes]` entry ("Main", "Discard", window name), else
/// "fallback (<name>)". Precedence semantics come from `route_for`; only the
/// label strings are duplicated from the GUI.
pub fn stream_destination_label(
    subscribers: &[String],
    stream_id: &str,
    routes: &BTreeMap<String, StreamRoute>,
    fallback: &str,
) -> String {
    match route_for(stream_id, !subscribers.is_empty(), routes, fallback) {
        RouteDecision::Subscribed => match subscribers.split_first() {
            Some((first, [])) => first.clone(),
            Some((first, rest)) => format!("{} +{}", first, rest.len()),
            // Unreachable: Subscribed implies at least one subscriber.
            None => format!("fallback ({})", fallback),
        },
        RouteDecision::Discard => "Discard".to_string(),
        RouteDecision::Deliver { .. } => {
            // Deliver flattens "explicit route" vs "fallback" into candidate
            // windows; disambiguate for display by consulting the route entry
            // the same way route_for does (case-insensitive).
            let route = routes
                .iter()
                .find(|(id, _)| id.eq_ignore_ascii_case(stream_id))
                .map(|(_, route)| route);
            match route {
                Some(StreamRoute::Main) => "Main".to_string(),
                Some(StreamRoute::Window(name)) => name.clone(),
                Some(StreamRoute::Discard) | None => format!("fallback ({})", fallback),
            }
        }
    }
}

/// Union stream ids from routes, layout window subscriptions, and streams
/// seen this session: trim, drop empties, de-duplicate case-insensitively
/// (first spelling wins), sort alphabetically. Mirrors the GUI panel's union.
fn known_stream_ids(app_core: &AppCore) -> Vec<String> {
    let mut layout_streams: Vec<String> = Vec::new();
    for def in &app_core.layout.windows {
        match def {
            config::WindowDef::Text { data, .. } => {
                layout_streams.extend(data.streams.iter().cloned());
            }
            config::WindowDef::Inventory { data, .. } | config::WindowDef::Reserve { data, .. } => {
                layout_streams.extend(data.streams.iter().cloned());
            }
            config::WindowDef::TabbedText { data, .. } => {
                for tab in &data.tabs {
                    layout_streams.extend(tab.get_streams());
                }
            }
            _ => {}
        }
    }
    let seen = app_core.message_processor.seen_streams();
    let mut out: Vec<String> = Vec::new();
    for id in app_core
        .config
        .streams
        .routes
        .keys()
        .map(String::as_str)
        .chain(layout_streams.iter().map(String::as_str))
        .chain(seen.iter().map(|(id, _)| id.as_str()))
    {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        if !out.iter().any(|s| s.eq_ignore_ascii_case(id)) {
            out.push(id.to_string());
        }
    }
    out.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    out
}

/// Build the `.streams` list menu: every known stream and its effective
/// destination. Selecting a row opens the per-stream route actions submenu.
/// Streams bound by a non-Text widget (tabbed tab, built-in) are read-only
/// here — that binding is edited in the Window Editor, matching the GUI.
pub fn build_streams_menu(app_core: &AppCore) -> Vec<PopupMenuItem> {
    let routes = &app_core.config.streams.routes;
    let fallback = &app_core.config.streams.fallback;
    let mut items: Vec<PopupMenuItem> = known_stream_ids(app_core)
        .into_iter()
        .map(|id| {
            let subscribers = app_core.message_processor.get_stream_subscribers(&id);
            let editable = subscribers.iter().all(|name| {
                matches!(
                    app_core.ui_state.windows.get(name).map(|w| &w.content),
                    Some(WindowContent::Text(_))
                )
            });
            let label = stream_destination_label(subscribers, &id, routes, fallback);
            if editable {
                PopupMenuItem {
                    text: format!("{} → {}", id, label),
                    command: format!("action:streamacts:{}", id),
                    disabled: false,
                }
            } else {
                PopupMenuItem {
                    text: format!("{} → {}  (edit in Window Editor)", id, label),
                    command: String::new(),
                    disabled: true,
                }
            }
        })
        .collect();
    if items.is_empty() {
        items.push(PopupMenuItem {
            text: "No streams known yet".to_string(),
            command: String::new(),
            disabled: true,
        });
    }
    items
}

/// Route actions for one stream. `[✓]` marks the current orphan policy; no
/// mark while a subscription overrides it (routes apply only when no window
/// subscribes, and picking one orphans the stream first — GUI semantics).
pub fn build_stream_actions_menu(app_core: &AppCore, stream: &str) -> Vec<PopupMenuItem> {
    let fallback = &app_core.config.streams.fallback;
    let has_sub = !app_core
        .message_processor
        .get_stream_subscribers(stream)
        .is_empty();
    let route = app_core
        .config
        .streams
        .routes
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(stream))
        .map(|(_, route)| route);
    let check = |on: bool| if on { "✓" } else { " " };
    vec![
        PopupMenuItem {
            text: "Send to window...".to_string(),
            command: format!("action:streamwin:{}", stream),
            disabled: false,
        },
        PopupMenuItem {
            text: format!(
                "[{}] Route to Main",
                check(!has_sub && matches!(route, Some(StreamRoute::Main)))
            ),
            command: format!("action:streamroute:main:{}", stream),
            disabled: false,
        },
        PopupMenuItem {
            text: format!(
                "[{}] Route to Discard",
                check(!has_sub && matches!(route, Some(StreamRoute::Discard)))
            ),
            command: format!("action:streamroute:discard:{}", stream),
            disabled: false,
        },
        PopupMenuItem {
            text: format!(
                "[{}] Clear route (fallback: {})",
                check(!has_sub && route.is_none()),
                fallback
            ),
            command: format!("action:streamroute:clear:{}", stream),
            disabled: false,
        },
        PopupMenuItem {
            text: "New window on this stream...".to_string(),
            command: format!("action:streamnew:{}", stream),
            disabled: false,
        },
    ]
}

/// Window picker for "Send to window...": every plain Text window, `[✓]` on
/// the current first subscriber.
pub fn build_stream_window_menu(app_core: &AppCore, stream: &str) -> Vec<PopupMenuItem> {
    let current = app_core
        .message_processor
        .get_stream_subscribers(stream)
        .first()
        .cloned();
    let mut names: Vec<String> = app_core
        .ui_state
        .windows
        .iter()
        .filter(|(_, w)| matches!(w.content, WindowContent::Text(_)))
        .map(|(name, _)| name.clone())
        .collect();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    let mut items: Vec<PopupMenuItem> = names
        .into_iter()
        .map(|name| PopupMenuItem {
            text: format!(
                "[{}] {}",
                if Some(&name) == current.as_ref() {
                    "✓"
                } else {
                    " "
                },
                name
            ),
            command: format!("action:streamsub:{}:{}", name, stream),
            disabled: false,
        })
        .collect();
    if items.is_empty() {
        items.push(PopupMenuItem {
            text: "No text windows in this layout".to_string(),
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
            assert!(
                !item.category.is_empty(),
                "{} has an empty category",
                def.key
            );
            assert_eq!(item.display_name, def.label, "{} label mismatch", def.key);
            assert_eq!(
                item.description.as_deref(),
                Some(def.description),
                "{} description mismatch",
                def.key
            );
            assert_eq!(
                item.sensitive, def.sensitive,
                "{} sensitive mismatch",
                def.key
            );
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
                    assert!(
                        !item.is_global,
                        "{} must always be character-scoped",
                        item.key
                    );
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

    // ---- Stream destination labels (mirrors GUI Streams panel strings) ----

    fn routes(entries: &[(&str, StreamRoute)]) -> BTreeMap<String, StreamRoute> {
        entries
            .iter()
            .map(|(id, route)| (id.to_string(), route.clone()))
            .collect()
    }

    fn subs(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn label_subscribed_window_beats_any_route() {
        let map = routes(&[("bounty", StreamRoute::Discard)]);
        let label = stream_destination_label(&subs(&["bounty_win"]), "bounty", &map, "main");
        assert_eq!(label, "bounty_win");
    }

    #[test]
    fn label_counts_extra_subscribers() {
        let label =
            stream_destination_label(&subs(&["a", "b", "c"]), "notes", &routes(&[]), "main");
        assert_eq!(label, "a +2");
    }

    #[test]
    fn label_shows_routes_case_insensitively() {
        let map = routes(&[
            ("speech", StreamRoute::Discard),
            ("ooc", StreamRoute::Main),
            ("bounty", StreamRoute::Window("bounty_win".to_string())),
        ]);
        assert_eq!(
            stream_destination_label(&[], "SPEECH", &map, "main"),
            "Discard"
        );
        assert_eq!(stream_destination_label(&[], "ooc", &map, "main"), "Main");
        assert_eq!(
            stream_destination_label(&[], "Bounty", &map, "main"),
            "bounty_win"
        );
    }

    #[test]
    fn label_unrouted_stream_is_fallback() {
        let label = stream_destination_label(&[], "bounty", &routes(&[]), "story");
        assert_eq!(label, "fallback (story)");
    }
}
