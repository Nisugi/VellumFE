//! Phase 1 tests for the tab moves (spec `.beads/artifacts/tab-tear-out/
//! spec.md`, "Tests — Phase 1"). Headless: an `AppCore` with a layout,
//! `init_windows`, and assertions on defs, live buffers, routing and undo.

use super::*;
use crate::config::WindowVisibility;
use crate::data::widget::{StyledLine, TextSegment};

fn line(text: String) -> StyledLine {
    StyledLine {
        segments: vec![TextSegment::plain(text)],
        stream: String::new(),
        timestamp: None,
    }
}

// ---- fixtures --------------------------------------------------------------

fn tabbed(name: &str, tabs: &[(&str, &[&str])], buffer_size: usize) -> WindowDef {
    let mut def = crate::core::local_catalog::seed("tabbedtext_custom").expect("seed");
    def.base_mut().name = name.to_string();
    def.base_mut().visibility = WindowVisibility::Shown;
    if let WindowDef::TabbedText { data, .. } = &mut def {
        data.tabs = tabs
            .iter()
            .map(|(tab_name, streams)| TabbedTextTab {
                id: 0,
                name: tab_name.to_string(),
                stream: None,
                streams: streams.iter().map(|s| s.to_string()).collect(),
                show_timestamps: None,
                ignore_activity: None,
                timestamp_position: None,
            })
            .collect();
        data.next_tab_id = 0;
        data.buffer_size = buffer_size;
    }
    def
}

fn text(name: &str, streams: &[&str], buffer_size: usize) -> WindowDef {
    let mut def = crate::core::local_catalog::seed(CUSTOM_TEXT_SEED).expect("seed");
    def.base_mut().name = name.to_string();
    def.base_mut().visibility = WindowVisibility::Shown;
    if let WindowDef::Text { data, .. } = &mut def {
        data.streams = streams.iter().map(|s| s.to_string()).collect();
        data.buffer_size = buffer_size;
    }
    def
}

fn core_with(windows: Vec<WindowDef>) -> AppCore {
    let mut core = AppCore::new_for_test();
    core.layout.windows = windows;
    core.init_windows(120, 40);
    core
}

fn tab_ids(core: &AppCore, window: &str) -> Vec<TabId> {
    match core.layout.windows.iter().find(|w| w.name() == window) {
        Some(WindowDef::TabbedText { data, .. }) => data.tabs.iter().map(|t| t.id).collect(),
        _ => panic!("'{window}' is not a tabbed def"),
    }
}

fn tab_names(core: &AppCore, window: &str) -> Vec<String> {
    match core.layout.windows.iter().find(|w| w.name() == window) {
        Some(WindowDef::TabbedText { data, .. }) => data.tabs.iter().map(|t| t.name.clone()).collect(),
        _ => panic!("'{window}' is not a tabbed def"),
    }
}

fn def_buffer_size(core: &AppCore, window: &str) -> usize {
    match core.layout.windows.iter().find(|w| w.name() == window) {
        Some(WindowDef::TabbedText { data, .. }) => data.buffer_size,
        Some(WindowDef::Text { data, .. }) => data.buffer_size,
        _ => panic!("'{window}' has no buffer size"),
    }
}

fn tab_content<'a>(core: &'a AppCore, window: &str, id: TabId) -> &'a TextContent {
    let Some(WindowContent::TabbedText(tabbed)) = core.ui_state.windows.get(window).map(|w| &w.content)
    else {
        panic!("'{window}' is not a live tabbed window");
    };
    let index = tabbed.index_of(id).expect("tab id present");
    &tabbed.tabs[index].content
}

fn text_content<'a>(core: &'a AppCore, window: &str) -> &'a TextContent {
    match core.ui_state.windows.get(window).map(|w| &w.content) {
        Some(WindowContent::Text(content)) => content,
        _ => panic!("'{window}' is not a live text window"),
    }
}

fn push_tab_lines(core: &mut AppCore, window: &str, id: TabId, n: usize) {
    let tabbed = core.live_tabbed_mut(window).unwrap();
    let index = tabbed.index_of(id).unwrap();
    for i in 0..n {
        tabbed.tabs[index]
            .content
            .add_line(line(format!("{window}/{id}/{i}")));
    }
}

fn push_text_lines(core: &mut AppCore, window: &str, n: usize) {
    let Some(WindowContent::Text(content)) = core.ui_state.windows.get_mut(window).map(|w| &mut w.content)
    else {
        panic!("'{window}' is not a live text window");
    };
    for i in 0..n {
        content.add_line(line(format!("{window}/{i}")));
    }
}

