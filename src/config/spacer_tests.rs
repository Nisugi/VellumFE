//! Spacer widget regression tests: template lookup, TOML round-trips,
//! and layout resize behavior for spacer windows.

use super::*;
use crate::data::geometry::{Col, Height, Row, Width};

/// The Other split: every former Other-dweller with a real home lands
/// in it; container/dialog runtime types get their own categories.
#[test]
fn widget_category_classification_covers_other_split() {
    use WidgetCategory as C;
    for (wt, want) in [
        ("inventory", C::Character),
        ("spells", C::Character),
        ("injury_doll", C::Character),
        ("gs4_experience", C::Character),
        ("encum", C::Character),
        ("room", C::Navigation),
        ("compass", C::Navigation),
        ("map", C::Navigation),
        ("quickbar", C::Hotbars),
        ("hotkeybar", C::Hotbars),
        ("minivitals", C::ProgressBar),
        ("container", C::Container),
        ("dialogpanel", C::Dialog),
        ("betrayer", C::Dialog),
        ("spacer", C::Other),
        ("command_input", C::Other),
    ] {
        assert_eq!(C::from_widget_type(wt), want, "widget type '{}'", wt);
    }
    // Round-trip: every ALL entry parses back from its Debug name.
    for c in C::ALL {
        assert_eq!(C::from_name(&format!("{:?}", c)), Some(c));
    }
}

#[test]
fn test_spacer_template_exists() {
    // RED: Spacer template should exist and be retrievable
    let template = Config::get_window_template("spacer");
    assert!(template.is_some(), "Spacer template should exist");
}

#[test]
fn test_spacer_template_returns_spacer_widget() {
    // RED: Template should return Spacer widget type
    let template = Config::get_window_template("spacer");
    assert!(template.is_some());

    match template.unwrap() {
        WindowDef::Spacer { .. } => {
            // Expected
        }
        _ => {
            panic!("Expected WindowDef::Spacer variant");
        }
    }
}

#[test]
fn test_spacer_template_widget_type() {
    // RED: widget_type() should return "spacer"
    let template = Config::get_window_template("spacer").expect("Spacer template exists");
    assert_eq!(template.widget_type(), "spacer");
}

#[test]
fn test_spacer_template_defaults() {
    // GREEN: Spacer template should have sensible defaults
    let template = Config::get_window_template("spacer").expect("Spacer template exists");

    if let WindowDef::Spacer { base, .. } = template {
        // Name should be empty (will be set by caller)
        assert_eq!(base.name, "");

        // Dimensions - minimal 2x2 spacer
        assert_eq!(base.rows.get(), 2);
        assert_eq!(base.cols.get(), 2);

        // Spacer should NOT show borders
        assert!(!base.show_border);

        // Spacer should NOT show title
        assert!(!base.show_title);

        // Should NOT be transparent (respects theme background color)
        assert!(!base.transparent_background);

        // Should be visible
        assert!(base.visibility.is_shown());
    } else {
        panic!("Expected WindowDef::Spacer variant");
    }
}

#[test]
fn test_spacer_in_templates_list() {
    // RED: Spacer should be in the list of available templates
    let templates = Config::list_window_templates();
    assert!(
        templates.contains(&"spacer".to_string()),
        "Spacer should be in available templates list"
    );
}

#[test]
fn test_spacer_widget_category() {
    // RED: Spacer should be categorized as "Other"
    let category = WidgetCategory::from_widget_type("spacer");
    assert_eq!(category, WidgetCategory::Other);
}

#[test]
fn test_spacer_in_templates_by_category() {
    // RED: Spacer should appear in templates by category under "Other"
    let by_category = Config::get_templates_by_category();

    if let Some(other_templates) = by_category.get(&WidgetCategory::Other) {
        assert!(
            other_templates.contains(&"spacer".to_string()),
            "Spacer should be in Other category"
        );
    } else {
        panic!("Other category should exist");
    }
}

#[test]
fn test_spacer_data_structure() {
    // RED: SpacerWidgetData should be valid
    let template = Config::get_window_template("spacer").expect("Spacer template exists");

    if let WindowDef::Spacer { data, .. } = template {
        // Should construct without issues
        let _data = SpacerWidgetData {};
        assert_eq!(data, SpacerWidgetData {});
    } else {
        panic!("Expected WindowDef::Spacer variant");
    }
}

