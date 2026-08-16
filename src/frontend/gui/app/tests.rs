//! Test module of the parent facade, split out for size —
//! `super` is still the parent module, so private access and
//! `use super::*` semantics are identical to the inline mod.

use super::widgets::parse_hex_color;
use super::{
    AppShortcut, FontRef, GlobalDispatchTarget, GuiLinkDispatch, TabKey, TabSettings, VellumGuiApp,
};
use crate::config::{AppKeybinds, Config, KeyBindAction, MacroAction, TargetListConfig};
use crate::core::state::{Creature, Player};
use crate::data::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::data::{LinkData, SpanType, TextSegment};
use eframe::egui::{Color32, Pos2};
use std::collections::HashMap;

use super::GuiTab;
use super::TabId;
use crate::config::WindowDef;

/// A minimal spacer WindowDef with the given name. Every WindowBase field
/// carries a serde default, so an empty table + name deserializes cleanly —
/// no giant literal to keep in sync with the struct.
fn window_def_named(name: &str) -> WindowDef {
    let base: crate::config::WindowBase =
        toml::from_str(&format!("name = \"{name}\"")).expect("window base from name");
    WindowDef::blank("spacer", base).expect("spacer def")
}

/// A live tab backed by `window_name`, keyed under `key`.
fn tab(key: TabKey, window_name: &str) -> (TabKey, GuiTab) {
    (
        key.clone(),
        GuiTab {
            id: TabId::new(key),
            window_name: window_name.to_string(),
        },
    )
}

#[test]
fn collect_available_tabs_uses_configured_title_not_internal_name() {
    use crate::core::AppCore;
    use crate::data::WindowState;

    let mut core = AppCore::new_for_test();
    // A custom text window: opaque internal name, human-facing title.
    let base: crate::config::WindowBase = toml::from_str("name = \"custom-text-1\"").unwrap();
    core.layout.windows.push(WindowDef::Text {
        base: crate::config::WindowBase {
            title: Some("Consumables".into()),
            ..base
        },
        data: crate::config::TextWidgetData {
            streams: vec!["consumables".into()],
            buffer_size: 1000,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    });
    core.ui_state.windows.insert(
        "custom-text-1".to_string(),
        WindowState::new_text("custom-text-1", 1000),
    );

    let fp_before = VellumGuiApp::available_tabs_fingerprint(&core);
    let tabs = VellumGuiApp::collect_available_tabs(&core);
    let tab = tabs
        .values()
        .find(|t| t.window_name == "custom-text-1")
        .expect("tab for the custom window");
    assert_eq!(
        tab.id.title, "Consumables",
        "tab title must be the configured title, not the internal id"
    );

    // Renaming (changing base.title) must change the fingerprint so the tab
    // list actually refreshes.
    if let Some(w) = core
        .layout
        .windows
        .iter_mut()
        .find(|w| w.name() == "custom-text-1")
    {
        w.base_mut().title = Some("Snacks".into());
    }
    let fp_after = VellumGuiApp::available_tabs_fingerprint(&core);
    assert_ne!(
        fp_before, fp_after,
        "a title change must alter the fingerprint"
    );
}

#[test]
fn arrangement_window_defs_keeps_only_windows_backing_a_tab() {
    // The character owns five windows; only three back a live tab. A
    // savelayout must persist exactly those three, not the whole universe
    // (voln/society would otherwise be injected into any profile the
    // layout loads into).
    let all = vec![
        window_def_named("main"),
        window_def_named("room"),
        window_def_named("health"),
        window_def_named("voln"),
        window_def_named("society"),
    ];
    let available_tabs: HashMap<TabKey, GuiTab> = [
        tab(TabKey::TextMain, "main"),
        tab(TabKey::Room, "room"),
        tab(
            TabKey::WindowByName {
                id: "health".into(),
            },
            "health",
        ),
    ]
    .into_iter()
    .collect();

    let saved = VellumGuiApp::arrangement_window_defs(&all, &available_tabs);
    let mut names: Vec<&str> = saved.iter().map(|def| def.name()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["health", "main", "room"]);
    assert!(
        !names.contains(&"voln") && !names.contains(&"society"),
        "unrelated windows must not be baked into the layout"
    );
}

#[test]
fn tabs_absent_from_layout_hides_extras_but_never_main_or_input() {
    // Loading a layout that names only main+room onto a session that also
    // has voln/society/input: the extras hide, but the main story window
    // and the command input are never hidden (invariants).
    let window_defs = vec![window_def_named("main"), window_def_named("room")];
    let available_tabs: HashMap<TabKey, GuiTab> = [
        tab(TabKey::TextMain, "main"),
        tab(TabKey::Room, "room"),
        tab(TabKey::WindowByName { id: "voln".into() }, "voln"),
        tab(
            TabKey::WindowByName {
                id: "society".into(),
            },
            "society",
        ),
        tab(TabKey::CommandInput, "input"),
    ]
    .into_iter()
    .collect();

    let mut hide = VellumGuiApp::tabs_absent_from_layout(&window_defs, &available_tabs);
    hide.sort_by_key(|key| key.short_id());
    assert_eq!(
        hide,
        vec![
            TabKey::WindowByName {
                id: "society".into()
            },
            TabKey::WindowByName { id: "voln".into() },
        ],
        "only the windows the layout omits are hidden"
    );
    assert!(
        !hide.contains(&TabKey::TextMain),
        "main story window is never hidden"
    );
    assert!(
        !hide.contains(&TabKey::CommandInput),
        "command input is never hidden"
    );
}

#[test]
fn rect_survivor_keeps_hidden_window_rect_but_drops_deleted() {
    // Before: loot + inventory are live tabs, both with stored rects.
    let previous: HashMap<TabKey, GuiTab> = [
        tab(TabKey::TextByName { id: "loot".into() }, "loot"),
        tab(
            TabKey::Inventory {
                id: "inventory".into(),
            },
            "inventory",
        ),
    ]
    .into_iter()
    .collect();
    // After a refresh both left the live tab set (one hidden, one deleted).
    let current: HashMap<TabKey, GuiTab> = HashMap::new();
    // loot was HIDDEN — its def survives in the layout. inventory was
    // DELETED — gone from the layout defs.
    let layout_windows = vec![window_def_named("loot")];

    let survivors = VellumGuiApp::rect_survivor_keys(&previous, &current, &layout_windows);
    assert!(
        survivors.contains(&TabKey::TextByName { id: "loot".into() }),
        "a hidden window keeps its rect for a later re-show"
    );
    assert!(
        !survivors.contains(&TabKey::Inventory {
            id: "inventory".into()
        }),
        "a deleted window does not keep its rect"
    );
}

#[test]
fn reference_canvas_prefers_canvas_size_over_restore_size() {
    // A layout saved while maximized records inner_size = the UN-maximized
    // restore geometry but canvas_size = the maximized canvas the rects
    // were actually laid out against. Rescale must use the latter, or the
    // rects blow up past the screen.
    use crate::frontend::gui::persistence::{GuiLayoutFileV1, MainViewportState};
    let mut layout = GuiLayoutFileV1::new("profile", "character");
    layout.main_viewport = Some(MainViewportState {
        outer_pos: None,
        inner_size: [1280.0, 720.0],
        maximized: true,
        canvas_size: Some([2560.0, 1400.0]),
    });
    let rects = HashMap::new();
    assert_eq!(
        VellumGuiApp::layout_reference_canvas(&layout, &rects),
        eframe::egui::Vec2::new(2560.0, 1400.0)
    );
    // Files predating the field fall back to inner_size.
    layout.main_viewport.as_mut().unwrap().canvas_size = None;
    assert_eq!(
        VellumGuiApp::layout_reference_canvas(&layout, &rects),
        eframe::egui::Vec2::new(1280.0, 720.0)
    );
}

#[test]
fn rect_survivor_always_keeps_live_tabs() {
    let previous: HashMap<TabKey, GuiTab> = HashMap::new();
    let current: HashMap<TabKey, GuiTab> = [tab(TabKey::Room, "room")].into_iter().collect();
    // Even with no matching layout def, a currently-live tab's rect stays.
    let survivors = VellumGuiApp::rect_survivor_keys(&previous, &current, &[]);
    assert!(survivors.contains(&TabKey::Room));
}

#[test]
fn tabs_absent_from_layout_hides_nothing_when_layout_matches_session() {
    let window_defs = vec![window_def_named("main"), window_def_named("room")];
    let available_tabs: HashMap<TabKey, GuiTab> =
        [tab(TabKey::TextMain, "main"), tab(TabKey::Room, "room")]
            .into_iter()
            .collect();
    assert!(
        VellumGuiApp::tabs_absent_from_layout(&window_defs, &available_tabs).is_empty(),
        "a layout that names every live window hides nothing"
    );
}

#[test]
fn test_parse_hex_color_with_hash() {
    assert_eq!(
        parse_hex_color("#FF00AA"),
        Some(Color32::from_rgb(255, 0, 170))
    );
}

#[test]
fn test_parse_hex_color_without_hash() {
    assert_eq!(
        parse_hex_color("00FF00"),
        Some(Color32::from_rgb(0, 255, 0))
    );
}

#[test]
fn test_parse_hex_color_invalid_input() {
    assert_eq!(parse_hex_color("#XYZ"), None);
    assert_eq!(parse_hex_color(""), None);
}

#[test]
fn test_resolve_layout_ids_prefers_connection_character() {
    let mut config = Config::default();
    config.character = Some("profile_a".to_string());
    config.connection.character = Some("Nisugi".to_string());

    let (profile, character) = VellumGuiApp::resolve_layout_ids(&config);
    assert_eq!(profile, "profile_a");
    assert_eq!(character, "Nisugi");
}

#[test]
fn test_global_dispatch_prefers_macro_over_shortcut() {
    let key_event = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CTRL);
    let mut keybind_map = HashMap::new();
    keybind_map.insert(
        key_event,
        KeyBindAction::Macro(MacroAction {
            macro_text: "look\r".to_string(),
        }),
    );

    let target = VellumGuiApp::resolve_global_dispatch_target(
        key_event,
        &keybind_map,
        &AppKeybinds::default(),
        false,
    );
    assert!(matches!(target, Some(GlobalDispatchTarget::Macro(_))));
}