fn active_id(core: &AppCore, window: &str) -> Option<TabId> {
    match core.ui_state.windows.get(window).map(|w| &w.content) {
        Some(WindowContent::TabbedText(tabbed)) => tabbed.active_tab_id(),
        _ => None,
    }
}

fn select(core: &mut AppCore, window: &str, id: TabId) {
    assert!(core.live_tabbed_mut(window).unwrap().select_tab_id(id));
}

fn subscribers(core: &AppCore, stream: &str) -> Vec<String> {
    core.message_processor.get_stream_subscribers(stream).to_vec()
}

fn custom_windows(core: &AppCore) -> Vec<String> {
    core.layout
        .windows
        .iter()
        .map(|w| w.name().to_string())
        .filter(|n| n.starts_with("custom-"))
        .collect()
}

fn is_text_def(core: &AppCore, window: &str) -> bool {
    matches!(
        core.layout.windows.iter().find(|w| w.name() == window),
        Some(WindowDef::Text { .. })
    )
}

fn bare_tabbed_data(ids: &[TabId], next_tab_id: TabId) -> TabbedTextWidgetData {
    TabbedTextWidgetData {
        tabs: ids
            .iter()
            .map(|&id| TabbedTextTab {
                id,
                name: format!("t{id}"),
                ..Default::default()
            })
            .collect(),
        next_tab_id,
        buffer_size: 100,
        tab_bar_position: "top".into(),
        tab_separator: false,
        tab_active_color: None,
        tab_inactive_color: None,
        tab_unread_color: None,
        tab_unread_prefix: None,
    }
}

fn ids_of(data: &TabbedTextWidgetData) -> Vec<TabId> {
    data.tabs.iter().map(|t| t.id).collect()
}

// ---- identity --------------------------------------------------------------

#[test]
fn legacy_tabs_get_sequential_ids_and_a_counter_above_them() {
    let mut data = bare_tabbed_data(&[0, 0, 0], 0);
    assert!(data.normalize_tab_ids());
    assert_eq!(ids_of(&data), vec![1, 2, 3]);
    assert_eq!(data.next_tab_id, 4);
    assert!(!data.normalize_tab_ids(), "idempotent");
}

#[test]
fn mixed_ids_are_reserved_before_repair() {
    let mut data = bare_tabbed_data(&[5, 0, 5, 7], 0);
    data.normalize_tab_ids();
    assert_eq!(ids_of(&data), vec![5, 8, 9, 7]);
    assert_eq!(data.next_tab_id, 10);
}

#[test]
fn a_stale_counter_is_raised_before_anything_allocates() {
    let mut data = bare_tabbed_data(&[5, 7], 2);
    data.normalize_tab_ids();
    assert_eq!(data.next_tab_id, 8);
    assert_eq!(data.alloc_tab_id(), 8);
}

#[test]
fn ids_are_assigned_at_init_and_never_recur() {
    let mut core = core_with(vec![tabbed("chat", &[("A", &["a"]), ("B", &["b"])], 100)]);
    assert_eq!(tab_ids(&core, "chat"), vec![1, 2]);

    // Tear B out (its id leaves), then join something back: fresh id.
    core.tear_out_tab("chat", 2).unwrap();
    core.layout.windows.push(text("loot", &["loot"], 100));
    let loot = core.layout.windows.last().unwrap().clone();
    core.add_new_window(&loot, 0, 0);
    let receipt = core.join_window_as_tab("loot", "chat", None).unwrap();
    assert_eq!(receipt.tab_id, Some(3), "id 2 is never handed out again");
}

/// The in-session boundary: a tab added without a reload gets an id from
/// the sync, and removing the *first* of two same-named tabs keeps the
/// second's buffer (the bug the id key fixes, driven through `AppCore`).
#[test]
fn in_session_tabs_get_ids_and_duplicate_names_keep_their_buffers() {
    let mut core = core_with(vec![tabbed("chat", &[("Combat", &["combat"])], 100)]);
    if let Some(WindowDef::TabbedText { data, .. }) =
        core.layout.windows.iter_mut().find(|w| w.name() == "chat")
    {
        data.tabs.push(TabbedTextTab {
            id: 0, // an editor that forgot to allocate
            name: "Combat".into(),
            streams: vec!["death".into()],
            ..Default::default()
        });
    }
    core.sync_tabbed_window_tabs("chat");
    assert_eq!(tab_ids(&core, "chat"), vec![1, 2]);
    push_tab_lines(&mut core, "chat", 2, 4);

    if let Some(WindowDef::TabbedText { data, .. }) =
        core.layout.windows.iter_mut().find(|w| w.name() == "chat")
    {
        data.tabs.remove(0);
    }
    core.sync_tabbed_window_tabs("chat");
    assert_eq!(tab_ids(&core, "chat"), vec![2]);
    assert_eq!(tab_content(&core, "chat", 2).lines.len(), 4);
}

