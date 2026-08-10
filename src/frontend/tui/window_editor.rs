//! Modal editor for window definitions used by the TUI layout manager.
//!
//! Presents a VellumFE-inspired popup that lets the user tweak geometry,
//! borders, and stream assignments for a given window definition.
//!
mod construction;
mod input;
mod navigation;
mod render;
mod sync;

use crate::data::input::{KeyCode, KeyEvent as TfKeyEvent};
use crate::config::Config;
use crate::frontend::tui::crossterm_bridge;
use crate::frontend::tui::textarea_bridge;
use crate::config::{DashboardIndicatorDef, TabbedTextTab, WindowDef};
use crate::theme::EditorTheme;
use std::char;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Widget},
};
use tui_textarea::TextArea;

/// Available content alignment options (matches VellumFE)
const CONTENT_ALIGN_OPTIONS: &[&str] = &[
    "top-left",
    "top-center",
    "top-right",
    "center-left",
    "center",
    "center-right",
    "bottom-left",
    "bottom-center",
    "bottom-right",
];

const TITLE_POSITION_OPTIONS: &[&str] = &[
    "top-left",
    "top-center",
    "top-right",
    "bottom-left",
    "bottom-center",
    "bottom-right",
];

const SORT_DIRECTION_OPTIONS: &[&str] = &[
    "ascending",
    "descending",
];

/// Actions that can result from mouse interaction with the window editor
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowEditorMouseAction {
    /// No special action, just focus change or drag
    None,
    /// User clicked Save button
    Save,
    /// User clicked Cancel button
    Cancel,
}

/// Field reference for linear navigation/rendering
#[derive(Debug, Clone, Copy, PartialEq)]
enum FieldRef {
    // Text inputs
    Name,
    Title,
    Row,
    Col,
    Rows,
    Cols,
    MinRows,
    MinCols,
    MaxRows,
    MaxCols,
    BgColor,
    BorderColor,
    BorderStyle,
    Streams,
    BufferSize,
    Wordwrap,
    Timestamps,
    TitlePosition,
    TextColor,
    EntityId,
    PromptIcon,
    PromptIconColor,
    CursorColor,
    CursorBg,
    CompletionColor,
    ContentAlign,

    // Checkboxes
    ShowTitle,
    Locked,
    TtsSpeak,
    TransparentBg,
    ShowBorder,
    BorderTop,
    BorderBottom,
    BorderLeft,
    BorderRight,
    TabBarPosition,
    TabActiveColor,
    TabInactiveColor,
    TabUnreadColor,
    TabUnreadPrefix,
    TabSeparator,
    ShowDesc,
    ShowObjs,
    ShowPlayers,
    ShowExits,
    ShowName,
    ProgressId,
    ProgressColor,
    ProgressNumbersOnly,
    ProgressCurrentOnly,
    CountdownId,
    CountdownIcon,
    CountdownColor,
    CountdownBgColor,
    CompassActiveColor,
    CompassInactiveColor,
    InjuryDefaultColor,
    Injury1Color,
    Injury2Color,
    Injury3Color,
    Scar1Color,
    Scar2Color,
    Scar3Color,
    IndicatorId,
    IndicatorIcon,
    IndicatorActiveColor,
    IndicatorInactiveColor,
    HandIcon,
    HandIconColor,
    HandTextColor,
    ActiveEffectsCategory,
    EditTabs,
    EditIndicators,
    EditMetrics,
    DashboardLayout,
    DashboardSpacing,
    DashboardHideInactive,

    // Perception widget fields
    // Note: stream and buffer_size are NOT configurable - hardcoded internally
    PerceptionSortDirection,
    PerceptionTextReplacements,
    PerceptionUseShortSpellNames,

    // Encumbrance widget fields
    EncumShowLabel,
    EncumColorLight,
    EncumColorModerate,
    EncumColorHeavy,
    EncumColorCritical,
    // GS4Experience widget fields
    GS4ExpShowLevel,
    GS4ExpShowExpBar,
    GS4ExpShowMindBar,
    GS4ExpShowTotalExp,
    GS4ExpShowAscensionExp,
    GS4ExpMindBarColor,
    GS4ExpExpBarColor,
    // MiniVitals widget fields
    MiniVitalsNumbersOnly,
    MiniVitalsCurrentOnly,
    MiniVitalsHealthColor,
    MiniVitalsManaColor,
    MiniVitalsStaminaColor,
    MiniVitalsSpiritColor,
    MiniVitalsDepletedColor,
    MiniVitalsEditBarOrder,
    // Betrayer widget fields
    BetrayerShowItems,
    BetrayerBarColor,
    // Text widget compact mode
    TextCompact,
    // Targets widget show arms/body parts count
    TargetsShowAppendages,
    // Targets widget status position (start/end)
    TargetsStatusPosition,
}

