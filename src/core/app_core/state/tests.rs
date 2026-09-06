//! Test module of the parent facade, split out for size —
//! `super` is still the parent module, so private access and
//! `use super::*` semantics are identical to the inline mod.

use super::*;
use crate::config::{BorderSides, Layout, SpacerWidgetData, WindowBase, WindowDef};

#[test]
fn reconnect_request_flag_is_consumed_exactly_once() {
    let mut core = AppCore::new_for_test();
    // Fresh core has no pending request.
    assert!(!core.take_reconnect_request());
    // The frontend dispatcher sets this on UiAction::Reconnect.
    core.reconnect_requested = true;
    // First drain sees it, then clears it — the runtime reconnects once.
    assert!(core.take_reconnect_request());
    assert!(!core.take_reconnect_request());
}

#[test]
fn remote_combat_target_menu_normalizes_protocol_prefixed_id() {
    let mut core = AppCore::new_for_test();
    let origin = crate::core::remote::MenuOrigin::Remote {
        client_id: 41,
        request_id: 7,
    };
    let link = crate::data::LinkData {
        exist_id: "#209691632".to_string(),
        noun: "king".to_string(),
        text: "a massive troll king".to_string(),
        coord: None,
    };

    let command = core
        .resolve_link_activation(&link, origin.clone())
        .expect("plain target link requests a context menu");

    assert_eq!(command, "_menu #209691632 1\n");
    let pending = core
        .pending_menu_requests
        .get("1")
        .expect("menu request is correlated");
    assert_eq!(pending.exist_id, "209691632");
    assert_eq!(pending.noun, "king");
    assert_eq!(pending.origin, origin);
}

// Test helper to create a minimal WindowBase
fn test_window_base(name: &str) -> WindowBase {
    WindowBase {
        name: name.to_string(),
        row: crate::data::geometry::Row::new(0),
        col: crate::data::geometry::Col::new(0),
        rows: crate::data::geometry::Height::new(2),
        cols: crate::data::geometry::Width::new(5),
        show_border: false,
        border_style: "single".to_string(),
        border_sides: BorderSides::default(),
        border_color: None,
        show_title: false,
        title: None,
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
        title_position: "top-left".to_string(),
    }
}