// ---- tear-out --------------------------------------------------------------

#[test]
fn tear_out_moves_the_scrollback_and_the_streams() {
    let mut core = core_with(vec![tabbed(
        "chat",
        &[("Main", &["main"]), ("Combat", &["combat", "death"])],
        500,
    )]);
    push_tab_lines(&mut core, "chat", 2, 3);
    let before = tab_content(&core, "chat", 2).clone();

    let receipt = core.tear_out_tab("chat", 2).unwrap();
    assert_eq!(receipt.kind, TabMoveKind::TearOut);
    let new_window = receipt.created_window.clone().expect("a window was created");
    assert!(new_window.starts_with("custom-"));
    assert_eq!(receipt.window, new_window);
    assert_eq!(receipt.old_scroll_id, "chat::tab#2");
    assert_eq!(receipt.new_scroll_id, new_window);

    let moved = text_content(&core, &new_window);
    assert_eq!(moved.lines.len(), 3);
    assert_eq!(moved.generation, before.generation);
    assert_eq!(moved.scroll_offset, before.scroll_offset);
    assert_eq!(moved.streams, vec!["combat", "death"], "routing streams installed");
    assert_eq!(moved.title, "Combat");

    assert_eq!(tab_names(&core, "chat"), vec!["Main"]);
    assert_eq!(subscribers(&core, "combat"), vec![new_window.clone()]);
    assert_eq!(subscribers(&core, "death"), vec![new_window]);
    assert_eq!(subscribers(&core, "main"), vec!["chat"]);
    assert!(core.layout_autosave_pending.is_some(), "autosave scheduled");
}

#[test]
fn tear_out_of_the_last_tab_converts_the_window_in_place() {
    let mut core = core_with(vec![tabbed("chat", &[("Combat", &["combat"])], 500)]);
    push_tab_lines(&mut core, "chat", 1, 2);
    // A tab's content never had its routing streams filled in.
    assert!(tab_content(&core, "chat", 1).streams.is_empty());

    let receipt = core.tear_out_tab("chat", 1).unwrap();
    assert_eq!(receipt.kind, TabMoveKind::ConvertedInPlace);
    assert_eq!(receipt.window, "chat");
    assert!(receipt.created_window.is_none());
    assert!(custom_windows(&core).is_empty(), "no extra window");
    assert!(is_text_def(&core, "chat"));
    assert_eq!(
        core.ui_state.windows["chat"].widget_type,
        WidgetType::Text,
        "runtime type follows the def"
    );
    assert_eq!(text_content(&core, "chat").lines.len(), 2);
    assert_eq!(text_content(&core, "chat").streams, vec!["combat"]);
    assert_eq!(subscribers(&core, "combat"), vec!["chat"]);
    assert!(
        core.ui_state.widgets_to_reset.contains(&"chat".to_string()),
        "frontends drop their typed cache"
    );
}

#[test]
fn tear_out_keeps_the_active_tab_by_id() {
    let mut core = core_with(vec![tabbed(
        "chat",
        &[("A", &["a"]), ("B", &["b"]), ("C", &["c"])],
        100,
    )]);
    select(&mut core, "chat", 2);
    core.tear_out_tab("chat", 1).unwrap();
    assert_eq!(active_id(&core, "chat"), Some(2), "B stays on screen");
}

#[test]
fn tearing_out_the_active_tab_selects_its_neighbour() {
    let mut core = core_with(vec![tabbed(
        "chat",
        &[("A", &["a"]), ("B", &["b"]), ("C", &["c"])],
        100,
    )]);
    select(&mut core, "chat", 2);
    core.tear_out_tab("chat", 2).unwrap();
    assert_eq!(active_id(&core, "chat"), Some(3), "right-hand neighbour");

    let mut core = core_with(vec![tabbed("chat", &[("A", &["a"]), ("B", &["b"])], 100)]);
    select(&mut core, "chat", 2);
    core.tear_out_tab("chat", 2).unwrap();
    assert_eq!(active_id(&core, "chat"), Some(1), "last tab");
}