#[test]
fn test_global_dispatch_fires_core_action_binds() {
    // f6 = "interact_mode" (and the TTS F-keys) are Action binds, not
    // macros — they must dispatch globally in the GUI.
    let key_event = KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE);
    let mut keybind_map = HashMap::new();
    keybind_map.insert(
        key_event,
        KeyBindAction::Action("interact_mode".to_string()),
    );

    let target = VellumGuiApp::resolve_global_dispatch_target(
        key_event,
        &keybind_map,
        &AppKeybinds::default(),
        false,
    );
    assert!(matches!(target, Some(GlobalDispatchTarget::Macro(_))));
}

#[test]
fn test_global_dispatch_leaves_widget_actions_to_the_gui() {
    // esc = "clear_search" is widget-level: dispatching it globally
    // would steal Esc from every editor and popup.
    let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let mut keybind_map = HashMap::new();
    keybind_map.insert(key_event, KeyBindAction::Action("clear_search".to_string()));

    let target = VellumGuiApp::resolve_global_dispatch_target(
        key_event,
        &keybind_map,
        &AppKeybinds::default(),
        false,
    );
    assert!(!matches!(target, Some(GlobalDispatchTarget::Macro(_))));
}

#[test]
fn test_global_dispatch_uses_shortcut_when_macro_capture_active() {
    let key_event = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CTRL);
    let mut keybind_map = HashMap::new();
    keybind_map.insert(
        key_event,
        KeyBindAction::Macro(MacroAction {
            macro_text: "look\r".to_string(),
        }),
    );

    let target = VellumGuiApp::resolve_global_dispatch_target(
        key_event,
        &keybind_map,
        &AppKeybinds::default(),
        true,
    );
    assert!(matches!(
        target,
        Some(GlobalDispatchTarget::Shortcut(AppShortcut::StartSearch))
    ));
}