impl FieldRef {
    /// Get the legacy field ID for this field (for compatibility with existing toggle/input logic)
    fn legacy_field_id(&self) -> usize {
        match self {
            FieldRef::Name => 0,
            FieldRef::Title => 1,
            FieldRef::Row => 2,
            FieldRef::Col => 3,
            FieldRef::Rows => 4,
            FieldRef::Cols => 5,
            FieldRef::MinRows => 6,
            FieldRef::MinCols => 7,
            FieldRef::MaxRows => 8,
            FieldRef::MaxCols => 9,
            FieldRef::BorderStyle => 11,
            FieldRef::ShowTitle => 12,
            FieldRef::Locked => 13,
            FieldRef::TtsSpeak => 121,
            FieldRef::TransparentBg => 14,
            FieldRef::ShowBorder => 15,
            FieldRef::BorderTop => 16,
            FieldRef::BorderBottom => 17,
            FieldRef::BorderLeft => 18,
            FieldRef::BorderRight => 19,
            FieldRef::BgColor => 20,
            FieldRef::BorderColor => 21,
            FieldRef::Streams => 22,
            FieldRef::TextColor => 23,
            FieldRef::CursorColor => 24,
            FieldRef::CursorBg => 25,
            FieldRef::ContentAlign => 26,
            FieldRef::TabBarPosition => 27,
            FieldRef::TabActiveColor => 28,
            FieldRef::TabInactiveColor => 29,
            FieldRef::TabUnreadColor => 30,
            FieldRef::TabUnreadPrefix => 31,
            FieldRef::ShowDesc => 32,
            FieldRef::ShowObjs => 33,
            FieldRef::ShowPlayers => 34,
            FieldRef::ShowExits => 35,
            FieldRef::ShowName => 36,
            FieldRef::ProgressId => 37,
            FieldRef::ProgressColor => 38,
            FieldRef::ProgressNumbersOnly => 39,
            FieldRef::ProgressCurrentOnly => 40,
            FieldRef::CountdownId => 41,
            FieldRef::CountdownIcon => 42,
            FieldRef::CountdownColor => 43,
            FieldRef::CountdownBgColor => 44,
            FieldRef::CompassActiveColor => 45,
            FieldRef::CompassInactiveColor => 46,
            FieldRef::InjuryDefaultColor => 47,
            FieldRef::Injury1Color => 48,
            FieldRef::Injury2Color => 49,
            FieldRef::Injury3Color => 50,
            FieldRef::Scar1Color => 51,
            FieldRef::Scar2Color => 52,
            FieldRef::Scar3Color => 53,
            FieldRef::ActiveEffectsCategory => 54,
            FieldRef::EditTabs => 55,
            FieldRef::EditIndicators => 56,
            FieldRef::EditMetrics => 117,
            FieldRef::DashboardLayout => 57,
            FieldRef::DashboardSpacing => 58,
            FieldRef::DashboardHideInactive => 59,
            FieldRef::BufferSize => 78,
            FieldRef::Wordwrap => 79,
            FieldRef::Timestamps => 80,
            FieldRef::TabSeparator => 81,
            FieldRef::TitlePosition => 82,
            FieldRef::PromptIcon => 83,
            FieldRef::PromptIconColor => 84,
            FieldRef::EntityId => 85,
            FieldRef::IndicatorIcon => 86,
            FieldRef::IndicatorActiveColor => 87,
            FieldRef::IndicatorInactiveColor => 88,
            FieldRef::IndicatorId => 92,
            FieldRef::HandIcon => 89,
            FieldRef::HandIconColor => 90,
            FieldRef::HandTextColor => 91,
            FieldRef::PerceptionSortDirection => 93,
            FieldRef::PerceptionTextReplacements => 94,
            FieldRef::PerceptionUseShortSpellNames => 95,
            FieldRef::EncumShowLabel => 96,
            FieldRef::EncumColorLight => 105,
            FieldRef::EncumColorModerate => 106,
            FieldRef::EncumColorHeavy => 107,
            FieldRef::EncumColorCritical => 108,
            FieldRef::GS4ExpShowLevel => 97,
            FieldRef::GS4ExpShowExpBar => 98,
            FieldRef::GS4ExpShowMindBar => 118,
            FieldRef::GS4ExpShowTotalExp => 119,
            FieldRef::GS4ExpShowAscensionExp => 120,
            FieldRef::GS4ExpMindBarColor => 109,
            FieldRef::GS4ExpExpBarColor => 110,
            FieldRef::MiniVitalsNumbersOnly => 99,
            FieldRef::MiniVitalsCurrentOnly => 100,
            FieldRef::MiniVitalsHealthColor => 101,
            FieldRef::MiniVitalsManaColor => 102,
            FieldRef::MiniVitalsStaminaColor => 103,
            FieldRef::MiniVitalsSpiritColor => 104,
            FieldRef::MiniVitalsDepletedColor => 122,
            FieldRef::MiniVitalsEditBarOrder => 115,
            FieldRef::BetrayerShowItems => 111,
            FieldRef::BetrayerBarColor => 112,
            FieldRef::TextCompact => 113,
            FieldRef::TargetsShowAppendages => 114,
            FieldRef::TargetsStatusPosition => 116,
            FieldRef::CompletionColor => 123,
        }
    }
}