#[test]
fn tear_out_never_names_the_window_after_the_tab() {
    // A built-in "familiar" window exists; the tab is titled "Familiar".
    let mut core = core_with(vec![
        text("familiar", &["familiar"], 100),
        tabbed("chat", &[("Main", &["main"]), ("Familiar", &["thoughts"])], 100),
    ]);
    let receipt = core.tear_out_tab("chat", 2).unwrap();
    let created = receipt.created_window.unwrap();
    assert!(created.starts_with("custom-"));
    assert_ne!(created, "familiar");
    let def = core.layout.windows.iter().find(|w| w.name() == created).unwrap();
    assert_eq!(def.base().title.as_deref(), Some("Familiar"));
    // The built-in is untouched.
    assert_eq!(subscribers(&core, "familiar"), vec!["familiar"]);
}

#[test]
fn tear_out_copies_the_source_appearance() {
    let mut source = tabbed("chat", &[("A", &["a"]), ("B", &["b"])], 100);
    source.base_mut().border_color = Some("red".into());
    source.base_mut().text_color = Some("#abcdef".into());
    source.base_mut().show_title = false;
    let mut core = core_with(vec![source]);
    let created = core.tear_out_tab("chat", 2).unwrap().created_window.unwrap();
    let base = core
        .layout
        .windows
        .iter()
        .find(|w| w.name() == created)
        .unwrap()
        .base()
        .clone();
    assert_eq!(base.border_color.as_deref(), Some("red"));
    assert_eq!(base.text_color.as_deref(), Some("#abcdef"));
    assert!(!base.show_title);
    assert_eq!(def_buffer_size(&core, &created), 100, "capacity follows the source");
}

#[test]
fn tear_out_rejects_bad_targets() {
    let mut core = core_with(vec![
        tabbed("chat", &[("A", &["a"])], 100),
        text("loot", &["loot"], 100),
    ]);
    assert_eq!(
        core.tear_out_tab("nope", 1),
        Err(TabMoveError::NoSuchWindow("nope".into()))
    );
    assert_eq!(core.tear_out_tab("loot", 1), Err(TabMoveError::NotTabbed("loot".into())));
    assert_eq!(
        core.tear_out_tab("chat", 9),
        Err(TabMoveError::NoSuchTab {
            window: "chat".into(),
            tab_id: 9
        })
    );
}

// ---- join ------------------------------------------------------------------

#[test]
fn join_moves_the_scrollback_in_and_removes_the_source() {
    let mut core = core_with(vec![
        tabbed("chat", &[("Main", &["main"]), ("Loot", &["loot"])], 500),
        text("combat", &["combat", "death"], 500),
    ]);
    push_text_lines(&mut core, "combat", 4);
    let before = text_content(&core, "combat").clone();

    let receipt = core.join_window_as_tab("combat", "chat", Some(1)).unwrap();
    assert_eq!(receipt.kind, TabMoveKind::Join);
    assert_eq!(receipt.removed_window.as_deref(), Some("combat"));
    let id = receipt.tab_id.expect("tab id");
    assert_eq!(receipt.old_scroll_id, "combat");
    assert_eq!(receipt.new_scroll_id, format!("chat::tab#{id}"));

    assert_eq!(tab_names(&core, "chat"), vec!["Main", "combat", "Loot"], "insert_at");
    assert_eq!(active_id(&core, "chat"), Some(id), "the new tab is active");
    let moved = tab_content(&core, "chat", id);
    assert_eq!(moved.lines.len(), 4);
    assert_eq!(moved.generation, before.generation);
    assert!(!core.ui_state.windows.contains_key("combat"));
    assert!(core.layout.windows.iter().all(|w| w.name() != "combat"));
    assert_eq!(subscribers(&core, "combat"), vec!["chat"]);
    assert_eq!(subscribers(&core, "death"), vec!["chat"]);
}