#[test]
fn test_global_dispatch_suppresses_macro_without_shortcut() {
    let key_event = KeyEvent::new(KeyCode::Keypad1, KeyModifiers::NONE);
    let mut keybind_map = HashMap::new();
    keybind_map.insert(
        key_event,
        KeyBindAction::Macro(MacroAction {
            macro_text: "sw\r".to_string(),
        }),
    );

    let target = VellumGuiApp::resolve_global_dispatch_target(
        key_event,
        &keybind_map,
        &AppKeybinds::default(),
        true,
    );
    assert!(target.is_none());
}

#[test]
fn test_global_dispatch_routes_gui_command_actions() {
    // Previously-dead keyboard actions (send_last_command, tab nav, search
    // match-nav, window switch, start_search) now resolve to a
    // GuiCommandAction target so the GUI runs them — this is the fix for
    // "send_last_command does nothing".
    for name in [
        "send_last_command",
        "send_second_last_command",
        "next_tab",
        "prev_tab",
        "next_unread_tab",
        "switch_current_window",
        "start_search",
        "next_search_match",
        "prev_search_match",
    ] {
        let key_event = KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE);
        let mut keybind_map = HashMap::new();
        keybind_map.insert(key_event, KeyBindAction::Action(name.to_string()));
        let target = VellumGuiApp::resolve_global_dispatch_target(
            key_event,
            &keybind_map,
            &AppKeybinds::default(),
            false,
        );
        assert!(
            matches!(target, Some(GlobalDispatchTarget::GuiCommandAction(ref n)) if n == name),
            "action '{name}' should resolve to GuiCommandAction, got {target:?}"
        );
    }
}

#[test]
fn test_command_input_owned_actions_stay_with_the_widget() {
    // send_command / previous_command / next_command / cursor_clear_line /
    // clear_search are owned by the command-input widget or search-close
    // path (which read the keybind config themselves); routing them
    // globally too would double-fire on their default keys.
    for name in [
        "send_command",
        "previous_command",
        "next_command",
        "cursor_clear_line",
        "clear_search",
    ] {
        let key_event = KeyEvent::new(KeyCode::F(9), KeyModifiers::NONE);
        let mut keybind_map = HashMap::new();
        keybind_map.insert(key_event, KeyBindAction::Action(name.to_string()));
        let target = VellumGuiApp::resolve_global_dispatch_target(
            key_event,
            &keybind_map,
            &AppKeybinds::default(),
            false,
        );
        assert!(
            !matches!(target, Some(GlobalDispatchTarget::GuiCommandAction(_))),
            "action '{name}' must NOT route globally (widget owns it), got {target:?}"
        );
    }
}

#[test]
fn test_frontend_keycode_to_egui_round_trips_common_keys() {
    use crate::data::input::KeyCode;
    assert_eq!(
        VellumGuiApp::frontend_keycode_to_egui(KeyCode::Up),
        Some(eframe::egui::Key::ArrowUp)
    );
    assert_eq!(
        VellumGuiApp::frontend_keycode_to_egui(KeyCode::Enter),
        Some(eframe::egui::Key::Enter)
    );
    assert_eq!(
        VellumGuiApp::frontend_keycode_to_egui(KeyCode::Char('r')),
        Some(eframe::egui::Key::R)
    );
    assert_eq!(
        VellumGuiApp::frontend_keycode_to_egui(KeyCode::F(3)),
        Some(eframe::egui::Key::F3)
    );
}

#[test]
fn test_egui_num_key_maps_to_keypad_event() {
    let event = VellumGuiApp::egui_key_to_frontend_event(
        eframe::egui::Key::Num1,
        eframe::egui::Modifiers::default(),
    )
    .expect("Num1 should map to a frontend key event");
    assert_eq!(event.code, KeyCode::Char('1'));
    assert_eq!(event.modifiers, KeyModifiers::NONE);
}

#[test]
fn test_numpad_binding_name_maps_to_keypad_codes() {
    assert_eq!(
        VellumGuiApp::numpad_binding_name_to_frontend_code("num_1"),
        Some(KeyCode::Keypad1)
    );
    assert_eq!(
        VellumGuiApp::numpad_binding_name_to_frontend_code("num_plus"),
        Some(KeyCode::KeypadPlus)
    );
    assert_eq!(
        VellumGuiApp::numpad_binding_name_to_frontend_code("num_decimal"),
        Some(KeyCode::KeypadPeriod)
    );
    assert_eq!(
        VellumGuiApp::numpad_binding_name_to_frontend_code("unknown"),
        None
    );
}

/// eframe's capture set is keyed by bare key name, with no modifier information, so
/// a modified bind like ctrl+num_plus must still register "num_plus" for capture.
/// If it didn't, the key would keep its native behavior and the chord would never
/// reach dispatch.
#[test]
fn test_numpad_capture_name_ignores_modifiers() {
    for name in ["num_0", "num_plus", "num_divide", "num_enter"] {
        let code = VellumGuiApp::numpad_binding_name_to_frontend_code(name)
            .unwrap_or_else(|| panic!("{name} should resolve"));

        // Whatever modifiers a binding carries, the capture name is the bare key.
        assert_eq!(
            VellumGuiApp::frontend_code_to_numpad_binding_name(code),
            Some(name),
            "{name} must round-trip to its capture name regardless of modifiers"
        );
    }
}

/// The GUI capture layer and keybinds.toml must agree on spelling, or a binding
/// saved by the editor would never be captured at runtime.
#[test]
fn test_numpad_capture_names_match_keybind_parser() {
    for name in crate::data::input::keypad_names() {
        let capture_code = VellumGuiApp::numpad_binding_name_to_frontend_code(name)
            .unwrap_or_else(|| panic!("{name} unknown to the GUI capture layer"));
        let (parsed_code, _) = crate::config::parse_key_string(name)
            .unwrap_or_else(|| panic!("{name} unknown to the keybind parser"));

        assert_eq!(
            capture_code, parsed_code,
            "{name} resolves differently in the GUI capture layer and the parser"
        );
    }
}

#[test]
fn test_resolve_link_dispatch_direct_cmd_prefers_noun() {
    let link = LinkData {
        exist_id: "_direct_".to_string(),
        noun: "get coin".to_string(),
        text: "GET COIN".to_string(),
        coord: None,
    };

    let dispatch = VellumGuiApp::resolve_link_dispatch(&link, None);
    assert_eq!(
        dispatch,
        Some(GuiLinkDispatch::NetworkCommand("get coin".to_string()))
    );
}