#[derive(Clone, Debug)]
struct TabEditItem {
    name: String,
    streams: Vec<String>,
    show_timestamps: bool,
    ignore_activity: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabEditorMode {
    List,
    Form,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabEditorFormField {
    Name,
    Streams,
    Timestamps,
    IgnoreActivity,
}

#[derive(Clone, Debug)]
struct TabEditor {
    tabs: Vec<TabEditItem>,
    selected: usize,
    mode: TabEditorMode,
    form_field: TabEditorFormField,
    name_input: TextArea<'static>,
    streams_input: TextArea<'static>,
    show_timestamps: bool,
    ignore_activity: bool,
    editing_index: Option<usize>,
    /// Click areas for mouse support: (row_index, y, x, width)
    click_areas: Vec<(usize, u16, u16, u16)>,
}

impl TabEditor {
    fn from_tabs(tabs: &[TabbedTextTab]) -> Self {
        let mut items: Vec<TabEditItem> = tabs
            .iter()
            .map(|t| TabEditItem {
                name: t.name.clone(),
                streams: t.get_streams(),
                show_timestamps: t.show_timestamps.unwrap_or(false),
                ignore_activity: t.ignore_activity.unwrap_or(false),
            })
            .collect();

        if items.is_empty() {
            items.push(TabEditItem {
                name: "Main".to_string(),
                streams: vec!["main".to_string()],
                show_timestamps: false,
                ignore_activity: false,
            });
        }

        let mut name_input = WindowEditor::create_textarea();
        let mut streams_input = WindowEditor::create_textarea();
        name_input.insert_str(items[0].name.clone());
        streams_input.insert_str(items[0].streams.join(", "));
        let initial_ts = items.get(0).map(|t| t.show_timestamps).unwrap_or(false);
        let initial_ignore = items.get(0).map(|t| t.ignore_activity).unwrap_or(false);

        Self {
            tabs: items,
            selected: 0,
            mode: TabEditorMode::List,
            form_field: TabEditorFormField::Name,
            name_input,
            streams_input,
            show_timestamps: initial_ts,
            ignore_activity: initial_ignore,
            editing_index: None,
            click_areas: Vec::new(),
        }
    }

    /// Handle mouse click - returns true if click was handled
    fn handle_mouse_click(&mut self, col: u16, row: u16) -> bool {
        if self.mode != TabEditorMode::List {
            return false;
        }
        for &(idx, y, x, width) in &self.click_areas {
            if row == y && col >= x && col < x + width {
                self.selected = idx;
                return true;
            }
        }
        false
    }

    fn to_tabs(&self) -> Vec<TabbedTextTab> {
        self.tabs
            .iter()
            .map(|t| TabbedTextTab {
                name: t.name.clone(),
                stream: None,
                streams: t.streams.clone(),
                show_timestamps: Some(t.show_timestamps),
                ignore_activity: Some(t.ignore_activity),
                timestamp_position: None,
            })
            .collect()
    }

    fn start_add(&mut self) {
        self.mode = TabEditorMode::Form;
        self.form_field = TabEditorFormField::Name;
        self.editing_index = None;
        self.name_input = WindowEditor::create_textarea();
        self.streams_input = WindowEditor::create_textarea();
        self.show_timestamps = false;
        self.ignore_activity = false;
    }

    fn start_edit(&mut self) {
        if let Some(item) = self.tabs.get(self.selected).cloned() {
            self.mode = TabEditorMode::Form;
            self.form_field = TabEditorFormField::Name;
            self.editing_index = Some(self.selected);
            self.name_input = WindowEditor::create_textarea();
            self.streams_input = WindowEditor::create_textarea();
            self.name_input.insert_str(item.name);
            self.streams_input.insert_str(item.streams.join(", "));
            self.show_timestamps = item.show_timestamps;
            self.ignore_activity = item.ignore_activity;
        }
    }

    fn save_form(&mut self) {
        let name = self
            .name_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let streams: Vec<String> = self
            .streams_input
            .lines()
            .get(0)
            .map(|s| {
                s.split(',')
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect()
            })
            .unwrap_or_else(Vec::new);

        if name.is_empty() || streams.is_empty() {
            return;
        }

        let item = TabEditItem {
            name,
            streams,
            show_timestamps: self.show_timestamps,
            ignore_activity: self.ignore_activity,
        };

        if let Some(idx) = self.editing_index {
            if idx < self.tabs.len() {
                self.tabs[idx] = item;
                self.selected = idx;
            }
        } else {
            self.tabs.push(item);
            self.selected = self.tabs.len().saturating_sub(1);
        }

        self.mode = TabEditorMode::List;
        self.editing_index = None;
    }

    fn cancel_form(&mut self) {
        self.mode = TabEditorMode::List;
        self.editing_index = None;
    }

    fn delete_selected(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        if self.selected < self.tabs.len() {
            self.tabs.remove(self.selected);
            if self.selected >= self.tabs.len() {
                self.selected = self.tabs.len().saturating_sub(1);
            }
        }
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.tabs.swap(self.selected, self.selected - 1);
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.tabs.len() {
            self.tabs.swap(self.selected, self.selected + 1);
            self.selected += 1;
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Text Replacements Editor (for Perception widget)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct TextReplacementItem {
    pattern: String,
    replace: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextReplacementsEditorMode {
    List,
    Form,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextReplacementsFormField {
    Pattern,
    Replace,
}

#[derive(Clone, Debug)]
struct TextReplacementsEditor {
    replacements: Vec<TextReplacementItem>,
    selected: usize,
    mode: TextReplacementsEditorMode,
    form_field: TextReplacementsFormField,
    pattern_input: TextArea<'static>,
    replace_input: TextArea<'static>,
    editing_index: Option<usize>,
}

impl TextReplacementsEditor {
    fn from_replacements(replacements: &[crate::config::TextReplacement]) -> Self {
        let items: Vec<TextReplacementItem> = replacements
            .iter()
            .map(|r| TextReplacementItem {
                pattern: r.pattern.clone(),
                replace: r.replace.clone(),
            })
            .collect();

        let pattern_input = WindowEditor::create_textarea();
        let replace_input = WindowEditor::create_textarea();

        Self {
            replacements: items,
            selected: 0,
            mode: TextReplacementsEditorMode::List,
            form_field: TextReplacementsFormField::Pattern,
            pattern_input,
            replace_input,
            editing_index: None,
        }
    }

    fn to_replacements(&self) -> Vec<crate::config::TextReplacement> {
        self.replacements
            .iter()
            .map(|r| crate::config::TextReplacement {
                pattern: r.pattern.clone(),
                replace: r.replace.clone(),
            })
            .collect()
    }

    fn start_add(&mut self) {
        self.mode = TextReplacementsEditorMode::Form;
        self.form_field = TextReplacementsFormField::Pattern;
        self.editing_index = None;
        self.pattern_input = WindowEditor::create_textarea();
        self.replace_input = WindowEditor::create_textarea();
    }

    fn start_edit(&mut self) {
        if let Some(item) = self.replacements.get(self.selected).cloned() {
            self.mode = TextReplacementsEditorMode::Form;
            self.form_field = TextReplacementsFormField::Pattern;
            self.editing_index = Some(self.selected);
            self.pattern_input = WindowEditor::create_textarea();
            self.replace_input = WindowEditor::create_textarea();
            self.pattern_input.insert_str(&item.pattern);
            self.replace_input.insert_str(&item.replace);
        }
    }

    fn save_form(&mut self) {
        let pattern = self
            .pattern_input
            .lines()
            .get(0)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let replace = self
            .replace_input
            .lines()
            .get(0)
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Pattern is required, but replace can be empty (to remove text)
        if pattern.is_empty() {
            return;
        }

        let item = TextReplacementItem { pattern, replace };

        if let Some(idx) = self.editing_index {
            if idx < self.replacements.len() {
                self.replacements[idx] = item;
                self.selected = idx;
            }
        } else {
            self.replacements.push(item);
            self.selected = self.replacements.len().saturating_sub(1);
        }

        self.mode = TextReplacementsEditorMode::List;
        self.editing_index = None;
    }

    fn cancel_form(&mut self) {
        self.mode = TextReplacementsEditorMode::List;
        self.editing_index = None;
    }

    fn delete_selected(&mut self) {
        if self.selected < self.replacements.len() {
            self.replacements.remove(self.selected);
            if self.selected >= self.replacements.len() && self.selected > 0 {
                self.selected = self.replacements.len().saturating_sub(1);
            }
        }
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.replacements.swap(self.selected, self.selected - 1);
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.replacements.len() {
            self.replacements.swap(self.selected, self.selected + 1);
            self.selected += 1;
        }
    }
}

#[derive(Clone, Debug)]
struct IndicatorItem {
    id: String,
    icon: String,
    colors: Vec<String>,
    /// Layer-stack group (GUI-only feature); preserved untouched on TUI edits.
    stack: String,
    enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndicatorEditorMode {
    List,
    Form,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndicatorFormField {
    Id,
    Icon,
    Colors,
}

#[derive(Clone, Debug)]
struct IndicatorEditor {
    indicators: Vec<IndicatorItem>,
    available: Vec<IndicatorItem>,
    selected: usize,
    mode: IndicatorEditorMode,
    form_field: IndicatorFormField,
    id_input: TextArea<'static>,
    icon_input: TextArea<'static>,
    colors_input: TextArea<'static>,
    editing_index: Option<usize>,
    /// Click areas for mouse support: (row_index, y, x, width)
    click_areas: Vec<(usize, u16, u16, u16)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PerfMetricGroup {
    FrameTiming,
    RenderPipeline,
    Network,
    Parser,
    Events,
    Memory,
    UptimeLines,
    Diagnostics,
}

impl PerfMetricGroup {
    fn label(&self) -> &'static str {
        match self {
            PerfMetricGroup::FrameTiming => "Draw cadence (draws/sec)",
            PerfMetricGroup::RenderPipeline => "Render pipeline (render/draw/wrap)",
            PerfMetricGroup::Network => "Network",
            PerfMetricGroup::Parser => "Parser",
            PerfMetricGroup::Events => "Events",
            PerfMetricGroup::Memory => "CPU & memory",
            PerfMetricGroup::UptimeLines => "Uptime & lines/windows",
            PerfMetricGroup::Diagnostics => "Diagnostics (spikes/window costs)",
        }
    }
}

#[derive(Clone, Debug)]
struct PerfMetricGroupState {
    group: PerfMetricGroup,
    enabled: bool,
}

#[derive(Clone, Debug)]
struct PerformanceMetricsEditor {
    items: Vec<PerfMetricGroupState>,
    selected: usize,
}

impl PerformanceMetricsEditor {
    fn new(items: Vec<PerfMetricGroupState>) -> Self {
        Self { items, selected: 0 }
    }

    fn toggle_selected(&mut self) {
        if let Some(item) = self.items.get_mut(self.selected) {
            item.enabled = !item.enabled;
        }
    }

    fn move_selection(&mut self, down: bool) {
        if self.items.is_empty() {
            return;
        }
        if down {
            self.selected = (self.selected + 1) % self.items.len();
        } else if self.selected == 0 {
            self.selected = self.items.len() - 1;
        } else {
            self.selected -= 1;
        }
    }
}

/// Modal list of stream ids Lich has pushed this session, opened from the
/// Streams field so the user can subscribe a window to a stream without typing
/// its id. Selecting a row appends the id to the Streams field. Parity with the
/// GUI custom-windows "seen this session" picker.
#[derive(Clone, Debug)]
struct StreamPicker {
    /// (stream id, optional friendly label), sorted by id — the snapshot taken
    /// when the picker was opened.
    streams: Vec<(String, Option<String>)>,
    selected: usize,
    /// Click areas for mouse support: (row_index, y, x, width).
    click_areas: Vec<(usize, u16, u16, u16)>,
}

impl StreamPicker {
    fn new(streams: Vec<(String, Option<String>)>) -> Self {
        Self {
            streams,
            selected: 0,
            click_areas: Vec::new(),
        }
    }

    fn move_selection(&mut self, down: bool) {
        if self.streams.is_empty() {
            return;
        }
        if down {
            self.selected = (self.selected + 1) % self.streams.len();
        } else if self.selected == 0 {
            self.selected = self.streams.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    /// The id of the currently highlighted row, if any.
    fn selected_id(&self) -> Option<&str> {
        self.streams.get(self.selected).map(|(id, _)| id.as_str())
    }
}

impl IndicatorEditor {
    fn from_defs(defs: &[DashboardIndicatorDef], available: Vec<IndicatorItem>) -> Self {
        // Merge available templates with current defs; mark enabled when present in defs
        use std::collections::{HashMap, HashSet};

        let def_map: HashMap<String, &DashboardIndicatorDef> =
            defs.iter().map(|d| (d.id.to_lowercase(), d)).collect();

        // Start with available templates
        let mut items: Vec<IndicatorItem> = available
            .iter()
            .map(|tpl| {
                if let Some(def) = def_map.get(&tpl.id.to_lowercase()) {
                    IndicatorItem {
                        id: tpl.id.clone(),
                        icon: if !def.icon.is_empty() {
                            def.icon.clone()
                        } else {
                            tpl.icon.clone()
                        },
                        colors: if def.colors.is_empty() {
                            tpl.colors.clone()
                        } else {
                            def.colors.clone()
                        },
                        stack: def.stack.clone(),
                        enabled: true,
                    }
                } else {
                    IndicatorItem {
                        id: tpl.id.clone(),
                        icon: tpl.icon.clone(),
                        colors: tpl.colors.clone(),
                        stack: String::new(),
                        enabled: false,
                    }
                }
            })
            .collect();

        // Add any defs not in available list so we don't drop custom ones
        let seen: HashSet<String> = items.iter().map(|i| i.id.to_lowercase()).collect();
        for def in defs {
            if !seen.contains(&def.id.to_lowercase()) {
                items.push(IndicatorItem {
                    id: def.id.clone(),
                    icon: def.icon.clone(),
                    colors: def.colors.clone(),
                    stack: def.stack.clone(),
                    enabled: true,
                });
            }
        }

        let mut id_input = WindowEditor::create_textarea();
        let mut icon_input = WindowEditor::create_textarea();
        let mut colors_input = WindowEditor::create_textarea();
        if let Some(first) = items.first() {
            id_input.insert_str(first.id.clone());
            icon_input.insert_str(first.icon.clone());
            colors_input.insert_str(first.colors.join(", "));
        }

        Self {
            indicators: items,
            available,
            selected: 0,
            mode: IndicatorEditorMode::List,
            form_field: IndicatorFormField::Id,
            id_input,
            icon_input,
            colors_input,
            editing_index: None,
            click_areas: Vec::new(),
        }
    }

    /// Handle mouse click - returns true if click was handled
    fn handle_mouse_click(&mut self, col: u16, row: u16) -> bool {
        if self.mode != IndicatorEditorMode::List {
            return false;
        }
        for &(idx, y, x, width) in &self.click_areas {
            if row == y && col >= x && col < x + width {
                self.selected = idx;
                // Toggle the indicator
                if let Some(ind) = self.indicators.get_mut(idx) {
                    ind.enabled = !ind.enabled;
                }
                return true;
            }
        }
        false
    }

    fn to_defs(&self) -> Vec<DashboardIndicatorDef> {
        self.indicators
            .iter()
            .filter(|ind| ind.enabled)
            .map(|ind| DashboardIndicatorDef {
                id: ind.id.clone(),
                icon: ind.icon.clone(),
                colors: ind.colors.clone(),
                stack: ind.stack.clone(),
            })
            .collect()
    }

    fn start_add(&mut self) {
        // Find first available indicator not already in the list
        let used: std::collections::HashSet<String> = self
            .indicators
            .iter()
            .map(|i| i.id.to_lowercase())
            .collect();
        if let Some(candidate) = self
            .available
            .iter()
            .find(|i| !used.contains(&i.id.to_lowercase()))
            .cloned()
        {
            self.mode = IndicatorEditorMode::Form;
            self.form_field = IndicatorFormField::Id;
            self.editing_index = None;
            self.id_input = WindowEditor::create_textarea();
            self.icon_input = WindowEditor::create_textarea();
            self.colors_input = WindowEditor::create_textarea();
            self.id_input.insert_str(candidate.id);
            self.icon_input.insert_str(candidate.icon);
            self.colors_input.insert_str(candidate.colors.join(", "));
            return;
        }

        self.mode = IndicatorEditorMode::Form;
        self.form_field = IndicatorFormField::Id;
        self.editing_index = None;
        self.id_input = WindowEditor::create_textarea();
        self.icon_input = WindowEditor::create_textarea();
        self.colors_input = WindowEditor::create_textarea();
        self.colors_input
            .insert_str("#000000, #ffffff".to_string());
    }

    fn start_edit(&mut self) {
        if let Some(item) = self.indicators.get(self.selected).cloned() {
            self.mode = IndicatorEditorMode::Form;
            self.form_field = IndicatorFormField::Id;
            self.editing_index = Some(self.selected);
            self.id_input = WindowEditor::create_textarea();
            self.icon_input = WindowEditor::create_textarea();
            self.colors_input = WindowEditor::create_textarea();
            self.id_input.insert_str(item.id);
            self.icon_input.insert_str(item.icon);
            self.colors_input.insert_str(item.colors.join(", "));
        }
    }

    fn save_form(&mut self) {
        let id_raw = self
            .id_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if id_raw.is_empty() {
            return;
        }

        // Only allow ids that exist in available indicators
        let available = match self
            .available
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(&id_raw))
            .cloned()
        {
            Some(a) => a,
            None => return,
        };

        // Prevent duplicates when adding
        if let Some(edit_idx) = self.editing_index {
            if self
                .indicators
                .iter()
                .enumerate()
                .any(|(idx, i)| idx != edit_idx && i.id.eq_ignore_ascii_case(&available.id))
            {
                return;
            }
        } else if self
            .indicators
            .iter()
            .any(|i| i.id.eq_ignore_ascii_case(&available.id))
        {
            return;
        }

        let item = IndicatorItem {
            id: available.id,
            icon: available.icon,
            colors: available.colors,
            stack: available.stack,
            enabled: true,
        };

        if let Some(idx) = self.editing_index {
            if idx < self.indicators.len() {
                self.indicators[idx] = item;
                self.selected = idx;
            }
        } else {
            self.indicators.push(item);
            self.selected = self.indicators.len().saturating_sub(1);
        }

        self.mode = IndicatorEditorMode::List;
        self.editing_index = None;
    }

    fn cancel_form(&mut self) {
        self.mode = IndicatorEditorMode::List;
        self.editing_index = None;
    }

    fn delete_selected(&mut self) {
        if self.indicators.is_empty() {
            return;
        }
        if self.selected < self.indicators.len() {
            self.indicators.remove(self.selected);
            if self.selected >= self.indicators.len() {
                self.selected = self.indicators.len().saturating_sub(1);
            }
        }
    }

    fn toggle_selected(&mut self) {
        if let Some(item) = self.indicators.get_mut(self.selected) {
            item.enabled = !item.enabled;
        }
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.indicators.swap(self.selected, self.selected - 1);
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.indicators.len() {
            self.indicators.swap(self.selected, self.selected + 1);
            self.selected += 1;
        }
    }
}

/// Bar order item for MiniVitals editor
#[derive(Clone, Debug)]
struct BarOrderItem {
    id: String,           // "health", "mana", "stamina", "spirit", "concentration"
    label: String,        // Display name
    enabled: bool,        // Whether this bar is shown
    color: Option<String>, // Custom color for the bar
}

/// Bar order editor focus - which column is active
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarOrderEditorFocus {
    Toggle, // Focus on the checkbox/toggle column
    Color,  // Focus on the color input column
}

/// Bar order editor for MiniVitals widget - supports 5 bars with colors
/// Two-column layout: toggles on left, color previews + inputs on right
#[derive(Clone, Debug)]
struct BarOrderEditor {
    bars: Vec<BarOrderItem>,
    selected: usize,
    focus: BarOrderEditorFocus,
    color_input: TextArea<'static>,
    /// Click areas for mouse support: (row_index, toggle_rect, color_rect)
    click_areas: Vec<(usize, (u16, u16, u16, u16), (u16, u16, u16, u16))>,
}

impl BarOrderEditor {
    const MAX_ENABLED: usize = 4; // Maximum bars that can be enabled at once

    fn from_minivitals_data(data: &crate::config::MiniVitalsWidgetData) -> Self {
        // All possible bars (5 total)
        let all_bars = ["health", "mana", "stamina", "spirit", "concentration"];
        let mut bars: Vec<BarOrderItem> = Vec::new();

        // First, add bars in the order they appear in bar_order (these are enabled)
        for bar_id in &data.bar_order {
            let id = bar_id.to_lowercase();
            if all_bars.contains(&id.as_str()) {
                bars.push(BarOrderItem {
                    id: id.clone(),
                    label: Self::id_to_label(&id),
                    enabled: true,
                    color: Self::get_color_for_id(&id, data),
                });
            }
        }

        // Then add any remaining bars that aren't in bar_order (these are disabled)
        for bar_id in all_bars {
            if !bars.iter().any(|b| b.id == bar_id) {
                bars.push(BarOrderItem {
                    id: bar_id.to_string(),
                    label: Self::id_to_label(bar_id),
                    enabled: false,
                    color: Self::get_color_for_id(bar_id, data),
                });
            }
        }

        // Initialize color input with first bar's color
        let mut color_input = WindowEditor::create_textarea();
        if let Some(first_bar) = bars.first() {
            if let Some(ref color) = first_bar.color {
                color_input.insert_str(color.clone());
            } else {
                color_input.insert_str(Self::default_color_for_id(&first_bar.id));
            }
        }

        Self {
            bars,
            selected: 0,
            focus: BarOrderEditorFocus::Toggle,
            color_input,
            click_areas: Vec::new(),
        }
    }

    fn get_color_for_id(id: &str, data: &crate::config::MiniVitalsWidgetData) -> Option<String> {
        match id {
            "health" => data.health_color.clone(),
            "mana" => data.mana_color.clone(),
            "stamina" => data.stamina_color.clone(),
            "spirit" => data.spirit_color.clone(),
            "concentration" => data.concentration_color.clone(),
            _ => None,
        }
    }

    fn default_color_for_id(id: &str) -> &'static str {
        match id {
            "health" => "red",
            "mana" => "blue",
            "stamina" => "yellow",
            "spirit" => "magenta",
            "concentration" => "cyan",
            _ => "white",
        }
    }

    fn id_to_label(id: &str) -> String {
        match id {
            "health" => "Health".to_string(),
            "mana" => "Mana".to_string(),
            "stamina" => "Stamina".to_string(),
            "spirit" => "Spirit".to_string(),
            "concentration" => "Concentration".to_string(),
            _ => id.to_string(),
        }
    }

    fn to_bar_order(&self) -> Vec<String> {
        self.bars
            .iter()
            .filter(|b| b.enabled)
            .map(|b| b.id.clone())
            .collect()
    }

    fn apply_colors_to_data(&self, data: &mut crate::config::MiniVitalsWidgetData) {
        for bar in &self.bars {
            match bar.id.as_str() {
                "health" => data.health_color = bar.color.clone(),
                "mana" => data.mana_color = bar.color.clone(),
                "stamina" => data.stamina_color = bar.color.clone(),
                "spirit" => data.spirit_color = bar.color.clone(),
                "concentration" => data.concentration_color = bar.color.clone(),
                _ => {}
            }
        }
    }

    fn enabled_count(&self) -> usize {
        self.bars.iter().filter(|b| b.enabled).count()
    }

    fn toggle_selected(&mut self) -> bool {
        // Pre-compute enabled count before borrowing mutably
        let current_count = self.enabled_count();
        if let Some(bar) = self.bars.get_mut(self.selected) {
            if bar.enabled {
                // Always allow disabling
                bar.enabled = false;
                true
            } else if current_count < Self::MAX_ENABLED {
                // Only enable if we haven't hit the limit
                bar.enabled = true;
                true
            } else {
                // Can't enable - at max
                false
            }
        } else {
            false
        }
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.bars.swap(self.selected, self.selected - 1);
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.bars.len() {
            self.bars.swap(self.selected, self.selected + 1);
            self.selected += 1;
        }
    }

    fn nav_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = self.bars.len().saturating_sub(1);
        }
    }

    fn nav_down(&mut self) {
        if self.selected + 1 < self.bars.len() {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }

    /// Switch focus to color column
    fn focus_color(&mut self) {
        self.focus = BarOrderEditorFocus::Color;
        self.sync_color_input_from_bar();
    }

    /// Switch focus to toggle column
    fn focus_toggle(&mut self) {
        // Save color before switching
        self.save_color_to_bar();
        self.focus = BarOrderEditorFocus::Toggle;
    }

    /// Sync the color_input textarea with the selected bar's current color
    fn sync_color_input_from_bar(&mut self) {
        self.color_input = WindowEditor::create_textarea();
        if let Some(bar) = self.bars.get(self.selected) {
            let color_str = bar.color.as_deref()
                .unwrap_or_else(|| Self::default_color_for_id(&bar.id));
            self.color_input.insert_str(color_str);
        }
    }

    /// Save the current color_input value to the selected bar
    fn save_color_to_bar(&mut self) {
        if let Some(bar) = self.bars.get_mut(self.selected) {
            let input = self.color_input.lines().join("");
            let trimmed = input.trim();
            if trimmed.is_empty() || trimmed == Self::default_color_for_id(&bar.id) {
                bar.color = None; // Use default
            } else {
                bar.color = Some(trimmed.to_string());
            }
        }
    }

    /// Check if we're editing a color (focus on color column)
    fn is_editing_color(&self) -> bool {
        self.focus == BarOrderEditorFocus::Color
    }

    /// Handle mouse click - returns true if click was handled
    fn handle_mouse_click(&mut self, col: u16, row: u16) -> bool {
        // First, find which area was clicked (avoid borrowing issues)
        enum ClickResult {
            Toggle(usize),
            Color(usize),
            None,
        }

        let result = {
            let mut found = ClickResult::None;
            for (bar_idx, toggle_rect, color_rect) in &self.click_areas {
                let (tx, ty, tw, th) = *toggle_rect;
                let (cx, cy, cw, ch) = *color_rect;

                // Check toggle area click
                if col >= tx && col < tx + tw && row >= ty && row < ty + th {
                    found = ClickResult::Toggle(*bar_idx);
                    break;
                }

                // Check color area click
                if col >= cx && col < cx + cw && row >= cy && row < cy + ch {
                    found = ClickResult::Color(*bar_idx);
                    break;
                }
            }
            found
        };

        // Now act on the result
        match result {
            ClickResult::Toggle(bar_idx) => {
                // Save any pending color edit
                if self.is_editing_color() {
                    self.save_color_to_bar();
                }
                self.selected = bar_idx;
                self.focus = BarOrderEditorFocus::Toggle;
                self.sync_color_input_from_bar();
                // Toggle the bar
                self.toggle_selected();
                true
            }
            ClickResult::Color(bar_idx) => {
                // Save any pending color edit if switching bars
                let was_editing_color = self.is_editing_color();
                let old_selected = self.selected;
                if was_editing_color && old_selected != bar_idx {
                    self.save_color_to_bar();
                }
                self.selected = bar_idx;
                self.focus = BarOrderEditorFocus::Color;
                if old_selected != bar_idx || !was_editing_color {
                    self.sync_color_input_from_bar();
                }
                true
            }
            ClickResult::None => false,
        }
    }

}

/// Window editor widget - 70x20 popup with single-page layout
pub struct WindowEditor {
    popup_x: u16,
    popup_y: u16,
    popup_width: u16,
    popup_height: u16,
    dragging: bool,
    drag_offset_x: u16,
    drag_offset_y: u16,
    // Linear navigation over fields
    field_order: Vec<FieldRef>,
    current_field_index: usize,
    pub focused_field: usize, // Legacy field index (for compatibility with existing input handling)

    // Text inputs
    name_input: TextArea<'static>,
    title_input: TextArea<'static>,
    row_input: TextArea<'static>,
    col_input: TextArea<'static>,
    rows_input: TextArea<'static>,
    cols_input: TextArea<'static>,
    min_rows_input: TextArea<'static>,
    min_cols_input: TextArea<'static>,
    max_rows_input: TextArea<'static>,
    max_cols_input: TextArea<'static>,
    bg_color_input: TextArea<'static>,
    border_color_input: TextArea<'static>,
    streams_input: TextArea<'static>,
    buffer_size_input: TextArea<'static>,
    text_wordwrap: bool,
    text_show_timestamps: bool,
    entity_id_input: TextArea<'static>,
    text_color_input: TextArea<'static>,
    prompt_icon_input: TextArea<'static>,
    prompt_icon_color_input: TextArea<'static>,
    cursor_color_input: TextArea<'static>,
    cursor_bg_input: TextArea<'static>,
    completion_color_input: TextArea<'static>,
    content_align_input: TextArea<'static>,
    tab_bar_position_input: TextArea<'static>,
    title_position_input: TextArea<'static>,
    tab_active_color_input: TextArea<'static>,
    tab_inactive_color_input: TextArea<'static>,
    tab_unread_color_input: TextArea<'static>,
    tab_unread_prefix_input: TextArea<'static>,
    tab_separator: bool,
    progress_id_input: TextArea<'static>,
    progress_color_input: TextArea<'static>,
    progress_numbers_only: bool,
    progress_current_only: bool,
    countdown_icon_input: TextArea<'static>,
    countdown_color_input: TextArea<'static>,
    countdown_bg_color_input: TextArea<'static>,
    countdown_id_input: TextArea<'static>,
    compass_active_color_input: TextArea<'static>,
    compass_inactive_color_input: TextArea<'static>,
    injury_default_color_input: TextArea<'static>,
    injury1_color_input: TextArea<'static>,
    injury2_color_input: TextArea<'static>,
    injury3_color_input: TextArea<'static>,
    scar1_color_input: TextArea<'static>,
    scar2_color_input: TextArea<'static>,
    scar3_color_input: TextArea<'static>,
    indicator_id_input: TextArea<'static>,
    indicator_icon_input: TextArea<'static>,
    indicator_active_color_input: TextArea<'static>,
    indicator_inactive_color_input: TextArea<'static>,
    active_effects_category_input: TextArea<'static>,
    hand_icon_input: TextArea<'static>,
    hand_icon_color_input: TextArea<'static>,
    hand_text_color_input: TextArea<'static>,
    dashboard_layout_input: TextArea<'static>,
    dashboard_spacing_input: TextArea<'static>,
    dashboard_hide_inactive: bool,
    perf_enabled: bool,
    show_desc: bool,
    show_objs: bool,
    show_players: bool,
    show_exits: bool,
    show_name: bool,
    perf_show_fps: bool,
    perf_show_render_times: bool,
    perf_show_ui_times: bool,
    perf_show_wrap_times: bool,
    perf_show_net: bool,
    perf_show_parse: bool,
    perf_show_events: bool,
    perf_show_cpu: bool,
    perf_show_memory: bool,
    perf_show_lines: bool,
    perf_show_uptime: bool,
    perf_show_spike_log: bool,
    perf_show_per_window: bool,
    perf_sparklines: bool,
    available_indicators: Vec<IndicatorItem>,

    // Perception widget (stream and buffer_size hardcoded, only sort_direction configurable)
    perception_sort_direction_input: TextArea<'static>,
    perception_use_short_spell_names: bool,

    // Encumbrance widget
    show_label_encum: bool,
    encum_color_light_input: TextArea<'static>,
    encum_color_moderate_input: TextArea<'static>,
    encum_color_heavy_input: TextArea<'static>,
    encum_color_critical_input: TextArea<'static>,

    // GS4Experience widget
    gs4_exp_show_level: bool,
    gs4_exp_show_exp_bar: bool,
    gs4_exp_show_mind_bar: bool,
    gs4_exp_show_total_exp: bool,
    gs4_exp_show_ascension_exp: bool,
    gs4_exp_mind_bar_color_input: TextArea<'static>,
    gs4_exp_exp_bar_color_input: TextArea<'static>,

    // MiniVitals widget
    minivitals_numbers_only: bool,
    minivitals_current_only: bool,
    minivitals_health_color_input: TextArea<'static>,
    minivitals_mana_color_input: TextArea<'static>,
    minivitals_stamina_color_input: TextArea<'static>,
    minivitals_spirit_color_input: TextArea<'static>,
    minivitals_depleted_color_input: TextArea<'static>,

    // Betrayer widget
    betrayer_show_items: bool,
    betrayer_bar_color_input: TextArea<'static>,

    // Text widget compact mode
    text_compact: bool,

    // Targets widget show arms count
    targets_show_arms_count: bool,
    // Targets widget status position ("start" or "end")
    targets_status_position: String,

    window_def: WindowDef,
    original_window_def: WindowDef,
    is_new: bool,
    status_message: String,
    tab_editor: Option<TabEditor>,
    indicator_editor: Option<IndicatorEditor>,
    performance_metrics_editor: Option<PerformanceMetricsEditor>,
    text_replacements_editor: Option<TextReplacementsEditor>,
    bar_order_editor: Option<BarOrderEditor>,
    /// Modal picker over streams Lich has sent this session (opened from the
    /// Streams field). `None` when not active.
    stream_picker: Option<StreamPicker>,
    /// Snapshot of streams seen this session, seeded by the caller (which has
    /// AppCore in scope) since the editor holds no app reference — mirrors how
    /// `available_indicators` is populated.
    seen_streams: Vec<(String, Option<String>)>,
    /// Stores (y_position, field_ref) for click-to-select
    field_click_areas: Vec<(u16, u16, FieldRef)>, // (y, x_start, field)
}

impl WindowEditor {

}

#[cfg(test)]
mod tests;