#[test]
fn join_permits_duplicate_names_and_rejects_bad_sources() {
    let mut core = core_with(vec![
        tabbed("chat", &[("Loot", &["loot"])], 100),
        text("loot2", &["loot2"], 100),
        tabbed("other", &[("X", &["x"])], 100),
    ]);
    if let Some(def) = core.layout.windows.iter_mut().find(|w| w.name() == "loot2") {
        def.base_mut().title = Some("Loot".into());
    }
    core.join_window_as_tab("loot2", "chat", None).unwrap();
    assert_eq!(tab_names(&core, "chat"), vec!["Loot", "Loot"]);
    assert_eq!(tab_ids(&core, "chat"), vec![1, 2]);

    assert_eq!(
        core.join_window_as_tab("other", "chat", None),
        Err(TabMoveError::NotText("other".into()))
    );
    assert_eq!(
        core.join_window_as_tab("chat", "chat", None),
        Err(TabMoveError::SameWindow("chat".into()))
    );
    assert_eq!(
        core.join_window_as_tab("ghost", "chat", None),
        Err(TabMoveError::NoSuchWindow("ghost".into()))
    );
}

#[test]
fn join_raises_the_target_capacity_instead_of_trimming() {
    let mut core = core_with(vec![
        tabbed("chat", &[("Main", &["main"])], 5000),
        text("big", &["big"], 0), // unlimited
    ]);
    push_text_lines(&mut core, "big", 8000);

    let receipt = core.join_window_as_tab("big", "chat", None).unwrap();
    let id = receipt.tab_id.unwrap();
    assert_eq!(receipt.capacity_changed, vec![("chat".to_string(), 5000, 8000)]);
    assert_eq!(def_buffer_size(&core, "chat"), 8000);
    assert_eq!(tab_content(&core, "chat", id).lines.len(), 8000, "nothing trimmed");
    assert_eq!(tab_content(&core, "chat", id).max_lines, 8000);
    assert_eq!(tab_content(&core, "chat", 1).max_lines, 8000, "every tab follows");

    // One in, one out from here on.
    push_tab_lines(&mut core, "chat", id, 1);
    assert_eq!(tab_content(&core, "chat", id).lines.len(), 8000);
}

#[test]
fn join_into_an_unlimited_target_stays_unlimited() {
    let mut core = core_with(vec![
        tabbed("chat", &[("Main", &["main"])], 0),
        text("big", &["big"], 100),
    ]);
    push_text_lines(&mut core, "big", 50);
    let receipt = core.join_window_as_tab("big", "chat", None).unwrap();
    assert!(receipt.capacity_changed.is_empty());
    assert_eq!(def_buffer_size(&core, "chat"), 0);
}

// ---- wrap / reorder --------------------------------------------------------

#[test]
fn wrap_then_convert_back_round_trips() {
    let mut core = core_with(vec![text("loot", &["loot"], 300)]);
    push_text_lines(&mut core, "loot", 5);

    let receipt = core.wrap_text_as_tabbed("loot").unwrap();
    assert_eq!(receipt.kind, TabMoveKind::WrappedInPlace);
    assert_eq!(receipt.tab_id, Some(1));
    assert_eq!(core.ui_state.windows["loot"].widget_type, WidgetType::TabbedText);
    assert_eq!(tab_content(&core, "loot", 1).lines.len(), 5);
    assert_eq!(tab_content(&core, "loot", 1).streams, vec!["loot"]);
    assert_eq!(subscribers(&core, "loot"), vec!["loot"]);

    let receipt = core.tear_out_tab("loot", 1).unwrap();
    assert_eq!(receipt.kind, TabMoveKind::ConvertedInPlace);
    assert!(is_text_def(&core, "loot"));
    assert_eq!(text_content(&core, "loot").lines.len(), 5);
    assert_eq!(subscribers(&core, "loot"), vec!["loot"]);
}

#[test]
fn reorder_keeps_buffers_and_selection_by_id() {
    let mut core = core_with(vec![tabbed(
        "chat",
        &[("A", &["a"]), ("B", &["b"]), ("C", &["c"])],
        100,
    )]);
    push_tab_lines(&mut core, "chat", 1, 2);
    select(&mut core, "chat", 2);
    core.reorder_tabs("chat", &[3, 1, 2]).unwrap();
    assert_eq!(tab_names(&core, "chat"), vec!["C", "A", "B"]);
    assert_eq!(active_id(&core, "chat"), Some(2));
    assert_eq!(tab_content(&core, "chat", 1).lines.len(), 2);
    assert!(core.reorder_tabs("chat", &[1, 2]).is_err(), "must be a permutation");
}

// ---- undo ------------------------------------------------------------------