#[test]
fn test_resolve_link_dispatch_direct_cmd_falls_back_to_text() {
    let link = LinkData {
        exist_id: "_direct_".to_string(),
        noun: String::new(),
        text: "SKILLS BASE".to_string(),
        coord: None,
    };

    let dispatch = VellumGuiApp::resolve_link_dispatch(&link, None);
    assert_eq!(
        dispatch,
        Some(GuiLinkDispatch::NetworkCommand("SKILLS BASE".to_string()))
    );
}

#[test]
fn test_resolve_link_dispatch_menu_request_for_regular_link() {
    let link = LinkData {
        exist_id: "12345".to_string(),
        noun: "sword".to_string(),
        text: "a rusty sword".to_string(),
        coord: None,
    };

    let dispatch = VellumGuiApp::resolve_link_dispatch(&link, None);
    assert_eq!(
        dispatch,
        Some(GuiLinkDispatch::MenuRequest {
            exist_id: "12345".to_string(),
            noun: "sword".to_string(),
        })
    );
}

#[test]
fn test_resolve_link_dispatch_url_sentinel_opens_browser() {
    let link = LinkData {
        exist_id: crate::data::URL_LINK_SENTINEL.to_string(),
        noun: "https://gswiki.play.net/Radial_Sweep".to_string(),
        text: "Radial Sweep".to_string(),
        coord: None,
    };
    let dispatch = VellumGuiApp::resolve_link_dispatch(&link, None);
    assert_eq!(
        dispatch,
        Some(GuiLinkDispatch::OpenUrl(
            "https://gswiki.play.net/Radial_Sweep".to_string()
        ))
    );
}

#[test]
fn test_resolve_link_dispatch_url_sentinel_rejects_non_http_schemes() {
    for bad in ["javascript:alert(1)", "file:///etc/passwd", "vellum://x"] {
        let link = LinkData {
            exist_id: crate::data::URL_LINK_SENTINEL.to_string(),
            noun: bad.to_string(),
            text: "x".to_string(),
            coord: None,
        };
        assert_eq!(
            VellumGuiApp::resolve_link_dispatch(&link, None),
            None,
            "{bad} must not dispatch"
        );
    }
}

#[test]
fn test_resolve_link_dispatch_coord_without_cmdlist_falls_back_to_menu() {
    let link = LinkData {
        exist_id: "12345".to_string(),
        noun: "sword".to_string(),
        text: "a rusty sword".to_string(),
        coord: Some("2524,2061".to_string()),
    };

    let dispatch = VellumGuiApp::resolve_link_dispatch(&link, None);
    assert_eq!(
        dispatch,
        Some(GuiLinkDispatch::MenuRequest {
            exist_id: "12345".to_string(),
            noun: "sword".to_string(),
        })
    );
}

#[test]
fn test_segment_has_clickable_link_for_monsterbold_link_segment() {
    let segment = TextSegment {
        text: "goblin".to_string(),
        fg: Some("#00ff00".to_string()),
        bg: None,
        bold: true,
        mono: false,
        span_type: SpanType::Monsterbold,
        link_data: Some(LinkData {
            exist_id: "12345".to_string(),
            noun: "goblin".to_string(),
            text: "goblin".to_string(),
            coord: None,
        }),
        custom_emoji: None,
        inline_image: None,
    };

    assert!(VellumGuiApp::segment_has_clickable_link(&segment));
}

#[test]
fn test_segment_has_clickable_link_false_without_link_data() {
    let segment = TextSegment {
        text: "plain text".to_string(),
        fg: None,
        bg: None,
        bold: false,
        mono: false,
        span_type: SpanType::Link,
        link_data: None,
        custom_emoji: None,
        inline_image: None,
    };

    assert!(!VellumGuiApp::segment_has_clickable_link(&segment));
}

#[test]
fn test_click_pos_to_grid_clamps_values() {
    let pos = Pos2::new(-10.0, 999999.0);
    let (x, y) = VellumGuiApp::click_pos_to_grid(pos);
    assert_eq!(x, 0);
    assert_eq!(y, u16::MAX);
}

#[test]
fn test_status_abbreviation_prefers_config_value() {
    let mut cfg = TargetListConfig::default();
    cfg.status_abbrev
        .insert("weirdstatus".to_string(), "wiz".to_string());

    let abbreviated = VellumGuiApp::status_abbreviation("weirdstatus", &cfg);
    assert_eq!(abbreviated, "wiz");
}

#[test]
fn test_status_abbreviation_falls_back_to_first_three_chars() {
    let cfg = TargetListConfig::default();

    let abbreviated = VellumGuiApp::status_abbreviation("awkward", &cfg);
    assert_eq!(abbreviated, "awk");
}

#[test]
fn test_normalize_entity_id_strips_hash_prefix() {
    assert_eq!(VellumGuiApp::normalize_entity_id("#12345"), "12345");
    assert_eq!(VellumGuiApp::normalize_entity_id("12345"), "12345");
}

#[test]
fn test_room_line_from_links_builds_one_labeled_line() {
    let lines = VellumGuiApp::room_line_from_links(
        "Obvious exits: ",
        ["north", "south"].iter().map(|dir| {
            (
                dir.to_string(),
                Some(crate::data::LinkData {
                    exist_id: "_direct_".to_string(),
                    noun: dir.to_string(),
                    text: dir.to_string(),
                    coord: None,
                }),
            )
        }),
    );

    assert_eq!(lines.len(), 1);
    let texts: Vec<&str> = lines[0].segments.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, vec!["Obvious exits: ", "north", ", ", "south"]);
    assert!(lines[0].segments[1].link_data.is_some());
    assert_eq!(lines[0].segments[1].span_type, crate::data::SpanType::Link);
    assert!(lines[0].segments[2].link_data.is_none());
}

#[test]
fn test_room_line_from_links_empty_entries_yield_no_line() {
    let lines = VellumGuiApp::room_line_from_links("Also here: ", std::iter::empty());
    assert!(lines.is_empty());
}