#[test]
fn test_edit_picker_reaches_hidden_windows() {
    // A hidden spacer must appear in the edit picker's template map when
    // include_hidden is set, and stay out of the visible-only map.
    let mut base = test_window_base("spacer_1");
    base.visibility = crate::config::WindowVisibility::Hidden;
    let layout = Layout {
        windows: vec![WindowDef::Spacer {
            base,
            data: SpacerWidgetData {},
        }],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let with_hidden = crate::core::local_catalog::layout_windows_by_category(&layout, false, true);
    assert!(with_hidden
        .get(&crate::config::WidgetCategory::Other)
        .is_some_and(|names| names.iter().any(|n| n == "spacer_1")));

    let visible_only = crate::core::local_catalog::visible_by_category(&layout, false);
    assert!(!visible_only
        .get(&crate::config::WidgetCategory::Other)
        .is_some_and(|names| names.iter().any(|n| n == "spacer_1")));
}

#[test]
fn test_generate_spacer_name_empty_layout() {
    // RED: With no spacers, should return spacer_1
    let layout = Layout {
        windows: vec![],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let name = AppCore::generate_spacer_name(&layout);
    assert_eq!(name, "spacer_1");
}

#[test]
fn test_generate_spacer_name_single_spacer() {
    // RED: With one spacer_1, should return spacer_2
    let spacer1 = WindowDef::Spacer {
        base: test_window_base("spacer_1"),
        data: SpacerWidgetData {},
    };
    let layout = Layout {
        windows: vec![spacer1],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let name = AppCore::generate_spacer_name(&layout);
    assert_eq!(name, "spacer_2");
}

#[test]
fn test_generate_spacer_name_multiple_spacers() {
    // RED: With spacer_1, spacer_2, spacer_3, should return spacer_4
    let spacer1 = WindowDef::Spacer {
        base: test_window_base("spacer_1"),
        data: SpacerWidgetData {},
    };
    let spacer2 = WindowDef::Spacer {
        base: test_window_base("spacer_2"),
        data: SpacerWidgetData {},
    };
    let spacer3 = WindowDef::Spacer {
        base: test_window_base("spacer_3"),
        data: SpacerWidgetData {},
    };
    let layout = Layout {
        windows: vec![spacer1, spacer2, spacer3],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let name = AppCore::generate_spacer_name(&layout);
    assert_eq!(name, "spacer_4");
}

#[test]
fn test_generate_spacer_name_with_gaps() {
    // RED: With spacer_1 and spacer_3 (gap at 2), should return spacer_4 (max + 1)
    let spacer1 = WindowDef::Spacer {
        base: test_window_base("spacer_1"),
        data: SpacerWidgetData {},
    };
    let spacer3 = WindowDef::Spacer {
        base: test_window_base("spacer_3"),
        data: SpacerWidgetData {},
    };
    let layout = Layout {
        windows: vec![spacer1, spacer3],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let name = AppCore::generate_spacer_name(&layout);
    assert_eq!(name, "spacer_4");
}

#[test]
fn test_format_category_label_standard() {
    assert_eq!(AppCore::format_category_label("cat_tools"), "Tools");
}

#[test]
fn test_format_category_label_single_char() {
    assert_eq!(AppCore::format_category_label("x"), "X");
}

#[test]
fn test_format_category_label_empty() {
    assert_eq!(AppCore::format_category_label(""), "Other");
}

#[test]
fn test_generate_spacer_name_ignores_non_spacers() {
    // RED: Non-spacer widgets should be ignored
    let text_widget = WindowDef::Text {
        base: test_window_base("main"),
        data: crate::config::TextWidgetData {
            streams: vec!["main".to_string()],
            buffer_size: 1000,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    };
    let spacer1 = WindowDef::Spacer {
        base: test_window_base("spacer_1"),
        data: SpacerWidgetData {},
    };
    let layout = Layout {
        windows: vec![text_widget, spacer1],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let name = AppCore::generate_spacer_name(&layout);
    assert_eq!(name, "spacer_2");
}

#[test]
fn test_generate_spacer_name_with_hidden_spacers() {
    // RED: Hidden spacers should be considered (widgets can be hidden, not deleted)
    let mut visible_base = test_window_base("spacer_1");
    visible_base.visibility = crate::config::WindowVisibility::Shown;

    let mut hidden_base = test_window_base("spacer_2");
    hidden_base.visibility = crate::config::WindowVisibility::Hidden;

    let visible_spacer = WindowDef::Spacer {
        base: visible_base,
        data: SpacerWidgetData {},
    };
    let hidden_spacer = WindowDef::Spacer {
        base: hidden_base,
        data: SpacerWidgetData {},
    };
    let layout = Layout {
        windows: vec![visible_spacer, hidden_spacer],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let name = AppCore::generate_spacer_name(&layout);
    assert_eq!(name, "spacer_3");
}

#[test]
fn test_generate_spacer_name_non_sequential() {
    // RED: With spacer_2, spacer_5 (max is 5), should return spacer_6
    let spacer2 = WindowDef::Spacer {
        base: test_window_base("spacer_2"),
        data: SpacerWidgetData {},
    };
    let spacer5 = WindowDef::Spacer {
        base: test_window_base("spacer_5"),
        data: SpacerWidgetData {},
    };
    let layout = Layout {
        windows: vec![spacer2, spacer5],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let name = AppCore::generate_spacer_name(&layout);
    assert_eq!(name, "spacer_6");
}

#[test]
fn test_generate_spacer_name_large_numbers() {
    // RED: Should handle large numbers correctly
    let spacer99 = WindowDef::Spacer {
        base: test_window_base("spacer_99"),
        data: SpacerWidgetData {},
    };
    let layout = Layout {
        windows: vec![spacer99],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let name = AppCore::generate_spacer_name(&layout);
    assert_eq!(name, "spacer_100");
}

// ========== calculate_window_positions characterization ==========
// This is the load/init positioning pass: it copies each window's EXACT
// col/row (no scaling — deliberately, so windows may sit offscreen) and
// clamps width/height to any min/max constraints. Pin that contract before
// a geometry newtype touches it.

fn positioned_text_def(name: &str, col: u16, row: u16, cols: u16, rows: u16) -> WindowDef {
    let mut base = test_window_base(name);
    base.col = crate::data::geometry::Col::new(col);
    base.row = crate::data::geometry::Row::new(row);
    base.cols = crate::data::geometry::Width::new(cols);
    base.rows = crate::data::geometry::Height::new(rows);
    WindowDef::Text {
        base,
        data: crate::config::TextWidgetData {
            streams: vec![],
            buffer_size: 1000,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    }
}

fn core_with_layout(windows: Vec<WindowDef>) -> AppCore {
    let mut core = AppCore::new_for_test();
    core.layout = Layout {
        windows,
        terminal_width: Some(80),
        terminal_height: Some(24),
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };
    core
}

#[test]
fn delete_and_stash_then_restore_roundtrips_the_exact_def() {
    let mut core = core_with_layout(vec![
        positioned_text_def("main", 0, 0, 40, 24),
        positioned_text_def("my_notes", 5, 5, 20, 8),
    ]);
    core.init_windows(80, 24);

    // Delete the custom window: gone from the layout, stashed.
    assert!(core.delete_and_stash_window("my_notes"));
    assert!(!core.layout.windows.iter().any(|w| w.name() == "my_notes"));
    assert!(!core.ui_state.windows.contains_key("my_notes"));
    assert_eq!(core.deleted_window_names(), vec!["my_notes".to_string()]);

    // Restore it: back in the layout with its exact geometry, live again.
    assert!(core.restore_deleted_window("my_notes", 80, 24));
    let def = core
        .layout
        .windows
        .iter()
        .find(|w| w.name() == "my_notes")
        .expect("restored def present");
    assert_eq!(def.base().col.get(), 5);
    assert_eq!(def.base().row.get(), 5);
    assert_eq!(def.base().cols.get(), 20);
    assert!(def.base().visibility.is_shown());
    assert!(core.ui_state.windows.contains_key("my_notes"));
    // Stash is now empty.
    assert!(core.deleted_window_names().is_empty());
}

#[test]
fn deleted_windows_for_restore_shows_title_not_internal_id() {
    // A custom window with an opaque id but a human title.
    let mut def = positioned_text_def("custom-text-1", 1, 1, 10, 5);
    def.base_mut().title = Some("Consumables".into());
    let mut core = core_with_layout(vec![def]);
    core.init_windows(80, 24);
    core.delete_and_stash_window("custom-text-1");

    let entries = core.deleted_windows_for_restore();
    assert_eq!(entries.len(), 1);
    let (name, title) = &entries[0];
    assert_eq!(name, "custom-text-1", "restore key is the stable id");
    assert_eq!(title, "Consumables", "menu shows the human title");

    // A titleless deleted window falls back to its name.
    core.delete_and_stash_window("custom-text-1"); // already stashed; re-add a bare one
    let mut bare = positioned_text_def("scratch-2", 0, 0, 5, 5);
    bare.base_mut().title = None;
    core.layout.windows.push(bare);
    core.init_windows(80, 24);
    core.delete_and_stash_window("scratch-2");
    let scratch = core
        .deleted_windows_for_restore()
        .into_iter()
        .find(|(n, _)| n == "scratch-2")
        .unwrap();
    assert_eq!(scratch.1, "scratch-2", "no title -> falls back to name");
}

#[test]
fn re_deleting_after_restore_keeps_one_stash_copy() {
    let mut core = core_with_layout(vec![positioned_text_def("notes", 1, 1, 10, 5)]);
    core.init_windows(80, 24);
    core.delete_and_stash_window("notes");
    core.restore_deleted_window("notes", 80, 24);
    core.delete_and_stash_window("notes");
    // Only one stashed copy, not two.
    assert_eq!(core.deleted_window_names(), vec!["notes".to_string()]);
}

#[test]
fn restore_refuses_when_name_is_reused_by_a_live_window() {
    let mut core = core_with_layout(vec![positioned_text_def("notes", 1, 1, 10, 5)]);
    core.init_windows(80, 24);
    core.delete_and_stash_window("notes");
    // A new window reuses the name.
    core.layout
        .windows
        .push(positioned_text_def("notes", 0, 0, 5, 5));
    core.init_windows(80, 24);
    // Restore is refused (won't clobber the live one); the stash keeps it.
    assert!(!core.restore_deleted_window("notes", 80, 24));
    assert_eq!(core.deleted_window_names(), vec!["notes".to_string()]);
}

#[test]
fn deleted_windows_persist_through_layout_serialization() {
    let mut core = core_with_layout(vec![positioned_text_def("gone", 2, 2, 12, 6)]);
    core.init_windows(80, 24);
    core.delete_and_stash_window("gone");
    // Serialize + reparse the layout: the stash survives.
    let toml = toml::to_string(&core.layout).expect("serialize layout");
    assert!(toml.contains("deleted_windows"), "stash must serialize");
    let reparsed: Layout = toml::from_str(&toml).expect("reparse layout");
    assert_eq!(reparsed.deleted_windows.len(), 1);
    assert_eq!(reparsed.deleted_windows[0].name(), "gone");
}

/// Positions and sizes pass through exactly (no scaling), even when the
/// window extends beyond the given terminal size.
#[test]
fn calculate_window_positions_uses_exact_values() {
    let core = core_with_layout(vec![
        positioned_text_def("a", 3, 5, 40, 10),
        positioned_text_def("offscreen", 100, 50, 20, 8), // beyond 80x24
    ]);
    let positions = core.calculate_window_positions(80, 24);

    let a = &positions["a"];
    assert_eq!(
        (a.x.get(), a.y.get(), a.width.get(), a.height.get()),
        (3, 5, 40, 10)
    );
    // Deliberately NOT clamped to the terminal — offscreen is allowed.
    let off = &positions["offscreen"];
    assert_eq!(
        (off.x.get(), off.y.get(), off.width.get(), off.height.get()),
        (100, 50, 20, 8)
    );
}

/// min/max constraints clamp the size (never the position).
#[test]
fn calculate_window_positions_applies_min_max_constraints() {
    let mut narrow = positioned_text_def("narrow", 0, 0, 4, 30);
    narrow.base_mut().min_cols = Some(10); // widen up to min
    narrow.base_mut().max_rows = Some(20); // cap height
    let core = core_with_layout(vec![narrow]);

    let p = &core.calculate_window_positions(80, 24)["narrow"];
    assert_eq!(p.x.get(), 0); // position untouched
    assert_eq!(p.y.get(), 0);
    assert_eq!(p.width.get(), 10); // 4 raised to min_cols
    assert_eq!(p.height.get(), 20); // 30 capped at max_rows
}

#[test]
fn known_windows_menu_reflects_state_and_toggle_flips_it() {
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};
    let mut core = core_with_layout(vec![]);
    core.layout.terminal_width = Some(80);
    core.layout.terminal_height = Some(24);

    // Discover a stream → bound, Hidden layout entry named "thoughts".
    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
    core.realize_offered_windows(80, 24);

    // Fresh discovery: hidden → "[ ]" and a __TOGGLE_WINDOW__ command.
    let menu = core.build_known_windows_menu();
    let row = menu
        .iter()
        .find(|i| i.command == "__TOGGLE_WINDOW__thoughts")
        .unwrap();
    assert!(row.text.starts_with("[ ]"), "row: {}", row.text);
    assert!(row.text.contains("Thoughts"));

    // Toggle shows it (creates UI state).
    core.toggle_known_window("thoughts");
    assert!(core.ui_state.windows.contains_key("thoughts"));
    let menu = core.build_known_windows_menu();
    let row = menu
        .iter()
        .find(|i| i.command == "__TOGGLE_WINDOW__thoughts")
        .unwrap();
    assert!(row.text.starts_with("[x]"), "row: {}", row.text);

    // Toggle again hides it.
    core.toggle_known_window("thoughts");
    assert!(!core.ui_state.windows.contains_key("thoughts"));
}

fn renamed_widget(display_name: &str, template_name: &str) -> WindowDef {
    // A widget the user placed via the Windows list: built from a
    // template (so category/id fields are set) but the editor renamed
    // it to a custom-* display name, losing the template name.
    let mut def = crate::core::local_catalog::seed(template_name)
        .unwrap_or_else(|| panic!("no template '{}'", template_name));
    def.base_mut().name = display_name.to_string();
    def
}

#[test]
fn dialog_readd_does_not_duplicate_a_renamed_singleton_widget() {
    // The bug: game re-sends the expr dialog; the user's placed widget
    // is "custom-gs4_experience-1", so the old exact-name check missed
    // it and spawned a duplicate on every re-send. U2: the pending
    // queue carries the DIALOG ID ("expr"); the equivalent renamed
    // widget gets ADOPTED (binding tagged) so re-sends resolve by id.
    let mut core = core_with_layout(vec![renamed_widget(
        "custom-gs4_experience-1",
        "gs4_experience",
    )]);
    assert_eq!(core.layout.windows.len(), 1);

    // Simulate several dialog re-sends (expr -> gs4_experience template).
    for _ in 0..3 {
        core.ui_state
            .pending_window_additions
            .push("expr".to_string());
        core.process_pending_window_additions(80, 24);
    }

    // Still exactly one gs4_experience window — no duplicate spawned...
    let count = core
        .layout
        .windows
        .iter()
        .filter(|w| w.widget_type() == "gs4_experience")
        .count();
    assert_eq!(count, 1, "duplicate gs4_experience window spawned");
    // ...and it was adopted: now bound to "expr".
    assert!(
        core.layout.has_window_bound_to("expr"),
        "the renamed widget should have been adopted and bound to expr"
    );
}

#[test]
fn first_sight_creates_a_bound_window() {
    // No existing widget: the first expr feed creates a gs4_experience
    // window bound to "expr", and a re-send doesn't duplicate it.
    let mut core = core_with_layout(vec![]);
    core.ui_state
        .pending_window_additions
        .push("expr".to_string());
    core.process_pending_window_additions(80, 24);

    assert!(core.layout.has_window_bound_to("expr"));
    assert_eq!(
        core.layout
            .windows
            .iter()
            .filter(|w| w.widget_type() == "gs4_experience")
            .count(),
        1
    );

    // Re-send: still one.
    core.ui_state
        .pending_window_additions
        .push("expr".to_string());
    core.process_pending_window_additions(80, 24);
    assert_eq!(
        core.layout
            .windows
            .iter()
            .filter(|w| w.widget_type() == "gs4_experience")
            .count(),
        1
    );
}

#[test]
fn one_feed_delivers_to_multiple_bound_windows() {
    // Nisugi's rule: 3 windows bound to "expr" all count as "exists"
    // (no new spawn) and windows_bound_to lists all of them for delivery.
    let mut core = core_with_layout(vec![]);
    for i in 0..3 {
        let mut def = crate::core::local_catalog::seed("gs4_experience").unwrap();
        def.base_mut().name = format!("xp{}", i);
        def.base_mut().binding = Some(crate::config::WindowBinding::Dialog("expr".to_string()));
        core.layout.windows.push(def);
    }
    // A feed for expr must NOT spawn a 4th window.
    core.ui_state
        .pending_window_additions
        .push("expr".to_string());
    core.process_pending_window_additions(80, 24);
    assert_eq!(core.layout.windows.len(), 3, "should not create a 4th");
    // All three are addressable for delivery.
    assert_eq!(core.layout.windows_bound_to("expr").len(), 3);
}

#[test]
fn set_known_window_shown_flips_layout_visibility() {
    use crate::config::WindowVisibility;
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};
    let mut core = core_with_layout(vec![]);
    core.layout.terminal_width = Some(80);
    core.layout.terminal_height = Some(24);

    // Discover a stream (bound, Hidden layout entry).
    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
    core.realize_offered_windows(80, 24);
    let vis = |c: &AppCore| {
        c.layout
            .windows
            .iter()
            .find(|w| w.name() == "thoughts")
            .unwrap()
            .base()
            .visibility
    };
    assert_eq!(vis(&core), WindowVisibility::Hidden);

    // Show it by name → visibility flips to Shown + UI state created.
    core.set_known_window_shown("thoughts", true, 80, 24);
    assert_eq!(vis(&core), WindowVisibility::Shown);
    assert!(core.ui_state.windows.contains_key("thoughts"));

    // Hide it → back to Hidden, removed from UI state.
    core.set_known_window_shown("thoughts", false, 80, 24);
    assert_eq!(vis(&core), WindowVisibility::Hidden);
    assert!(!core.ui_state.windows.contains_key("thoughts"));
}

#[test]
fn showing_a_dialog_window_syncs_shown_dialog_ids() {
    // U6: showing/hiding a dialog-bound window flips its id in
    // shown_dialog_ids, which the message processor's popup gate reads.
    use crate::config::WindowBinding;
    let mut core = core_with_layout(vec![]);
    core.layout.terminal_width = Some(80);
    core.layout.terminal_height = Some(24);
    let mut bank = crate::core::local_catalog::seed("stance").unwrap();
    bank.base_mut().name = "bank".to_string();
    bank.base_mut().binding = Some(WindowBinding::Dialog("bank".to_string()));
    bank.base_mut().visibility = crate::config::WindowVisibility::Hidden;
    core.layout.windows.push(bank);

    assert!(!core.ui_state.shown_dialog_ids.contains("bank"));
    core.set_known_window_shown("bank", true, 80, 24);
    assert!(core.ui_state.shown_dialog_ids.contains("bank"));
    core.set_known_window_shown("bank", false, 80, 24);
    assert!(!core.ui_state.shown_dialog_ids.contains("bank"));
}

#[test]
fn showing_a_dialog_panel_does_not_pop_it_up_as_a_dialog() {
    // UberBar bug: a DialogPanel-bound dialog renders IN THE PANEL. Showing
    // it must NOT add its id to shown_dialog_ids, or every dialogData frame
    // would ALSO fire an active_dialog popup — a duplicate window (empty
    // panel + populated popup). Only true popup dialogs (bank) join the set.
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};
    let mut core = core_with_layout(vec![]);
    core.layout.terminal_width = Some(80);
    core.layout.terminal_height = Some(24);

    // Register UberBar the way the game does: a DialogPanel discovery.
    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "UberBar".to_string(),
            title: "Nisugi's Uberbar".to_string(),
            kind: WindowDiscoveryKind::DialogPanel,
            save: false,
        });
    core.realize_offered_windows(80, 24);
    assert!(
        core.layout
            .windows
            .iter()
            .any(|w| matches!(w, crate::config::WindowDef::DialogPanel { .. })),
        "the discovery should have created a DialogPanel window"
    );

    core.set_known_window_shown("UberBar", true, 80, 24);
    assert!(
        !core.ui_state.shown_dialog_ids.contains("UberBar"),
        "a DialogPanel must not join the popup allow-set (that causes the duplicate window)"
    );

    // The runtime window must carry DialogPanel content bound to the id —
    // NOT WindowContent::Empty (the blank-panel bug: add_new_window had no
    // DialogPanel arm, so the shown panel rendered nothing).
    let win = core
        .ui_state
        .windows
        .get("UberBar")
        .expect("shown UberBar has a runtime window");
    match &win.content {
        crate::data::WindowContent::DialogPanel { dialog_id } => {
            assert_eq!(dialog_id, "UberBar", "panel content bound to the dialog id");
        }
        other => panic!("expected DialogPanel content, got {:?}", other),
    }
}

#[test]
fn deleting_a_shown_dialog_window_clears_the_popup_allow_set() {
    // Rysk's bug: show a dialog-bound window (seeds shown_dialog_ids),
    // then DELETE it. Delete must scrub the id from the popup allow-set
    // and drop any live popup — otherwise the next dialogData the game
    // sends re-pops the deleted dialog as a bare "Dialog" popup.
    use crate::config::WindowBinding;
    let mut core = core_with_layout(vec![]);
    core.layout.terminal_width = Some(80);
    core.layout.terminal_height = Some(24);
    let mut win = crate::core::local_catalog::seed("stance").unwrap();
    win.base_mut().name = "activespells".to_string();
    win.base_mut().binding = Some(WindowBinding::Dialog("activespells".to_string()));
    win.base_mut().visibility = crate::config::WindowVisibility::Hidden;
    core.layout.windows.push(win);

    core.set_known_window_shown("activespells", true, 80, 24);
    assert!(core.ui_state.shown_dialog_ids.contains("activespells"));
    // Simulate a live popup for this id.
    core.ui_state.active_dialog = Some(crate::data::DialogState::empty(
        "activespells".to_string(),
        Some("Dialog".to_string()),
    ));

    assert!(core.delete_and_stash_window("activespells"));
    assert!(
        !core.ui_state.shown_dialog_ids.contains("activespells"),
        "delete must remove the dialog id from the popup allow-set"
    );
    assert!(
        core.ui_state.active_dialog.is_none(),
        "delete must close a popup that was showing the deleted dialog"
    );
}

#[test]
fn rediscovery_of_a_persisted_window_is_idempotent() {
    // U4: after a persisted discovered window reloads (simulated: a
    // bound Hidden layout entry already present), the game re-announcing
    // it must NOT create a duplicate, and must NOT force it visible.
    use crate::config::{WindowBinding, WindowVisibility};
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};
    let mut core = core_with_layout(vec![]);
    // Simulate a reloaded layout: combat already bound + Hidden.
    let mut combat = crate::core::local_catalog::seed("stance").unwrap();
    combat.base_mut().name = "combat".to_string();
    combat.base_mut().binding = Some(WindowBinding::Dialog("combat".to_string()));
    combat.base_mut().visibility = WindowVisibility::Hidden;
    core.layout.windows.push(combat);
    assert_eq!(core.layout.windows.len(), 1);

    // The game re-announces combat this session.
    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "combat".to_string(),
            title: "Combat".to_string(),
            kind: WindowDiscoveryKind::DialogPanel,
            save: false,
        });
    core.realize_offered_windows(80, 24);

    // No duplicate; still Hidden.
    assert_eq!(core.layout.windows_bound_to("combat").len(), 1);
    assert_eq!(
        core.layout
            .windows
            .iter()
            .find(|w| w.name() == "combat")
            .unwrap()
            .base()
            .visibility,
        WindowVisibility::Hidden
    );
}

#[test]
fn stream_discovery_adopts_existing_subscriber_no_duplicate() {
    use crate::config::{WindowBinding, WindowDef};
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};

    // A single-stream text window already subscribes to "thoughts"
    // (like the default layout's thoughts window, unbound).
    let mut thoughts = crate::core::local_catalog::seed("text_custom").unwrap();
    thoughts.base_mut().name = "Thoughts".to_string();
    if let WindowDef::Text { data, .. } = &mut thoughts {
        data.streams.push("thoughts".to_string());
    }
    let mut core = core_with_layout(vec![thoughts]);

    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
    core.realize_offered_windows(80, 24);

    // No duplicate — the existing window was adopted (bound), not cloned.
    assert_eq!(core.layout.windows.len(), 1, "no duplicate thoughts window");
    assert_eq!(
        core.layout.windows[0].base().binding,
        Some(WindowBinding::Stream("thoughts".to_string()))
    );
}

#[test]
fn stream_discovery_skips_when_a_tab_already_routes_it() {
    use crate::config::WindowDef;
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};

    // A tabbedtext window has a tab subscribing to "thoughts".
    let mut tabbed = crate::core::local_catalog::seed("tabbedtext_custom").unwrap();
    tabbed.base_mut().name = "chat".to_string();
    if let WindowDef::TabbedText { data, .. } = &mut tabbed {
        data.tabs.push(crate::config::TabbedTextTab {
            name: "Thoughts".to_string(),
            stream: Some("thoughts".to_string()),
            streams: vec!["thoughts".to_string()],
            ..Default::default()
        });
    }
    let mut core = core_with_layout(vec![tabbed]);

    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
    core.realize_offered_windows(80, 24);

    // No new window: the tab already routes it (whole tabbed window not
    // bound, since it carries many streams).
    assert_eq!(
        core.layout.windows.len(),
        1,
        "no duplicate for tab-routed stream"
    );
    assert!(core.layout.windows[0].base().binding.is_none());
}

#[test]
fn window_discoveries_register_as_bound_hidden_layout_entries() {
    use crate::config::{WindowBinding, WindowVisibility};
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};
    let mut core = core_with_layout(vec![]);

    // A stream and a resident dialog panel are discovered.
    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "combat".to_string(),
            title: "Combat".to_string(),
            kind: WindowDiscoveryKind::DialogPanel,
            save: false,
        });
    core.realize_offered_windows(80, 24);

    // Both became bound, Hidden layout entries (known forever, not shown).
    assert!(core.layout.has_window_bound_to("thoughts"));
    assert!(core.layout.has_window_bound_to("combat"));
    for id in ["thoughts", "combat"] {
        let w = core
            .layout
            .windows
            .iter()
            .find(|w| w.base().binding.as_ref().is_some_and(|b| b.id() == id))
            .unwrap();
        assert_eq!(w.base().visibility, WindowVisibility::Hidden, "{id} hidden");
    }
    // The stream window subscribes to its stream id.
    let stream_win = core
        .layout
        .windows
        .iter()
        .find(|w| w.base().binding == Some(WindowBinding::Stream("thoughts".to_string())))
        .unwrap();
    if let crate::config::WindowDef::Text { data, .. } = stream_win {
        assert!(data.streams.contains(&"thoughts".to_string()));
    } else {
        panic!("stream discovery should be a text window");
    }

    // Idempotent: re-discovering doesn't add duplicates.
    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "thoughts".to_string(),
            title: "Thoughts".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
    core.realize_offered_windows(80, 24);
    assert_eq!(core.layout.windows_bound_to("thoughts").len(), 1);
}