#[test]
fn undo_tear_out_restores_the_tab_at_its_index_with_its_buffer() {
    let mut core = core_with(vec![tabbed(
        "chat",
        &[("A", &["a"]), ("B", &["b"]), ("C", &["c"])],
        100,
    )]);
    push_tab_lines(&mut core, "chat", 2, 3);
    select(&mut core, "chat", 2);
    let created = core.tear_out_tab("chat", 2).unwrap().created_window.unwrap();

    let outcome = core.undo_tab_move().unwrap();
    assert_eq!(outcome.receipt.removed_window.as_deref(), Some(created.as_str()));
    assert_eq!(tab_ids(&core, "chat"), vec![1, 2, 3], "same id, same index");
    assert_eq!(tab_content(&core, "chat", 2).lines.len(), 3);
    assert_eq!(active_id(&core, "chat"), Some(2), "selection restored");
    assert!(!core.ui_state.windows.contains_key(&created));
    assert!(custom_windows(&core).is_empty());
    assert_eq!(subscribers(&core, "b"), vec!["chat"]);
    assert_eq!(core.undo_tab_move(), Err(TabMoveError::NothingToUndo), "single level");
}

#[test]
fn undo_in_place_conversion_restores_the_tabbed_definition() {
    let mut source = tabbed("chat", &[("Combat", &["combat"])], 100);
    source.base_mut().border_color = Some("blue".into());
    let mut core = core_with(vec![source]);
    push_tab_lines(&mut core, "chat", 1, 2);
    let original = core.layout.windows[0].clone();

    core.tear_out_tab("chat", 1).unwrap();
    assert!(is_text_def(&core, "chat"));
    core.undo_tab_move().unwrap();

    assert_eq!(core.layout.windows[0], original, "definition byte-identical");
    assert_eq!(core.ui_state.windows["chat"].widget_type, WidgetType::TabbedText);
    assert_eq!(tab_content(&core, "chat", 1).lines.len(), 2);
    assert_eq!(subscribers(&core, "combat"), vec!["chat"]);
}

#[test]
fn undo_wrap_restores_the_text_definition() {
    let mut core = core_with(vec![text("loot", &["loot"], 300)]);
    push_text_lines(&mut core, "loot", 5);
    let original = core.layout.windows[0].clone();
    core.wrap_text_as_tabbed("loot").unwrap();
    core.undo_tab_move().unwrap();
    assert_eq!(core.layout.windows[0], original);
    assert_eq!(text_content(&core, "loot").lines.len(), 5);
    assert_eq!(core.ui_state.windows["loot"].widget_type, WidgetType::Text);
}

#[test]
fn undo_join_recreates_the_source_under_its_own_name() {
    let mut combat = text("combat", &["combat"], 500);
    combat.base_mut().title = Some("Combat!".into());
    combat.base_mut().border_color = Some("red".into());
    let mut core = core_with(vec![
        tabbed("chat", &[("Main", &["main"]), ("Loot", &["loot"])], 500),
        combat,
    ]);
    push_text_lines(&mut core, "combat", 4);
    select(&mut core, "chat", 2);
    let original = core.layout.windows[1].clone();

    core.join_window_as_tab("combat", "chat", Some(0)).unwrap();
    let outcome = core.undo_tab_move().unwrap();

    assert_eq!(outcome.receipt.created_window.as_deref(), Some("combat"));
    let restored = core.layout.windows.iter().find(|w| w.name() == "combat").unwrap();
    assert_eq!(restored, &original, "name, title, colours, capacity all back");
    assert_eq!(text_content(&core, "combat").lines.len(), 4);
    assert_eq!(tab_names(&core, "chat"), vec!["Main", "Loot"]);
    assert_eq!(active_id(&core, "chat"), Some(2), "target's prior selection");
    assert_eq!(subscribers(&core, "combat"), vec!["combat"]);
    assert!(custom_windows(&core).is_empty());
}