#[test]
fn test_spacer_toml_serialization() {
    // RED: Spacer widget should serialize to TOML
    let spacer = WindowDef::Spacer {
        base: WindowBase {
            name: "spacer_1".to_string(),
            row: Row::new(2),
            col: Col::new(5),
            rows: Height::new(3),
            cols: Width::new(8),
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
        },
        data: SpacerWidgetData {},
    };

    let layout = Layout {
        windows: vec![spacer],
        terminal_width: Some(200),
        terminal_height: Some(50),
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    // Should serialize without error
    let toml_str = toml::to_string_pretty(&layout).expect("Failed to serialize layout");
    assert!(!toml_str.is_empty());
    assert!(toml_str.contains("spacer_1"));
}

#[test]
fn test_spacer_toml_deserialization() {
    // RED: Spacer widget should deserialize from TOML
    let toml_str = r#"
terminal_width = 200
terminal_height = 50

[[windows]]
widget_type = "spacer"
name = "spacer_1"
row = 2
col = 5
rows = 3
cols = 8
show_border = false
show_title = false
transparent_background = false
visible = true
"#;

    let layout: Layout = toml::from_str(toml_str).expect("Failed to deserialize layout");
    assert_eq!(layout.windows.len(), 1);
    assert_eq!(layout.windows[0].widget_type(), "spacer");
    assert_eq!(layout.windows[0].name(), "spacer_1");
}

#[test]
fn test_spacer_toml_round_trip() {
    // RED: Layout with spacer should survive serialize/deserialize round-trip
    let spacer = WindowDef::Spacer {
        base: WindowBase {
            name: "spacer_2".to_string(),
            row: Row::new(5),
            col: Col::new(10),
            rows: Height::new(4),
            cols: Width::new(6),
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
        },
        data: SpacerWidgetData {},
    };

    let original_layout = Layout {
        windows: vec![spacer],
        terminal_width: Some(240),
        terminal_height: Some(60),
        base_layout: Some("default".to_string()),
        theme: Some("classic".to_string()),
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    // Serialize to TOML
    let toml_str = toml::to_string_pretty(&original_layout).expect("Failed to serialize");

    // Deserialize back
    let restored_layout: Layout = toml::from_str(&toml_str).expect("Failed to deserialize");

    // Verify structure
    assert_eq!(restored_layout.windows.len(), 1);
    assert_eq!(restored_layout.terminal_width, Some(240));
    assert_eq!(restored_layout.terminal_height, Some(60));
    assert_eq!(restored_layout.base_layout, Some("default".to_string()));
    assert_eq!(restored_layout.theme, Some("classic".to_string()));

    // Verify spacer properties
    assert_eq!(restored_layout.windows[0].widget_type(), "spacer");
    assert_eq!(restored_layout.windows[0].name(), "spacer_2");
    let base = restored_layout.windows[0].base();
    assert_eq!(base.row.get(), 5);
    assert_eq!(base.col.get(), 10);
    assert_eq!(base.rows.get(), 4);
    assert_eq!(base.cols.get(), 6);
    assert!(!base.show_border);
    assert!(!base.show_title);
    assert!(!base.transparent_background);
    assert!(base.visibility.is_shown());
}

#[test]
fn test_multiple_spacers_toml_round_trip() {
    // RED: Layout with multiple spacers should preserve all of them
    let spacer1 = WindowDef::Spacer {
        base: WindowBase {
            name: "spacer_1".to_string(),
            row: Row::new(0),
            col: Col::new(0),
            rows: Height::new(2),
            cols: Width::new(5),
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
        },
        data: SpacerWidgetData {},
    };

    let spacer2 = WindowDef::Spacer {
        base: WindowBase {
            name: "spacer_2".to_string(),
            row: Row::new(10),
            col: Col::new(20),
            rows: Height::new(3),
            cols: Width::new(8),
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
        },
        data: SpacerWidgetData {},
    };

    let original_layout = Layout {
        windows: vec![spacer1, spacer2],
        terminal_width: Some(200),
        terminal_height: Some(50),
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    // Serialize and deserialize
    let toml_str = toml::to_string_pretty(&original_layout).expect("Failed to serialize");
    let restored_layout: Layout = toml::from_str(&toml_str).expect("Failed to deserialize");

    // Verify both spacers are present
    assert_eq!(restored_layout.windows.len(), 2);
    assert_eq!(restored_layout.windows[0].name(), "spacer_1");
    assert_eq!(restored_layout.windows[1].name(), "spacer_2");
    assert_eq!(restored_layout.windows[0].base().row.get(), 0);
    assert_eq!(restored_layout.windows[1].base().row.get(), 10);
}

#[test]
fn test_hidden_spacer_toml_round_trip() {
    // RED: Hidden spacers should persist in layout files
    let visible_spacer = WindowDef::Spacer {
        base: WindowBase {
            name: "spacer_1".to_string(),
            row: Row::new(0),
            col: Col::new(0),
            rows: Height::new(2),
            cols: Width::new(5),
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
        },
        data: SpacerWidgetData {},
    };

    let hidden_spacer = WindowDef::Spacer {
        base: WindowBase {
            name: "spacer_2".to_string(),
            row: Row::new(5),
            col: Col::new(10),
            rows: Height::new(2),
            cols: Width::new(5),
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
            visibility: crate::config::WindowVisibility::Hidden,
            binding: None,
            content_align: None, // Hidden!
            tts_speak: false,
            text_size: None,
            font_family: None,
            title_position: "top-left".to_string(),
        },
        data: SpacerWidgetData {},
    };

    let original_layout = Layout {
        windows: vec![visible_spacer, hidden_spacer],
        terminal_width: Some(200),
        terminal_height: Some(50),
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    // Serialize and deserialize
    let toml_str = toml::to_string_pretty(&original_layout).expect("Failed to serialize");
    let restored_layout: Layout = toml::from_str(&toml_str).expect("Failed to deserialize");

    // Verify both spacers are present, including hidden one
    assert_eq!(restored_layout.windows.len(), 2);
    assert_eq!(restored_layout.windows[0].name(), "spacer_1");
    assert_eq!(restored_layout.windows[1].name(), "spacer_2");

    // Verify visibility state is preserved
    assert!(restored_layout.windows[0].base().visibility.is_shown());
    assert!(!restored_layout.windows[1].base().visibility.is_shown());
}

#[test]
fn test_spacer_resize_scales_proportionally() {
    // RED: Spacers should scale proportionally during resize
    // Create layout: Widget A (0,0 10x10) - spacer (10,0 5x10) - Widget B (15,0 10x10)
    let widget_a = WindowDef::Text {
        base: WindowBase {
            name: "widget_a".to_string(),
            row: Row::new(0),
            col: Col::new(0),
            rows: Height::new(10),
            cols: Width::new(10),
            show_border: true,
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
        },
        data: TextWidgetData {
            streams: vec!["main".to_string()],
            buffer_size: 1000,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    };

    let spacer = WindowDef::Spacer {
        base: WindowBase {
            name: "spacer_1".to_string(),
            row: Row::new(0),
            col: Col::new(10),
            rows: Height::new(10),
            cols: Width::new(5),
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
        },
        data: SpacerWidgetData {},
    };

    let widget_b = WindowDef::Text {
        base: WindowBase {
            name: "widget_b".to_string(),
            row: Row::new(0),
            col: Col::new(15),
            rows: Height::new(10),
            cols: Width::new(10),
            show_border: true,
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
        },
        data: TextWidgetData {
            streams: vec!["main".to_string()],
            buffer_size: 1000,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    };

    let mut layout = Layout {
        windows: vec![widget_a, spacer, widget_b],
        terminal_width: Some(50),
        terminal_height: Some(20),
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    // Resize to 100x40 (2x scale)
    layout.scale_to_terminal_size(100, 40);

    // Verify spacer scaled proportionally
    let spacer_base = layout.windows[1].base();
    assert_eq!(spacer_base.col.get(), 20); // 10 * 2 = 20
    assert_eq!(spacer_base.cols.get(), 10); // 5 * 2 = 10
    assert_eq!(spacer_base.row.get(), 0); // 0 * 2 = 0
    assert_eq!(spacer_base.rows.get(), 20); // 10 * 2 = 20
}

#[test]
fn test_spacer_maintains_gap_after_resize() {
    // RED: Spacer should maintain gap between widgets after resize
    // Setup: Widget A at col 0 (10 wide), spacer at col 10 (5 wide), Widget B at col 15 (10 wide)
    let widget_a = WindowDef::Text {
        base: WindowBase {
            name: "a".to_string(),
            row: Row::new(0),
            col: Col::new(0),
            rows: Height::new(10),
            cols: Width::new(10),
            show_border: true,
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
        },
        data: TextWidgetData {
            streams: vec!["main".to_string()],
            buffer_size: 1000,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    };

    let spacer = WindowDef::Spacer {
        base: WindowBase {
            name: "spacer_1".to_string(),
            row: Row::new(0),
            col: Col::new(10),
            rows: Height::new(10),
            cols: Width::new(5),
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
        },
        data: SpacerWidgetData {},
    };

    let widget_b = WindowDef::Text {
        base: WindowBase {
            name: "b".to_string(),
            row: Row::new(0),
            col: Col::new(15),
            rows: Height::new(10),
            cols: Width::new(10),
            show_border: true,
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
        },
        data: TextWidgetData {
            streams: vec!["main".to_string()],
            buffer_size: 1000,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    };

    let mut layout = Layout {
        windows: vec![widget_a, spacer, widget_b],
        terminal_width: Some(50),
        terminal_height: Some(20),
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    // Verify gap before resize: A ends at 10, spacer starts at 10, B starts at 15
    assert_eq!(
        (layout.windows[0].base().col + layout.windows[0].base().cols).get(),
        10
    ); // A: 0+10
    assert_eq!(layout.windows[1].base().col.get(), 10); // Spacer starts at 10
    assert_eq!(layout.windows[2].base().col.get(), 15); // B starts at 15

    // Resize to 100x40 (2x scale)
    layout.scale_to_terminal_size(100, 40);

    // After resize: Gap should still exist, proportionally
    // A at 0, 20 wide -> ends at 20
    // Spacer at 20, 10 wide -> covers 20-30
    // B at 30, 20 wide -> starts at 30
    let a_end = layout.windows[0].base().col + layout.windows[0].base().cols;
    let spacer_start = layout.windows[1].base().col;
    let b_start = layout.windows[2].base().col;

    // Gap maintained: A-end == spacer-start, spacer-end == B-start
    assert_eq!(a_end, spacer_start);
    assert_eq!(spacer_start + layout.windows[1].base().cols, b_start);
}

#[test]
fn test_spacer_no_widget_collision_after_resize() {
    // RED: Spacers should prevent widget collisions after resize
    // Setup: Simple 2-widget layout separated by spacer
    let widget_a = WindowDef::Text {
        base: WindowBase {
            name: "main".to_string(),
            row: Row::new(0),
            col: Col::new(0),
            rows: Height::new(20),
            cols: Width::new(30),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: true,
            title: Some("Main".to_string()),
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
        data: TextWidgetData {
            streams: vec!["main".to_string()],
            buffer_size: 5000,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    };

    let spacer = WindowDef::Spacer {
        base: WindowBase {
            name: "spacer_1".to_string(),
            row: Row::new(0),
            col: Col::new(30),
            rows: Height::new(20),
            cols: Width::new(2),
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
        },
        data: SpacerWidgetData {},
    };

    let widget_b = WindowDef::Text {
        base: WindowBase {
            name: "status".to_string(),
            row: Row::new(0),
            col: Col::new(32),
            rows: Height::new(20),
            cols: Width::new(20),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: true,
            title: Some("Status".to_string()),
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
        data: TextWidgetData {
            streams: vec!["status".to_string()],
            buffer_size: 100,
            wordwrap: true,
            show_timestamps: false,
            timestamp_position: None,
            compact: false,
        },
    };

    let mut layout = Layout {
        windows: vec![widget_a, spacer, widget_b],
        terminal_width: Some(100),
        terminal_height: Some(25),
        base_layout: None,
        theme: None,
        unknown_windows: Vec::new(),
        deleted_windows: Vec::new(),
    };

    // Verify initial no overlap
    let a_end = layout.windows[0].base().col + layout.windows[0].base().cols;
    let spacer_start = layout.windows[1].base().col;
    assert_eq!(
        a_end, spacer_start,
        "Initial state: A should end where spacer starts"
    );

    // Resize to 200x50 (2x scale)
    layout.scale_to_terminal_size(200, 50);

    // Verify no collision after resize
    let a_end = layout.windows[0].base().col + layout.windows[0].base().cols;
    let spacer_start = layout.windows[1].base().col;
    let spacer_end = layout.windows[1].base().col + layout.windows[1].base().cols;
    let b_start = layout.windows[2].base().col;

    // Should maintain separation
    assert!(a_end <= spacer_start, "A should not overlap spacer");
    assert!(spacer_end <= b_start, "Spacer should not overlap B");
}