#[test]
fn test_room_component_lines_preserve_segments_and_set_stream() {
    let component = vec![vec![TextSegment::plain("Room text")]];

    let lines = VellumGuiApp::room_component_lines(Some(&component));

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].stream, "room");
    assert_eq!(lines[0].segments[0].text, "Room text");
}

#[test]
fn test_format_target_line_respects_status_position() {
    let cfg = TargetListConfig::default();
    let creature = Creature {
        name: "a goblin".to_string(),
        noun: Some("goblin".to_string()),
        id: "#101".to_string(),
        status: Some("stunned".to_string()),
        flags: None,
    };

    let start = VellumGuiApp::format_target_line(&creature, &cfg, "start");
    assert_eq!(start, "[stu] a goblin");

    let end = VellumGuiApp::format_target_line(&creature, &cfg, "end");
    assert_eq!(end, "a goblin [stu]");
}

#[test]
fn test_format_target_line_joins_crtr_statuses() {
    let cfg = TargetListConfig::default();
    let creature = Creature {
        name: "a sea nymph".to_string(),
        noun: Some("nymph".to_string()),
        id: "#607736".to_string(),
        // Structured flags beat the legacy text status
        status: Some("stunned".to_string()),
        flags: Some(crate::core::state::CreatureFlags {
            statuses: vec!["stunned".to_string(), "prone".to_string()],
            hostile: true,
            ..Default::default()
        }),
    };

    let line = VellumGuiApp::format_target_line(&creature, &cfg, &cfg.status_position);
    assert_eq!(line, "a sea nymph [stu,prn]");
}

#[test]
fn test_format_player_line_includes_both_statuses() {
    let mut cfg = TargetListConfig::default();
    cfg.status_position = "start".to_string();
    let player = Player {
        name: "Nisugi".to_string(),
        id: "-42".to_string(),
        primary_status: Some("stunned".to_string()),
        secondary_status: Some("prone".to_string()),
        dead: false,
    };

    let start = VellumGuiApp::format_player_line(&player, &cfg);
    assert_eq!(start, "[stu] [prn] Nisugi");

    cfg.status_position = "end".to_string();
    let end = VellumGuiApp::format_player_line(&player, &cfg);
    assert_eq!(end, "Nisugi [stu] [prn]");
}

#[test]
fn test_format_player_line_dead_leads_with_ded() {
    let cfg = TargetListConfig::default(); // status_position defaults to "end"
                                           // Dead + prone (the stacked case from live logs).
    let player = Player {
        name: "Regyy".to_string(),
        id: "-1".to_string(),
        primary_status: None,
        secondary_status: Some("prone".to_string()),
        dead: true,
    };
    let line = VellumGuiApp::format_player_line(&player, &cfg);
    assert_eq!(line, "Regyy [ded] [prn]");
}

#[test]
fn test_is_valid_target_filters_dead_and_excluded_nouns() {
    // Filtering is now canonical on Creature::is_valid_target; the GUI
    // routes through it. Default excluded_nouns = ["arm", "coal"].
    let cfg = TargetListConfig::default();
    let dead_creature = Creature {
        name: "a dead goblin".to_string(),
        noun: Some("goblin".to_string()),
        id: "#1".to_string(),
        status: Some("dead".to_string()),
        flags: None,
    };
    let body_part_creature = Creature {
        name: "an arm".to_string(),
        noun: Some("arm".to_string()),
        id: "#2".to_string(),
        status: None,
        flags: None,
    };

    assert!(!dead_creature.is_valid_target(&cfg.excluded_nouns));
    assert!(!body_part_creature.is_valid_target(&cfg.excluded_nouns));
}

#[test]
fn test_is_valid_target_keeps_live_creatures() {
    let cfg = TargetListConfig::default();
    let live_creature = Creature {
        name: "a forest troll".to_string(),
        noun: Some("troll".to_string()),
        id: "#3".to_string(),
        status: Some("stunned".to_string()),
        flags: None,
    };

    assert!(live_creature.is_valid_target(&cfg.excluded_nouns));
}

// ── drop_tab_from_groups (bug #5: deleting a grouped window) ─────────

fn group(members: &[TabKey]) -> super::TabGroup {
    super::TabGroup {
        members: members.to_vec(),
        horizontal: false,
        merged: members.to_vec(),
        end_anchored: members.to_vec(),
        weights: Vec::new(),
    }
}

#[test]
fn distribute_group_heights_equal_when_weights_default() {
    // Two flexible members, no gap: an empty/default weight list keeps
    // the historical 50/50 split.
    let h = super::VellumGuiApp::distribute_group_heights(100.0, 0.0, &[None, None], &[1.0, 1.0]);
    assert_eq!(h, vec![50.0, 50.0]);
}

#[test]
fn distribute_group_heights_weighted_split() {
    // buffs=2, cooldowns=1 → 2:1 split of the 90 leftover.
    let h = super::VellumGuiApp::distribute_group_heights(90.0, 0.0, &[None, None], &[2.0, 1.0]);
    assert_eq!(h, vec![60.0, 30.0]);
}

#[test]
fn distribute_group_heights_fixed_members_keep_natural() {
    // A fixed 20px bar plus two flexible members weighted 3:1 over the
    // remaining 200 (220 - 20); both shares clear the flex floor.
    let h = super::VellumGuiApp::distribute_group_heights(
        220.0,
        0.0,
        &[Some(20.0), None, None],
        &[1.0, 3.0, 1.0],
    );
    assert_eq!(h, vec![20.0, 150.0, 50.0]);
}

#[test]
fn distribute_group_heights_nonpositive_weight_is_neutral() {
    // A zero/negative weight is treated as 1.0, not collapsed.
    let h = super::VellumGuiApp::distribute_group_heights(100.0, 0.0, &[None, None], &[0.0, -5.0]);
    assert_eq!(h, vec![50.0, 50.0]);
}

#[test]
fn distribute_group_heights_floors_tiny_share() {
    // A tiny weight still yields at least the flex floor (24), so a
    // member never collapses to nothing.
    let h =
        super::VellumGuiApp::distribute_group_heights(100.0, 0.0, &[None, None], &[1000.0, 0.001]);
    assert!(h[1] >= 24.0, "tiny-weight member floored, got {}", h[1]);
}