#[test]
fn spells_stream_discovery_creates_a_spells_widget_not_text() {
    use crate::config::WindowBinding;
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};
    let mut core = core_with_layout(vec![]);

    // The game declares its spellbook window via <streamWindow id="Spells">.
    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "Spells".to_string(),
            title: "Spells".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
    core.realize_offered_windows(80, 24);

    // It must be the dedicated spells widget (whose buffer-replay pipeline
    // populates it), NOT a generic text window that would render empty.
    let win = core
        .layout
        .windows
        .iter()
        .find(|w| w.base().binding == Some(WindowBinding::Stream("Spells".to_string())))
        .expect("Spells stream should register a bound window");
    assert!(
        matches!(win, crate::config::WindowDef::Spells { .. }),
        "Spells stream discovery must produce a spells widget, got {:?}",
        win.widget_type()
    );
}

#[test]
fn widget_backed_streams_discover_their_widget_not_text() {
    use crate::config::{WindowBinding, WindowDef};
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};

    // Each of these stream ids has a dedicated widget; auto-discovery must
    // produce that widget, not a generic (empty) text window.
    let cases: &[(&str, fn(&WindowDef) -> bool)] = &[
        ("Spells", |w| matches!(w, WindowDef::Spells { .. })),
        ("inv", |w| matches!(w, WindowDef::Inventory { .. })),
        ("reserve", |w| matches!(w, WindowDef::Reserve { .. })),
        ("room", |w| matches!(w, WindowDef::Room { .. })),
    ];

    for (id, is_expected) in cases {
        let mut core = core_with_layout(vec![]);
        core.ui_state
            .pending_window_discoveries
            .push(WindowDiscovery {
                id: id.to_string(),
                title: id.to_string(),
                kind: WindowDiscoveryKind::Stream,
                save: false,
            });
        core.realize_offered_windows(80, 24);
        let win = core
            .layout
            .windows
            .iter()
            .find(|w| w.base().binding == Some(WindowBinding::Stream(id.to_string())))
            .unwrap_or_else(|| panic!("stream '{id}' should register a bound window"));
        assert!(
            is_expected(win),
            "stream '{id}' must discover its widget, got {:?}",
            win.widget_type()
        );
    }
}