#[test]
fn undo_join_reconciles_capacity_for_both_windows() {
    let mut core = core_with(vec![
        tabbed("chat", &[("Main", &["main"]), ("Loot", &["loot"])], 5000),
        text("combat", &["combat"], 1000),
    ]);
    // The source holds far more than its own cap says (its cap was raised
    // earlier); model that directly.
    {
        let Some(WindowContent::Text(c)) = core.ui_state.windows.get_mut("combat").map(|w| &mut w.content)
        else {
            unreachable!()
        };
        c.max_lines = 0;
    }
    push_text_lines(&mut core, "combat", 6000);

    let receipt = core.join_window_as_tab("combat", "chat", None).unwrap();
    let joined = receipt.tab_id.unwrap();
    assert_eq!(def_buffer_size(&core, "chat"), 6000);

    // Life goes on inside the target: the joined tab churns, another tab
    // grows past the old 5,000 cap.
    push_tab_lines(&mut core, "chat", joined, 200);
    assert_eq!(tab_content(&core, "chat", joined).lines.len(), 6000);
    push_tab_lines(&mut core, "chat", 2, 5100);
    assert_eq!(tab_content(&core, "chat", 2).lines.len(), 5100);

    let outcome = core.undo_tab_move().unwrap();
    assert_eq!(def_buffer_size(&core, "chat"), 5100, "what the survivors need");
    assert_eq!(tab_content(&core, "chat", 2).lines.len(), 5100);
    assert_eq!(tab_content(&core, "chat", 2).max_lines, 5100);
    assert_eq!(def_buffer_size(&core, "combat"), 6000, "what the source holds");
    assert_eq!(text_content(&core, "combat").lines.len(), 6000);
    assert_eq!(text_content(&core, "combat").max_lines, 6000);
    let mut changed = outcome.receipt.capacity_changed.clone();
    changed.sort();
    assert_eq!(
        changed,
        vec![
            ("chat".to_string(), 6000, 5100),
            ("combat".to_string(), 1000, 6000)
        ]
    );
}

#[test]
fn undo_join_keeps_an_unlimited_source_unlimited() {
    let mut core = core_with(vec![
        tabbed("chat", &[("Main", &["main"])], 100),
        text("big", &["big"], 0),
    ]);
    push_text_lines(&mut core, "big", 250);
    core.join_window_as_tab("big", "chat", None).unwrap();
    core.undo_tab_move().unwrap();
    assert_eq!(def_buffer_size(&core, "big"), 0);
    assert_eq!(text_content(&core, "big").max_lines, 0);
    assert_eq!(text_content(&core, "big").lines.len(), 250);
}

#[test]
fn undo_reorder_restores_order_and_selection() {
    let mut core = core_with(vec![tabbed(
        "chat",
        &[("A", &["a"]), ("B", &["b"]), ("C", &["c"])],
        100,
    )]);
    select(&mut core, "chat", 3);
    core.reorder_tabs("chat", &[2, 3, 1]).unwrap();
    core.undo_tab_move().unwrap();
    assert_eq!(tab_ids(&core, "chat"), vec![1, 2, 3]);
    assert_eq!(active_id(&core, "chat"), Some(3));
}

#[test]
fn undo_refuses_after_any_definition_change() {
    let mut core = core_with(vec![
        tabbed("chat", &[("A", &["a"]), ("B", &["b"])], 100),
        tabbed("other", &[("X", &["x"])], 100),
    ]);
    core.tear_out_tab("chat", 2).unwrap();
    // A rename in an unrelated window is still a def change.
    if let Some(WindowDef::TabbedText { data, .. }) =
        core.layout.windows.iter_mut().find(|w| w.name() == "other")
    {
        data.tabs[0].name = "Y".into();
    }
    assert_eq!(core.undo_tab_move(), Err(TabMoveError::LayoutChanged));
    assert_eq!(core.undo_tab_move(), Err(TabMoveError::NothingToUndo), "record cleared");
}

#[test]
fn undo_survives_gui_only_state_but_not_a_reload() {
    let mut core = core_with(vec![tabbed("chat", &[("A", &["a"]), ("B", &["b"])], 100)]);
    core.tear_out_tab("chat", 2).unwrap();
    // GUI placement is not in the layout defs.
    core.set_tab_undo_gui(serde_json::json!({ "rect": [1, 2, 3, 4] }));
    assert_eq!(core.layout_hash(), core.last_tab_move.as_ref().unwrap().layout_hash);

    // A reload rebuilds every buffer: nothing to restore onto.
    core.init_windows(120, 40);
    assert_eq!(core.undo_tab_move(), Err(TabMoveError::NothingToUndo));
}

#[test]
fn undo_join_refuses_when_the_source_name_is_taken() {
    let mut core = core_with(vec![
        tabbed("chat", &[("Main", &["main"])], 100),
        text("combat", &["combat"], 100),
    ]);
    core.join_window_as_tab("combat", "chat", None).unwrap();
    // An ephemeral runtime window with the old name (no def, so the layout
    // hash is unchanged) blocks recreation.
    core.ui_state.windows.insert(
        "combat".into(),
        crate::data::window::WindowState::new_text("combat", 10),
    );
    assert_eq!(core.undo_tab_move(), Err(TabMoveError::NameTaken("combat".into())));
}