#[test]
fn distribute_group_heights_accounts_for_gaps() {
    // Two members, gap 10 → leftover 90 split evenly.
    let h = super::VellumGuiApp::distribute_group_heights(100.0, 10.0, &[None, None], &[1.0, 1.0]);
    assert_eq!(h, vec![45.0, 45.0]);
}

#[test]
fn drop_tab_dissolves_two_member_group() {
    // Parent+child pair: deleting either must dissolve the whole group
    // so the survivor is a free, standalone window again — not a
    // follower stuck rendering inside a leader that no longer exists.
    let parent = TabKey::TextByName {
        id: "parent".into(),
    };
    let child = TabKey::TextByName { id: "child".into() };
    let mut groups = vec![group(&[parent.clone(), child.clone()])];

    VellumGuiApp::drop_tab_from_groups(&mut groups, &parent);

    assert!(
        groups.is_empty(),
        "group left with one member must dissolve"
    );
}

#[test]
fn drop_tab_shrinks_larger_group_and_purges_side_lists() {
    // A three-member group loses one member but survives; the removed
    // key must also leave the merged/end_anchored side lists so no stale
    // reference lingers.
    let a = TabKey::TextByName { id: "a".into() };
    let b = TabKey::TextByName { id: "b".into() };
    let c = TabKey::TextByName { id: "c".into() };
    let mut groups = vec![group(&[a.clone(), b.clone(), c.clone()])];

    VellumGuiApp::drop_tab_from_groups(&mut groups, &b);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members, vec![a.clone(), c.clone()]);
    assert!(!groups[0].merged.contains(&b));
    assert!(!groups[0].end_anchored.contains(&b));
}

#[test]
fn drop_tab_is_noop_when_key_absent() {
    let a = TabKey::TextByName { id: "a".into() };
    let b = TabKey::TextByName { id: "b".into() };
    let stranger = TabKey::TextByName {
        id: "stranger".into(),
    };
    let mut groups = vec![group(&[a.clone(), b.clone()])];

    VellumGuiApp::drop_tab_from_groups(&mut groups, &stranger);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members, vec![a, b]);
}