#[test]
fn plain_text_streams_still_discover_a_text_window() {
    use crate::config::{WindowBinding, WindowDef};
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};
    // A stream with no dedicated widget stays a text window.
    let mut core = core_with_layout(vec![]);
    core.ui_state
        .pending_window_discoveries
        .push(WindowDiscovery {
            id: "custom_feed".to_string(),
            title: "Custom".to_string(),
            kind: WindowDiscoveryKind::Stream,
            save: false,
        });
    core.realize_offered_windows(80, 24);
    let win = core
        .layout
        .windows
        .iter()
        .find(|w| w.base().binding == Some(WindowBinding::Stream("custom_feed".to_string())))
        .expect("stream should register a window");
    assert!(matches!(win, WindowDef::Text { .. }));
}

#[test]
fn enumerate_known_windows_covers_layout_and_ephemeral() {
    use crate::core::known_windows::KnownWindowKind;
    // A bound (discovered) hidden dialog window, an unbound plain
    // widget, and the un-hideable essentials.
    let mut core = core_with_layout(vec![]);
    let mut combat = crate::core::local_catalog::seed("stance").unwrap();
    combat.base_mut().name = "combat".to_string();
    combat.base_mut().title = Some("Combat".to_string());
    combat.base_mut().binding = Some(crate::config::WindowBinding::Dialog("combat".to_string()));
    combat.base_mut().visibility = crate::config::WindowVisibility::Hidden;
    core.layout.windows.push(combat);
    core.layout
        .windows
        .push(positioned_text_def("main", 0, 0, 40, 10)); // essential
    core.layout
        .windows
        .push(positioned_text_def("my_notes", 0, 0, 20, 5)); // plain

    let known = core.enumerate_known_windows();
    // "main" is listed like any other window (hideable under the
    // main-stream invariant — see hide_window).
    let main = known
        .iter()
        .find(|k| k.name == "main")
        .expect("main listed");
    assert!(main.shown);
    // The bound combat window is classified as a Dialog, hidden.
    let combat = known
        .iter()
        .find(|k| k.name == "combat")
        .expect("combat listed");
    assert_eq!(combat.kind, KnownWindowKind::Dialog);
    assert!(!combat.shown);
    assert_eq!(combat.title, "Combat");
    // The unbound widget is a plain Layout window.
    let notes = known
        .iter()
        .find(|k| k.name == "my_notes")
        .expect("notes listed");
    assert_eq!(notes.kind, KnownWindowKind::Layout);
    assert!(notes.shown);
}

