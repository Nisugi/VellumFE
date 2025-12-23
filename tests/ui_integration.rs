//! End-to-end tests: feed real XML through MessageProcessor into UiState and assert window state.

use std::collections::HashMap;
use vellum_fe::{
    config::Config,
    core::{messages::MessageProcessor, GameState},
    data::{
        widget::{
            ActiveEffect, ActiveEffectsContent, CompassData, CountdownData, IndicatorData,
            ProgressData, StyledLine, TextContent, TextSegment,
        },
        window::{WindowContent, WindowPosition, WindowState, WidgetType},
        RoomContent, UiState,
    },
    parser::XmlParser,
};

/// Feed lines through parser + processor into UiState
fn run_fixture(
    lines: &str,
    ui_state: &mut UiState,
    processor: &mut MessageProcessor,
    game_state: &mut GameState,
    parser: &mut XmlParser,
) {
    // Update stream subscriber map from windows before processing
    processor.update_text_stream_subscribers(ui_state);

    let mut room_components: HashMap<String, Vec<Vec<vellum_fe::data::TextSegment>>> = HashMap::new();
    let mut current_room_component: Option<String> = None;
    let mut room_window_dirty = false;
    let mut nav_room_id: Option<String> = None;
    let mut lich_room_id: Option<String> = None;
    let mut room_subtitle: Option<String> = None;
    for line in lines.lines() {
        let elements = parser.parse_line(line);
        for elem in elements {
            processor.process_element(
                &elem,
                game_state,
                ui_state,
                &mut room_components,
                &mut current_room_component,
                &mut room_window_dirty,
                &mut nav_room_id,
                &mut lich_room_id,
                &mut room_subtitle,
                None,
            );
        }
    }
}

fn init_state() -> (UiState, MessageProcessor, GameState, XmlParser) {
    let config = Config::default();
    let ui_state = UiState::new();
    let mut processor = MessageProcessor::new(config.clone());
    processor.update_squelch_patterns();
    processor.update_redirect_cache();
    let game_state = GameState::default();
    let parser = XmlParser::new();
    (ui_state, processor, game_state, parser)
}

fn position() -> WindowPosition {
    WindowPosition {
        x: 0,
        y: 0,
        width: 80,
        height: 5,
    }
}