fn migration_test_layout() -> crate::config::Layout {
    use crate::config::{BorderSides, CompassWidgetData, TextWidgetData, WindowBase, WindowDef};
    let base = |name: &str| WindowBase {
        name: name.to_string(),
        row: crate::data::geometry::Row::new(0),
        col: crate::data::geometry::Col::new(0),
        rows: crate::data::geometry::Height::new(10),
        cols: crate::data::geometry::Width::new(40),
        show_border: true,
        border_style: "single".to_string(),
        border_sides: BorderSides::default(),
        border_color: None,
        show_title: true,
        title: None,
        title_position: "top-left".to_string(),
        background_color: None,
        text_color: None,
        transparent_background: false,
        locked: false,
        min_rows: None,
        max_rows: None,
        min_cols: None,
        max_cols: None,
        visibility: crate::config::WindowVisibility::Shown,
        binding: None,
        content_align: None,
        tts_speak: false,
        text_size: None,
        font_family: None,
    };
    crate::config::Layout {
        windows: vec![
            WindowDef::Text {
                base: base("main"),
                data: TextWidgetData {
                    streams: vec!["main".to_string()],
                    buffer_size: 100,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            },
            WindowDef::Compass {
                base: base("compass"),
                data: CompassWidgetData {
                    active_color: None,
                    inactive_color: None,
                },
            },
        ],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    }
}

fn migration_name_of(key: &TabKey) -> Option<String> {
    match key {
        TabKey::TextMain => Some("main".to_string()),
        TabKey::Compass => Some("compass".to_string()),
        _ => None,
    }
}

#[test]
fn test_migrate_tab_settings_moves_values_onto_defs() {
    let mut layout = migration_test_layout();
    let mut settings = HashMap::new();
    settings.insert(
        TabKey::TextMain,
        TabSettings {
            text_size: Some(16.0),
            font_primary: FontRef::Named("Fira Code".to_string()),
            wrap_text: false,
            ..Default::default()
        },
    );
    // Compass has no wordwrap field: wrap stays a legacy GUI setting.
    settings.insert(
        TabKey::Compass,
        TabSettings {
            wrap_text: false,
            ..Default::default()
        },
    );

    let (layout_changed, gui_changed) =
        VellumGuiApp::migrate_tab_settings_to_layout(&mut settings, &mut layout, migration_name_of);
    assert!(layout_changed);
    assert!(gui_changed);

    assert_eq!(layout.windows[0].base().text_size, Some(16.0));
    assert_eq!(
        layout.windows[0].base().font_family.as_deref(),
        Some("Fira Code")
    );
    let crate::config::WindowDef::Text { data, .. } = &layout.windows[0] else {
        panic!("main window should be a text def");
    };
    assert!(!data.wordwrap);

    let migrated = settings.get(&TabKey::TextMain).unwrap();
    assert_eq!(migrated.text_size, None);
    assert!(matches!(migrated.font_primary, FontRef::SystemDefault));
    assert!(migrated.wrap_text);

    // Compass: nothing moved onto the def; legacy wrap preserved.
    assert_eq!(layout.windows[1].base().text_size, None);
    assert!(!settings.get(&TabKey::Compass).unwrap().wrap_text);

    // Idempotent: a second run changes nothing.
    let (layout_changed, gui_changed) =
        VellumGuiApp::migrate_tab_settings_to_layout(&mut settings, &mut layout, migration_name_of);
    assert!(!layout_changed);
    assert!(!gui_changed);
}

#[test]
fn test_migrate_tab_settings_existing_def_value_wins() {
    let mut layout = migration_test_layout();
    layout.windows[0].base_mut().text_size = Some(20.0);
    layout.windows[0].base_mut().font_family = Some("Consolas".to_string());

    let mut settings = HashMap::new();
    settings.insert(
        TabKey::TextMain,
        TabSettings {
            text_size: Some(16.0),
            font_primary: FontRef::Named("Fira Code".to_string()),
            ..Default::default()
        },
    );

    let (layout_changed, gui_changed) =
        VellumGuiApp::migrate_tab_settings_to_layout(&mut settings, &mut layout, migration_name_of);
    assert!(!layout_changed);
    // The legacy fields still empty out so they stop shadowing the def.
    assert!(gui_changed);
    assert_eq!(layout.windows[0].base().text_size, Some(20.0));
    assert_eq!(
        layout.windows[0].base().font_family.as_deref(),
        Some("Consolas")
    );
    let migrated = settings.get(&TabKey::TextMain).unwrap();
    assert_eq!(migrated.text_size, None);
    assert!(matches!(migrated.font_primary, FontRef::SystemDefault));
}

#[test]
fn test_migrate_tab_settings_ignores_unmapped_tabs() {
    let mut layout = migration_test_layout();
    let mut settings = HashMap::new();
    settings.insert(
        TabKey::Vitals,
        TabSettings {
            text_size: Some(16.0),
            ..Default::default()
        },
    );

    let (layout_changed, gui_changed) =
        VellumGuiApp::migrate_tab_settings_to_layout(&mut settings, &mut layout, migration_name_of);
    assert!(!layout_changed);
    assert!(!gui_changed);
    assert_eq!(settings.get(&TabKey::Vitals).unwrap().text_size, Some(16.0));
}

// --- Keybind bug #1: punctuation keys must map through the egui→frontend
// translation, or Capture ignores them and the live matcher never fires. ---

/// Every punctuation key reported unbindable maps to its UNSHIFTED base
/// char — the canonical form the keybind map and keybinds.toml store
/// (`parse_key_string` treats a single char as `KeyCode::Char`, and
/// `key_event_to_string` lowercases). Shift is carried as a modifier, not
/// folded into a shifted glyph.
#[test]
fn egui_punctuation_keys_map_to_unshifted_chars() {
    use eframe::egui::{Key, Modifiers};
    let none = Modifiers::NONE;
    let cases = [
        (Key::Quote, '\''),
        (Key::Semicolon, ';'),
        (Key::Comma, ','),
        (Key::Period, '.'),
        (Key::Slash, '/'),
        (Key::Minus, '-'),
        (Key::Equals, '='),
        (Key::Backtick, '`'),
        (Key::OpenBracket, '['),
        (Key::CloseBracket, ']'),
        (Key::Backslash, '\\'),
    ];
    for (key, expected) in cases {
        assert_eq!(
            VellumGuiApp::egui_key_to_frontend_code(key, none),
            Some(KeyCode::Char(expected)),
            "egui::Key::{key:?} should map to Char({expected:?})"
        );
    }
}

/// Capture → serialize → parse → match round-trip: a captured punctuation
/// press must produce the same `KeyEvent` the live matcher builds from a
/// keybind-map key loaded off disk. If these diverge, the binding "exists"
/// but never fires (the reported bug).
#[test]
fn punctuation_keybind_round_trips_capture_to_matcher() {
    use crate::core::menu_actions::key_event_to_string;
    use eframe::egui::{Key, Modifiers};

    for (key, ch) in [
        (Key::Semicolon, ';'),
        (Key::Slash, '/'),
        (Key::Minus, '-'),
        (Key::OpenBracket, '['),
    ] {
        // Capture side: egui press → frontend KeyEvent.
        let captured = VellumGuiApp::egui_key_to_frontend_event(key, Modifiers::NONE)
            .expect("punctuation press should capture");
        // Serialize the way the form fills its Key-combo field / saves TOML.
        let combo = key_event_to_string(captured);
        assert_eq!(combo, ch.to_string());
        // Matcher side: the string re-parses to the same KeyEvent that the
        // keybind map is keyed on.
        let (code, modifiers) = crate::config::parse_key_string(&combo).expect("combo parses");
        assert_eq!(KeyEvent::new(code, modifiers), captured);
    }
}

/// Shift + punctuation carries the base char plus a shift modifier (the
/// chosen convention), not a shifted glyph — so `shift+;` serializes to
/// `"shift+;"`, matching the TUI/config model.
#[test]
fn shift_punctuation_carries_base_char_and_shift_modifier() {
    use crate::core::menu_actions::key_event_to_string;
    use eframe::egui::{Key, Modifiers};

    let event = VellumGuiApp::egui_key_to_frontend_event(Key::Semicolon, Modifiers::SHIFT)
        .expect("shift+; should capture");
    assert_eq!(event.code, KeyCode::Char(';'));
    assert!(event.modifiers.shift);
    assert_eq!(key_event_to_string(event), "shift+;");
}

// --- Keybind bug #2: alt+key / shift+key produce a printable Event::Text
// alongside the Event::Key. When a keybind consumes the press we must strip
// BOTH, or the char leaks into the command input. This tests the retain
// predicate `handle_global_input` applies to the processed event vector. ---

/// The consume filter removes the leaked printable Text (and the Key,
/// Copy/Cut/Paste) while leaving unrelated events (pointer motion) intact.
#[test]
fn dispatched_keybind_strips_leaked_text_event() {
    use eframe::egui::{Event, Key, Modifiers, Pos2};

    // What an alt+2 press looks like in the processed event vector: the Key
    // (already targeted by consume_key) plus the printable Text egui emits
    // for alt/shift chords, plus an unrelated pointer event.
    let mut events = vec![
        Event::Key {
            key: Key::Num2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::ALT,
        },
        Event::Text("2".to_string()),
        Event::PointerMoved(Pos2::new(1.0, 2.0)),
    ];

    // Identical predicate to handle_global_input's consume block.
    events.retain(|event| {
        !matches!(
            event,
            Event::Key { .. } | Event::Text(_) | Event::Paste(_) | Event::Copy | Event::Cut
        )
    });

    assert!(
        !events.iter().any(|e| matches!(e, Event::Text(_))),
        "the leaked '2' Text event must be stripped so it can't reach the input line"
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::PointerMoved(_))),
        "unrelated pointer events must survive"
    );
}

/// Build a text window whose buffer holds `lines`, one line per entry.
#[cfg(test)]
fn text_window_with(name: &str, lines: &[&str]) -> crate::data::WindowState {
    use crate::data::{StyledLine, WindowState};
    let mut window = WindowState::new_text(name, 1000);
    if let crate::data::WindowContent::Text(content) = &mut window.content {
        for text in lines {
            content.add_line(StyledLine {
                segments: vec![TextSegment::plain(text.to_string())],
                stream: name.to_string(),
                timestamp: None,
            });
        }
    }
    window
}