/// Full-catalog rows: every template is listed even before it exists
/// in the layout; seed templates and spacers stay out; a layout entry
/// wins over its template row (no duplicates, live state preserved).
#[test]
fn enumerate_known_windows_lists_full_template_catalog() {
    let core = core_with_layout(vec![positioned_text_def("thoughts", 0, 0, 10, 5)]);
    let known = core.enumerate_known_windows();

    // Never-added template → unchecked row.
    let compass = known
        .iter()
        .find(|k| k.name == "compass")
        .expect("compass listed");
    assert!(!compass.shown);
    assert!(!compass.ephemeral);
    // main appears as a template row even though the layout lacks it.
    assert!(known.iter().any(|k| k.name == "main"));
    // Creation seeds are flows, not windows.
    assert!(!known.iter().any(|k| k.name.ends_with("_custom")));
    assert!(!known.iter().any(|k| k.name == "spacer"));
    // Layout entry dedups its template row and keeps live state.
    let thoughts: Vec<_> = known.iter().filter(|k| k.name == "thoughts").collect();
    assert_eq!(thoughts.len(), 1);
    assert!(thoughts[0].shown);
}

/// Ticking a catalog row whose template isn't in the layout yet
/// conjures it: added to the layout shown + materialized in ui_state.
#[test]
fn set_known_window_shown_conjures_template_not_in_layout() {
    let mut core = core_with_layout(vec![]);
    assert!(core.layout.get_window("compass").is_none());
    core.set_known_window_shown("compass", true, 80, 24);
    assert!(core
        .layout
        .get_window("compass")
        .map(|w| w.base().visibility.is_shown())
        .unwrap_or(false));
    assert!(core.ui_state.windows.contains_key("compass"));
}