fn add_progress_window(ui_state: &mut UiState, name: &str, progress_id: &str) {
    let window = WindowState {
        name: name.to_string(),
        widget_type: WidgetType::Progress,
        content: WindowContent::Progress(ProgressData {
            value: 0,
            max: 0,
            label: String::new(),
            color: None,
            progress_id: progress_id.to_string(),
        }),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window(name.to_string(), window);
}

fn add_countdown_window(ui_state: &mut UiState, name: &str, countdown_id: &str) {
    let window = WindowState {
        name: name.to_string(),
        widget_type: WidgetType::Countdown,
        content: WindowContent::Countdown(CountdownData {
            end_time: 0,
            label: name.to_string(),
            countdown_id: countdown_id.to_string(),
        }),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window(name.to_string(), window);
}

fn add_indicator_window(ui_state: &mut UiState, name: &str, indicator_id: &str) {
    let window = WindowState {
        name: name.to_string(),
        widget_type: WidgetType::Indicator,
        content: WindowContent::Indicator(IndicatorData {
            indicator_id: indicator_id.to_string(),
            active: false,
            color: None,
        }),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window(name.to_string(), window);
}

fn add_targets_window(ui_state: &mut UiState) {
    let window = WindowState {
        name: "targets".to_string(),
        widget_type: WidgetType::Targets,
        content: WindowContent::Targets,
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window("targets".to_string(), window);
}

fn add_players_window(ui_state: &mut UiState) {
    let window = WindowState {
        name: "players".to_string(),
        widget_type: WidgetType::Players,
        content: WindowContent::Players,
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window("players".to_string(), window);
}

fn add_players_window_with_id(ui_state: &mut UiState, name: &str, _entity_id: &str) {
    let window = WindowState {
        name: name.to_string(),
        widget_type: WidgetType::Players,
        content: WindowContent::Players,
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window(name.to_string(), window);
}

fn add_hand_window(ui_state: &mut UiState, name: &str) {
    let window = WindowState {
        name: name.to_string(),
        widget_type: WidgetType::Hand,
        content: WindowContent::Hand {
            item: None,
            link: None,
        },
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window(name.to_string(), window);
}

fn add_compass_window(ui_state: &mut UiState) {
    let window = WindowState {
        name: "compass".to_string(),
        widget_type: WidgetType::Compass,
        content: WindowContent::Compass(CompassData { directions: vec![] }),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window("compass".to_string(), window);
}

fn add_dashboard_window(ui_state: &mut UiState, name: &str, indicators: Vec<(String, u8)>) {
    let window = WindowState {
        name: name.to_string(),
        widget_type: WidgetType::Dashboard,
        content: WindowContent::Dashboard { indicators },
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window(name.to_string(), window);
}

fn add_active_effects_window(ui_state: &mut UiState, name: &str, category: &str) {
    let window = WindowState {
        name: name.to_string(),
        widget_type: WidgetType::ActiveEffects,
        content: WindowContent::ActiveEffects(ActiveEffectsContent {
            category: category.to_string(),
            effects: vec![],
        }),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window(name.to_string(), window);
}

fn add_injury_window(ui_state: &mut UiState) {
    let window = WindowState {
        name: "injuries".to_string(),
        widget_type: WidgetType::InjuryDoll,
        content: WindowContent::InjuryDoll(vellum_fe::data::widget::InjuryDollData::new()),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window("injuries".to_string(), window);
}

fn add_tabbed_window(
    ui_state: &mut UiState,
    name: &str,
    tabs: Vec<(String, Vec<String>, bool, bool)>,
    max_lines: usize,
) {
    // Convert 4-tuple to 5-tuple with default TimestampPosition
    let tabs_with_timestamp: Vec<_> = tabs
        .into_iter()
        .map(|(name, streams, show_ts, ignore)| {
            (name, streams, show_ts, ignore, vellum_fe::config::TimestampPosition::default())
        })
        .collect();
    let window = WindowState {
        name: name.to_string(),
        widget_type: WidgetType::TabbedText,
        content: WindowContent::TabbedText(vellum_fe::data::widget::TabbedTextContent::new(
            tabs_with_timestamp,
            max_lines,
        )),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window(name.to_string(), window);
}

fn add_text_window(ui_state: &mut UiState, name: &str, max_lines: usize) {
    let mut text_content = TextContent::new(name, max_lines);
    // Set streams to match window name (for routing)
    text_content.streams = vec![name.to_string()];
    let window = WindowState {
        name: name.to_string(),
        widget_type: WidgetType::Text,
        content: WindowContent::Text(text_content),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window(name.to_string(), window);
}

fn add_inventory_window(ui_state: &mut UiState) {
    let window = WindowState {
        name: "inventory".to_string(),
        widget_type: WidgetType::Inventory,
        content: WindowContent::Inventory(TextContent::new("inventory", 100)),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window("inventory".to_string(), window);
}

fn add_spells_window(ui_state: &mut UiState) {
    let window = WindowState {
        name: "spells".to_string(),
        widget_type: WidgetType::Spells,
        content: WindowContent::Spells(TextContent::new("spells", 200)),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window("spells".to_string(), window);
}

fn lines_to_strings(lines: &std::collections::VecDeque<StyledLine>) -> Vec<String> {
    lines
        .iter()
        .map(|l| l.segments.iter().map(|s| s.text.as_str()).collect::<String>())
        .collect()
}

fn segments_to_string(segments: &[TextSegment]) -> String {
    segments.iter().map(|s| s.text.as_str()).collect()
}

// ---------------- Text routing ----------------

#[test]
fn ui_routes_tabbed_streams_to_matching_tabs() {
    const XML: &str = "<clearStream id='thoughts'/><pushStream id='thoughts'/>Thoughts line<popStream/>\
                       <clearStream id='trash'/><pushStream id='trash'/>Trash line<popStream/>\
                       <prompt time='1759294807'>&gt;</prompt>";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_tabbed_window(
        &mut ui_state,
        "tabbed",
        vec![
            ("Thoughts".to_string(), vec!["thoughts".to_string()], false, false),
            ("Trash".to_string(), vec!["trash".to_string()], false, true),
        ],
        20,
    );

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let tabbed = ui_state
        .windows
        .get("tabbed")
        .and_then(|w| {
            if let WindowContent::TabbedText(content) = &w.content {
                Some(content.clone())
            } else {
                None
            }
        })
        .expect("tabbed window should exist");

    let thoughts_lines = lines_to_strings(&tabbed.tabs[0].content.lines);
    let trash_lines = lines_to_strings(&tabbed.tabs[1].content.lines);

    assert!(
        thoughts_lines.iter().any(|l| l.contains("Thoughts line")),
        "thoughts stream should populate Thoughts tab"
    );
    assert!(
        trash_lines.iter().any(|l| l.contains("Trash line")),
        "trash stream should populate Trash tab even when ignore_activity is true"
    );
}

#[test]
fn ui_routes_streams_to_text_windows() {
    const XML: &str = include_str!("fixtures/text_routing.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 200);
    add_text_window(&mut ui_state, "thoughts", 50);
    add_text_window(&mut ui_state, "speech", 50);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let thoughts_lines = ui_state
        .windows
        .get("thoughts")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();
    assert!(
        thoughts_lines.iter().any(|l| l.contains("Thoughts line")),
        "thoughts stream should land in thoughts window"
    );

    let speech_lines = ui_state
        .windows
        .get("speech")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();
    assert!(
        speech_lines.iter().any(|l| l.contains("Speech line")),
        "speech stream should land in speech window"
    );
}

#[test]
fn ui_falls_back_to_main_when_stream_window_missing() {
    const XML: &str = include_str!("fixtures/text_routing_no_window.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 200);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let main_lines = ui_state
        .windows
        .get("main")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    // Speech/talk/whisper streams are dropped when no dedicated window exists
    // This prevents duplicates since the game sends both a speech stream line AND a main stream line
    assert!(
        main_lines.is_empty(),
        "speech stream should be dropped (not fall back) to prevent duplicates in main window"
    );
}

#[test]
fn ui_updates_inventory_stream_when_window_present() {
    const XML: &str = include_str!("fixtures/session_start.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 200);
    add_inventory_window(&mut ui_state);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let inv_lines = ui_state
        .windows
        .get("inventory")
        .and_then(|w| {
            if let WindowContent::Inventory(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        inv_lines.iter().any(|l| l.contains("worn items")),
        "inventory stream should populate inventory window"
    );
}

#[test]
fn inventory_stream_is_discarded_without_window() {
    const XML: &str = include_str!("fixtures/session_start.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 200);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let main_lines = ui_state
        .windows
        .get("main")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        main_lines.is_empty(),
        "inventory stream should be discarded instead of falling back to main"
    );
}

#[test]
fn unknown_stream_falls_back_to_main() {
    const XML: &str = include_str!("fixtures/unknown_stream.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 100);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let main_lines = ui_state
        .windows
        .get("main")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        main_lines.iter().any(|l| l.contains("Odd line")),
        "unknown stream should route to main window"
    );
}

#[test]
fn playerlist_stream_is_discarded_without_window() {
    const XML: &str = include_str!("fixtures/playerlist_stream.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 100);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let main_lines = ui_state
        .windows
        .get("main")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        main_lines.is_empty(),
        "playerlist stream should be discarded when no players window exists"
    );
}

#[test]
fn speech_stream_dropped_without_window() {
    const XML: &str = include_str!("fixtures/speech_duplicate.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 100);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let main_lines = ui_state
        .windows
        .get("main")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    // Should have exactly ONE line (the non-speech-stream copy)
    // The speech stream line should be dropped since no speech window exists
    assert_eq!(
        main_lines.len(),
        1,
        "should have exactly one line (speech stream line should be dropped)"
    );
    assert!(
        main_lines[0].contains("You politely say"),
        "main window should receive the regular copy"
    );
}

#[test]
fn spells_stream_populates_spells_window() {
    const XML: &str = include_str!("fixtures/spells_stream.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 200);
    add_spells_window(&mut ui_state);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let spells_lines = ui_state
        .windows
        .get("spells")
        .and_then(|w| {
            if let WindowContent::Spells(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        spells_lines.iter().any(|l| l.contains("Spell listing")),
        "spells stream should populate spells window when present"
    );
}

#[test]
fn spells_stream_falls_back_to_main_without_window() {
    const XML: &str = include_str!("fixtures/spells_stream.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 200);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let main_lines = ui_state
        .windows
        .get("main")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        main_lines.iter().any(|l| l.contains("Spell listing")),
        "spells stream should fall back to main when spells window is missing"
    );
}

#[test]
fn prompt_is_skipped_after_silent_stream() {
    const XML: &str = "<clearStream id='percWindow'/><pushStream id='percWindow'/>A swing!<popStream/>\
                       <prompt time='1759294809'>&gt;</prompt>";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 50);
    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let main_lines = ui_state
        .windows
        .get("main")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        main_lines.is_empty(),
        "prompt should be skipped when only silent combat updates were received"
    );
}

#[test]
fn prompt_is_rendered_after_main_text() {
    const XML: &str =
        "<clearStream id='main'/><pushStream id='main'/>Regular line<popStream/>\
         <prompt time='1759294810'>&gt;</prompt>";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_text_window(&mut ui_state, "main", 50);
    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let main_lines = ui_state
        .windows
        .get("main")
        .and_then(|w| {
            if let WindowContent::Text(content) = &w.content {
                Some(lines_to_strings(&content.lines))
            } else {
                None
            }
        })
        .unwrap_or_default();

    let joined: String = main_lines.join("\n");
    assert!(!main_lines.is_empty(), "main window should have rendered lines");
    assert!(
        joined.contains("Regular line"),
        "main window should include the text line from main stream"
    );
    assert!(
        joined.contains(">"),
        "main window should include the prompt when chunk has main text"
    );
}

// ---------------- Vitals / Progress ----------------

#[test]
fn ui_updates_mindstate_progress() {
    const XML: &str = include_str!("fixtures/progress_mindstate.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_progress_window(&mut ui_state, "mind", "mindState");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let mind = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Progress) && w.name == "mind")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max, p.label.clone()))
            } else {
                None
            }
        })
        .expect("mind progress window should exist");

    assert_eq!(mind.0, 100);
    assert_eq!(mind.1, 100, "mindState defaults max to 100 in feed");
    assert!(mind.2.contains("must rest"));
}

#[test]
fn ui_updates_custom_progress_ids() {
    const XML: &str = "<progressBar id='encumlevel' value='5' max='100' text='None'/>";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_progress_window(&mut ui_state, "encum", "encumlevel");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let encum = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Progress) && w.name == "encum")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max, p.label.clone()))
            } else {
                None
            }
        })
        .expect("encum progress window should exist");

    assert_eq!(encum.0, 5);
    assert_eq!(encum.1, 100);
    assert!(encum.2.contains("None"));
}

// ---------------- Countdown ----------------

#[test]
fn countdown_does_not_update_wrong_id() {
    const XML: &str = "<roundTime value='10'/>";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_countdown_window(&mut ui_state, "casttime", "casttime");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let ct = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Countdown) && w.name == "casttime")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Countdown(c) = &w.content {
                Some(c.end_time)
            } else {
                None
            }
        })
        .unwrap_or(0);

    assert_eq!(ct, 0, "casttime should remain untouched when only roundtime arrives");
}

#[test]
fn countdown_updates_when_name_matches_even_if_id_differs() {
    const XML: &str = include_str!("fixtures/roundtime_name_match.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    // countdown_id intentionally does not match the incoming id; name does
    add_countdown_window(&mut ui_state, "roundtime", "rt_custom");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let rt = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Countdown) && w.name == "roundtime")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Countdown(c) = &w.content {
                Some(c.end_time)
            } else {
                None
            }
        })
        .unwrap_or(0);

    assert!(
        rt > 0,
        "roundtime countdown should update when window name matches even if id differs"
    );
}

// ---------------- Indicators / Dashboard ----------------

#[test]
fn dashboard_updates_indicator_statuses() {
    const XML: &str = "<indicator id='IconHIDDEN' visible='y'/><indicator id='iconstunned' visible='n'/>";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_dashboard_window(
        &mut ui_state,
        "dash",
        vec![("HIDDEN".to_string(), 0), ("STUNNED".to_string(), 0)],
    );

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let indicators = ui_state
        .windows
        .get("dash")
        .and_then(|w| {
            if let WindowContent::Dashboard { indicators } = &w.content {
                Some(indicators.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let hidden = indicators.iter().find(|(id, _)| id == "HIDDEN").map(|(_, v)| *v);
    let stunned = indicators.iter().find(|(id, _)| id == "STUNNED").map(|(_, v)| *v);
    assert_eq!(hidden, Some(1), "hidden indicator should be marked active");
    assert_eq!(stunned, Some(0), "stunned indicator should stay inactive");
}

#[test]
fn indicators_preserve_casing_when_added_to_dashboard() {
    const XML: &str = "<indicator id='IconStAnDing' visible='y'/>";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_dashboard_window(&mut ui_state, "dash", vec![]);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let indicators = ui_state
        .windows
        .get("dash")
        .and_then(|w| {
            if let WindowContent::Dashboard { indicators } = &w.content {
                Some(indicators.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        indicators.iter().any(|(id, v)| id == "StAnDing" && *v == 1),
        "dashboard should store indicator id with original casing"
    );
}

#[test]
fn indicators_match_case_insensitively_without_rewriting_id() {
    const XML: &str = "<indicator id='IconSTANDING' visible='y'/>";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_indicator_window(&mut ui_state, "standing", "standing");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let standing = ui_state
        .windows
        .get("standing")
        .and_then(|w| {
            if let WindowContent::Indicator(ind) = &w.content {
                Some((ind.indicator_id.clone(), ind.active))
            } else {
                None
            }
        })
        .expect("indicator window should exist");

    assert_eq!(
        standing.0, "standing",
        "existing indicator id casing should remain unchanged"
    );
    assert!(standing.1, "indicator should activate even when feed casing differs");
}

// ---------------- Active Effects ----------------

#[test]
fn active_effects_update_from_dialogdata() {
    const XML: &str = include_str!("fixtures/active_effects.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_active_effects_window(&mut ui_state, "buffs", "Buffs");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let effects: Vec<ActiveEffect> = ui_state
        .windows
        .get("buffs")
        .and_then(|w| {
            if let WindowContent::ActiveEffects(content) = &w.content {
                Some(content.effects.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert_eq!(effects.len(), 1, "buffs should capture one active effect");
    let effect = &effects[0];
    assert_eq!(effect.id, "115");
    assert_eq!(effect.value, 74);
    assert!(effect.text.contains("Fasthr"), "text should come from feed");
    assert_eq!(effect.time, "03:06:54");
}

#[test]
fn active_effects_clear_removes_entries() {
    const XML: &str = include_str!("fixtures/active_effects_clear.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_active_effects_window(&mut ui_state, "buffs", "Buffs");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let effects: Vec<ActiveEffect> = ui_state
        .windows
        .get("buffs")
        .and_then(|w| {
            if let WindowContent::ActiveEffects(content) = &w.content {
                Some(content.effects.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        effects.is_empty(),
        "clear='t' dialogData should remove all active effects"
    );
}

#[test]
fn active_spells_category_is_normalized_and_populates() {
    const XML: &str = include_str!("fixtures/active_spells.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_active_effects_window(&mut ui_state, "active_spells", "ActiveSpells");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let effects: Vec<ActiveEffect> = ui_state
        .windows
        .get("active_spells")
        .and_then(|w| {
            if let WindowContent::ActiveEffects(content) = &w.content {
                Some(content.effects.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert_eq!(effects.len(), 1, "Active Spells should normalize and populate");
    let effect = &effects[0];
    assert_eq!(effect.id, "905");
    assert!(effect.text.contains("Prismatic"), "text should carry spell name");
}

#[test]
fn active_spells_clear_removes_entries() {
    const XML: &str = include_str!("fixtures/active_spells_clear.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_active_effects_window(&mut ui_state, "active_spells", "ActiveSpells");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let effects: Vec<ActiveEffect> = ui_state
        .windows
        .get("active_spells")
        .and_then(|w| {
            if let WindowContent::ActiveEffects(content) = &w.content {
                Some(content.effects.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        effects.is_empty(),
        "clear='t' for Active Spells should wipe effects after initial populate"
    );
}

#[test]
fn active_cooldowns_populate_and_clear() {
    const XML: &str = include_str!("fixtures/active_cooldowns.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_active_effects_window(&mut ui_state, "cooldowns", "Cooldowns");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let effects: Vec<ActiveEffect> = ui_state
        .windows
        .get("cooldowns")
        .and_then(|w| {
            if let WindowContent::ActiveEffects(content) = &w.content {
                Some(content.effects.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
    assert_eq!(effects.len(), 1, "cooldowns should populate from Cooldowns dialogData");

    // Clear
    const CLEAR_XML: &str = include_str!("fixtures/active_cooldowns_clear.xml");
    run_fixture(CLEAR_XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let effects_after: Vec<ActiveEffect> = ui_state
        .windows
        .get("cooldowns")
        .and_then(|w| {
            if let WindowContent::ActiveEffects(content) = &w.content {
                Some(content.effects.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        effects_after.is_empty(),
        "clear='t' for Cooldowns should remove entries"
    );
}

#[test]
fn active_debuffs_populate_and_clear() {
    const XML: &str = include_str!("fixtures/active_debuffs.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_active_effects_window(&mut ui_state, "debuffs", "Debuffs");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let effects: Vec<ActiveEffect> = ui_state
        .windows
        .get("debuffs")
        .and_then(|w| {
            if let WindowContent::ActiveEffects(content) = &w.content {
                Some(content.effects.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
    assert_eq!(effects.len(), 1, "debuffs should populate from Debuffs dialogData");

    // Now clear
    const CLEAR_XML: &str = include_str!("fixtures/active_debuffs_clear.xml");
    run_fixture(CLEAR_XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let effects_after: Vec<ActiveEffect> = ui_state
        .windows
        .get("debuffs")
        .and_then(|w| {
            if let WindowContent::ActiveEffects(content) = &w.content {
                Some(content.effects.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert!(
        effects_after.is_empty(),
        "clear='t' for Debuffs should remove active debuffs"
    );
}

// ---------------- Injuries ----------------

#[test]
fn injuries_update_levels_from_dialogdata() {
    const XML: &str = include_str!("fixtures/injuries.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_injury_window(&mut ui_state);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let injuries = ui_state
        .windows
        .get("injuries")
        .and_then(|w| {
            if let WindowContent::InjuryDoll(doll) = &w.content {
                Some(doll.injuries.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert_eq!(injuries.get("head"), Some(&2), "Injury2 should set level 2");
    assert_eq!(injuries.get("leftLeg"), Some(&6), "Scar3 should set level 6");
}

#[test]
fn injuries_clear_when_body_part_matches_name() {
    const XML: &str = include_str!("fixtures/injuries_clear.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_injury_window(&mut ui_state);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let injuries = ui_state
        .windows
        .get("injuries")
        .and_then(|w| {
            if let WindowContent::InjuryDoll(doll) = &w.content {
                Some(doll.injuries.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert_eq!(
        injuries.get("head"),
        Some(&0u8),
        "when name matches id the injury should clear to level 0"
    );
}

#[test]
fn progress_does_not_update_when_id_mismatch() {
    const XML: &str = include_str!("fixtures/progress_wrong_id.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_progress_window(&mut ui_state, "health", "health");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let health = ui_state
        .windows
        .values()
        .find(|w| w.name == "health")
        .and_then(|w| {
            if let WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max))
            } else {
                None
            }
        })
        .expect("health window should exist");

    assert_eq!(
        health,
        (0, 0),
        "progress with different id should not alter unrelated progress windows"
    );
}

#[test]
fn progress_ids_are_case_sensitive() {
    const XML: &str = include_str!("fixtures/progress_case_mismatch.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_progress_window(&mut ui_state, "health", "health");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let health = ui_state
        .windows
        .values()
        .find(|w| w.name == "health")
        .and_then(|w| {
            if let WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max, p.label.clone()))
            } else {
                None
            }
        })
        .expect("health window should exist");

    assert_eq!(
        health,
        (0, 0, String::new()),
        "uppercase id in feed should not update lowercase-configured progress window"
    );
}

#[test]
fn indicator_dialogdata_updates_indicator_window_case_insensitive() {
    const XML: &str = include_str!("fixtures/icon_dialogdata.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_indicator_window(&mut ui_state, "bleed", "bleeding");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let bleed = ui_state
        .windows
        .get("bleed")
        .and_then(|w| {
            if let WindowContent::Indicator(ind) = &w.content {
                Some(ind.active)
            } else {
                None
            }
        })
        .unwrap_or(false);

    assert!(bleed, "dialogData Icon* indicators should toggle matching indicator windows");
}

#[test]
fn indicator_dialogdata_clear_turns_off_indicator() {
    const XML: &str = include_str!("fixtures/icon_dialogdata_clear.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_indicator_window(&mut ui_state, "bleed", "bleeding");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let bleed = ui_state
        .windows
        .get("bleed")
        .and_then(|w| {
            if let WindowContent::Indicator(ind) = &w.content {
                Some(ind.active)
            } else {
                None
            }
        })
        .unwrap_or(true);

    assert!(
        !bleed,
        "clear value should deactivate indicator, even with different casing"
    );
}

#[test]
fn spell_hand_updates_from_spell_tag() {
    const XML: &str = include_str!("fixtures/spell_hand.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_hand_window(&mut ui_state, "spell_hand");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let spell_item = ui_state
        .windows
        .get("spell_hand")
        .and_then(|w| {
            if let WindowContent::Hand { item, .. } = &w.content {
                item.clone()
            } else {
                None
            }
        });

    assert_eq!(
        spell_item.as_deref(),
        Some("Prismatic Guard"),
        "spell hand should mirror last <spell> text"
    );
}

#[test]
fn spell_hand_clears_on_empty_spell() {
    const SET_XML: &str = include_str!("fixtures/spell_hand.xml");
    const CLEAR_XML: &str = include_str!("fixtures/spell_hand_clear.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_hand_window(&mut ui_state, "spell_hand");

    // Set
    run_fixture(SET_XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);
    // Clear
    run_fixture(CLEAR_XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let spell_item = ui_state
        .windows
        .get("spell_hand")
        .and_then(|w| {
            if let WindowContent::Hand { item, .. } = &w.content {
                item.clone()
            } else {
                None
            }
        });

    assert!(
        spell_item.is_none(),
        "empty <spell> should clear spell hand item"
    );
}

#[test]
fn hands_clear_on_empty_left_and_right_tags() {
    const LEFT_SET: &str = "<left noun='sword'>a longsword</left>";
    const LEFT_CLEAR: &str = include_str!("fixtures/left_hand_clear.xml");
    const RIGHT_SET: &str = "<right>a shield</right>";
    const RIGHT_CLEAR: &str = include_str!("fixtures/right_hand_clear.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_hand_window(&mut ui_state, "left");
    add_hand_window(&mut ui_state, "right");

    // Set both
    run_fixture(LEFT_SET, &mut ui_state, &mut processor, &mut game_state, &mut parser);
    run_fixture(RIGHT_SET, &mut ui_state, &mut processor, &mut game_state, &mut parser);
    // Clear both
    run_fixture(LEFT_CLEAR, &mut ui_state, &mut processor, &mut game_state, &mut parser);
    run_fixture(RIGHT_CLEAR, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let left_item = ui_state
        .windows
        .get("left")
        .and_then(|w| {
            if let WindowContent::Hand { item, .. } = &w.content {
                item.clone()
            } else {
                None
            }
        });
    let right_item = ui_state
        .windows
        .get("right")
        .and_then(|w| {
            if let WindowContent::Hand { item, .. } = &w.content {
                item.clone()
            } else {
                None
            }
        });

    assert!(left_item.is_none(), "empty <left> should clear left hand");
    assert!(right_item.is_none(), "empty <right> should clear right hand");
}

#[test]
fn left_hand_link_data_is_preserved() {
    const XML: &str = include_str!("fixtures/left_hand_link.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_hand_window(&mut ui_state, "left");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let link = ui_state
        .windows
        .get("left")
        .and_then(|w| {
            if let WindowContent::Hand { link, .. } = &w.content {
                link.clone()
            } else {
                None
            }
        })
        .expect("left hand should have link data");

    assert_eq!(link.noun, "longsword");
    assert_eq!(link.exist_id, "123");
}

// ---------------- Tabbed unread/ignore ----------------

#[test]
fn tabbed_marks_unread_unless_ignore_activity() {
    const XML: &str = include_str!("fixtures/tabbed_unread.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    // Two tabs: speech (normal), trash (ignore activity)
    add_tabbed_window(
        &mut ui_state,
        "tabbed",
        vec![
            ("Speech".to_string(), vec!["speech".to_string()], false, false),
            ("Trash".to_string(), vec!["trash".to_string()], false, true),
        ],
        10,
    );

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let tabbed = ui_state
        .windows
        .get("tabbed")
        .and_then(|w| {
            if let WindowContent::TabbedText(content) = &w.content {
                Some(content.clone())
            } else {
                None
            }
        })
        .expect("tabbed window should exist");

    let speech_lines = lines_to_strings(&tabbed.tabs[0].content.lines);
    let trash_lines = lines_to_strings(&tabbed.tabs[1].content.lines);

    assert!(
        speech_lines.iter().any(|l| l.contains("speech line 1")),
        "speech tab should receive its stream"
    );
    assert!(
        trash_lines.iter().any(|l| l.contains("trash line 1")),
        "ignore_activity tab should still receive text"
    );
    assert!(tabbed.tabs[1].definition.ignore_activity, "trash tab should keep ignore_activity=true");
}

// ---------------- Dashboard add-on update ----------------

#[test]
fn dashboard_adds_indicator_when_missing() {
    const XML: &str = include_str!("fixtures/indicator_dashboard_add.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    // Start with empty dashboard indicators
    add_dashboard_window(&mut ui_state, "dash", vec![]);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let indicators = ui_state
        .windows
        .get("dash")
        .and_then(|w| {
            if let WindowContent::Dashboard { indicators } = &w.content {
                Some(indicators.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert_eq!(indicators.len(), 1, "dashboard should add missing indicator entry");
    assert_eq!(indicators[0].0, "HIDDEN");
    assert_eq!(indicators[0].1, 1);
}

#[test]
fn dashboard_updates_existing_indicator_without_duplication() {
    const XML: &str = include_str!("fixtures/indicator_dashboard_update.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_dashboard_window(
        &mut ui_state,
        "dash",
        vec![("HIDDEN".to_string(), 0)],
    );

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let indicators = ui_state
        .windows
        .get("dash")
        .and_then(|w| {
            if let WindowContent::Dashboard { indicators } = &w.content {
                Some(indicators.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    assert_eq!(
        indicators.len(),
        1,
        "dashboard should update existing indicator instead of duplicating"
    );
    assert_eq!(indicators[0].0, "HIDDEN");
    assert_eq!(indicators[0].1, 0, "second update should turn indicator off");
}

// ---------------- Vitals / Progress ----------------

#[test]
fn ui_updates_progress_vitals_from_dialogdata() {
    const XML: &str = include_str!("fixtures/vitals_indicators.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_progress_window(&mut ui_state, "health", "health");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    // health window should exist and have updated ProgressData
    let health = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Progress) && w.name == "health")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max, p.label.clone()))
            } else {
                None
            }
        })
        .expect("health progress window should exist");

    assert_eq!(health.0, 325);
    assert_eq!(health.1, 326);
    assert!(health.2.contains("health"), "label should include text from feed");
}

// ---------------- Countdown ----------------

#[test]
fn ui_updates_roundtime_countdown() {
    // use combat fixture which carries roundtime dialogData
    const XML: &str = include_str!("fixtures/combat_roundtime.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_countdown_window(&mut ui_state, "roundtime", "roundtime");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    // roundtime countdown window default name/id "roundtime"
    let rt = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Countdown) && w.name == "roundtime")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Countdown(c) = &w.content {
                Some(c.end_time)
            } else {
                None
            }
        })
        .expect("roundtime countdown should exist");

    assert!(rt > 0, "roundtime end_time should be set");
}

#[test]
fn ui_updates_cast_and_roundtime_countdowns() {
    const XML: &str = include_str!("fixtures/countdown_casttime.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_countdown_window(&mut ui_state, "roundtime", "roundtime");
    add_countdown_window(&mut ui_state, "casttime", "casttime");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let rt = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Countdown) && w.name == "roundtime")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Countdown(c) = &w.content {
                Some(c.end_time)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let ct = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Countdown) && w.name == "casttime")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Countdown(c) = &w.content {
                Some(c.end_time)
            } else {
                None
            }
        })
        .unwrap_or(0);

    assert!(rt > 0, "roundtime end_time should be set from roundTime tag");
    assert!(ct > 0, "casttime end_time should be set from castTime tag");
}

// ---------------- Indicators ----------------

#[test]
fn ui_sets_indicator_status_from_icon() {
    const XML: &str = "<indicator id='IconSTUNNED' visible='y'/>";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_indicator_window(&mut ui_state, "stunned", "STUNNED");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let stunned_active = ui_state
        .windows
        .values()
        .find(|w| w.name == "stunned")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Indicator(ind) = &w.content {
                Some(ind.active)
            } else {
                None
            }
        })
        .unwrap_or(false);

    assert!(stunned_active, "stunned indicator should be active");
}

#[test]
fn ui_updates_multiple_indicators() {
    const XML: &str = include_str!("fixtures/indicators_multi.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_indicator_window(&mut ui_state, "hidden", "HIDDEN");
    add_indicator_window(&mut ui_state, "stunned", "STUNNED");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let hidden_active = ui_state
        .windows
        .values()
        .find(|w| w.name == "hidden")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Indicator(ind) = &w.content {
                Some(ind.active)
            } else {
                None
            }
        })
        .unwrap_or(false);
    let stunned_active = ui_state
        .windows
        .values()
        .find(|w| w.name == "stunned")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Indicator(ind) = &w.content {
                Some(ind.active)
            } else {
                None
            }
        })
        .unwrap_or(true);

    assert!(hidden_active, "hidden indicator should be active");
    assert!(!stunned_active, "stunned indicator should remain inactive");
}

// ---------------- Targets / Players ----------------


#[test]
fn ui_ignores_playercount_and_list_streams() {
    const XML: &str = include_str!("fixtures/player_counts.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_players_window(&mut ui_state);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    assert!(
        game_state.room_players.is_empty(),
        "playercount/playerlist streams are ignored by component-based players"
    );
}

#[test]
fn players_streams_are_ignored_without_window() {
    const XML: &str = include_str!("fixtures/player_counts.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    // No players window: counts should not be stored anywhere
    let has_players = ui_state
        .windows
        .values()
        .any(|w| matches!(w.widget_type, WidgetType::Players));
    assert!(!has_players, "no players window should exist");
}

// ---------------- Hands / Compass ----------------

#[test]
fn ui_sets_hands_and_compass() {
    const VITALS_XML: &str = include_str!("fixtures/vitals_indicators.xml");
    const ROOM_XML: &str = include_str!("fixtures/room_navigation.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_hand_window(&mut ui_state, "left");
    add_compass_window(&mut ui_state);

    run_fixture(VITALS_XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);
    run_fixture(ROOM_XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    // Left hand content
    let left = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Hand) && w.name == "left")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Hand { item, .. } = &w.content {
                item.clone()
            } else {
                None
            }
        })
        .unwrap_or_default();
    assert!(!left.is_empty(), "left hand should have item text");

    // Compass exits present
    let exits = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Compass))
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Compass(c) = &w.content {
                Some(c.directions.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
    assert!(!exits.is_empty(), "compass should have directions");
}

#[test]
fn ui_updates_room_subtitle_and_compass() {
    const XML: &str = include_str!("fixtures/room_with_compass.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_compass_window(&mut ui_state);
    // Add room window to retain components/subtitle
    let room_window = WindowState {
        name: "room".to_string(),
        widget_type: WidgetType::Room,
        content: WindowContent::Room(RoomContent {
            name: String::new(),
            description: vec![],
            exits: vec![],
            players: vec![],
            objects: vec![],
        }),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window("room".to_string(), room_window);

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let exits = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Compass))
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Compass(c) = &w.content {
                Some(c.directions.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
    assert_eq!(exits, vec!["s".to_string(), "w".to_string()]);
}

#[test]
fn ui_stores_room_components_when_room_window_present() {
    const XML: &str = include_str!("fixtures/room_components.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    let room_window = WindowState {
        name: "room".to_string(),
        widget_type: WidgetType::Room,
        content: WindowContent::Room(RoomContent {
            name: String::new(),
            description: vec![],
            exits: vec![],
            players: vec![],
            objects: vec![],
        }),
        position: position(),
        visible: true,
        focused: false,
        content_align: None,
        ephemeral: false,
    };
    ui_state.set_window("room".to_string(), room_window);

    // Run fixture but keep access to room_components to ensure they are captured
    let mut room_components: std::collections::HashMap<String, Vec<Vec<TextSegment>>> =
        std::collections::HashMap::new();
    let mut current_room_component: Option<String> = None;
    let mut room_window_dirty = false;
    let mut nav_room_id: Option<String> = None;
    let mut lich_room_id: Option<String> = None;
    let mut room_subtitle: Option<String> = None;
    for line in XML.lines() {
        let elements = parser.parse_line(line);
        for elem in elements {
            processor.process_element(
                &elem,
                &mut game_state,
                &mut ui_state,
                &mut room_components,
                &mut current_room_component,
                &mut room_window_dirty,
                &mut nav_room_id,
                &mut lich_room_id,
                &mut room_subtitle,
                None,
            );
        }
    }

    let desc = room_components
        .get("room desc")
        .and_then(|lines| lines.get(0))
        .map(|segments| segments_to_string(segments))
        .unwrap_or_default();
    let exits = room_components
        .get("room exits")
        .and_then(|lines| lines.get(0))
        .map(|segments| segments_to_string(segments))
        .unwrap_or_default();

    assert!(!desc.is_empty(), "room description should be captured");
    assert!(exits.contains("north"), "room exits should include parsed directions");
    assert!(room_window_dirty, "room window should be marked dirty when components arrive");
}

#[test]
fn ui_ignores_room_components_when_no_room_window() {
    const XML: &str = include_str!("fixtures/room_components.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    assert!(game_state.room_name.is_none(), "room name should remain unset without room window");
    assert!(game_state.exits.is_empty(), "exits should remain empty without room window");
}

// ---------------- Progress variants ----------------

#[test]
fn ui_updates_stance_progress() {
    const XML: &str = "<progressBar id='pbarStance' value='100' max='100' text='defensive (100%)' />";
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_progress_window(&mut ui_state, "stance", "pbarStance");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let stance = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Progress) && w.name == "stance")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max, p.label.clone()))
            } else {
                None
            }
        })
        .expect("stance progress window should exist");

    assert_eq!(stance.0, 100);
    assert_eq!(stance.1, 100);
    assert!(stance.2.contains("defensive"), "label should include stance text");
}

#[test]
fn ui_updates_buffs_progress_bars() {
    const XML: &str = include_str!("fixtures/buffs_progress.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_progress_window(&mut ui_state, "buff", "115");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let buff = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Progress) && w.name == "buff")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Progress(p) = &w.content {
                Some((p.value, p.label.clone()))
            } else {
                None
            }
        })
        .expect("buff progress window should exist");

    assert_eq!(buff.0, 89);
    assert!(buff.1.contains("Fasthr"), "buff label should include spell name");
}

#[test]
fn ui_updates_lblbps_progress_label_only() {
    const XML: &str = include_str!("fixtures/progress_lblbps.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_progress_window(&mut ui_state, "bp", "lblBPs");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let bp = ui_state
        .windows
        .values()
        .find(|w| matches!(w.widget_type, WidgetType::Progress) && w.name == "bp")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max, p.label.clone()))
            } else {
                None
            }
        })
        .expect("lblBPs progress window should exist");

    assert_eq!(bp.0, 100);
    assert_eq!(bp.1, 100, "label-only progress defaults max to 100");
    assert!(bp.2.contains("Blood Points"));
}

#[test]
fn ui_updates_multiple_progress_variants() {
    const XML: &str = include_str!("fixtures/progress_variants.xml");
    let (mut ui_state, mut processor, mut game_state, mut parser) = init_state();
    add_progress_window(&mut ui_state, "stance", "pbarStance");
    add_progress_window(&mut ui_state, "health", "health");
    add_progress_window(&mut ui_state, "foo", "fooCustom");

    run_fixture(XML, &mut ui_state, &mut processor, &mut game_state, &mut parser);

    let stance = ui_state
        .windows
        .values()
        .find(|w| w.name == "stance")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max, p.label.clone()))
            } else {
                None
            }
        })
        .expect("stance progress window should exist");
    assert_eq!(stance.0, 75);
    assert!(stance.2.contains("defensive"));

    let health = ui_state
        .windows
        .values()
        .find(|w| w.name == "health")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max, p.label.clone()))
            } else {
                None
            }
        })
        .expect("health progress window should exist");
    assert_eq!(health.0, 99);
    assert_eq!(health.1, 150);

    let foo = ui_state
        .windows
        .values()
        .find(|w| w.name == "foo")
        .and_then(|w| {
            if let vellum_fe::data::WindowContent::Progress(p) = &w.content {
                Some((p.value, p.max, p.label.clone()))
            } else {
                None
            }
        })
        .expect("custom progress window should exist");
    assert_eq!(foo.0, 5);
    assert_eq!(foo.1, 10, "custom with max missing should still use parsed max when present");
    assert!(foo.2.contains("foo"));
}

// =============================================================================
// Highlight Engine Integration Tests
// =============================================================================

use vellum_fe::config::{HighlightPattern, RedirectMode};
use vellum_fe::core::highlight_engine::CoreHighlightEngine;
use vellum_fe::data::SpanType;

fn make_highlight_pattern(
    pattern: &str,
    fg: Option<&str>,
    bg: Option<&str>,
    bold: bool,
    color_entire_line: bool,
    fast_parse: bool,
) -> HighlightPattern {
    HighlightPattern {
        pattern: pattern.to_string(),
        fg: fg.map(String::from),
        bg: bg.map(String::from),
        bold,
        color_entire_line,
        fast_parse,
        sound: None,
        sound_volume: None,
        category: None,
        squelch: false,
        silent_prompt: false,
        redirect_to: None,
        redirect_mode: RedirectMode::default(),
        replace: None,
        stream: None,
        window: None,
        compiled_regex: None,
    }
}

fn make_segment(text: &str) -> TextSegment {
    TextSegment {
        text: text.to_string(),
        fg: None,
        bg: None,
        bold: false,
        span_type: SpanType::Normal,
        link_data: None,
    }
}

// ---------------- Basic Highlight Application ----------------

#[test]
fn highlight_engine_applies_color_to_matching_text() {
    let patterns = vec![make_highlight_pattern(
        "goblin",
        Some("red"),
        None,
        false,
        false,
        false,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("You see a goblin approaching.")];
    let result = engine.apply_highlights(&segments, "main");

    // Find the segment containing "goblin"
    let goblin_segment = result
        .segments
        .iter()
        .find(|s| s.text.contains("goblin"));
    assert!(goblin_segment.is_some(), "Should have a segment with 'goblin'");
    assert_eq!(
        goblin_segment.unwrap().fg.as_deref(),
        Some("red"),
        "Goblin segment should have red foreground"
    );
}

#[test]
fn highlight_engine_applies_bold_style() {
    let patterns = vec![make_highlight_pattern(
        "attack",
        None,
        None,
        true,
        false,
        false,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("The creature attack you!")];
    let result = engine.apply_highlights(&segments, "main");

    let attack_segment = result.segments.iter().find(|s| s.text.contains("attack"));
    assert!(attack_segment.is_some(), "Should have segment with 'attack'");
    assert!(attack_segment.unwrap().bold, "Attack segment should be bold");
}

#[test]
fn highlight_engine_applies_background_color() {
    let patterns = vec![make_highlight_pattern(
        "critical",
        Some("white"),
        Some("red"),
        true,
        false,
        false,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("A critical hit!")];
    let result = engine.apply_highlights(&segments, "main");

    let critical_segment = result.segments.iter().find(|s| s.text.contains("critical"));
    assert!(critical_segment.is_some());
    let seg = critical_segment.unwrap();
    assert_eq!(seg.fg.as_deref(), Some("white"));
    assert_eq!(seg.bg.as_deref(), Some("red"));
    assert!(seg.bold);
}

// ---------------- Color Entire Line ----------------

#[test]
fn highlight_engine_color_entire_line_colors_all_segments() {
    let patterns = vec![make_highlight_pattern(
        "treasure",
        Some("gold"),
        None,
        false,
        true, // color_entire_line
        false,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("You found a treasure chest!")];
    let result = engine.apply_highlights(&segments, "main");

    // All segments should be gold colored when color_entire_line is true
    for seg in &result.segments {
        if !seg.text.is_empty() {
            assert_eq!(
                seg.fg.as_deref(),
                Some("gold"),
                "All segments should be gold when color_entire_line is set: {:?}",
                seg.text
            );
        }
    }
}

#[test]
fn highlight_engine_color_entire_line_with_multiple_segments() {
    let patterns = vec![make_highlight_pattern(
        "hit",
        Some("yellow"),
        None,
        false,
        true, // color_entire_line
        false,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![
        make_segment("You swing and "),
        make_segment("hit "),
        make_segment("the target!"),
    ];
    let result = engine.apply_highlights(&segments, "main");

    // All non-empty segments should be yellow
    for seg in &result.segments {
        if !seg.text.is_empty() {
            assert_eq!(
                seg.fg.as_deref(),
                Some("yellow"),
                "Segment '{}' should be yellow",
                seg.text
            );
        }
    }
}

// ---------------- Fast Parse (Aho-Corasick) ----------------

#[test]
fn highlight_engine_fast_parse_matches_literals() {
    let patterns = vec![make_highlight_pattern(
        "troll|orc|goblin",
        Some("red"),
        None,
        false,
        false,
        true, // fast_parse
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("A troll and an orc attack!")];
    let result = engine.apply_highlights(&segments, "main");

    let troll_segment = result.segments.iter().find(|s| s.text.contains("troll"));
    let orc_segment = result.segments.iter().find(|s| s.text.contains("orc"));

    assert!(troll_segment.is_some());
    assert_eq!(troll_segment.unwrap().fg.as_deref(), Some("red"));
    assert!(orc_segment.is_some());
    assert_eq!(orc_segment.unwrap().fg.as_deref(), Some("red"));
}

#[test]
fn highlight_engine_fast_parse_handles_multiple_matches_in_line() {
    let patterns = vec![make_highlight_pattern(
        "coin",
        Some("gold"),
        None,
        false,
        false,
        true, // fast_parse
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("coin coin coin")];
    let result = engine.apply_highlights(&segments, "main");

    // Count how many segments are gold colored
    let gold_count = result
        .segments
        .iter()
        .filter(|s| s.fg.as_deref() == Some("gold"))
        .count();
    assert!(gold_count >= 3, "Should have at least 3 gold-colored 'coin' segments");
}

// ---------------- Regex Pattern Matching ----------------

#[test]
fn highlight_engine_regex_captures_patterns() {
    let patterns = vec![make_highlight_pattern(
        r"\d+ silver",
        Some("silver"),
        None,
        false,
        false,
        false, // regex mode
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("You receive 50 silver coins.")];
    let result = engine.apply_highlights(&segments, "main");

    let silver_segment = result.segments.iter().find(|s| s.text.contains("silver"));
    assert!(silver_segment.is_some());
    assert_eq!(silver_segment.unwrap().fg.as_deref(), Some("silver"));
}

#[test]
fn highlight_engine_regex_case_insensitive() {
    let patterns = vec![make_highlight_pattern(
        "(?i)warning",
        Some("orange"),
        None,
        false,
        false,
        false,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("WARNING: Danger ahead!")];
    let result = engine.apply_highlights(&segments, "main");

    let warning_segment = result.segments.iter().find(|s| s.text.contains("WARNING"));
    assert!(warning_segment.is_some());
    assert_eq!(warning_segment.unwrap().fg.as_deref(), Some("orange"));
}

// ---------------- Multiple Patterns ----------------

#[test]
fn highlight_engine_applies_multiple_patterns() {
    let patterns = vec![
        make_highlight_pattern("health", Some("green"), None, false, false, true),
        make_highlight_pattern("mana", Some("blue"), None, false, false, true),
        make_highlight_pattern("stamina", Some("yellow"), None, false, false, true),
    ];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("health: 100, mana: 50, stamina: 75")];
    let result = engine.apply_highlights(&segments, "main");

    let health_seg = result.segments.iter().find(|s| s.text.contains("health"));
    let mana_seg = result.segments.iter().find(|s| s.text.contains("mana"));
    let stamina_seg = result.segments.iter().find(|s| s.text.contains("stamina"));

    assert!(health_seg.is_some());
    assert_eq!(health_seg.unwrap().fg.as_deref(), Some("green"));
    assert!(mana_seg.is_some());
    assert_eq!(mana_seg.unwrap().fg.as_deref(), Some("blue"));
    assert!(stamina_seg.is_some());
    assert_eq!(stamina_seg.unwrap().fg.as_deref(), Some("yellow"));
}

#[test]
fn highlight_engine_first_pattern_wins() {
    // When multiple patterns match the same text, first one wins
    let patterns = vec![
        make_highlight_pattern("attack", Some("red"), None, false, false, true),
        make_highlight_pattern("attack", Some("blue"), None, false, false, true),
    ];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("You attack!")];
    let result = engine.apply_highlights(&segments, "main");

    let attack_seg = result.segments.iter().find(|s| s.text.contains("attack"));
    assert!(attack_seg.is_some());
    assert_eq!(
        attack_seg.unwrap().fg.as_deref(),
        Some("red"),
        "First pattern should win"
    );
}

// ---------------- System Span Preservation ----------------

#[test]
fn highlight_engine_preserves_system_spans() {
    let patterns = vec![make_highlight_pattern(
        "test",
        Some("red"),
        None,
        false,
        false,
        false,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let mut system_segment = make_segment("test message");
    system_segment.span_type = SpanType::System;
    let segments = vec![system_segment];

    let result = engine.apply_highlights(&segments, "main");

    // System spans should not be modified
    assert_eq!(result.segments.len(), 1);
    assert!(
        result.segments[0].fg.is_none(),
        "System span should not have highlight applied"
    );
}

// ---------------- Empty and Edge Cases ----------------

#[test]
fn highlight_engine_handles_empty_segments() {
    let patterns = vec![make_highlight_pattern(
        "test",
        Some("red"),
        None,
        false,
        false,
        false,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments: Vec<TextSegment> = vec![];
    let result = engine.apply_highlights(&segments, "main");

    assert!(result.segments.is_empty());
}

#[test]
fn highlight_engine_handles_no_match() {
    let patterns = vec![make_highlight_pattern(
        "goblin",
        Some("red"),
        None,
        false,
        false,
        false,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("A peaceful meadow.")];
    let result = engine.apply_highlights(&segments, "main");

    // All segments should have no color applied
    for seg in &result.segments {
        assert!(seg.fg.is_none(), "No pattern matched, should have no color");
    }
}

#[test]
fn highlight_engine_handles_empty_pattern_list() {
    let patterns: Vec<HighlightPattern> = vec![];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("Some text here.")];
    let result = engine.apply_highlights(&segments, "main");

    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].text, "Some text here.");
}

// ---------------- Replacement Tests ----------------

#[test]
fn highlight_engine_applies_replacement() {
    let mut pattern = make_highlight_pattern(
        "Fading roisaen",
        Some("cyan"),
        None,
        false,
        false,
        false,
    );
    pattern.replace = Some("★ Fading roisaen".to_string());

    let patterns = vec![pattern];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("Fading roisaen (42)")];
    let result = engine.apply_highlights(&segments, "main");

    let combined_text: String = result.segments.iter().map(|s| s.text.as_str()).collect();
    assert!(
        combined_text.contains("★"),
        "Replacement should add the star prefix: {}",
        combined_text
    );
}

// ---------------- Sound Trigger Tests ----------------

#[test]
fn highlight_engine_returns_sound_triggers() {
    let mut pattern = make_highlight_pattern(
        "level up",
        Some("gold"),
        None,
        true,
        false,
        false,
    );
    pattern.sound = Some("fanfare.wav".to_string());
    pattern.sound_volume = Some(0.8);

    let patterns = vec![pattern];
    let engine = CoreHighlightEngine::new(patterns);

    let segments = vec![make_segment("Congratulations! You level up!")];
    let result = engine.apply_highlights(&segments, "main");

    assert_eq!(result.sounds.len(), 1, "Should have one sound trigger");
    assert_eq!(result.sounds[0].file, "fanfare.wav");
    assert_eq!(result.sounds[0].volume, Some(0.8));
}

// ---------------- Get First Match Color ----------------

#[test]
fn highlight_engine_get_first_match_color_returns_matching_color() {
    let patterns = vec![
        make_highlight_pattern("enemy", Some("red"), None, false, false, true),
        make_highlight_pattern("ally", Some("green"), None, false, false, true),
    ];
    let engine = CoreHighlightEngine::new(patterns);

    let color = engine.get_first_match_color("An enemy approaches!");
    assert_eq!(color, Some("red".to_string()));

    let color2 = engine.get_first_match_color("Your ally arrives.");
    assert_eq!(color2, Some("green".to_string()));
}

#[test]
fn highlight_engine_get_first_match_color_returns_none_when_no_match() {
    let patterns = vec![make_highlight_pattern(
        "goblin",
        Some("red"),
        None,
        false,
        false,
        true,
    )];
    let engine = CoreHighlightEngine::new(patterns);

    let color = engine.get_first_match_color("A peaceful day.");
    assert!(color.is_none());
}

// ---------------- Stream Filter Tests ----------------

#[test]
fn highlight_engine_stream_filter_applies_to_matching_stream() {
    let mut pattern = make_highlight_pattern(
        "combat text",
        Some("red"),
        None,
        false,
        false,
        false,
    );
    pattern.stream = Some("combat".to_string());

    let patterns = vec![pattern];
    let engine = CoreHighlightEngine::new(patterns);

    // Test with matching stream
    let segments = vec![make_segment("combat text here")];
    let result = engine.apply_highlights(&segments, "combat");

    let colored = result.segments.iter().find(|s| s.fg.as_deref() == Some("red"));
    assert!(colored.is_some(), "Pattern should apply to matching stream");
}

#[test]
fn highlight_engine_stream_filter_skips_non_matching_stream() {
    let mut pattern = make_highlight_pattern(
        "combat text",
        Some("red"),
        None,
        false,
        false,
        false,
    );
    pattern.stream = Some("combat".to_string());

    let patterns = vec![pattern];
    let engine = CoreHighlightEngine::new(patterns);

    // Test with non-matching stream
    let segments = vec![make_segment("combat text here")];
    let result = engine.apply_highlights(&segments, "thoughts");

    // Should not apply color since stream doesn't match
    let colored = result.segments.iter().find(|s| s.fg.as_deref() == Some("red"));
    assert!(colored.is_none(), "Pattern should NOT apply to non-matching stream");
}

// ---------------- Silent Prompt Tests ----------------

#[test]
fn highlight_engine_silent_prompt_marks_fully_covered_line_as_silent() {
    let mut pattern = make_highlight_pattern(
        "You go east.",
        None,
        None,
        false,
        false,
        true, // fast_parse
    );
    pattern.silent_prompt = true;

    let patterns = vec![pattern];
    let engine = CoreHighlightEngine::new(patterns);

    // Test with exact match
    let segments = vec![make_segment("You go east.")];
    let result = engine.apply_highlights(&segments, "main");

    assert!(
        result.line_is_silent,
        "Line should be marked as silent when entire line is covered by silent_prompt pattern"
    );
}

#[test]
fn highlight_engine_silent_prompt_non_matching_line_is_not_silent() {
    let mut pattern = make_highlight_pattern(
        "You go east.",
        None,
        None,
        false,
        false,
        true, // fast_parse
    );
    pattern.silent_prompt = true;

    let patterns = vec![pattern];
    let engine = CoreHighlightEngine::new(patterns);

    // Test with non-matching text
    let segments = vec![make_segment("You go west.")];
    let result = engine.apply_highlights(&segments, "main");

    assert!(
        !result.line_is_silent,
        "Line should NOT be marked as silent when pattern doesn't match"
    );
}

#[test]
fn highlight_engine_silent_prompt_partial_match_is_not_silent() {
    let mut pattern = make_highlight_pattern(
        "east",
        None,
        None,
        false,
        false,
        true, // fast_parse
    );
    pattern.silent_prompt = true;

    let patterns = vec![pattern];
    let engine = CoreHighlightEngine::new(patterns);

    // Test with partial match - "east" matches but doesn't cover the whole line
    let segments = vec![make_segment("You go east.")];
    let result = engine.apply_highlights(&segments, "main");

    assert!(
        !result.line_is_silent,
        "Line should NOT be marked as silent when pattern only partially covers it"
    );
}