#[test]
fn search_targets_list_every_searchable_window_and_name_tabs() {
    use crate::core::AppCore;
    use crate::data::{
        TabDefinition, TabState, TabbedTextContent, TextContent, WindowContent, WindowState,
    };

    let mut core = AppCore::new_for_test();
    core.ui_state
        .windows
        .insert("main".into(), text_window_with("main", &["a gas cloud"]));
    // A window with no text buffer at all must never be offered as a target;
    // this is the `command_input` case that made match-nav dead on arrival.
    core.ui_state.windows.insert(
        "command_input".into(),
        WindowState::new_command_input("command_input"),
    );
    let mut tabbed = WindowState::new_text("tabs", 100);
    tabbed.widget_type = crate::data::WidgetType::TabbedText;
    let tab_of = |name: &str| TabState {
        definition: TabDefinition {
            name: name.to_string(),
            streams: vec![name.to_string()],
            show_timestamps: false,
            ignore_activity: false,
            timestamp_position: Default::default(),
        },
        content: TextContent::new(name, 100),
        has_unread: false,
    };
    tabbed.content = WindowContent::TabbedText(TabbedTextContent {
        tabs: vec![tab_of("story"), tab_of("thoughts")],
        active_tab_index: 1,
    });
    core.ui_state.windows.insert("tabs".into(), tabbed);

    let targets = VellumGuiApp::search_targets(&core);
    let ids: Vec<&str> = targets.iter().map(|(_, id)| id.as_str()).collect();
    assert!(ids.contains(&"main"), "text window is a target: {ids:?}");
    assert!(
        !ids.iter().any(|id| id.starts_with("command_input")),
        "command_input has no buffer and must not be offered: {ids:?}"
    );
    // The ACTIVE tab is the target, addressed by its scroll id, and labeled
    // by the tab's own name so the dropdown reads "tabs ▸ thoughts".
    assert!(ids.contains(&"tabs::tab1"), "active tab is a target: {ids:?}");
    let label = targets
        .iter()
        .find(|(_, id)| id == "tabs::tab1")
        .map(|(label, _)| label.as_str())
        .unwrap();
    assert!(label.contains("thoughts"), "tab labeled by name: {label}");
}

#[test]
fn search_hits_are_scoped_to_the_chosen_target() {
    use crate::core::AppCore;

    let mut core = AppCore::new_for_test();
    core.ui_state.windows.insert(
        "main".into(),
        text_window_with("main", &["a gas cloud", "no match", "more gas"]),
    );
    core.ui_state
        .windows
        .insert("thoughts".into(), text_window_with("thoughts", &["gas"]));

    // Searching "main" sees only main's hits — never the other window's,
    // which is the whole point of decoupling the target from focus.
    let (id, hits) = VellumGuiApp::search_hits_in(&core, "main", "gas").expect("main searchable");
    assert_eq!(id, "main");
    assert_eq!(hits, vec![0, 2], "line indices into main's buffer");

    let (_, hits) =
        VellumGuiApp::search_hits_in(&core, "thoughts", "gas").expect("thoughts searchable");
    assert_eq!(hits, vec![0]);

    // A clean search resolves, but with nothing to cycle.
    let (_, hits) = VellumGuiApp::search_hits_in(&core, "main", "zzz").expect("resolves");
    assert!(hits.is_empty());

    // A window with no buffer resolves to None, so the caller can say so
    // instead of silently doing nothing.
    assert!(VellumGuiApp::search_hits_in(&core, "nonexistent", "gas").is_none());
}

#[test]
fn match_cursor_survives_lines_scrolling_off_a_full_buffer() {
    use crate::core::AppCore;
    use crate::data::{StyledLine, WindowContent, WindowState};

    // A buffer capped at 4 lines: adding a 5th drops the oldest, shifting
    // every surviving index down by one. This is the live-window case where
    // an index-based cursor silently slides onto a different line.
    let mut core = AppCore::new_for_test();
    let mut window = WindowState::new_text("main", 4);
    let push = |content: &mut crate::data::TextContent, text: &str| {
        content.add_line(StyledLine {
            segments: vec![TextSegment::plain(text.to_string())],
            stream: "main".into(),
            timestamp: None,
        });
    };
    if let WindowContent::Text(content) = &mut window.content {
        push(content, "a troll appears");   // index 0
        push(content, "filler one");        // index 1
        push(content, "another troll");     // index 2
        push(content, "filler two");        // index 3
    }
    core.ui_state.windows.insert("main".into(), window);

    let content = VellumGuiApp::search_content(&core, "main").expect("buffer");
    let (_, hits) = VellumGuiApp::search_hits_in(&core, "main", "troll").unwrap();
    assert_eq!(hits, vec![0, 2]);
    // Pin the cursor to the SECOND match ("another troll", index 2).
    let cursor = VellumGuiApp::absolute_line(content, 2);
    assert_eq!(VellumGuiApp::line_index_of(content, cursor), Some(2));

    // One new line arrives: the buffer is full, so everything shifts down.
    if let Some(WindowContent::Text(content)) =
        core.ui_state.windows.get_mut("main").map(|w| &mut w.content)
    {
        push(content, "a new line");
    }
    let content = VellumGuiApp::search_content(&core, "main").expect("buffer");
    let (_, hits) = VellumGuiApp::search_hits_in(&core, "main", "troll").unwrap();
    // "a troll appears" fell off the front; "another troll" moved 2 -> 1.
    assert_eq!(hits, vec![1]);
    assert_eq!(
        VellumGuiApp::line_index_of(content, cursor),
        Some(1),
        "the cursor still names the same LINE, at its new index"
    );

    // Once the cursor's own line scrolls away, it resolves to None so the
    // caller restarts cleanly instead of pointing at a stranger's line.
    if let Some(WindowContent::Text(content)) =
        core.ui_state.windows.get_mut("main").map(|w| &mut w.content)
    {
        push(content, "x");
        push(content, "y");
    }
    let content = VellumGuiApp::search_content(&core, "main").expect("buffer");
    assert_eq!(VellumGuiApp::line_index_of(content, cursor), None);
}