/// Regression: deleting a widget-backed window whose id the game ALSO
/// feeds as a resident dialog (minivitals, expr, encum, Buffs, ...) and
/// then re-showing it must restore its real WIDGET, not conjure a generic
/// `panel_<id>` dialog panel.
///
/// Repro (Rysk/Crinbar): minivitals owns the `minivitals` MiniVitals
/// widget template, but the game streams `<dialogData id='minivitals'>`
/// every vitals tick, which always accumulates into `dialog_store`. After
/// deleting the window, `dialog_store.contains_key("minivitals")` was true,
/// so `set_known_window_shown` built `panel_minivitals` instead of the
/// widget. `set_known_window_shown` now checks the real widget template
/// FIRST, ahead of the dialog-store and container conjure branches, so a
/// widget-backed id can never be resurrected as a generic panel.
#[test]
fn reshowing_deleted_widget_backed_dialog_restores_widget_not_panel() {
    let minivitals_def =
        crate::core::local_catalog::seed("minivitals").expect("minivitals template exists");
    let mut core = core_with_layout(vec![minivitals_def]);
    core.init_windows(80, 24);
    assert!(core.ui_state.windows.contains_key("minivitals"));

    // 1. Delete the widget window (stashed for restore).
    assert!(core.delete_and_stash_window("minivitals"));
    assert!(!core.ui_state.windows.contains_key("minivitals"));

    // 2. Game keeps streaming resident minivitals dialogData → dialog_store
    //    fills even though no window is bound to it.
    core.inject_test_line(
            "<dialogData id='minivitals'><progressBar id='mana' value='94' text='mana 386/407' left='76.7%' top='0%' width='23.3%' height='100%'/></dialogData>",
        );
    assert!(
        core.ui_state.dialog_store.contains_key("minivitals"),
        "resident dialogData should accumulate in the store"
    );

    // 3. Re-show minivitals from the Windows list.
    core.set_known_window_shown("minivitals", true, 80, 24);

    // The REAL widget is restored; no generic panel is conjured.
    assert!(
        core.ui_state.windows.contains_key("minivitals"),
        "minivitals widget must be restored"
    );
    assert!(
        !core.ui_state.windows.contains_key("panel_minivitals"),
        "no generic panel_minivitals may be created"
    );
    assert!(
        core.layout.windows.iter().any(|w| w.name() == "minivitals"
            && matches!(w, crate::config::WindowDef::MiniVitals { .. })),
        "restored window is the MiniVitals widget, not a DialogPanel"
    );
}

/// Bug #1: a named GUI layout saved on one character carries the full
/// window defs; loading it into a profile that only has the default
/// windows must recreate the missing ones (in both the layout def list
/// and ui_state) while leaving existing windows untouched.
#[test]
fn materialize_missing_windows_creates_only_the_absent() {
    let mut core = core_with_layout(vec![positioned_text_def("story", 0, 0, 40, 10)]);
    core.init_windows(80, 24);
    assert!(core.ui_state.windows.contains_key("story"));

    let saved_defs = vec![
        positioned_text_def("story", 0, 0, 40, 10), // already present
        positioned_text_def("room", 40, 0, 20, 8),  // missing
        positioned_text_def("map", 60, 0, 20, 8),   // missing
    ];
    let created = core.materialize_missing_windows(&saved_defs, 80, 24);

    // Only the two absent windows are created, in order.
    assert_eq!(created, vec!["room".to_string(), "map".to_string()]);
    // Both live in ui_state AND the authoritative layout def list.
    for name in ["room", "map"] {
        assert!(
            core.ui_state.windows.contains_key(name),
            "{name} in ui_state"
        );
        assert!(
            core.layout.windows.iter().any(|w| w.name() == name),
            "{name} in layout defs"
        );
    }
    // The pre-existing window is not duplicated.
    assert_eq!(
        core.layout
            .windows
            .iter()
            .filter(|w| w.name() == "story")
            .count(),
        1
    );
}

/// A text window subscribed to the main stream.
fn main_text_def(name: &str) -> WindowDef {
    let mut def = positioned_text_def(name, 0, 0, 40, 10);
    if let WindowDef::Text { data, .. } = &mut def {
        data.streams = vec!["main".to_string()];
    }
    def
}

/// The story feed must always have a shown subscriber: hiding the
/// last main-stream window is refused; with a second subscriber the
/// window named "main" hides like any other.
#[test]
fn hide_window_gates_on_main_stream_invariant() {
    let mut core = core_with_layout(vec![main_text_def("main")]);
    core.init_windows(80, 24);

    // Sole subscriber → refused, still shown.
    core.hide_window("main");
    assert!(core.ui_state.windows.contains_key("main"));
    assert!(core
        .layout
        .get_window("main")
        .unwrap()
        .base()
        .visibility
        .is_shown());

    // A second subscriber makes main hideable.
    let second = main_text_def("story_tab");
    core.layout.windows.push(second.clone());
    core.add_new_window(&second, 80, 24);
    core.hide_window("main");
    assert!(!core.ui_state.windows.contains_key("main"));
    assert!(!core
        .layout
        .get_window("main")
        .unwrap()
        .base()
        .visibility
        .is_shown());

    // Now story_tab is the last subscriber → refused in turn.
    core.hide_window("story_tab");
    assert!(core.ui_state.windows.contains_key("story_tab"));
}

/// `.deletewindow` truly deletes in BOTH frontends (it used to redirect to
/// hide in the TUI while the GUI removed for real). The window leaves the
/// layout and lands in the restore stash.
#[test]
fn delete_window_removes_and_stashes() {
    let mut core = core_with_layout(vec![main_text_def("main"), main_text_def("combat")]);
    core.init_windows(80, 24);

    core.delete_window("combat");

    // Gone from the live UI and from the layout — not merely hidden.
    assert!(!core.ui_state.windows.contains_key("combat"));
    assert!(core.layout.get_window("combat").is_none());
    // Recoverable: the def is stashed for restore.
    assert!(core
        .deleted_window_names()
        .iter()
        .any(|name| name == "combat"));
}

