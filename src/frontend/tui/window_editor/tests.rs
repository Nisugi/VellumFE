//! Test module of the parent facade, split out for size —
//! `super` is still the parent module, so private access and
//! `use super::*` semantics are identical to the inline mod.

use super::*;
use crate::config::{Layout, SpacerWidgetData};

/// A throwaway WindowBase for field-order tests.
fn test_base(name: &str) -> crate::config::WindowBase {
    crate::config::WindowBase {
        name: name.to_string(),
        row: crate::data::geometry::Row::new(0),
        col: crate::data::geometry::Col::new(0),
        rows: crate::data::geometry::Height::new(5),
        cols: crate::data::geometry::Width::new(20),
        show_border: true,
        border_style: "single".to_string(),
        border_sides: crate::config::BorderSides::default(),
        border_color: None,
        show_title: true,
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

/// Hardening guard for the id-threaded window editor: every FieldRef in a
/// widget's navigation order must have a UNIQUE legacy_field_id. A
/// duplicate id is the fragility bug class — two fields would then fight
/// over the same click/focus target, and a copy-paste slip when adding a
/// field wouldn't be caught anywhere else.
#[test]
fn field_order_has_no_duplicate_ids() {
    let widgets: Vec<WindowDef> = vec![
        WindowDef::Text {
            base: test_base("main"),
            data: crate::config::TextWidgetData {
                streams: vec![],
                buffer_size: 1000,
                wordwrap: true,
                show_timestamps: false,
                timestamp_position: None,
                compact: false,
            },
        },
        WindowDef::GS4Experience {
            base: test_base("exp"),
            data: crate::config::GS4ExperienceWidgetData::default(),
        },
    ];
    for w in &widgets {
        let fields = super::WindowEditor::build_field_order_for(w);
        let mut seen = std::collections::HashMap::new();
        for f in &fields {
            let id = f.legacy_field_id();
            if let Some(prev) = seen.insert(id, *f) {
                panic!(
                    "duplicate legacy_field_id {} shared by {:?} and {:?} in {} widget",
                    id,
                    prev,
                    f,
                    w.widget_type()
                );
            }
        }
    }
}

/// The GS4 experience editor must reach every field of
/// GS4ExperienceWidgetData — the parity gap this closed. If a field is
/// added to the struct but not threaded into the editor, this fails.
#[test]
fn gs4_experience_field_order_is_complete() {
    let w = WindowDef::GS4Experience {
        base: test_base("exp"),
        data: crate::config::GS4ExperienceWidgetData::default(),
    };
    let fields = super::WindowEditor::build_field_order_for(&w);
    for expected in [
        FieldRef::GS4ExpShowLevel,
        FieldRef::GS4ExpShowExpBar,
        FieldRef::GS4ExpShowMindBar,
        FieldRef::GS4ExpShowTotalExp,
        FieldRef::GS4ExpShowAscensionExp,
        FieldRef::GS4ExpMindBarColor,
        FieldRef::GS4ExpExpBarColor,
    ] {
        assert!(
            fields.contains(&expected),
            "GS4 experience editor missing field {:?}",
            expected
        );
    }
}

/// Text windows expose the per-window TTS opt-in (tts_speak parity).
#[test]
fn text_window_offers_tts_speak() {
    let w = WindowDef::Text {
        base: test_base("main"),
        data: crate::config::TextWidgetData {
            streams: vec![],
            buffer_size: 1000,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    };
    let fields = super::WindowEditor::build_field_order_for(&w);
    assert!(fields.contains(&FieldRef::TtsSpeak));
}

#[test]
fn test_new_window_spacer_auto_naming_empty_layout() {
    // RED: Test that new_window_with_layout generates auto-name for spacer in empty layout
    let layout = Layout {
        windows: vec![],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let editor = WindowEditor::new_window_with_layout("spacer".to_string(), &layout);
    let lines = editor.name_input.lines();
    let name = if !lines.is_empty() { &lines[0] } else { "" };
    assert_eq!(name, "spacer_1");
}

#[test]
fn test_new_window_spacer_auto_naming_existing_spacers() {
    // RED: Test that new_window_with_layout generates next sequential name
    let spacer1 = WindowDef::Spacer {
        base: crate::config::WindowBase {
            name: "spacer_1".to_string(),
            row: crate::data::geometry::Row::new(0),
            col: crate::data::geometry::Col::new(0),
            rows: crate::data::geometry::Height::new(2),
            cols: crate::data::geometry::Width::new(5),
            show_border: false,
            border_style: "single".to_string(),
            border_sides: crate::config::BorderSides::default(),
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
        },
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

    let editor = WindowEditor::new_window_with_layout("spacer".to_string(), &layout);
    let lines = editor.name_input.lines();
    let name = if !lines.is_empty() { &lines[0] } else { "" };
    assert_eq!(name, "spacer_2");
}

#[test]
fn test_new_window_tabbedtext_auto_naming_empty_layout() {
    // Test that new_window_with_layout generates custom-tabbedtext-1 for tabbedtext in empty layout
    let layout = Layout {
        windows: vec![],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let editor = WindowEditor::new_window_with_layout("tabbedtext".to_string(), &layout);
    let lines = editor.name_input.lines();
    let name = if !lines.is_empty() { &lines[0] } else { "" };
    assert_eq!(name, "custom-tabbedtext-1");
}

#[test]
fn test_new_window_text_auto_naming_empty_layout() {
    // Test that new_window_with_layout generates custom-text-1 for text in empty layout
    let layout = Layout {
        windows: vec![],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let editor = WindowEditor::new_window_with_layout("text".to_string(), &layout);
    let lines = editor.name_input.lines();
    let name = if !lines.is_empty() { &lines[0] } else { "" };
    assert_eq!(name, "custom-text-1");
}

#[test]
fn test_new_window_progress_auto_naming_empty_layout() {
    // Test that new_window_with_layout generates custom-progress-1 for progress in empty layout
    let layout = Layout {
        windows: vec![],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let editor = WindowEditor::new_window_with_layout("progress".to_string(), &layout);
    let lines = editor.name_input.lines();
    let name = if !lines.is_empty() { &lines[0] } else { "" };
    assert_eq!(name, "custom-progress-1");
}

#[test]
fn test_custom_suffix_stripped_in_widget_name() {
    // Test that _custom suffix is stripped from widget type in auto-naming
    // e.g. "tabbedtext_custom" → "custom-tabbedtext-1" (not "custom-tabbedtext_custom-1")
    let layout = Layout {
        windows: vec![],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    // Test tabbedtext_custom generates same pattern as tabbedtext
    let editor = WindowEditor::new_window_with_layout("tabbedtext_custom".to_string(), &layout);
    let lines = editor.name_input.lines();
    let name = if !lines.is_empty() { &lines[0] } else { "" };
    assert_eq!(name, "custom-tabbedtext-1");

    // Test text_custom generates same pattern as text
    let editor = WindowEditor::new_window_with_layout("text_custom".to_string(), &layout);
    let lines = editor.name_input.lines();
    let name = if !lines.is_empty() { &lines[0] } else { "" };
    assert_eq!(name, "custom-text-1");

    // Test progress_custom generates same pattern as progress
    let editor = WindowEditor::new_window_with_layout("progress_custom".to_string(), &layout);
    let lines = editor.name_input.lines();
    let name = if !lines.is_empty() { &lines[0] } else { "" };
    assert_eq!(name, "custom-progress-1");
}

#[test]
fn test_indicators_from_layout_includes_templates() {
    let layout = Layout {
        windows: vec![],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    let indicators = WindowEditor::indicators_from_layout(&layout);
    let ids: Vec<String> = indicators.iter().map(|i| i.id.to_lowercase()).collect();

    // Ensure all built-in indicator templates are present
    assert!(ids.contains(&"poisoned".to_string()));
    assert!(ids.contains(&"bleeding".to_string()));
    assert!(ids.contains(&"diseased".to_string()));
    assert!(ids.contains(&"stunned".to_string()));
    assert!(ids.contains(&"webbed".to_string()));
}

// ===========================================
// Stream picker (seen-streams parity with GUI)
// ===========================================

fn sample_seen() -> Vec<(String, Option<String>)> {
    vec![
        ("bounty".to_string(), None),
        ("familiar".to_string(), Some("Familiar".to_string())),
    ]
}

#[test]
fn test_stream_picker_move_selection_wraps() {
    let mut picker = StreamPicker::new(sample_seen());
    assert_eq!(picker.selected_id(), Some("bounty"));
    picker.move_selection(true);
    assert_eq!(picker.selected_id(), Some("familiar"));
    // Wrap forward back to the first entry.
    picker.move_selection(true);
    assert_eq!(picker.selected_id(), Some("bounty"));
    // Wrap backward to the last entry.
    picker.move_selection(false);
    assert_eq!(picker.selected_id(), Some("familiar"));
}

#[test]
fn test_stream_picker_empty_has_no_selection() {
    let mut picker = StreamPicker::new(Vec::new());
    assert_eq!(picker.selected_id(), None);
    // Navigating an empty list must not panic or change anything.
    picker.move_selection(true);
    assert_eq!(picker.selected_id(), None);
}

#[test]
fn test_open_stream_picker_guards_on_empty_snapshot() {
    let layout = Layout {
        windows: vec![],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };
    let mut editor = WindowEditor::new_window_with_layout("text_custom".to_string(), &layout);
    // No streams seeded yet -> picker does not open.
    assert!(!editor.open_stream_picker());
    assert!(!editor.is_sub_editor_active());
    // Once seeded, it opens.
    editor.set_seen_streams(sample_seen());
    assert!(editor.open_stream_picker());
    assert!(editor.is_sub_editor_active());
}

#[test]
fn test_append_stream_to_field_dedups_and_separates() {
    let layout = Layout {
        windows: vec![],
        terminal_width: None,
        terminal_height: None,
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };
    let mut editor = WindowEditor::new_window_with_layout("text_custom".to_string(), &layout);
    // text_custom seeds streams = ["custom"]; start from a clean field.
    editor.streams_input = WindowEditor::create_textarea();

    editor.append_stream_to_field("bounty");
    assert_eq!(
        editor.streams_input.lines().first().map(String::as_str),
        Some("bounty")
    );

    editor.append_stream_to_field("notes");
    assert_eq!(
        editor.streams_input.lines().first().map(String::as_str),
        Some("bounty, notes")
    );

    // Duplicate (case-insensitive) is ignored.
    editor.append_stream_to_field("Bounty");
    assert_eq!(
        editor.streams_input.lines().first().map(String::as_str),
        Some("bounty, notes")
    );
}