#[test]
fn undo_hands_back_the_gui_placement_blob() {
    let mut core = core_with(vec![
        tabbed("chat", &[("Main", &["main"])], 100),
        text("combat", &["combat"], 100),
    ]);
    core.join_window_as_tab("combat", "chat", None).unwrap();
    core.set_tab_undo_gui(serde_json::json!({ "zone": "left" }));
    let outcome = core.undo_tab_move().unwrap();
    assert_eq!(outcome.gui["zone"], "left");
}


// ---- move_tab (tab-to-tab) ---------------------------------------------------

#[test]
fn move_tab_carries_buffer_and_undo_restores_order() {
    let mut core = core_with(vec![
        tabbed("a", &[("one", &["thoughts"]), ("two", &["speech"])], 100),
        tabbed("b", &[("x", &["combat"])], 50),
    ]);
    let ids_a = tab_ids(&core, "a");
    push_tab_lines(&mut core, "a", ids_a[1], 80);
    let receipt = core.move_tab("a", ids_a[1], "b", None).expect("move");
    assert_eq!(receipt.kind, TabMoveKind::Move);
    assert_eq!(receipt.window, "b");
    assert_eq!(receipt.removed_window, None);
    assert_eq!(tab_names(&core, "a"), vec!["one"]);
    assert_eq!(tab_names(&core, "b"), vec!["x", "two"]);
    let moved_id = receipt.tab_id.expect("id in target");
    assert_eq!(tab_content(&core, "b", moved_id).lines.len(), 80);
    assert_eq!(active_id(&core, "b"), Some(moved_id));
    // Capacity raised, never trimmed.
    assert_eq!(def_buffer_size(&core, "b"), 80);
    assert_eq!(receipt.capacity_changed, vec![("b".to_string(), 50, 80)]);

    let outcome = core.undo_tab_move().expect("undo");
    assert_eq!(outcome.receipt.kind, TabMoveKind::Move);
    assert_eq!(tab_names(&core, "a"), vec!["one", "two"]);
    assert_eq!(tab_names(&core, "b"), vec!["x"]);
    assert_eq!(tab_ids(&core, "a"), ids_a, "original ids come back");
    assert_eq!(tab_content(&core, "a", ids_a[1]).lines.len(), 80);
    assert_eq!(def_buffer_size(&core, "b"), 50, "target capacity back to what x needs");
}

#[test]
fn moving_the_last_tab_removes_the_source_and_undo_recreates_it() {
    let mut core = core_with(vec![
        tabbed("solo", &[("only", &["thoughts"])], 100),
        tabbed("b", &[("x", &["combat"])], 100),
    ]);
    let only = tab_ids(&core, "solo")[0];
    push_tab_lines(&mut core, "solo", only, 5);
    let receipt = core.move_tab("solo", only, "b", Some(0)).expect("move");
    assert_eq!(receipt.removed_window.as_deref(), Some("solo"));
    assert!(core.layout.windows.iter().all(|w| w.name() != "solo"));
    assert!(!core.ui_state.windows.contains_key("solo"));
    assert_eq!(tab_names(&core, "b"), vec!["only", "x"]);

    let outcome = core.undo_tab_move().expect("undo");
    assert_eq!(outcome.receipt.created_window.as_deref(), Some("solo"));
    assert_eq!(tab_names(&core, "solo"), vec!["only"]);
    assert_eq!(tab_content(&core, "solo", only).lines.len(), 5);
    assert_eq!(tab_names(&core, "b"), vec!["x"]);
}

#[test]
fn move_tab_rejects_same_window_and_text_targets() {
    let mut core = core_with(vec![
        tabbed("a", &[("one", &["thoughts"]), ("two", &["speech"])], 100),
        text("t", &["combat"], 100),
    ]);
    let id = tab_ids(&core, "a")[0];
    assert!(matches!(core.move_tab("a", id, "a", None), Err(TabMoveError::SameWindow(_))));
    assert!(matches!(core.move_tab("a", id, "t", None), Err(TabMoveError::NotTabbed(_))));
    assert_eq!(tab_names(&core, "a"), vec!["one", "two"], "nothing changed");
}