/// Deleting the last story-feed window is refused for the same reason
/// hiding it is — and more so, since a delete has no visibility checkbox
/// to undo it.
#[test]
fn delete_window_gates_on_main_stream_invariant() {
    let mut core = core_with_layout(vec![main_text_def("main")]);
    core.init_windows(80, 24);

    core.delete_window("main");
    assert!(core.ui_state.windows.contains_key("main"));
    assert!(core.layout.get_window("main").is_some());
    assert!(core.deleted_window_names().is_empty());

    // With a second subscriber the delete goes through.
    let second = main_text_def("story_tab");
    core.layout.windows.push(second.clone());
    core.add_new_window(&second, 80, 24);
    core.delete_window("main");
    assert!(core.layout.get_window("main").is_none());
}

/// TUI force-show: a hidden command_input still materializes at init,
/// and hiding it persists the layout flag without dropping the UI
/// window. Without the flag (GUI), it hides like any other window.
#[test]
fn command_input_hidden_flag_vs_tui_force_show() {
    let cmd = {
        let mut base = test_window_base("command_input");
        base.visibility = crate::config::WindowVisibility::Hidden;
        WindowDef::CommandInput {
            base,
            data: crate::config::CommandInputWidgetData::default(),
        }
    };
    // GUI mode (no force): hidden stays out of ui_state.
    let mut core = core_with_layout(vec![cmd.clone()]);
    core.init_windows(80, 24);
    assert!(!core.ui_state.windows.contains_key("command_input"));

    // TUI mode: force-show materializes it despite the hidden flag.
    let mut core = core_with_layout(vec![cmd]);
    core.force_show_command_input = true;
    core.init_windows(80, 24);
    assert!(core.ui_state.windows.contains_key("command_input"));

    // Hiding under force-show flips the layout flag but keeps the UI.
    core.layout.windows[0].base_mut().visibility = crate::config::WindowVisibility::Shown;
    core.hide_window("command_input");
    assert!(!core
        .layout
        .get_window("command_input")
        .unwrap()
        .base()
        .visibility
        .is_shown());
    assert!(core.ui_state.windows.contains_key("command_input"));
}

#[test]
fn dialog_readd_disambiguates_active_effects_by_category() {
    // Buffs and Debuffs share the ActiveEffects widget type. Having a
    // Buffs window must NOT suppress auto-adding Debuffs.
    let buffs = renamed_widget("custom-buffs", "buffs");
    let mut core = core_with_layout(vec![buffs]);

    // Buffs re-send: recognized, no add.
    core.ui_state
        .pending_window_additions
        .push("buffs".to_string());
    core.process_pending_window_additions(80, 24);
    assert_eq!(
        core.layout
            .windows
            .iter()
            .filter(|w| w.widget_type() == "active_effects")
            .count(),
        1
    );

    // Debuffs first sight: NOT shadowed by Buffs → added.
    core.ui_state
        .pending_window_additions
        .push("debuffs".to_string());
    core.process_pending_window_additions(80, 24);
    assert_eq!(
        core.layout
            .windows
            .iter()
            .filter(|w| w.widget_type() == "active_effects")
            .count(),
        2,
        "debuffs was wrongly suppressed by the buffs window"
    );
}

#[test]
fn container_show_hide_and_sighting_via_session_set() {
    // U3: containers are ephemeral session windows. A sighted container
    // auto-(re)opens only if the user opted it in (shown_container_titles);
    // showing/hiding by name adds/removes it. Multi-word titles work.
    let mut core = AppCore::new_for_test();
    core.layout.terminal_width = Some(80);
    core.layout.terminal_height = Some(24);
    // The registry knows the container (so it's listable), title has a space.
    core.game_state.objects.register_container(
        "268435466".to_string(),
        "My Pack".to_string(),
        Some("#268435466".to_string()),
    );

    // Sighted while not opted in → no window.
    core.message_processor.newly_registered_container =
        Some(("268435466".to_string(), "My Pack".to_string()));
    core.realize_offered_windows(80, 24);
    assert!(!core.ui_state.windows.contains_key("my_pack"));

    // Show it by (window) name → opted in + window created.
    core.set_known_window_shown("my_pack", true, 80, 24);
    assert!(core.ui_state.windows.contains_key("my_pack"));
    assert!(core.ui_state.shown_container_titles.contains("My Pack"));

    // Hide it → window closes, opt-in cleared (multi-word title works).
    core.set_known_window_shown("my_pack", false, 80, 24);
    assert!(!core.ui_state.windows.contains_key("my_pack"));
    assert!(!core.ui_state.shown_container_titles.contains("My Pack"));

    // Opt in, then a re-sight re-opens it automatically.
    core.ui_state
        .shown_container_titles
        .insert("My Pack".to_string());
    core.message_processor.newly_registered_container =
        Some(("268435466".to_string(), "My Pack".to_string()));
    core.realize_offered_windows(80, 24);
    assert!(core.ui_state.windows.contains_key("my_pack"));
}

#[test]
fn discovery_burst_on_backfilled_layout_creates_zero_windows() {
    // Redesign Phase 2 gate: after the load-time binding backfill, a
    // login burst re-declaring every feed the layout already hosts
    // must create NOTHING — binding identity short-circuits before
    // adoption even runs.
    use crate::data::{WindowDiscovery, WindowDiscoveryKind};
    let mut windows: Vec<WindowDef> = ["thoughts", "inventory", "buffs", "injuries"]
        .iter()
        .map(|name| crate::core::local_catalog::seed(name).expect(name))
        .collect();
    let mut layout = crate::config::Layout {
        windows: std::mem::take(&mut windows),
        terminal_width: Some(80),
        terminal_height: Some(24),
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };
    assert!(crate::config::Layout::backfill_bindings(&mut layout) > 0);
    let mut core = core_with_layout(std::mem::take(&mut layout.windows));

    let before = core.layout.windows.len();
    let bindings_before: Vec<_> = core
        .layout
        .windows
        .iter()
        .map(|w| w.base().binding.clone())
        .collect();
    for (id, kind) in [
        ("thoughts", WindowDiscoveryKind::Stream),
        ("inv", WindowDiscoveryKind::Stream),
        ("Buffs", WindowDiscoveryKind::DialogPanel),
        ("injuries", WindowDiscoveryKind::DialogPanel),
    ] {
        core.ui_state
            .pending_window_discoveries
            .push(WindowDiscovery {
                id: id.to_string(),
                title: id.to_string(),
                kind,
                save: false,
            });
    }
    core.realize_offered_windows(80, 24);

    assert_eq!(core.layout.windows.len(), before, "zero windows created");
    let bindings_after: Vec<_> = core
        .layout
        .windows
        .iter()
        .map(|w| w.base().binding.clone())
        .collect();
    assert_eq!(bindings_after, bindings_before, "bindings untouched");
}

#[test]
fn registry_bindings_join_known_windows_and_conjure_bound_windows() {
    // Redesign Phase 3: discovery memory joins the Windows-list union
    // — a feed seen in a PAST session is re-addable in a fresh layout
    // before the game re-declares it.
    use crate::core::known_windows::KnownWindowKind;
    let mut core = core_with_layout(vec![]);
    core.window_registry.record("stream", "voln", "Voln");
    core.window_registry.record("dialog", "combat", "Combat");
    // Dedicated-view ids stay owned by their template rows (with the
    // template pass's game gating) — no duplicate registry row.
    core.window_registry.record("stream", "inv", "Inventory");

    let known = core.enumerate_known_windows();
    let row = |name: &str| known.iter().find(|k| k.name == name);
    let voln = row("voln").expect("registry stream row");
    assert_eq!(voln.kind, KnownWindowKind::Stream);
    assert!(!voln.shown);
    let combat = row("combat").expect("registry dialog row");
    assert_eq!(combat.kind, KnownWindowKind::Dialog);
    assert!(
        row("inv").is_none(),
        "dedicated view owned by the template row"
    );

    // Ticking the rows conjures bound windows exactly as a live
    // discovery would, and shows them.
    core.set_known_window_shown("voln", true, 80, 24);
    let win = core
        .layout
        .windows
        .iter()
        .find(|w| w.name() == "voln")
        .expect("conjured layout window");
    assert_eq!(
        win.base().binding,
        Some(crate::config::WindowBinding::Stream("voln".into()))
    );
    assert!(win.base().visibility.is_shown());
    assert_eq!(win.widget_type(), "text");

    core.set_known_window_shown("combat", true, 80, 24);
    let win = core
        .layout
        .windows
        .iter()
        .find(|w| w.name() == "combat")
        .expect("conjured dialog panel");
    assert_eq!(
        win.base().binding,
        Some(crate::config::WindowBinding::Dialog("combat".into()))
    );
    assert_eq!(win.widget_type(), "dialogpanel");

    // Re-enumerating lists them as layout rows now, not registry rows.
    let known = core.enumerate_known_windows();
    assert_eq!(
        known.iter().filter(|k| k.name == "voln").count(),
        1,
        "no duplicate row after conjuring"
    );
}

#[test]
fn expose_lifecycle_show_dismiss_reshow_and_user_block() {
    // Redesign Phase 4d gate: expose = show; closeDialog dismisses
    // without eating the NEXT expose; the user's Hidden is the block.
    let mut core = core_with_layout(vec![]);

    // 1. First arrival via exposeStream: registers bound and SHOWS
    //    (the expose default), unlike plain discoveries (hidden).
    core.ui_state
        .pending_exposes
        .push(("stream".to_string(), "charprofile".to_string()));
    core.realize_offered_windows(80, 24);
    let vis = core
        .layout
        .windows
        .iter()
        .find(|w| {
            w.base()
                .binding
                .as_ref()
                .is_some_and(|b| b.id() == "charprofile")
        })
        .map(|w| (w.name().to_string(), w.base().visibility))
        .expect("expose registered a bound window");
    assert!(vis.1.is_shown(), "expose default is SHOWN");
    assert!(
        core.ui_state.windows.contains_key(&vis.0),
        "and the window materialized"
    );

    // 2. The matching closeDialog dismisses the DISPLAY only: the
    //    runtime window goes, the persisted visibility does not flip
    //    to Hidden (a game dismissal is not a user block).
    core.ui_state
        .pending_expose_closes
        .push("charprofile".to_string());
    core.ui_state
        .expose_shown_ids
        .insert("charprofile".to_string());
    core.realize_offered_windows(80, 24);
    assert!(
        !core.ui_state.windows.contains_key(&vis.0),
        "dematerialized"
    );
    let still_shown = core
        .layout
        .windows
        .iter()
        .find(|w| w.name() == vis.0)
        .unwrap()
        .base()
        .visibility
        .is_shown();
    assert!(still_shown, "persisted visibility untouched by game close");

    // 3. Re-expose re-materializes (the walk-back-into-the-bank flow).
    core.ui_state
        .pending_exposes
        .push(("stream".to_string(), "charprofile".to_string()));
    core.realize_offered_windows(80, 24);
    assert!(core.ui_state.windows.contains_key(&vis.0), "re-shown");

    // 4. The user hides it: that IS the block — the next expose no-ops.
    core.set_known_window_shown(&vis.0, false, 80, 24);
    core.ui_state
        .pending_exposes
        .push(("stream".to_string(), "charprofile".to_string()));
    core.realize_offered_windows(80, 24);
    assert!(
        !core.ui_state.windows.contains_key(&vis.0),
        "expose blocked by the user's Hidden"
    );

    // 5. Defensive closes of never-opened ids (withdraw/deposit) no-op.
    core.ui_state
        .pending_expose_closes
        .push("withdraw".to_string());
    core.realize_offered_windows(80, 24);
}

#[test]
fn declared_size_hint_shapes_new_windows_but_never_dedicated_views() {
    // Owner rule: every window respects the game's declared size at
    // creation; saved/user geometry wins afterward (creation-time-only
    // application), and dedicated views keep their curated sizes.
    let mut core = core_with_layout(vec![]);
    core.ui_state.window_hints.insert(
        "charprofile".to_string(),
        vec![
            ("location".to_string(), "force-center".to_string()),
            ("height".to_string(), "320".to_string()),
            ("width".to_string(), "400".to_string()),
        ],
    );
    core.ui_state
        .pending_exposes
        .push(("stream".to_string(), "charprofile".to_string()));
    core.realize_offered_windows(120, 60);
    let def = core
        .layout
        .windows
        .iter()
        .find(|w| {
            w.base()
                .binding
                .as_ref()
                .is_some_and(|b| b.id() == "charprofile")
        })
        .expect("expose registered");
    assert_eq!(
        def.base().rows.get(),
        320 / 16 + 1,
        "declared height in cells"
    );
    assert_eq!(
        def.base().cols.get(),
        400 / 8 + 2,
        "declared width in cells"
    );

    // A dedicated view (inventory via its claimed stream) keeps its
    // template size even when the game hints something else.
    core.ui_state.window_hints.insert(
        "inv".to_string(),
        vec![("height".to_string(), "2100".to_string())],
    );
    core.ui_state
        .pending_window_discoveries
        .push(crate::data::WindowDiscovery {
            id: "inv".to_string(),
            title: "Inventory".to_string(),
            kind: crate::data::WindowDiscoveryKind::Stream,
            save: false,
        });
    core.realize_offered_windows(120, 60);
    let inv = core
        .layout
        .windows
        .iter()
        .find(|w| w.base().binding.as_ref().is_some_and(|b| b.id() == "inv"))
        .expect("inv discovered");
    assert_eq!(inv.widget_type(), "inventory");
    let template_rows = crate::core::local_catalog::seed("inventory")
        .unwrap()
        .base()
        .rows
        .get();
    assert_eq!(
        inv.base().rows.get(),
        template_rows,
        "dedicated view untouched"
    );
}
