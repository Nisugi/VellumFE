//! Modal editor for window definitions used by the TUI layout manager.
//!
//! Presents a VellumFE-inspired popup that lets the user tweak geometry,
//! borders, and stream assignments for a given window definition.
//!
use crate::frontend::common::{KeyCode, KeyEvent as TfKeyEvent};
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
    ContentAlign,

    // Checkboxes
    ShowTitle,
    Locked,
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
    GS4ExpMindBarColor,
    GS4ExpExpBarColor,
    // MiniVitals widget fields
    MiniVitalsNumbersOnly,
    MiniVitalsCurrentOnly,
    MiniVitalsHealthColor,
    MiniVitalsManaColor,
    MiniVitalsStaminaColor,
    MiniVitalsSpiritColor,
    MiniVitalsEditBarOrder,
    // Betrayer widget fields
    BetrayerShowItems,
    BetrayerBarColor,
    // Text widget compact mode
    TextCompact,
    // Targets widget show arms/body parts count
    TargetsShowArmsCount,
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
            FieldRef::GS4ExpMindBarColor => 109,
            FieldRef::GS4ExpExpBarColor => 110,
            FieldRef::MiniVitalsNumbersOnly => 99,
            FieldRef::MiniVitalsCurrentOnly => 100,
            FieldRef::MiniVitalsHealthColor => 101,
            FieldRef::MiniVitalsManaColor => 102,
            FieldRef::MiniVitalsStaminaColor => 103,
            FieldRef::MiniVitalsSpiritColor => 104,
            FieldRef::MiniVitalsEditBarOrder => 115,
            FieldRef::BetrayerShowItems => 111,
            FieldRef::BetrayerBarColor => 112,
            FieldRef::TextCompact => 113,
            FieldRef::TargetsShowArmsCount => 114,
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
        }
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
}

impl PerfMetricGroup {
    fn label(&self) -> &'static str {
        match self {
            PerfMetricGroup::FrameTiming => "Frame timing (FPS/jitter/spikes)",
            PerfMetricGroup::RenderPipeline => "Render pipeline (render/UI/wrap)",
            PerfMetricGroup::Network => "Network",
            PerfMetricGroup::Parser => "Parser",
            PerfMetricGroup::Events => "Events",
            PerfMetricGroup::Memory => "Memory",
            PerfMetricGroup::UptimeLines => "Uptime & lines/windows",
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
                        enabled: true,
                    }
                } else {
                    IndicatorItem {
                        id: tpl.id.clone(),
                        icon: tpl.icon.clone(),
                        colors: tpl.colors.clone(),
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
        }
    }

    fn to_defs(&self) -> Vec<DashboardIndicatorDef> {
        self.indicators
            .iter()
            .filter(|ind| ind.enabled)
            .map(|ind| DashboardIndicatorDef {
                id: ind.id.clone(),
                icon: ind.icon.clone(),
                colors: ind.colors.clone(),
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

    /// Change selection - saves color if in color mode, then syncs new bar's color
    fn change_selection(&mut self, new_selected: usize) {
        if new_selected != self.selected && new_selected < self.bars.len() {
            // Save current color if editing
            if self.is_editing_color() {
                self.save_color_to_bar();
            }
            self.selected = new_selected;
            self.sync_color_input_from_bar();
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
    perf_show_frame_times: bool,
    perf_show_render_times: bool,
    perf_show_ui_times: bool,
    perf_show_wrap_times: bool,
    perf_show_net: bool,
    perf_show_parse: bool,
    perf_show_events: bool,
    perf_show_memory: bool,
    perf_show_lines: bool,
    perf_show_uptime: bool,
    perf_show_jitter: bool,
    perf_show_frame_spikes: bool,
    perf_show_event_lag: bool,
    perf_show_memory_delta: bool,
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
    gs4_exp_mind_bar_color_input: TextArea<'static>,
    gs4_exp_exp_bar_color_input: TextArea<'static>,

    // MiniVitals widget
    minivitals_numbers_only: bool,
    minivitals_current_only: bool,
    minivitals_health_color_input: TextArea<'static>,
    minivitals_mana_color_input: TextArea<'static>,
    minivitals_stamina_color_input: TextArea<'static>,
    minivitals_spirit_color_input: TextArea<'static>,

    // Betrayer widget
    betrayer_show_items: bool,
    betrayer_bar_color_input: TextArea<'static>,

    // Text widget compact mode
    text_compact: bool,

    // Targets widget show arms count
    targets_show_arms_count: bool,

    window_def: WindowDef,
    original_window_def: WindowDef,
    is_new: bool,
    status_message: String,
    tab_editor: Option<TabEditor>,
    indicator_editor: Option<IndicatorEditor>,
    performance_metrics_editor: Option<PerformanceMetricsEditor>,
    text_replacements_editor: Option<TextReplacementsEditor>,
    bar_order_editor: Option<BarOrderEditor>,
    /// Stores (y_position, field_ref) for click-to-select
    field_click_areas: Vec<(u16, u16, FieldRef)>, // (y, x_start, field)
}

impl WindowEditor {
    /// Set the window name input and underlying WindowDef name.
    pub fn set_name(&mut self, name: &str) {
        self.name_input = Self::create_textarea();
        self.name_input.insert_str(name);
        self.window_def.base_mut().name = name.to_string();
    }

    fn create_textarea() -> TextArea<'static> {
        let mut ta = TextArea::default();
        ta.set_cursor_line_style(Style::default());
        ta.set_max_histories(0);
        ta
    }

    fn indicator_templates() -> Vec<IndicatorItem> {
        let mut templates = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for template_name in Config::list_window_templates() {
            if let Some(crate::config::WindowDef::Indicator { data, .. }) =
                Config::get_window_template(&template_name)
            {
                let id = data
                    .indicator_id
                    .clone()
                    .unwrap_or_else(|| template_name.to_string());
                let key = id.to_lowercase();
                if seen.contains(&key) {
                    continue;
                }

                let icon = data.icon.unwrap_or_default();
                let inactive = data
                    .inactive_color
                    .unwrap_or_else(|| "#555555".to_string());
                let active = data.active_color.unwrap_or_else(|| "#00ff00".to_string());

                seen.insert(key);
                templates.push(IndicatorItem {
                    id,
                    icon,
                    colors: vec![inactive, active],
                    enabled: false,
                });
            }
        }

        templates.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
        templates
    }

    fn indicators_from_layout(layout: &crate::config::Layout) -> Vec<IndicatorItem> {
        // Start with all templates (disabled by default)
        let mut items = Self::indicator_templates();
        let mut index: std::collections::HashMap<String, usize> = items
            .iter()
            .enumerate()
            .map(|(idx, ind)| (ind.id.to_lowercase(), idx))
            .collect();

        for window in &layout.windows {
            if let crate::config::WindowDef::Indicator { data, .. } = window {
                let id = data
                    .indicator_id
                    .clone()
                    .unwrap_or_else(|| window.name().to_string());
                let icon = data.icon.clone().unwrap_or_default();
                let inactive = data
                    .inactive_color
                    .clone()
                    .unwrap_or_else(|| "#555555".to_string());
                let active = data
                    .active_color
                    .clone()
                    .unwrap_or_else(|| "#00ff00".to_string());
                let key = id.to_lowercase();
                if let Some(idx) = index.get(&key).copied() {
                    let item = &mut items[idx];
                    if !icon.is_empty() {
                        item.icon = icon;
                    }
                    item.colors = vec![inactive, active];
                    item.enabled = true;
                } else {
                    index.insert(key, items.len());
                    items.push(IndicatorItem {
                        id,
                        icon,
                        colors: vec![inactive, active],
                        enabled: true,
                    });
                }
            } else if let crate::config::WindowDef::Dashboard { data, .. } = window {
                for ind in &data.indicators {
                    let key = ind.id.to_lowercase();
                    let colors = if ind.colors.is_empty() {
                        vec!["#555555".to_string(), "#00ff00".to_string()]
                    } else {
                        ind.colors.clone()
                    };
                    if let Some(idx) = index.get(&key).copied() {
                        let item = &mut items[idx];
                        if !ind.icon.is_empty() {
                            item.icon = ind.icon.clone();
                        }
                        if !colors.is_empty() {
                            item.colors = colors;
                        }
                        item.enabled = true;
                    } else {
                        index.insert(key, items.len());
                        items.push(IndicatorItem {
                            id: ind.id.clone(),
                            icon: ind.icon.clone(),
                            colors,
                            enabled: true,
                        });
                    }
                }
            }
        }

        items.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
        items
    }

    fn textarea_with_value(value: u16) -> TextArea<'static> {
        let mut ta = Self::create_textarea();
        ta.insert_str(value.to_string());
        ta
    }

    /// Build the linear field order used for Tab/Shift+Tab navigation
    fn build_field_order_for(window_def: &WindowDef) -> Vec<FieldRef> {
        let mut fields = vec![
            // Identity + geometry (left column)
            FieldRef::Name,
            FieldRef::Title,
            FieldRef::TitlePosition,
            FieldRef::ContentAlign,
            FieldRef::BorderStyle,
            FieldRef::Row,
            FieldRef::Col,
            FieldRef::Rows,
            FieldRef::Cols,
            FieldRef::MinRows,
            FieldRef::MinCols,
            FieldRef::MaxRows,
            FieldRef::MaxCols,
            // Appearance (right column)
            FieldRef::Locked,
            FieldRef::ShowTitle,
            FieldRef::TransparentBg,
            FieldRef::ShowBorder,
            FieldRef::BorderTop,
            FieldRef::BorderBottom,
            FieldRef::BorderLeft,
            FieldRef::BorderRight,
            FieldRef::BgColor,
            FieldRef::BorderColor,
        ];

        // Special section fields appended at end
        match window_def {
            WindowDef::CommandInput { .. } => {
                fields.push(FieldRef::PromptIcon);
                fields.push(FieldRef::PromptIconColor);
                fields.push(FieldRef::TextColor);
                fields.push(FieldRef::CursorColor);
                fields.push(FieldRef::CursorBg);
            }
            WindowDef::Text { .. } => {
                // Bounty window is special: hide Streams and BufferSize
                let is_bounty = window_def.base().name.eq_ignore_ascii_case("bounty");
                if !is_bounty {
                    fields.push(FieldRef::Streams);
                    fields.push(FieldRef::BufferSize);
                }
                fields.push(FieldRef::Wordwrap);
                fields.push(FieldRef::Timestamps);
                fields.push(FieldRef::TextCompact);
            }
            WindowDef::Inventory { .. } => {
                fields.push(FieldRef::Streams);
                fields.push(FieldRef::BufferSize);
                fields.push(FieldRef::Wordwrap);
                fields.push(FieldRef::Timestamps);
            }
            WindowDef::Quickbar { .. } => {}
            WindowDef::TabbedText { .. } => {
                fields.push(FieldRef::TabBarPosition);
                fields.push(FieldRef::TabSeparator);
                fields.push(FieldRef::TabUnreadPrefix);
                fields.push(FieldRef::EditTabs);
                fields.push(FieldRef::TabActiveColor);
                fields.push(FieldRef::TabInactiveColor);
                fields.push(FieldRef::TabUnreadColor);
            }
            WindowDef::Room { .. } => {
                fields.push(FieldRef::ShowName);
                fields.push(FieldRef::ShowDesc);
                fields.push(FieldRef::ShowObjs);
                fields.push(FieldRef::ShowPlayers);
                fields.push(FieldRef::ShowExits);
            }
            WindowDef::Progress { .. } => {
                fields.push(FieldRef::ProgressNumbersOnly);
                fields.push(FieldRef::ProgressCurrentOnly);
                fields.push(FieldRef::ProgressId);
                fields.push(FieldRef::TextColor);
                fields.push(FieldRef::ProgressColor);
            }
            WindowDef::Countdown { .. } => {
                fields.push(FieldRef::CountdownIcon);
                fields.push(FieldRef::CountdownId);
                fields.push(FieldRef::CountdownColor);
                fields.push(FieldRef::CountdownBgColor);
            }
            WindowDef::Compass { .. } => {
                fields.push(FieldRef::CompassActiveColor);
                fields.push(FieldRef::CompassInactiveColor);
            }
            WindowDef::InjuryDoll { .. } => {
                fields.push(FieldRef::Injury1Color);
                fields.push(FieldRef::Injury2Color);
                fields.push(FieldRef::Injury3Color);
                fields.push(FieldRef::InjuryDefaultColor);
                fields.push(FieldRef::Injury1Color);
                fields.push(FieldRef::Injury2Color);
                fields.push(FieldRef::Injury3Color);
                fields.push(FieldRef::Scar1Color);
                fields.push(FieldRef::Scar2Color);
                fields.push(FieldRef::Scar3Color);
            }
            WindowDef::Indicator { .. } => {
                fields.push(FieldRef::IndicatorId);
                fields.push(FieldRef::IndicatorIcon);
                fields.push(FieldRef::IndicatorActiveColor);
                fields.push(FieldRef::IndicatorInactiveColor);
            }
            WindowDef::Hand { .. } => {
                fields.push(FieldRef::HandIcon);
                fields.push(FieldRef::HandIconColor);
                fields.push(FieldRef::HandTextColor);
            }
            WindowDef::Dashboard { .. } => {
                fields.push(FieldRef::DashboardLayout);
                fields.push(FieldRef::DashboardSpacing);
                fields.push(FieldRef::DashboardHideInactive);
                fields.push(FieldRef::EditIndicators);
            }
            WindowDef::ActiveEffects { .. } => {
                fields.push(FieldRef::ActiveEffectsCategory);
            }
            WindowDef::Targets { .. } => {
                fields.push(FieldRef::EntityId);
                fields.push(FieldRef::TargetsShowArmsCount);
            }
            WindowDef::Players { .. } => {
                fields.push(FieldRef::EntityId);
            }
            WindowDef::Container { .. } => {
                // Could add container_id field in the future
            }
            WindowDef::Spacer { .. } | WindowDef::Spells { .. } | WindowDef::Performance { .. } => {}
            WindowDef::Perception { .. } => {
                // Only sort_direction is configurable (stream="percWindow", buffer_size=100 are hardcoded)
                fields.push(FieldRef::PerceptionSortDirection);
                fields.push(FieldRef::PerceptionUseShortSpellNames);
                fields.push(FieldRef::PerceptionTextReplacements);
            }
            WindowDef::Experience { .. } => {
                // Experience widget - alignment is configurable via content_align in base
                // No special fields beyond base settings
            }
            WindowDef::GS4Experience { .. } => {
                // GS4 Experience widget - show_level and show_exp_bar toggles, bar colors
                fields.push(FieldRef::GS4ExpShowLevel);
                fields.push(FieldRef::GS4ExpShowExpBar);
                fields.push(FieldRef::GS4ExpMindBarColor);
                fields.push(FieldRef::GS4ExpExpBarColor);
            }
            WindowDef::Encumbrance { .. } => {
                // Encumbrance widget - show_label toggle and bar colors
                fields.push(FieldRef::EncumShowLabel);
                fields.push(FieldRef::EncumColorLight);
                fields.push(FieldRef::EncumColorModerate);
                fields.push(FieldRef::EncumColorHeavy);
                fields.push(FieldRef::EncumColorCritical);
            }
            WindowDef::MiniVitals { .. } => {
                // MiniVitals widget display mode toggles
                fields.push(FieldRef::MiniVitalsNumbersOnly);
                fields.push(FieldRef::MiniVitalsCurrentOnly);
                // Bar order and colors editor (handles all 5 bars)
                fields.push(FieldRef::MiniVitalsEditBarOrder);
            }
            WindowDef::Betrayer { .. } => {
                // Betrayer widget - show_items toggle and bar color
                fields.push(FieldRef::BetrayerShowItems);
                fields.push(FieldRef::BetrayerBarColor);
            }
        }

        fields
    }

    fn refresh_size_inputs(&mut self) {
        // Show total rows/cols (not content rows) - VellumFE style
        self.rows_input = Self::textarea_with_value(self.window_def.base().rows.max(1));
        self.cols_input = Self::textarea_with_value(self.window_def.base().cols.max(1));

        // Also refresh min/max inputs (they adjust with border changes)
        self.min_rows_input = Self::create_textarea();
        if let Some(min_rows) = self.window_def.base().min_rows {
            self.min_rows_input.insert_str(min_rows.to_string());
        }
        self.min_cols_input = Self::create_textarea();
        if let Some(min_cols) = self.window_def.base().min_cols {
            self.min_cols_input.insert_str(min_cols.to_string());
        }
        self.max_rows_input = Self::create_textarea();
        if let Some(max_rows) = self.window_def.base().max_rows {
            self.max_rows_input.insert_str(max_rows.to_string());
        }
        self.max_cols_input = Self::create_textarea();
        if let Some(max_cols) = self.window_def.base().max_cols {
            self.max_cols_input.insert_str(max_cols.to_string());
        }
    }

    /// Current content alignment value (defaults to first option)
    fn current_content_align_value(&self) -> &str {
        self.content_align_input
            .lines()
            .get(0)
            .map(|s| if s.is_empty() { None } else { Some(s.as_str()) })
            .flatten()
            .or_else(|| {
                self.window_def
                    .base()
                    .content_align
                    .as_ref()
                    .map(|s| s.as_str())
            })
            .unwrap_or_else(|| CONTENT_ALIGN_OPTIONS[0])
    }

    pub fn new(window_def: WindowDef) -> Self {
        let mut name_input = Self::create_textarea();
        name_input.insert_str(window_def.name());

        let mut title_input = Self::create_textarea();
        if let Some(ref title) = window_def.base().title {
            title_input.insert_str(title);
        }

        let mut row_input = Self::create_textarea();
        row_input.insert_str(window_def.base().row.to_string());

        let mut col_input = Self::create_textarea();
        col_input.insert_str(window_def.base().col.to_string());

        // Show total rows/cols (not content rows) - VellumFE style
        // User sets actual widget size; content adjusts based on borders
        let rows_input = Self::textarea_with_value(window_def.base().rows.max(1));

        let cols_input = Self::textarea_with_value(window_def.base().cols.max(1));

        let mut min_rows_input = Self::create_textarea();
        if let Some(min_rows) = window_def.base().min_rows {
            min_rows_input.insert_str(min_rows.to_string());
        }

        let mut min_cols_input = Self::create_textarea();
        if let Some(min_cols) = window_def.base().min_cols {
            min_cols_input.insert_str(min_cols.to_string());
        }

        let mut max_rows_input = Self::create_textarea();
        if let Some(max_rows) = window_def.base().max_rows {
            max_rows_input.insert_str(max_rows.to_string());
        }

        let mut max_cols_input = Self::create_textarea();
        if let Some(max_cols) = window_def.base().max_cols {
            max_cols_input.insert_str(max_cols.to_string());
        }

        let mut bg_color_input = Self::create_textarea();
        if let Some(ref bg_color) = window_def.base().background_color {
            bg_color_input.insert_str(bg_color);
        }

        let mut border_color_input = Self::create_textarea();
        if let Some(ref border_color) = window_def.base().border_color {
            border_color_input.insert_str(border_color);
        }

        let mut streams_input = Self::create_textarea();
        let mut buffer_size_input = Self::create_textarea();
        let mut text_wordwrap = true;
        let mut text_show_timestamps = false;
        let mut text_compact = false;
        let mut entity_id_input = Self::create_textarea();
        let mut targets_show_arms_count = false;
        if let crate::config::WindowDef::Text { data, .. } = &window_def {
            streams_input.insert_str(data.streams.join(", "));
            buffer_size_input.insert_str(data.buffer_size.to_string());
            text_wordwrap = data.wordwrap;
            text_show_timestamps = data.show_timestamps;
            text_compact = data.compact;
        }
        if let crate::config::WindowDef::Inventory { data, .. } = &window_def {
            streams_input.insert_str(data.streams.join(", "));
            buffer_size_input.insert_str(data.buffer_size.to_string());
            text_wordwrap = data.wordwrap;
            text_show_timestamps = data.show_timestamps;
        }
        if let crate::config::WindowDef::Targets { data, .. } = &window_def {
            entity_id_input.insert_str(&data.entity_id);
            targets_show_arms_count = data.show_body_part_count;
        }
        if let crate::config::WindowDef::Players { data, .. } = &window_def {
            entity_id_input.insert_str(&data.entity_id);
        }

        let mut text_color_input = Self::create_textarea();
        let mut prompt_icon_input = Self::create_textarea();
        let mut prompt_icon_color_input = Self::create_textarea();
        let mut cursor_color_input = Self::create_textarea();
        let mut cursor_bg_input = Self::create_textarea();
        let mut tab_bar_position_input = Self::create_textarea();
        let mut title_position_input = Self::create_textarea();
        title_position_input.insert_str(&window_def.base().title_position);
        let mut tab_active_color_input = Self::create_textarea();
        let mut tab_inactive_color_input = Self::create_textarea();
        let mut tab_unread_color_input = Self::create_textarea();
        let mut tab_unread_prefix_input = Self::create_textarea();
        let mut tab_separator = false;
        let mut progress_id_input = Self::create_textarea();
        let mut progress_color_input = Self::create_textarea();
        let mut countdown_id_input = Self::create_textarea();
        let mut countdown_icon_input = Self::create_textarea();
        let mut countdown_color_input = Self::create_textarea();
        let mut countdown_bg_color_input = Self::create_textarea();
        let mut compass_active_color_input = Self::create_textarea();
        let mut compass_inactive_color_input = Self::create_textarea();
        let mut injury_default_color_input = Self::create_textarea();
        let mut injury1_color_input = Self::create_textarea();
        let mut injury2_color_input = Self::create_textarea();
        let mut injury3_color_input = Self::create_textarea();
        let mut scar1_color_input = Self::create_textarea();
        let mut scar2_color_input = Self::create_textarea();
        let mut scar3_color_input = Self::create_textarea();
        let mut indicator_id_input = Self::create_textarea();
        let mut indicator_icon_input = Self::create_textarea();
        let mut indicator_active_color_input = Self::create_textarea();
        let mut indicator_inactive_color_input = Self::create_textarea();
        let mut active_effects_category_input = Self::create_textarea();
        let mut hand_icon_input = Self::create_textarea();
        let mut hand_icon_color_input = Self::create_textarea();
        let mut hand_text_color_input = Self::create_textarea();
        let mut dashboard_layout_input = Self::create_textarea();
        let mut dashboard_spacing_input = Self::create_textarea();
        let mut dashboard_hide_inactive = false;
        let mut perf_enabled = true;
        let mut perf_show_fps = true;
        let mut perf_show_frame_times = false;
        let mut perf_show_render_times = true;
        let mut perf_show_ui_times = true;
        let mut perf_show_wrap_times = true;
        let mut perf_show_net = true;
        let mut perf_show_parse = true;
        let mut perf_show_events = true;
        let mut perf_show_memory = true;
        let mut perf_show_lines = true;
        let mut perf_show_uptime = true;
        let mut perf_show_jitter = false;
        let mut perf_show_frame_spikes = false;
        let mut perf_show_event_lag = false;
        let mut perf_show_memory_delta = true;
        let mut show_desc = true;
        let mut show_objs = true;
        let mut show_players = true;
        let mut show_exits = true;
        let mut show_name = false;
        let mut progress_numbers_only = false;
        let mut progress_current_only = false;
        if let Some(ref color) = window_def.base().text_color {
            text_color_input.insert_str(color);
        }
        if let crate::config::WindowDef::CommandInput { data, .. } = &window_def {
            if let Some(ref color) = data.text_color {
                text_color_input.insert_str(color);
            }
            if let Some(ref icon) = data.prompt_icon {
                prompt_icon_input.insert_str(icon);
            }
            if let Some(ref color) = data.prompt_icon_color {
                prompt_icon_color_input.insert_str(color);
            }
            if let Some(ref color) = data.cursor_color {
                cursor_color_input.insert_str(color);
            }
            if let Some(ref color) = data.cursor_background_color {
                cursor_bg_input.insert_str(color);
            }
        }

        if let crate::config::WindowDef::TabbedText { data, .. } = &window_def {
            tab_bar_position_input.insert_str(&data.tab_bar_position);
            tab_separator = data.tab_separator;
            if let Some(ref c) = data.tab_active_color {
                tab_active_color_input.insert_str(c);
            }
            if let Some(ref c) = data.tab_inactive_color {
                tab_inactive_color_input.insert_str(c);
            }
            if let Some(ref c) = data.tab_unread_color {
                tab_unread_color_input.insert_str(c);
            }
            if let Some(ref prefix) = data.tab_unread_prefix {
                tab_unread_prefix_input.insert_str(prefix);
            }
        }

        if let crate::config::WindowDef::Progress { data, .. } = &window_def {
            if let Some(ref id) = data.id {
                progress_id_input.insert_str(id);
            } else {
                progress_id_input.insert_str(&window_def.base().name);
            }
            if let Some(ref color) = data.color {
                progress_color_input.insert_str(color);
            }
            progress_numbers_only = data.numbers_only;
            progress_current_only = data.current_only;
        }

        if let crate::config::WindowDef::Countdown { data, .. } = &window_def {
            if let Some(ref id) = data.id {
                countdown_id_input.insert_str(id);
            }
            if let Some(icon) = data.icon {
                countdown_icon_input.insert_str(&icon.to_string());
            }
            if let Some(ref color) = data.color {
                countdown_color_input.insert_str(color);
            } else if let Some(ref color) = window_def.base().text_color {
                // Use the template's text color as the default icon color
                countdown_color_input.insert_str(color);
            }
            if let Some(ref color) = data.background_color {
                countdown_bg_color_input.insert_str(color);
            }
        }

        if let crate::config::WindowDef::Compass { data, .. } = &window_def {
            if let Some(ref c) = data.active_color {
                compass_active_color_input.insert_str(c);
            }
            if let Some(ref c) = data.inactive_color {
                compass_inactive_color_input.insert_str(c);
            }
        }

        if let crate::config::WindowDef::InjuryDoll { data, .. } = &window_def {
            if let Some(ref c) = data.injury_default_color {
                injury_default_color_input.insert_str(c);
            }
            if let Some(ref c) = data.injury1_color {
                injury1_color_input.insert_str(c);
            }
            if let Some(ref c) = data.injury2_color {
                injury2_color_input.insert_str(c);
            }
            if let Some(ref c) = data.injury3_color {
                injury3_color_input.insert_str(c);
            }
            if let Some(ref c) = data.scar1_color {
                scar1_color_input.insert_str(c);
            }
            if let Some(ref c) = data.scar2_color {
                scar2_color_input.insert_str(c);
            }
            if let Some(ref c) = data.scar3_color {
                scar3_color_input.insert_str(c);
            }
        }

        if let crate::config::WindowDef::Hand { data, .. } = &window_def {
            if let Some(ref icon) = data.icon {
                hand_icon_input.insert_str(icon);
            } else {
                // Default icons based on common hand names
                let default_icon = match window_def.base().name.as_str() {
                    "left" | "left_hand" => Some("L:"),
                    "right" | "right_hand" => Some("R:"),
                    "spell" | "spell_hand" => Some("S:"),
                    _ => None,
                };
                if let Some(icon) = default_icon {
                    hand_icon_input.insert_str(icon);
                }
            }
            if let Some(ref c) = data.icon_color {
                hand_icon_color_input.insert_str(c);
            }
            if let Some(ref c) = data.text_color {
                hand_text_color_input.insert_str(c);
            }
        }

        if let crate::config::WindowDef::Indicator { data, .. } = &window_def {
            if let Some(ref id) = data.indicator_id {
                indicator_id_input.insert_str(id);
            } else {
                indicator_id_input.insert_str(&window_def.base().name);
            }
            if let Some(ref icon) = data.icon {
                indicator_icon_input.insert_str(icon);
            }
            if let Some(ref color) = data.active_color {
                indicator_active_color_input.insert_str(color);
            }
            if let Some(ref color) = data.inactive_color {
                indicator_inactive_color_input.insert_str(color);
            }
        }

        if let crate::config::WindowDef::ActiveEffects { data, .. } = &window_def {
            active_effects_category_input.insert_str(&data.category);
        }

        if let crate::config::WindowDef::Dashboard { data, .. } = &window_def {
            dashboard_layout_input.insert_str(&data.layout);
            dashboard_spacing_input.insert_str(data.spacing.to_string());
            dashboard_hide_inactive = data.hide_inactive;
        }

        if let crate::config::WindowDef::Performance { data, .. } = &window_def {
            perf_enabled = data.enabled;
            perf_show_fps = data.show_fps;
            perf_show_frame_times = data.show_frame_times;
            perf_show_render_times = data.show_render_times;
            perf_show_ui_times = data.show_ui_times;
            perf_show_wrap_times = data.show_wrap_times;
            perf_show_net = data.show_net;
            perf_show_parse = data.show_parse;
            perf_show_events = data.show_events;
            perf_show_memory = data.show_memory;
            perf_show_lines = data.show_lines;
            perf_show_uptime = data.show_uptime;
            perf_show_jitter = data.show_jitter;
            perf_show_frame_spikes = data.show_frame_spikes;
            perf_show_event_lag = data.show_event_lag;
            perf_show_memory_delta = data.show_memory_delta;
        }

        if let crate::config::WindowDef::Room { data, .. } = &window_def {
            show_desc = data.show_desc;
            show_objs = data.show_objs;
            show_players = data.show_players;
            show_exits = data.show_exits;
            show_name = data.show_name;
        }

        // Perception widget fields
        // Note: stream and buffer_size are hardcoded - only sort_direction is user-configurable
        let mut perception_sort_direction_input = Self::create_textarea();
        let mut perception_use_short_spell_names = false;

        if let crate::config::WindowDef::Perception { data, .. } = &window_def {
            perception_sort_direction_input.insert_str(match data.sort_direction {
                crate::config::SortDirection::Ascending => "ascending",
                crate::config::SortDirection::Descending => "descending",
            });
            perception_use_short_spell_names = data.use_short_spell_names;
        }

        // Encumbrance widget fields - defaults match widget defaults (green, yellow, orange, red)
        let (
            show_label_encum,
            encum_color_light,
            encum_color_moderate,
            encum_color_heavy,
            encum_color_critical,
        ) = if let crate::config::WindowDef::Encumbrance { data, .. } = &window_def {
            (
                data.show_label,
                data.color_light.clone().unwrap_or_else(|| "#00FF00".to_string()),
                data.color_moderate.clone().unwrap_or_else(|| "#FFFF00".to_string()),
                data.color_heavy.clone().unwrap_or_else(|| "#FFA500".to_string()),
                data.color_critical.clone().unwrap_or_else(|| "#FF0000".to_string()),
            )
        } else {
            (true, "#00FF00".to_string(), "#FFFF00".to_string(), "#FFA500".to_string(), "#FF0000".to_string())
        };

        let mut encum_color_light_input = Self::create_textarea();
        encum_color_light_input.insert_str(&encum_color_light);
        let mut encum_color_moderate_input = Self::create_textarea();
        encum_color_moderate_input.insert_str(&encum_color_moderate);
        let mut encum_color_heavy_input = Self::create_textarea();
        encum_color_heavy_input.insert_str(&encum_color_heavy);
        let mut encum_color_critical_input = Self::create_textarea();
        encum_color_critical_input.insert_str(&encum_color_critical);

        // GS4Experience widget fields - mind bar default cyan, exp bar default empty (theme bg)
        let (
            gs4_exp_show_level,
            gs4_exp_show_exp_bar,
            gs4_exp_mind_bar_color,
            gs4_exp_exp_bar_color,
        ) = if let crate::config::WindowDef::GS4Experience { data, .. } = &window_def {
            (
                data.show_level,
                data.show_exp_bar,
                data.mind_bar_color.clone().unwrap_or_else(|| "#00FFFF".to_string()),
                data.exp_bar_color.clone().unwrap_or_default(), // Empty = theme background
            )
        } else {
            (true, true, "#00FFFF".to_string(), String::new())
        };

        let mut gs4_exp_mind_bar_color_input = Self::create_textarea();
        gs4_exp_mind_bar_color_input.insert_str(&gs4_exp_mind_bar_color);
        let mut gs4_exp_exp_bar_color_input = Self::create_textarea();
        gs4_exp_exp_bar_color_input.insert_str(&gs4_exp_exp_bar_color);

        // MiniVitals widget fields
        let (
            minivitals_numbers_only,
            minivitals_current_only,
            minivitals_health_color,
            minivitals_mana_color,
            minivitals_stamina_color,
            minivitals_spirit_color,
        ) = if let crate::config::WindowDef::MiniVitals { data, .. } = &window_def {
            (
                data.numbers_only,
                data.current_only,
                data.health_color.clone().unwrap_or_else(|| "#6e0202".to_string()),
                data.mana_color.clone().unwrap_or_else(|| "#08086d".to_string()),
                data.stamina_color.clone().unwrap_or_else(|| "#bd7b00".to_string()),
                data.spirit_color.clone().unwrap_or_else(|| "#6e727c".to_string()),
            )
        } else {
            (false, false, "#6e0202".to_string(), "#08086d".to_string(), "#bd7b00".to_string(), "#6e727c".to_string())
        };

        let mut minivitals_health_color_input = Self::create_textarea();
        minivitals_health_color_input.insert_str(&minivitals_health_color);
        let mut minivitals_mana_color_input = Self::create_textarea();
        minivitals_mana_color_input.insert_str(&minivitals_mana_color);
        let mut minivitals_stamina_color_input = Self::create_textarea();
        minivitals_stamina_color_input.insert_str(&minivitals_stamina_color);
        let mut minivitals_spirit_color_input = Self::create_textarea();
        minivitals_spirit_color_input.insert_str(&minivitals_spirit_color);

        // Betrayer widget fields
        let (betrayer_show_items, betrayer_bar_color) =
            if let crate::config::WindowDef::Betrayer { data, .. } = &window_def {
                (
                    data.show_items,
                    data.bar_color.clone().unwrap_or_else(|| "#8b0000".to_string()),
                )
            } else {
                (true, "#8b0000".to_string())
            };

        let mut betrayer_bar_color_input = Self::create_textarea();
        betrayer_bar_color_input.insert_str(&betrayer_bar_color);

        let mut content_align_input = Self::create_textarea();
        if let Some(ref align) = window_def.base().content_align {
            content_align_input.insert_str(align);
        }

        let field_order = Self::build_field_order_for(&window_def);

        Self {
            popup_x: 0,
            popup_y: 0,
            popup_width: 70,
            popup_height: 20,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            field_order,
            current_field_index: 0,
            focused_field: FieldRef::Name.legacy_field_id(),
            name_input,
            title_input,
            row_input,
            col_input,
            rows_input,
            cols_input,
            min_rows_input,
            min_cols_input,
            max_rows_input,
            max_cols_input,
            bg_color_input,
            border_color_input,
            streams_input,
            buffer_size_input,
            text_wordwrap,
            text_show_timestamps,
            entity_id_input,
            text_color_input,
            prompt_icon_input,
            prompt_icon_color_input,
            cursor_color_input,
            cursor_bg_input,
            content_align_input,
            tab_bar_position_input,
            title_position_input,
            tab_active_color_input,
            tab_inactive_color_input,
            tab_unread_color_input,
            tab_unread_prefix_input,
            tab_separator,
            progress_id_input,
            progress_color_input,
            progress_numbers_only,
            progress_current_only,
            countdown_id_input,
            countdown_icon_input,
            countdown_color_input,
            countdown_bg_color_input,
            compass_active_color_input,
            compass_inactive_color_input,
            injury_default_color_input,
            injury1_color_input,
            injury2_color_input,
            injury3_color_input,
            scar1_color_input,
            scar2_color_input,
            scar3_color_input,
            indicator_id_input,
            indicator_icon_input,
            indicator_active_color_input,
            indicator_inactive_color_input,
            active_effects_category_input,
            hand_icon_input,
            hand_icon_color_input,
            hand_text_color_input,
            dashboard_layout_input,
            dashboard_spacing_input,
            dashboard_hide_inactive,
            perf_enabled,
            show_desc,
            show_objs,
            show_players,
            show_exits,
            show_name,
            perf_show_fps,
            perf_show_frame_times,
            perf_show_render_times,
            perf_show_ui_times,
            perf_show_wrap_times,
            perf_show_net,
            perf_show_parse,
            perf_show_events,
            perf_show_memory,
            perf_show_lines,
            perf_show_uptime,
            perf_show_jitter,
            perf_show_frame_spikes,
            perf_show_event_lag,
            perf_show_memory_delta,
            available_indicators: Vec::new(),
            perception_sort_direction_input,
            perception_use_short_spell_names,
            show_label_encum,
            encum_color_light_input,
            encum_color_moderate_input,
            encum_color_heavy_input,
            encum_color_critical_input,
            gs4_exp_show_level,
            gs4_exp_show_exp_bar,
            gs4_exp_mind_bar_color_input,
            gs4_exp_exp_bar_color_input,
            minivitals_numbers_only,
            minivitals_current_only,
            minivitals_health_color_input,
            minivitals_mana_color_input,
            minivitals_stamina_color_input,
            minivitals_spirit_color_input,
            betrayer_show_items,
            betrayer_bar_color_input,
            text_compact,
            targets_show_arms_count,
            window_def: window_def.clone(),
            original_window_def: window_def,
            is_new: false,
            status_message: "Tab/Shift+Tab: Navigate | Ctrl+S: Save | Esc: Cancel".to_string(),
            tab_editor: None,
            indicator_editor: None,
            performance_metrics_editor: None,
            text_replacements_editor: None,
            bar_order_editor: None,
            field_click_areas: Vec::new(),
        }
    }

    /// Create editor for a new window from a template
    pub fn new_from_template(template: WindowDef) -> Self {
        // Create editor with template (reuse new() logic)
        let mut editor = Self::new(template);
        // Mark as new so Ctrl+s adds instead of updates
        editor.is_new = true;
        editor
    }

    pub fn new_with_layout(window_def: WindowDef, layout: &crate::config::Layout) -> Self {
        let mut editor = Self::new(window_def);
        editor.available_indicators = Self::indicators_from_layout(layout);
        editor
    }

    pub fn new_window(widget_type: String) -> Self {
        use crate::config::{
            BorderSides, CommandInputWidgetData, PerformanceWidgetData, RoomWidgetData, SpacerWidgetData,
            TextWidgetData, WindowBase, WindowDef,
        };

        // Create base configuration with defaults
        let base = WindowBase {
            name: String::new(),
            row: 0,
            col: 0,
            rows: 10,
            cols: 40,
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: false,
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
            visible: true,
            content_align: None,
        };

        // Create window_def based on widget type
        let window_def = match widget_type.to_lowercase().as_str() {
            "text" => WindowDef::Text {
                base,
                data: TextWidgetData {
                    streams: vec![],
                    buffer_size: 10000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            },
            "room" => WindowDef::Room {
                base,
                data: RoomWidgetData {
                    buffer_size: 0,
                    show_desc: true,
                    show_objs: true,
                    show_players: true,
                    show_exits: true,
                    show_name: false,
                },
            },
            "command_input" => WindowDef::CommandInput {
                base,
                data: CommandInputWidgetData::default(),
            },
            "spacer" => WindowDef::Spacer {
                base,
                data: SpacerWidgetData {},
            },
            "performance" => WindowDef::Performance {
                base,
                data: PerformanceWidgetData {
                    enabled: true,
                    show_fps: true,
                    show_frame_times: true,
                    show_render_times: true,
                    show_ui_times: true,
                    show_wrap_times: true,
                    show_net: true,
                    show_parse: true,
                    show_events: true,
                    show_memory: true,
                    show_lines: true,
                    show_uptime: true,
                    show_jitter: true,
                    show_frame_spikes: true,
                    show_event_lag: true,
                    show_memory_delta: true,
                },
            },
            _ => WindowDef::Text {
                base,
                data: TextWidgetData {
                    streams: vec![],
                    buffer_size: 10000,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            },
        };

        let name_input = Self::create_textarea();
        let title_input = Self::create_textarea();

        let mut row_input = Self::create_textarea();
        row_input.insert_str("0");

        let mut col_input = Self::create_textarea();
        col_input.insert_str("0");

        // Show total rows/cols (not content rows) - VellumFE style
        let rows_input = Self::textarea_with_value(window_def.base().rows.max(1));

        let cols_input = Self::textarea_with_value(window_def.base().cols.max(1));

        let min_rows_input = Self::create_textarea();
        let min_cols_input = Self::create_textarea();
        let max_rows_input = Self::create_textarea();
        let max_cols_input = Self::create_textarea();
        let bg_color_input = Self::create_textarea();
        let border_color_input = Self::create_textarea();
        let streams_input = Self::create_textarea();
        let mut buffer_size_input = Self::create_textarea();
        buffer_size_input.insert_str("10000");
        let text_wordwrap = true;
        let text_show_timestamps = false;
        let text_compact = false;
        let entity_id_input = Self::create_textarea();
        let targets_show_arms_count = false;
        let text_color_input = Self::create_textarea();
        let prompt_icon_input = Self::create_textarea();
        let prompt_icon_color_input = Self::create_textarea();
        let cursor_color_input = Self::create_textarea();
        let cursor_bg_input = Self::create_textarea();
        let content_align_input = Self::create_textarea();
        let mut tab_bar_position_input = Self::create_textarea();
        tab_bar_position_input.insert_str("top");
        let mut title_position_input = Self::create_textarea();
        title_position_input.insert_str("top-left");
        let tab_active_color_input = Self::create_textarea();
        let tab_inactive_color_input = Self::create_textarea();
        let tab_unread_color_input = Self::create_textarea();
        let tab_unread_prefix_input = Self::create_textarea();
        let tab_separator = false;
        let mut progress_id_input = Self::create_textarea();
        if let crate::config::WindowDef::Progress { .. } = &window_def {
            progress_id_input.insert_str(&window_def.base().name);
        }
        let progress_color_input = Self::create_textarea();
        let progress_numbers_only = false;
        let progress_current_only = false;
        let countdown_id_input = Self::create_textarea();
        let countdown_icon_input = Self::create_textarea();
        let countdown_color_input = Self::create_textarea();
        let countdown_bg_color_input = Self::create_textarea();
        let compass_active_color_input = Self::create_textarea();
        let compass_inactive_color_input = Self::create_textarea();
        let injury_default_color_input = Self::create_textarea();
        let injury1_color_input = Self::create_textarea();
        let injury2_color_input = Self::create_textarea();
        let injury3_color_input = Self::create_textarea();
        let scar1_color_input = Self::create_textarea();
        let scar2_color_input = Self::create_textarea();
        let scar3_color_input = Self::create_textarea();
        let indicator_id_input = Self::create_textarea();
        let indicator_icon_input = Self::create_textarea();
        let indicator_active_color_input = Self::create_textarea();
        let indicator_inactive_color_input = Self::create_textarea();
        let active_effects_category_input = Self::create_textarea();
        let hand_icon_input = Self::create_textarea();
        let hand_icon_color_input = Self::create_textarea();
        let hand_text_color_input = Self::create_textarea();
        let dashboard_layout_input = Self::create_textarea();
        let dashboard_spacing_input = Self::create_textarea();
        let dashboard_hide_inactive = false;
        let perf_enabled = true;
        let perf_show_fps = true;
        let perf_show_frame_times = false;
        let perf_show_render_times = true;
        let perf_show_ui_times = true;
        let perf_show_wrap_times = true;
        let perf_show_net = true;
        let perf_show_parse = true;
        let perf_show_events = true;
        let perf_show_memory = true;
        let perf_show_lines = true;
        let perf_show_uptime = true;
        let perf_show_jitter = false;
        let perf_show_frame_spikes = false;
        let perf_show_event_lag = false;
        let perf_show_memory_delta = true;
        let show_desc = true;
        let show_objs = true;
        let show_players = true;
        let show_exits = true;
        let show_name = false;

        // Perception widget - default to descending sort, short names off
        let mut perception_sort_direction_input = Self::create_textarea();
        perception_sort_direction_input.insert_str("descending");
        let perception_use_short_spell_names = false;

        let field_order = Self::build_field_order_for(&window_def);

        Self {
            popup_x: 0,
            popup_y: 0,
            popup_width: 70,
            popup_height: 20,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            field_order,
            current_field_index: 0,
            focused_field: FieldRef::Name.legacy_field_id(),
            name_input,
            title_input,
            row_input,
            col_input,
            rows_input,
            cols_input,
            min_rows_input,
            min_cols_input,
            max_rows_input,
            max_cols_input,
            bg_color_input,
            border_color_input,
            streams_input,
            buffer_size_input,
            text_wordwrap,
            text_show_timestamps,
            entity_id_input,
            text_color_input,
            prompt_icon_input,
            prompt_icon_color_input,
            cursor_color_input,
            cursor_bg_input,
            content_align_input,
            tab_bar_position_input,
            title_position_input,
            tab_active_color_input,
            tab_inactive_color_input,
            tab_unread_color_input,
            tab_unread_prefix_input,
            tab_separator,
            progress_id_input,
            progress_color_input,
            progress_numbers_only,
            progress_current_only,
            countdown_id_input,
            countdown_icon_input,
            countdown_color_input,
            countdown_bg_color_input,
            compass_active_color_input,
            compass_inactive_color_input,
            injury_default_color_input,
            injury1_color_input,
            injury2_color_input,
            injury3_color_input,
            scar1_color_input,
            scar2_color_input,
            scar3_color_input,
            indicator_id_input,
            indicator_icon_input,
            indicator_active_color_input,
            indicator_inactive_color_input,
            active_effects_category_input,
            hand_icon_input,
            hand_icon_color_input,
            hand_text_color_input,
            dashboard_layout_input,
            dashboard_spacing_input,
            dashboard_hide_inactive,
            perf_enabled,
            show_desc,
            show_objs,
            show_players,
            show_exits,
            show_name,
            perf_show_fps,
            perf_show_frame_times,
            perf_show_render_times,
            perf_show_ui_times,
            perf_show_wrap_times,
            perf_show_net,
            perf_show_parse,
            perf_show_events,
            perf_show_memory,
            perf_show_lines,
            perf_show_uptime,
            perf_show_jitter,
            perf_show_frame_spikes,
            perf_show_event_lag,
            perf_show_memory_delta,
            available_indicators: Vec::new(),
            perception_sort_direction_input,
            perception_use_short_spell_names,
            show_label_encum: true,
            encum_color_light_input: Self::create_textarea(),
            encum_color_moderate_input: Self::create_textarea(),
            encum_color_heavy_input: Self::create_textarea(),
            encum_color_critical_input: Self::create_textarea(),
            gs4_exp_show_level: true,
            gs4_exp_show_exp_bar: true,
            gs4_exp_mind_bar_color_input: Self::create_textarea(),
            gs4_exp_exp_bar_color_input: Self::create_textarea(),
            minivitals_numbers_only: false,
            minivitals_current_only: false,
            minivitals_health_color_input: Self::create_textarea(),
            minivitals_mana_color_input: Self::create_textarea(),
            minivitals_stamina_color_input: Self::create_textarea(),
            minivitals_spirit_color_input: Self::create_textarea(),
            betrayer_show_items: true,
            betrayer_bar_color_input: Self::create_textarea(),
            text_compact,
            targets_show_arms_count,
            window_def: window_def.clone(),
            original_window_def: window_def,
            is_new: true,
            status_message: "Tab/Shift+Tab: Navigate | Ctrl+S: Save | Esc: Cancel".to_string(),
            tab_editor: None,
            indicator_editor: None,
            performance_metrics_editor: None,
            text_replacements_editor: None,
            bar_order_editor: None,
            field_click_areas: Vec::new(),
        }
    }

    /// Create a new window editor with auto-naming for all custom widgets
    /// Uses Layout::generate_spacer_name() for spacers (spacer_N pattern)
    /// Uses Layout::generate_widget_name() for other types (custom-{type}-N pattern)
    pub fn new_window_with_layout(widget_type: String, layout: &crate::config::Layout) -> Self {
        // Prefer the configured template (so defaults like tabs/streams are respected)
        let mut editor = if let Some(template) = Config::get_window_template(&widget_type) {
            WindowEditor::new_from_template(template)
        } else {
            WindowEditor::new_window(widget_type.clone())
        };
        editor.available_indicators = Self::indicators_from_layout(layout);

        // Auto-generate a name for all custom widgets
        let auto_name = if widget_type.to_lowercase() == "spacer" {
            // Spacers use the spacer_N pattern for backward compatibility
            layout.generate_spacer_name()
        } else {
            // All other widget types use custom-{type}-N pattern
            layout.generate_widget_name(&widget_type)
        };

        // Set the auto-generated name in both the input field and the window def
        editor.name_input.insert_str(&auto_name);
        editor.window_def.base_mut().name = auto_name;

        editor
    }

    fn is_command_input(&self) -> bool {
        matches!(self.window_def, WindowDef::CommandInput { .. })
    }

    fn current_field_ref(&self) -> Option<FieldRef> {
        self.field_order.get(self.current_field_index).copied()
    }

    /// Move to next field (Tab)
    pub fn next_field(&mut self) {
        if self.field_order.is_empty() {
            return;
        }

        self.current_field_index = (self.current_field_index + 1) % self.field_order.len();
        self.sync_focused_field();
    }

    /// Move to previous field (Shift+Tab)
    pub fn previous_field(&mut self) {
        if self.field_order.is_empty() {
            return;
        }

        self.current_field_index = if self.current_field_index == 0 {
            self.field_order.len() - 1
        } else {
            self.current_field_index - 1
        };

        self.sync_focused_field();
    }

    /// Sync the legacy focused_field index with current global field
    fn sync_focused_field(&mut self) {
        if let Some(field_ref) = self.current_field_ref() {
            self.focused_field = field_ref.legacy_field_id();
        }
    }

    pub fn is_sub_editor_active(&self) -> bool {
        self.tab_editor.is_some() || self.indicator_editor.is_some() || self.performance_metrics_editor.is_some() || self.text_replacements_editor.is_some() || self.bar_order_editor.is_some()
    }

    fn footer_help_text(&self) -> &str {
        if self.performance_metrics_editor.is_some() {
            return "[Space/Enter/T: Toggle]─[Esc: Back]";
        }
        if let Some(editor) = self.indicator_editor.as_ref() {
            if matches!(editor.mode, IndicatorEditorMode::List) {
                return "[T: Toggle]─[Del: Delete]─[Shift+↑/↓: Re-order]─[Esc: Back]";
            }
        }
        if let Some(editor) = self.tab_editor.as_ref() {
            if matches!(editor.mode, TabEditorMode::List) {
                return "[A: Add]─[E: Edit]─[Del: Delete]─[Shift+↑/↓: Re-order]─[Esc: Back]";
            }
        }
        if let Some(editor) = self.text_replacements_editor.as_ref() {
            if matches!(editor.mode, TextReplacementsEditorMode::List) {
                return "[A: Add]─[E: Edit]─[Del: Delete]─[Shift+↑/↓: Re-order]─[Esc: Back]";
            }
        }
        if self.bar_order_editor.is_some() {
            return "[Ctrl+S: Save]─[Shift+↑/↓: Reorder]─[Esc: Cancel]";
        }
        "[Ctrl+S: Save] [Esc: Cancel]"
    }

    fn open_tab_editor(&mut self) {
        if let WindowDef::TabbedText { data, .. } = &self.window_def {
            self.tab_editor = Some(TabEditor::from_tabs(&data.tabs));
        } else {
            self.status_message =
                "Tab editor only available for TabbedText windows".to_string();
        }
    }

    fn open_indicator_editor(&mut self) {
        if self.available_indicators.is_empty() {
            self.available_indicators = Self::indicator_templates();
        }
        if let WindowDef::Dashboard { data, .. } = &self.window_def {
            self.indicator_editor =
                Some(IndicatorEditor::from_defs(&data.indicators, self.available_indicators.clone()));
        } else {
            self.status_message =
                "Indicator editor only available for Dashboard windows".to_string();
        }
    }

    fn open_performance_metrics_editor(&mut self) {
        let items = self.perf_group_states();
        self.performance_metrics_editor = Some(PerformanceMetricsEditor::new(items));
    }

    fn open_perception_replacements_editor(&mut self) {
        if let WindowDef::Perception { data, .. } = &self.window_def {
            self.text_replacements_editor =
                Some(TextReplacementsEditor::from_replacements(&data.text_replacements));
        } else {
            self.status_message =
                "Text replacements editor only available for Perception windows".to_string();
        }
    }

    fn open_bar_order_editor(&mut self) {
        if let WindowDef::MiniVitals { data, .. } = &self.window_def {
            self.bar_order_editor = Some(BarOrderEditor::from_minivitals_data(data));
        } else {
            self.status_message =
                "Bar order editor only available for MiniVitals windows".to_string();
        }
    }

    fn commit_bar_order_editor(&mut self) {
        // Save any pending color input before committing
        if let Some(ref mut editor) = self.bar_order_editor {
            if editor.is_editing_color() {
                editor.save_color_to_bar();
            }
        }
        if let (Some(editor), WindowDef::MiniVitals { data, .. }) =
            (self.bar_order_editor.clone(), &mut self.window_def)
        {
            data.bar_order = editor.to_bar_order();
            editor.apply_colors_to_data(data);
        }
    }

    fn commit_text_replacements_editor(&mut self) {
        if let (Some(editor), WindowDef::Perception { data, .. }) =
            (self.text_replacements_editor.clone(), &mut self.window_def)
        {
            // If the editor is in form mode, capture in-progress edits
            let mut editor = editor;
            if editor.mode == TextReplacementsEditorMode::Form {
                editor.save_form();
            }
            data.text_replacements = editor.to_replacements();
            self.text_replacements_editor = Some(editor);
        }
    }

    fn commit_tab_editor(&mut self) {
        if let (Some(tab_editor), WindowDef::TabbedText { data, .. }) =
            (self.tab_editor.clone(), &mut self.window_def)
        {
            // If the tab editor is currently in form mode, capture the in-progress edits
            let mut editor = tab_editor;
            if editor.mode == TabEditorMode::Form {
                // save_form will no-op if the inputs are empty
                editor.save_form();
            }
            data.tabs = editor.to_tabs();
            // Update the in-memory editor so subsequent interactions reflect saved values
            self.tab_editor = Some(editor);
        }
    }

    fn commit_indicator_editor(&mut self) {
        if let (Some(editor), WindowDef::Dashboard { data, .. }) =
            (&self.indicator_editor, &mut self.window_def)
        {
            data.indicators = editor.to_defs();
        }
    }

    fn commit_performance_metrics_editor(&mut self) {
        if let Some(editor) = &self.performance_metrics_editor {
            let items = editor.items.clone();
            self.apply_perf_group_states(&items);
        }
    }

    pub fn commit_sub_editors(&mut self) {
        if self.tab_editor.is_some() {
            self.commit_tab_editor();
        }
        if self.indicator_editor.is_some() {
            self.commit_indicator_editor();
        }
        if self.performance_metrics_editor.is_some() {
            self.commit_performance_metrics_editor();
        }
        if self.text_replacements_editor.is_some() {
            self.commit_text_replacements_editor();
        }
        if self.bar_order_editor.is_some() {
            self.commit_bar_order_editor();
        }
    }

    fn perf_group_states(&self) -> Vec<PerfMetricGroupState> {
        vec![
            PerfMetricGroupState {
                group: PerfMetricGroup::FrameTiming,
                enabled: self.perf_show_fps
                    || self.perf_show_frame_times
                    || self.perf_show_jitter
                    || self.perf_show_frame_spikes,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::RenderPipeline,
                enabled: self.perf_show_render_times
                    || self.perf_show_ui_times
                    || self.perf_show_wrap_times,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::Network,
                enabled: self.perf_show_net,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::Parser,
                enabled: self.perf_show_parse,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::Events,
                enabled: self.perf_show_events || self.perf_show_event_lag,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::Memory,
                enabled: self.perf_show_memory || self.perf_show_memory_delta,
            },
            PerfMetricGroupState {
                group: PerfMetricGroup::UptimeLines,
                enabled: self.perf_show_uptime || self.perf_show_lines,
            },
        ]
    }

    fn apply_perf_group_states(&mut self, states: &[PerfMetricGroupState]) {
        for state in states {
            match state.group {
                PerfMetricGroup::FrameTiming => {
                    self.perf_show_fps = state.enabled;
                    self.perf_show_frame_times = state.enabled;
                    self.perf_show_jitter = state.enabled;
                    self.perf_show_frame_spikes = state.enabled;
                }
                PerfMetricGroup::RenderPipeline => {
                    self.perf_show_render_times = state.enabled;
                    self.perf_show_ui_times = state.enabled;
                    self.perf_show_wrap_times = state.enabled;
                }
                PerfMetricGroup::Network => {
                    self.perf_show_net = state.enabled;
                }
                PerfMetricGroup::Parser => {
                    self.perf_show_parse = state.enabled;
                }
                PerfMetricGroup::Events => {
                    self.perf_show_events = state.enabled;
                    self.perf_show_event_lag = state.enabled;
                }
                PerfMetricGroup::Memory => {
                    self.perf_show_memory = state.enabled;
                    self.perf_show_memory_delta = state.enabled;
                }
                PerfMetricGroup::UptimeLines => {
                    self.perf_show_uptime = state.enabled;
                    self.perf_show_lines = state.enabled;
                }
            }
        }
    }

    /// Save the active sub-editor form (tab/indicator) and keep the editor open.
    /// Returns true if a sub-editor form was active and handled.
    pub fn save_active_sub_editor_form(&mut self) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            if matches!(editor.mode, TabEditorMode::Form) {
                editor.save_form();
                return true;
            }
        }
        if let Some(editor) = self.indicator_editor.as_mut() {
            if matches!(editor.mode, IndicatorEditorMode::Form) {
                editor.save_form();
                return true;
            }
        }
        if let Some(editor) = self.text_replacements_editor.as_mut() {
            if matches!(editor.mode, TextReplacementsEditorMode::Form) {
                editor.save_form();
                return true;
            }
        }
        false
    }

    fn close_sub_editor(&mut self) -> bool {
        if self.tab_editor.is_some() {
            self.commit_tab_editor();
            self.tab_editor = None;
            return true;
        }
        if self.indicator_editor.is_some() {
            self.commit_indicator_editor();
            self.indicator_editor = None;
            return true;
        }
        if self.performance_metrics_editor.is_some() {
            self.commit_performance_metrics_editor();
            self.performance_metrics_editor = None;
            return true;
        }
        if self.text_replacements_editor.is_some() {
            self.commit_text_replacements_editor();
            self.text_replacements_editor = None;
            return true;
        }
        if self.bar_order_editor.is_some() {
            self.commit_bar_order_editor();
            self.bar_order_editor = None;
            return true;
        }
        false
    }

    /// Tab navigation (calls next_field for compatibility)
    pub fn navigate_down(&mut self) {
        self.next_field();
    }

    /// Up arrow navigation (calls previous_field for compatibility)
    pub fn navigate_up(&mut self) {
        self.previous_field();
    }

    /// Check if the currently focused field is a checkbox (fields 12-19)
    pub fn is_on_checkbox(&self) -> bool {
        matches!(
            self.current_field_ref(),
            Some(
                FieldRef::ShowTitle
                    | FieldRef::Locked
                    | FieldRef::TransparentBg
                    | FieldRef::ShowBorder
                    | FieldRef::BorderTop
                    | FieldRef::BorderBottom
                    | FieldRef::BorderLeft
                    | FieldRef::BorderRight
                    | FieldRef::ShowDesc
                    | FieldRef::ShowObjs
                    | FieldRef::ShowPlayers
                    | FieldRef::ShowExits
                    | FieldRef::ShowName
                    | FieldRef::Wordwrap
                    | FieldRef::Timestamps
                    | FieldRef::ProgressNumbersOnly
                    | FieldRef::ProgressCurrentOnly
                    | FieldRef::TabSeparator
                    | FieldRef::DashboardHideInactive
                    | FieldRef::EncumShowLabel
                    | FieldRef::GS4ExpShowLevel
                    | FieldRef::GS4ExpShowExpBar
                    | FieldRef::MiniVitalsNumbersOnly
                    | FieldRef::MiniVitalsCurrentOnly
                    | FieldRef::BetrayerShowItems
                    | FieldRef::TextCompact
                    | FieldRef::TargetsShowArmsCount
            )
        )
    }

    /// Check if the currently focused field is the border style dropdown
    pub fn is_on_border_style(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::BorderStyle))
    }

    /// Check if the currently focused field is the content alignment dropdown
    pub fn is_on_content_align(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::ContentAlign))
    }

    /// Check if the currently focused field is the title alignment dropdown
    pub fn is_on_title_position(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::TitlePosition))
    }

    /// Check if focused on tab bar position dropdown (TabbedText)
    pub fn is_on_tab_bar_position(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::TabBarPosition))
    }

    /// Check if the current field is the Edit Tabs button
    pub fn is_on_edit_tabs(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::EditTabs))
    }

    /// Check if the current field is the Edit Indicators button
    pub fn is_on_edit_indicators(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::EditIndicators))
    }

    /// Check if the current field is the Edit Bar Order button
    pub fn is_on_edit_bar_order(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::MiniVitalsEditBarOrder))
    }

    /// Check if the current field is the Perception Sort Direction dropdown
    pub fn is_on_perception_sort_direction(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::PerceptionSortDirection))
    }

    /// Check if the current field is the Perception Text Replacements button
    pub fn is_on_perception_replacements(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::PerceptionTextReplacements))
    }

    /// Check if the current field is the Perception Short Spell Names checkbox
    pub fn is_on_perception_short_spell_names(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::PerceptionUseShortSpellNames))
    }

    /// Toggle the perception short spell names setting
    pub fn toggle_perception_short_spell_names(&mut self) {
        self.perception_use_short_spell_names = !self.perception_use_short_spell_names;
    }

    /// Check if the current field is the Dashboard Layout dropdown
    pub fn is_on_dashboard_layout(&self) -> bool {
        matches!(self.current_field_ref(), Some(FieldRef::DashboardLayout))
    }

    /// Cycle through dashboard layout options
    pub fn cycle_dashboard_layout(&mut self) {
        let current = self
            .dashboard_layout_input
            .lines()
            .get(0)
            .map(|s| s.as_str())
            .unwrap_or("horizontal")
            .to_lowercase();
        let options = [
            "horizontal",
            "vertical",
            "flow",
            "grid:2x2",
            "grid:2x3",
            "grid:3x3",
        ];
        let idx = options
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&current))
            .unwrap_or(0);
        let next = options[(idx + 1) % options.len()];
        let mut ta = Self::create_textarea();
        ta.insert_str(next);
        self.dashboard_layout_input = ta;
    }

    /// Cycle to the next/previous border style
    pub fn cycle_border_style(&mut self, reverse: bool) {
        let options = ["single", "double", "rounded", "thick"];
        let current = &self.window_def.base().border_style;
        let len = options.len();
        let current_idx = options
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(current))
            .unwrap_or(0);
        let next_idx = if reverse {
            if current_idx == 0 {
                len - 1
            } else {
                current_idx - 1
            }
        } else {
            (current_idx + 1) % len
        };
        self.window_def.base_mut().border_style = options[next_idx].to_string();
    }

    /// Cycle content alignment through the presets
    pub fn cycle_content_align(&mut self, reverse: bool) {
        let current = self.current_content_align_value().to_string();
        let len = CONTENT_ALIGN_OPTIONS.len();
        let current_idx = CONTENT_ALIGN_OPTIONS
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&current))
            .unwrap_or(0);
        let next_idx = if reverse {
            if current_idx == 0 {
                len - 1
            } else {
                current_idx - 1
            }
        } else {
            (current_idx + 1) % len
        };
        let new_value = CONTENT_ALIGN_OPTIONS[next_idx];

        let mut new_input = Self::create_textarea();
        new_input.insert_str(new_value);
        self.content_align_input = new_input;
        self.window_def.base_mut().content_align = Some(new_value.to_string());
    }

    /// Cycle title alignment through the supported positions
    pub fn cycle_title_position(&mut self, reverse: bool) {
        let current = self
            .title_position_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.window_def.base().title_position.clone());

        let len = TITLE_POSITION_OPTIONS.len();
        let current_idx = TITLE_POSITION_OPTIONS
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&current))
            .unwrap_or(0);
        let next_idx = if reverse {
            if current_idx == 0 {
                len - 1
            } else {
                current_idx - 1
            }
        } else {
            (current_idx + 1) % len
        };
        let new_value = TITLE_POSITION_OPTIONS[next_idx];

        let mut ta = Self::create_textarea();
        ta.insert_str(new_value);
        self.title_position_input = ta;
        self.window_def.base_mut().title_position = new_value.to_string();
    }

    /// Cycle tab bar position for tabbed text windows
    pub fn cycle_tab_bar_position(&mut self) {
        let next = match self
            .tab_bar_position_input
            .lines()
            .get(0)
            .map(|s| s.as_str())
            .unwrap_or("top")
        {
            "top" => "bottom",
            _ => "top",
        };
        let mut ta = Self::create_textarea();
        ta.insert_str(next);
        self.tab_bar_position_input = ta;
    }

    /// Cycle perception sort direction
    pub fn cycle_perception_sort_direction(&mut self) {
        let current = self
            .perception_sort_direction_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_else(|| "descending".to_string());

        let len = SORT_DIRECTION_OPTIONS.len();
        let current_idx = SORT_DIRECTION_OPTIONS
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&current))
            .unwrap_or(1); // Default to "descending" if not found
        let next_idx = (current_idx + 1) % len;
        let new_value = SORT_DIRECTION_OPTIONS[next_idx];

        let mut ta = Self::create_textarea();
        ta.insert_str(new_value);
        self.perception_sort_direction_input = ta;
    }

    pub fn input(&mut self, input: ratatui::crossterm::event::KeyEvent) {
        // Route input to appropriate TextArea based on focused_field
        let id = self.focused_field;
        match id {
            _ if id == FieldRef::Name.legacy_field_id() => {
                self.name_input.input(input);
            }
            _ if id == FieldRef::Title.legacy_field_id() => {
                self.title_input.input(input);
            }
            _ if id == FieldRef::Row.legacy_field_id() => {
                self.row_input.input(input);
            }
            _ if id == FieldRef::Col.legacy_field_id() => {
                self.col_input.input(input);
            }
            _ if id == FieldRef::Rows.legacy_field_id() => {
                self.rows_input.input(input);
            }
            _ if id == FieldRef::Cols.legacy_field_id() => {
                self.cols_input.input(input);
            }
            _ if id == FieldRef::MinRows.legacy_field_id() => {
                self.min_rows_input.input(input);
            }
            _ if id == FieldRef::MinCols.legacy_field_id() => {
                self.min_cols_input.input(input);
            }
            _ if id == FieldRef::MaxRows.legacy_field_id() => {
                self.max_rows_input.input(input);
            }
            _ if id == FieldRef::MaxCols.legacy_field_id() => {
                self.max_cols_input.input(input);
            }
            _ if id == FieldRef::BgColor.legacy_field_id() => {
                self.bg_color_input.input(input);
            }
            _ if id == FieldRef::BorderColor.legacy_field_id() => {
                self.border_color_input.input(input);
            }
            _ if id == FieldRef::Streams.legacy_field_id() => {
                self.streams_input.input(input);
            }
            _ if id == FieldRef::TextColor.legacy_field_id() => {
                self.text_color_input.input(input);
            }
            _ if id == FieldRef::CursorColor.legacy_field_id() => {
                self.cursor_color_input.input(input);
            }
            _ if id == FieldRef::CursorBg.legacy_field_id() => {
                self.cursor_bg_input.input(input);
            }
            _ if id == FieldRef::ContentAlign.legacy_field_id() => {
                self.content_align_input.input(input);
            }
            _ if id == FieldRef::TabBarPosition.legacy_field_id() => {
                self.tab_bar_position_input.input(input);
            }
            _ if id == FieldRef::TitlePosition.legacy_field_id() => {
                self.title_position_input.input(input);
            }
            _ if id == FieldRef::TabActiveColor.legacy_field_id() => {
                self.tab_active_color_input.input(input);
            }
            _ if id == FieldRef::TabInactiveColor.legacy_field_id() => {
                self.tab_inactive_color_input.input(input);
            }
            _ if id == FieldRef::TabUnreadColor.legacy_field_id() => {
                self.tab_unread_color_input.input(input);
            }
            _ if id == FieldRef::TabUnreadPrefix.legacy_field_id() => {
                self.tab_unread_prefix_input.input(input);
            }
            _ if id == FieldRef::ProgressId.legacy_field_id() => {
                self.progress_id_input.input(input);
            }
            _ if id == FieldRef::ProgressColor.legacy_field_id() => {
                self.progress_color_input.input(input);
            }
            _ if id == FieldRef::CountdownId.legacy_field_id() => {
                self.countdown_id_input.input(input);
            }
            _ if id == FieldRef::CountdownIcon.legacy_field_id() => {
                self.countdown_icon_input.input(input);
            }
            _ if id == FieldRef::CountdownColor.legacy_field_id() => {
                self.countdown_color_input.input(input);
            }
            _ if id == FieldRef::CountdownBgColor.legacy_field_id() => {
                self.countdown_bg_color_input.input(input);
            }
            _ if id == FieldRef::HandIcon.legacy_field_id() => {
                self.hand_icon_input.input(input);
            }
            _ if id == FieldRef::HandIconColor.legacy_field_id() => {
                self.hand_icon_color_input.input(input);
            }
            _ if id == FieldRef::HandTextColor.legacy_field_id() => {
                self.hand_text_color_input.input(input);
            }
            _ if id == FieldRef::CompassActiveColor.legacy_field_id() => {
                self.compass_active_color_input.input(input);
            }
            _ if id == FieldRef::CompassInactiveColor.legacy_field_id() => {
                self.compass_inactive_color_input.input(input);
            }
            _ if id == FieldRef::InjuryDefaultColor.legacy_field_id() => {
                self.injury_default_color_input.input(input);
            }
            _ if id == FieldRef::Injury1Color.legacy_field_id() => {
                self.injury1_color_input.input(input);
            }
            _ if id == FieldRef::Injury2Color.legacy_field_id() => {
                self.injury2_color_input.input(input);
            }
            _ if id == FieldRef::Injury3Color.legacy_field_id() => {
                self.injury3_color_input.input(input);
            }
            _ if id == FieldRef::Scar1Color.legacy_field_id() => {
                self.scar1_color_input.input(input);
            }
            _ if id == FieldRef::Scar2Color.legacy_field_id() => {
                self.scar2_color_input.input(input);
            }
            _ if id == FieldRef::Scar3Color.legacy_field_id() => {
                self.scar3_color_input.input(input);
            }
            _ if id == FieldRef::MiniVitalsHealthColor.legacy_field_id() => {
                self.minivitals_health_color_input.input(input);
            }
            _ if id == FieldRef::MiniVitalsManaColor.legacy_field_id() => {
                self.minivitals_mana_color_input.input(input);
            }
            _ if id == FieldRef::MiniVitalsStaminaColor.legacy_field_id() => {
                self.minivitals_stamina_color_input.input(input);
            }
            _ if id == FieldRef::MiniVitalsSpiritColor.legacy_field_id() => {
                self.minivitals_spirit_color_input.input(input);
            }
            _ if id == FieldRef::EncumColorLight.legacy_field_id() => {
                self.encum_color_light_input.input(input);
            }
            _ if id == FieldRef::EncumColorModerate.legacy_field_id() => {
                self.encum_color_moderate_input.input(input);
            }
            _ if id == FieldRef::EncumColorHeavy.legacy_field_id() => {
                self.encum_color_heavy_input.input(input);
            }
            _ if id == FieldRef::EncumColorCritical.legacy_field_id() => {
                self.encum_color_critical_input.input(input);
            }
            _ if id == FieldRef::GS4ExpMindBarColor.legacy_field_id() => {
                self.gs4_exp_mind_bar_color_input.input(input);
            }
            _ if id == FieldRef::GS4ExpExpBarColor.legacy_field_id() => {
                self.gs4_exp_exp_bar_color_input.input(input);
            }
            _ if id == FieldRef::BetrayerBarColor.legacy_field_id() => {
                self.betrayer_bar_color_input.input(input);
            }
            _ if id == FieldRef::IndicatorId.legacy_field_id() => {
                self.indicator_id_input.input(input);
            }
            _ if id == FieldRef::IndicatorIcon.legacy_field_id() => {
                self.indicator_icon_input.input(input);
            }
            _ if id == FieldRef::IndicatorActiveColor.legacy_field_id() => {
                self.indicator_active_color_input.input(input);
            }
            _ if id == FieldRef::IndicatorInactiveColor.legacy_field_id() => {
                self.indicator_inactive_color_input.input(input);
            }
            _ if id == FieldRef::ActiveEffectsCategory.legacy_field_id() => {
                self.active_effects_category_input.input(input);
            }
            _ if id == FieldRef::PerceptionSortDirection.legacy_field_id() => {
                // Dropdown field - do not accept text input (use Enter/Space to cycle)
            }
            _ if id == FieldRef::PerceptionTextReplacements.legacy_field_id() => {
                // Button field - do not accept text input (use Enter/Space to activate)
            }
            _ if id == FieldRef::PerceptionUseShortSpellNames.legacy_field_id() => {
                // Checkbox field - do not accept text input (use Enter/Space to toggle)
            }
            _ if id == FieldRef::DashboardLayout.legacy_field_id() => {
                // Dropdown field - do not accept text input (use Enter/Space to cycle)
            }
            _ if id == FieldRef::DashboardSpacing.legacy_field_id() => {
                self.dashboard_spacing_input.input(input);
            }
            _ if id == FieldRef::BufferSize.legacy_field_id() => {
                self.buffer_size_input.input(input);
            }
            _ if id == FieldRef::PromptIcon.legacy_field_id() => {
                self.prompt_icon_input.input(input);
            }
            _ if id == FieldRef::PromptIconColor.legacy_field_id() => {
                self.prompt_icon_color_input.input(input);
            }
            _ if id == FieldRef::EntityId.legacy_field_id() => {
                self.entity_id_input.input(input);
            }
            _ => {} // Checkboxes/dropdowns don't handle text input
        }
    }

    pub fn toggle_field(&mut self) {
        match self.focused_field {
            12 => {
                let current = self.window_def.base().show_title;
                self.window_def.base_mut().show_title = !current;
            }
            13 => {
                let current = self.window_def.base().locked;
                self.window_def.base_mut().locked = !current;
            }
            14 => {
                let current = self.window_def.base().transparent_background;
                self.window_def.base_mut().transparent_background = !current;
            }
            15 => {
                let new_show = !self.window_def.base().show_border;
                let sides = self.window_def.base().border_sides.clone();
                self.window_def
                    .base_mut()
                    .apply_border_configuration(new_show, sides);
                self.refresh_size_inputs();
            }
            16 => {
                let show_border = self.window_def.base().show_border;
                let mut sides = self.window_def.base().border_sides.clone();
                sides.top = !sides.top;
                self.window_def
                    .base_mut()
                    .apply_border_configuration(show_border, sides);
                self.refresh_size_inputs();
            }
            17 => {
                let show_border = self.window_def.base().show_border;
                let mut sides = self.window_def.base().border_sides.clone();
                sides.bottom = !sides.bottom;
                self.window_def
                    .base_mut()
                    .apply_border_configuration(show_border, sides);
                self.refresh_size_inputs();
            }
            18 => {
                let show_border = self.window_def.base().show_border;
                let mut sides = self.window_def.base().border_sides.clone();
                sides.left = !sides.left;
                self.window_def
                    .base_mut()
                    .apply_border_configuration(show_border, sides);
                self.refresh_size_inputs();
            }
            19 => {
                let show_border = self.window_def.base().show_border;
                let mut sides = self.window_def.base().border_sides.clone();
                sides.right = !sides.right;
                self.window_def
                    .base_mut()
                    .apply_border_configuration(show_border, sides);
                self.refresh_size_inputs();
            }
            _ => {
                if let Some(field_ref) = self.current_field_ref() {
                    match field_ref {
                        FieldRef::ShowDesc => {
                            self.show_desc = !self.show_desc;
                        }
                        FieldRef::ShowObjs => {
                            self.show_objs = !self.show_objs;
                        }
                        FieldRef::ShowPlayers => {
                            self.show_players = !self.show_players;
                        }
                        FieldRef::ShowExits => {
                            self.show_exits = !self.show_exits;
                        }
                        FieldRef::ShowName => {
                            self.show_name = !self.show_name;
                        }
                        FieldRef::Wordwrap => {
                            self.text_wordwrap = !self.text_wordwrap;
                        }
                        FieldRef::Timestamps => {
                            self.text_show_timestamps = !self.text_show_timestamps;
                        }
                        FieldRef::TextCompact => {
                            self.text_compact = !self.text_compact;
                        }
                        FieldRef::TargetsShowArmsCount => {
                            self.targets_show_arms_count = !self.targets_show_arms_count;
                        }
                        FieldRef::ProgressNumbersOnly => {
                            self.progress_numbers_only = !self.progress_numbers_only;
                        }
                        FieldRef::ProgressCurrentOnly => {
                            self.progress_current_only = !self.progress_current_only;
                        }
                        FieldRef::TabSeparator => {
                            self.tab_separator = !self.tab_separator;
                        }
                        FieldRef::TabBarPosition => {
                            self.cycle_tab_bar_position();
                        }
                        FieldRef::TitlePosition => {
                            self.cycle_title_position(false);
                        }
                        FieldRef::PerceptionSortDirection => {
                            self.cycle_perception_sort_direction();
                        }
                        FieldRef::EditTabs => {
                            self.open_tab_editor();
                        }
                        FieldRef::EditIndicators => {
                            self.open_indicator_editor();
                        }
                        FieldRef::PerceptionTextReplacements => {
                            self.open_perception_replacements_editor();
                        }
                        FieldRef::MiniVitalsEditBarOrder => {
                            self.open_bar_order_editor();
                        }
                        FieldRef::DashboardHideInactive => {
                            self.dashboard_hide_inactive = !self.dashboard_hide_inactive;
                        }
                        FieldRef::DashboardLayout => {
                            self.cycle_dashboard_layout();
                        }
                        FieldRef::EncumShowLabel => {
                            let prev_show = self.show_label_encum;
                            self.show_label_encum = !self.show_label_encum;
                            // Apply row adjustment for optional content row
                            self.window_def
                                .base_mut()
                                .apply_optional_content_row(self.show_label_encum, prev_show);
                            self.refresh_size_inputs();
                        }
                        FieldRef::GS4ExpShowLevel => {
                            let prev_show = self.gs4_exp_show_level;
                            self.gs4_exp_show_level = !self.gs4_exp_show_level;
                            // Apply row adjustment for optional content row
                            self.window_def
                                .base_mut()
                                .apply_optional_content_row(self.gs4_exp_show_level, prev_show);
                            self.refresh_size_inputs();
                        }
                        FieldRef::GS4ExpShowExpBar => {
                            let prev_show = self.gs4_exp_show_exp_bar;
                            self.gs4_exp_show_exp_bar = !self.gs4_exp_show_exp_bar;
                            // Apply row adjustment for optional content row
                            self.window_def
                                .base_mut()
                                .apply_optional_content_row(self.gs4_exp_show_exp_bar, prev_show);
                            self.refresh_size_inputs();
                        }
                        FieldRef::MiniVitalsNumbersOnly => {
                            self.minivitals_numbers_only = !self.minivitals_numbers_only;
                            // If numbers_only is enabled, disable current_only
                            if self.minivitals_numbers_only {
                                self.minivitals_current_only = false;
                            }
                        }
                        FieldRef::MiniVitalsCurrentOnly => {
                            self.minivitals_current_only = !self.minivitals_current_only;
                            // If current_only is enabled, disable numbers_only
                            if self.minivitals_current_only {
                                self.minivitals_numbers_only = false;
                            }
                        }
                        FieldRef::BetrayerShowItems => {
                            let prev_show = self.betrayer_show_items;
                            self.betrayer_show_items = !self.betrayer_show_items;
                            // Adjust rows based on show_items toggle
                            // When toggling OFF: remove item rows (calculate from current - ideal_without_items)
                            // When toggling ON: add 1 row (minimum item row, auto-resize will correct later)
                            // Read min/max from text inputs (not window_def which may not have latest edits)
                            let min_rows: u16 = self
                                .min_rows_input
                                .lines()
                                .first()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(1);
                            let max_rows: u16 = self
                                .max_rows_input
                                .lines()
                                .first()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(u16::MAX);
                            let base = self.window_def.base_mut();
                            let border_rows = base.horizontal_border_units();
                            let bar_rows = 1u16;
                            if prev_show && !self.betrayer_show_items {
                                // Toggling OFF - set rows to just bar + borders
                                let ideal_rows_off = bar_rows + border_rows;
                                base.rows = ideal_rows_off.max(min_rows).min(max_rows);
                            } else if !prev_show && self.betrayer_show_items {
                                // Toggling ON - add 1 item row (minimum)
                                let ideal_rows_on = bar_rows + 1 + border_rows;
                                base.rows = ideal_rows_on.max(min_rows).min(max_rows);
                            }
                            self.refresh_size_inputs();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn handle_sub_editor_cancel(&mut self) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            if matches!(editor.mode, TabEditorMode::Form) {
                editor.cancel_form();
                return true;
            }
        }
        if let Some(editor) = self.indicator_editor.as_mut() {
            if matches!(editor.mode, IndicatorEditorMode::Form) {
                editor.cancel_form();
                return true;
            }
        }
        if let Some(editor) = self.text_replacements_editor.as_mut() {
            if matches!(editor.mode, TextReplacementsEditorMode::Form) {
                editor.cancel_form();
                return true;
            }
        }
        self.close_sub_editor()
    }

    pub fn handle_sub_editor_navigation(&mut self, down: bool) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            match editor.mode {
                TabEditorMode::List => {
                    if down {
                        if editor.tabs.is_empty() {
                            editor.selected = 0;
                        } else if editor.selected + 1 < editor.tabs.len() {
                            editor.selected += 1;
                        } else {
                            editor.selected = 0; // wrap
                        }
                    } else if !editor.tabs.is_empty() {
                        if editor.selected == 0 {
                            editor.selected = editor.tabs.len().saturating_sub(1);
                        } else {
                            editor.selected -= 1;
                        }
                    }
                }
                TabEditorMode::Form => {
                    editor.form_field = match (editor.form_field, down) {
                        (TabEditorFormField::Name, true) => TabEditorFormField::Streams,
                        (TabEditorFormField::Streams, true) => TabEditorFormField::Timestamps,
                        (TabEditorFormField::Timestamps, true) => {
                            TabEditorFormField::IgnoreActivity
                        }
                        (TabEditorFormField::IgnoreActivity, true) => TabEditorFormField::Name,
                        (TabEditorFormField::Name, false) => TabEditorFormField::IgnoreActivity,
                        (TabEditorFormField::Streams, false) => TabEditorFormField::Name,
                        (TabEditorFormField::Timestamps, false) => TabEditorFormField::Streams,
                        (TabEditorFormField::IgnoreActivity, false) => {
                            TabEditorFormField::Timestamps
                        }
                    };
                }
            }
            return true;
        }

        if let Some(editor) = self.indicator_editor.as_mut() {
            match editor.mode {
                IndicatorEditorMode::List => {
                    if down {
                        if editor.selected + 1 < editor.indicators.len() {
                            editor.selected += 1;
                        }
                    } else if editor.selected > 0 {
                        editor.selected -= 1;
                    }
                }
                IndicatorEditorMode::Form => {
                    editor.form_field = match (editor.form_field, down) {
                        (IndicatorFormField::Id, true) => IndicatorFormField::Icon,
                        (IndicatorFormField::Icon, true) => IndicatorFormField::Colors,
                        (IndicatorFormField::Colors, true) => IndicatorFormField::Id,
                        (IndicatorFormField::Colors, false) => IndicatorFormField::Icon,
                        (IndicatorFormField::Icon, false) => IndicatorFormField::Id,
                        (IndicatorFormField::Id, false) => IndicatorFormField::Colors,
                    };
                }
            }
            return true;
        }

        if let Some(editor) = self.performance_metrics_editor.as_mut() {
            editor.move_selection(down);
            return true;
        }

        if let Some(editor) = self.text_replacements_editor.as_mut() {
            match editor.mode {
                TextReplacementsEditorMode::List => {
                    let len = editor.replacements.len();
                    if len == 0 {
                        editor.selected = 0;
                    } else if down {
                        editor.selected = (editor.selected + 1) % len;
                    } else if editor.selected == 0 {
                        editor.selected = len.saturating_sub(1);
                    } else {
                        editor.selected -= 1;
                    }
                }
                TextReplacementsEditorMode::Form => {
                    editor.form_field = match (editor.form_field, down) {
                        (TextReplacementsFormField::Pattern, true) => TextReplacementsFormField::Replace,
                        (TextReplacementsFormField::Replace, true) => TextReplacementsFormField::Pattern,
                        (TextReplacementsFormField::Pattern, false) => TextReplacementsFormField::Replace,
                        (TextReplacementsFormField::Replace, false) => TextReplacementsFormField::Pattern,
                    };
                }
            }
            return true;
        }

        if let Some(editor) = self.bar_order_editor.as_mut() {
            if down {
                editor.nav_down();
            } else {
                editor.nav_up();
            }
            return true;
        }

        false
    }

    pub fn handle_sub_editor_reorder(&mut self, down: bool) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            if matches!(editor.mode, TabEditorMode::List) {
                if down {
                    editor.move_down();
                } else {
                    editor.move_up();
                }
                return true;
            }
        }

        if let Some(editor) = self.indicator_editor.as_mut() {
            if matches!(editor.mode, IndicatorEditorMode::List) {
                if down {
                    editor.move_down();
                } else {
                    editor.move_up();
                }
                return true;
            }
        }

        if self.performance_metrics_editor.is_some() {
            return true;
        }

        if let Some(editor) = self.text_replacements_editor.as_mut() {
            if matches!(editor.mode, TextReplacementsEditorMode::List) {
                if down {
                    editor.move_down();
                } else {
                    editor.move_up();
                }
                return true;
            }
        }

        if let Some(editor) = self.bar_order_editor.as_mut() {
            if down {
                editor.move_down();
            } else {
                editor.move_up();
            }
            return true;
        }

        false
    }

    pub fn handle_sub_editor_key(&mut self, key_event: TfKeyEvent) -> bool {
        if let Some(editor) = self.tab_editor.as_mut() {
            match editor.mode {
                TabEditorMode::List => match key_event.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        editor.start_add();
                        return true;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
                        editor.start_edit();
                        return true;
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => {
                        editor.delete_selected();
                        return true;
                    }
                    KeyCode::Up => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_up();
                        } else if editor.selected > 0 {
                            editor.selected -= 1;
                        } else if !editor.tabs.is_empty() {
                            editor.selected = editor.tabs.len().saturating_sub(1);
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_down();
                        } else if editor.selected + 1 < editor.tabs.len() {
                            editor.selected += 1;
                        } else if !editor.tabs.is_empty() {
                            editor.selected = 0;
                        }
                        return true;
                    }
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    _ => {}
                },
                TabEditorMode::Form => match key_event.code {
                    KeyCode::Esc => {
                        editor.cancel_form();
                        return true;
                    }
                    KeyCode::Enter => {
                        editor.save_form();
                        return true;
                    }
                    KeyCode::Tab => {
                        self.handle_sub_editor_navigation(true);
                        return true;
                    }
                    KeyCode::BackTab => {
                        self.handle_sub_editor_navigation(false);
                        return true;
                    }
                    KeyCode::Char(' ') => {
                        match editor.form_field {
                            TabEditorFormField::Timestamps => {
                                editor.show_timestamps = !editor.show_timestamps;
                                return true;
                            }
                            TabEditorFormField::IgnoreActivity => {
                                editor.ignore_activity = !editor.ignore_activity;
                                return true;
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        let ct_code = crossterm_bridge::to_crossterm_keycode(key_event.code);
                        let ct_mods =
                            crossterm_bridge::to_crossterm_modifiers(key_event.modifiers);
                        let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                        let ev = textarea_bridge::to_textarea_event(key);
                        match editor.form_field {
                            TabEditorFormField::Name => {
                                editor.name_input.input(ev);
                            }
                            TabEditorFormField::Streams => {
                                editor.streams_input.input(ev);
                            }
                            TabEditorFormField::Timestamps => {
                                editor.show_timestamps = !editor.show_timestamps;
                            }
                            TabEditorFormField::IgnoreActivity => {
                                editor.ignore_activity = !editor.ignore_activity;
                            }
                        };
                        return true;
                    }
                },
            }
        }

        if let Some(editor) = self.indicator_editor.as_mut() {
            match editor.mode {
                IndicatorEditorMode::List => match key_event.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        editor.toggle_selected();
                        return true;
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') => {
                        editor.toggle_selected();
                        return true;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
                        editor.toggle_selected();
                        return true;
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => {
                        editor.delete_selected();
                        return true;
                    }
                    KeyCode::Up => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_up();
                        } else if editor.selected > 0 {
                            editor.selected -= 1;
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_down();
                        } else if editor.selected + 1 < editor.indicators.len() {
                            editor.selected += 1;
                        }
                        return true;
                    }
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    _ => {}
                },
                IndicatorEditorMode::Form => match key_event.code {
                    KeyCode::Esc => {
                        editor.cancel_form();
                        return true;
                    }
                    KeyCode::Enter => {
                        editor.save_form();
                        return true;
                    }
                    KeyCode::Tab => {
                        self.handle_sub_editor_navigation(true);
                        return true;
                    }
                    KeyCode::BackTab => {
                        self.handle_sub_editor_navigation(false);
                        return true;
                    }
                    _ => {
                        let ct_code = crossterm_bridge::to_crossterm_keycode(key_event.code);
                        let ct_mods =
                            crossterm_bridge::to_crossterm_modifiers(key_event.modifiers);
                        let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                        let ev = textarea_bridge::to_textarea_event(key);
                        match editor.form_field {
                            IndicatorFormField::Id => {
                                editor.id_input.input(ev);
                            }
                            IndicatorFormField::Icon => {
                                editor.icon_input.input(ev);
                            }
                            IndicatorFormField::Colors => {
                                editor.colors_input.input(ev);
                            }
                        };
                        return true;
                    }
                },
            }
        }

        if let Some(editor) = self.performance_metrics_editor.as_mut() {
            match key_event.code {
                KeyCode::Up => {
                    editor.move_selection(false);
                    return true;
                }
                KeyCode::Down => {
                    editor.move_selection(true);
                    return true;
                }
                KeyCode::Char('t') | KeyCode::Char('T') | KeyCode::Char(' ') | KeyCode::Enter => {
                    editor.toggle_selected();
                    return true;
                }
                KeyCode::Esc => {
                    self.close_sub_editor();
                    return true;
                }
                _ => {}
            }
        }

        if let Some(editor) = self.text_replacements_editor.as_mut() {
            match editor.mode {
                TextReplacementsEditorMode::List => match key_event.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        editor.start_add();
                        return true;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
                        if !editor.replacements.is_empty() {
                            editor.start_edit();
                        }
                        return true;
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => {
                        editor.delete_selected();
                        return true;
                    }
                    KeyCode::Up => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_up();
                        } else if editor.replacements.is_empty() {
                            editor.selected = 0;
                        } else if editor.selected == 0 {
                            editor.selected = editor.replacements.len().saturating_sub(1);
                        } else {
                            editor.selected -= 1;
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_down();
                        } else if editor.replacements.is_empty() {
                            editor.selected = 0;
                        } else {
                            editor.selected = (editor.selected + 1) % editor.replacements.len();
                        }
                        return true;
                    }
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    _ => {}
                },
                TextReplacementsEditorMode::Form => match key_event.code {
                    KeyCode::Esc => {
                        editor.cancel_form();
                        return true;
                    }
                    KeyCode::Enter => {
                        editor.save_form();
                        return true;
                    }
                    KeyCode::Tab => {
                        self.handle_sub_editor_navigation(true);
                        return true;
                    }
                    KeyCode::BackTab => {
                        self.handle_sub_editor_navigation(false);
                        return true;
                    }
                    _ => {
                        let ct_code = crossterm_bridge::to_crossterm_keycode(key_event.code);
                        let ct_mods =
                            crossterm_bridge::to_crossterm_modifiers(key_event.modifiers);
                        let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                        let ev = textarea_bridge::to_textarea_event(key);
                        match editor.form_field {
                            TextReplacementsFormField::Pattern => {
                                editor.pattern_input.input(ev);
                            }
                            TextReplacementsFormField::Replace => {
                                editor.replace_input.input(ev);
                            }
                        };
                        return true;
                    }
                },
            }
        }

        if let Some(editor) = self.bar_order_editor.as_mut() {
            match editor.focus {
                BarOrderEditorFocus::Toggle => match key_event.code {
                    KeyCode::Up => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_up();
                        } else {
                            editor.nav_up();
                            editor.sync_color_input_from_bar();
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        if key_event.modifiers.contains_shift() {
                            editor.move_down();
                        } else {
                            editor.nav_down();
                            editor.sync_color_input_from_bar();
                        }
                        return true;
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') | KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Char(' ') | KeyCode::Enter => {
                        if !editor.toggle_selected() {
                            self.status_message = format!(
                                "Max {} bars enabled. Disable one first.",
                                BarOrderEditor::MAX_ENABLED
                            );
                        }
                        return true;
                    }
                    KeyCode::Tab | KeyCode::Right => {
                        // T→C: Move to color column (same row)
                        editor.focus_color();
                        return true;
                    }
                    KeyCode::BackTab | KeyCode::Left => {
                        // Go to previous row's color (wrap to last if at first)
                        if editor.selected > 0 {
                            editor.selected -= 1;
                        } else {
                            editor.selected = editor.bars.len().saturating_sub(1);
                        }
                        editor.sync_color_input_from_bar();
                        editor.focus_color();
                        return true;
                    }
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    _ => {}
                },
                BarOrderEditorFocus::Color => match key_event.code {
                    KeyCode::Esc => {
                        self.close_sub_editor();
                        return true;
                    }
                    KeyCode::Tab | KeyCode::Right => {
                        // C→T: Save color, move to next row's toggle (wrap to first if at last)
                        editor.save_color_to_bar();
                        if editor.selected + 1 < editor.bars.len() {
                            editor.selected += 1;
                        } else {
                            editor.selected = 0;
                        }
                        editor.sync_color_input_from_bar();
                        editor.focus_toggle();
                        return true;
                    }
                    KeyCode::BackTab | KeyCode::Left => {
                        // C→T: Save and move back to same row's toggle
                        editor.focus_toggle();
                        return true;
                    }
                    KeyCode::Up => {
                        // Save current, move to previous bar's color
                        editor.save_color_to_bar();
                        editor.nav_up();
                        editor.sync_color_input_from_bar();
                        return true;
                    }
                    KeyCode::Down | KeyCode::Enter => {
                        // Save current, move to next bar's color
                        editor.save_color_to_bar();
                        editor.nav_down();
                        editor.sync_color_input_from_bar();
                        return true;
                    }
                    _ => {
                        // Forward keypress to color input textarea
                        let ct_code = crossterm_bridge::to_crossterm_keycode(key_event.code);
                        let ct_mods =
                            crossterm_bridge::to_crossterm_modifiers(key_event.modifiers);
                        let key = crossterm::event::KeyEvent::new(ct_code, ct_mods);
                        let ev = textarea_bridge::to_textarea_event(key);
                        editor.color_input.input(ev);
                        return true;
                    }
                },
            }
        }

        false
    }

    pub fn sync_to_window_def(&mut self) {
        self.commit_sub_editors();
        self.window_def.base_mut().name = self.name_input.lines()[0].to_string();
        self.window_def.base_mut().title =
            Some(self.title_input.lines()[0].to_string()).filter(|s| !s.is_empty());
        self.window_def.base_mut().row = self.row_input.lines()[0].parse().unwrap_or(0);
        self.window_def.base_mut().col = self.col_input.lines()[0].parse().unwrap_or(0);
        // Rows/cols is now total size (VellumFE style), not content size
        // User specifies actual widget dimensions; content adjusts based on borders
        let total_rows = self.rows_input.lines().first()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(1);
        let total_cols = self.cols_input.lines().first()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(40);
        self.window_def.base_mut().rows = total_rows.max(1);
        self.window_def.base_mut().cols = total_cols.max(1);
        self.window_def.base_mut().min_rows = self.min_rows_input.lines()[0].parse().ok();
        self.window_def.base_mut().min_cols = self.min_cols_input.lines()[0].parse().ok();
        self.window_def.base_mut().max_rows = self.max_rows_input.lines()[0].parse().ok();
        self.window_def.base_mut().max_cols = self.max_cols_input.lines()[0].parse().ok();
        self.window_def.base_mut().background_color =
            Some(self.bg_color_input.lines()[0].to_string()).filter(|s| !s.is_empty());
        self.window_def.base_mut().border_color =
            Some(self.border_color_input.lines()[0].to_string()).filter(|s| !s.is_empty());
        if matches!(self.window_def, crate::config::WindowDef::Progress { .. }) {
            self.window_def.base_mut().text_color =
                Some(self.text_color_input.lines()[0].to_string()).filter(|s| !s.is_empty());
        }
        self.window_def.base_mut().title_position = self
            .title_position_input
            .lines()
            .get(0)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "top-left".to_string());
        self.window_def.base_mut().content_align =
            Some(self.content_align_input.lines()[0].to_string()).filter(|s| !s.is_empty());

        // Update streams only for Text variant
        if let crate::config::WindowDef::Text { data, .. } = &mut self.window_def {
            let streams: Vec<String> = self.streams_input.lines()[0]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            data.streams = streams;
            data.buffer_size = self
                .buffer_size_input
                .lines()
                .get(0)
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(data.buffer_size);
            data.wordwrap = self.text_wordwrap;
            data.show_timestamps = self.text_show_timestamps;
            data.compact = self.text_compact;
        }

        if let crate::config::WindowDef::Inventory { data, .. } = &mut self.window_def {
            let streams: Vec<String> = self.streams_input.lines()[0]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            data.streams = streams;
            data.buffer_size = self
                .buffer_size_input
                .lines()
                .get(0)
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(data.buffer_size);
            data.wordwrap = self.text_wordwrap;
            data.show_timestamps = self.text_show_timestamps;
        }

        if let crate::config::WindowDef::TabbedText { data, .. } = &mut self.window_def {
            data.tab_bar_position = self
                .tab_bar_position_input
                .lines()
                .get(0)
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "top".to_string());
            data.tab_active_color = self
                .tab_active_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.tab_inactive_color = self
                .tab_inactive_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.tab_unread_color = self
                .tab_unread_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.tab_unread_prefix = self
                .tab_unread_prefix_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.tab_separator = self.tab_separator;
        }

        if let crate::config::WindowDef::Room { data, .. } = &mut self.window_def {
            data.show_desc = self.show_desc;
            data.show_objs = self.show_objs;
            data.show_players = self.show_players;
            data.show_exits = self.show_exits;
            data.show_name = self.show_name;
        }

        if let crate::config::WindowDef::Perception { data, .. } = &mut self.window_def {
            // Stream is ALWAYS "percWindow" - hardcoded, not user-editable
            data.stream = "percWindow".to_string();

            // Buffer size is ALWAYS 100 - hardcoded (window clears on each update)
            data.buffer_size = 100;

            // Parse sort direction
            data.sort_direction = match self.perception_sort_direction_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_lowercase())
                .as_deref()
            {
                Some("ascending") => crate::config::SortDirection::Ascending,
                _ => crate::config::SortDirection::Descending,
            };

            // Short spell names toggle
            data.use_short_spell_names = self.perception_use_short_spell_names;

            // text_replacements are handled by the TextReplacementsEditor
        }

        if let crate::config::WindowDef::Progress { data, .. } = &mut self.window_def {
            data.id = self
                .progress_id_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.color = self
                .progress_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.numbers_only = self.progress_numbers_only;
            data.current_only = self.progress_current_only;
        }

        if let crate::config::WindowDef::Encumbrance { data, .. } = &mut self.window_def {
            data.show_label = self.show_label_encum;
            data.color_light = self.encum_color_light_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.color_moderate = self.encum_color_moderate_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.color_heavy = self.encum_color_heavy_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.color_critical = self.encum_color_critical_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::GS4Experience { data, .. } = &mut self.window_def {
            data.show_level = self.gs4_exp_show_level;
            data.show_exp_bar = self.gs4_exp_show_exp_bar;
            data.mind_bar_color = self.gs4_exp_mind_bar_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.exp_bar_color = self.gs4_exp_exp_bar_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::MiniVitals { data, .. } = &mut self.window_def {
            data.numbers_only = self.minivitals_numbers_only;
            data.current_only = self.minivitals_current_only;
            data.health_color = self.minivitals_health_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.mana_color = self.minivitals_mana_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.stamina_color = self.minivitals_stamina_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.spirit_color = self.minivitals_spirit_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Betrayer { data, .. } = &mut self.window_def {
            data.show_items = self.betrayer_show_items;
            data.bar_color = self.betrayer_bar_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Countdown { data, .. } = &mut self.window_def {
            data.id = self
                .countdown_id_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.icon = self
                .countdown_icon_input
                .lines()
                .get(0)
                .and_then(|s| s.chars().next());
            data.color = self
                .countdown_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Hand { data, .. } = &mut self.window_def {
            data.icon = self
                .hand_icon_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.icon_color = self
                .hand_icon_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.text_color = self
                .hand_text_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Compass { data, .. } = &mut self.window_def {
            data.active_color = self
                .compass_active_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.inactive_color = self
                .compass_inactive_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::InjuryDoll { data, .. } = &mut self.window_def {
            data.injury_default_color = self
                .injury_default_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.injury1_color = self
                .injury1_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.injury2_color = self
                .injury2_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.injury3_color = self
                .injury3_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.scar1_color = self
                .scar1_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.scar2_color = self
                .scar2_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.scar3_color = self
                .scar3_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Indicator { data, .. } = &mut self.window_def {
            data.indicator_id = self
                .indicator_id_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.icon = self
                .indicator_icon_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.active_color = self
                .indicator_active_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            data.inactive_color = self
                .indicator_inactive_color_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }

        if let crate::config::WindowDef::Dashboard { data, .. } = &mut self.window_def {
            data.layout = self
                .dashboard_layout_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "horizontal".to_string());
            data.spacing = self
                .dashboard_spacing_input
                .lines()
                .get(0)
                .and_then(|s| s.trim().parse::<u16>().ok())
                .unwrap_or(1);
            data.hide_inactive = self.dashboard_hide_inactive;
        }

        if let crate::config::WindowDef::ActiveEffects { data, .. } = &mut self.window_def {
            data.category = self
                .active_effects_category_input
                .lines()
                .get(0)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "ActiveSpells".to_string());
        }

        if let crate::config::WindowDef::Performance { data, .. } = &mut self.window_def {
            data.enabled = self.perf_enabled;
            data.show_fps = self.perf_show_fps;
            data.show_frame_times = self.perf_show_frame_times;
            data.show_render_times = self.perf_show_render_times;
            data.show_ui_times = self.perf_show_ui_times;
            data.show_wrap_times = self.perf_show_wrap_times;
            data.show_net = self.perf_show_net;
            data.show_parse = self.perf_show_parse;
            data.show_events = self.perf_show_events;
            data.show_memory = self.perf_show_memory;
            data.show_lines = self.perf_show_lines;
            data.show_uptime = self.perf_show_uptime;
            data.show_jitter = self.perf_show_jitter;
            data.show_frame_spikes = self.perf_show_frame_spikes;
            data.show_event_lag = self.perf_show_event_lag;
            data.show_memory_delta = self.perf_show_memory_delta;
        }

        if let crate::config::WindowDef::CommandInput { data, .. } = &mut self.window_def {
            data.prompt_icon = Some(self.prompt_icon_input.lines()[0].trim().to_string())
                .filter(|s| !s.is_empty());
            data.prompt_icon_color =
                Some(self.prompt_icon_color_input.lines()[0].trim().to_string())
                    .filter(|s| !s.is_empty());
            data.text_color =
                Some(self.text_color_input.lines()[0].trim().to_string()).filter(|s| !s.is_empty());
            data.cursor_color = Some(self.cursor_color_input.lines()[0].trim().to_string())
                .filter(|s| !s.is_empty());
            data.cursor_background_color =
                Some(self.cursor_bg_input.lines()[0].trim().to_string()).filter(|s| !s.is_empty());
        }
        if let crate::config::WindowDef::Targets { data, .. } = &mut self.window_def {
            data.entity_id = self.entity_id_input.lines()[0].trim().to_string();
            data.show_body_part_count = self.targets_show_arms_count;
        }
        if let crate::config::WindowDef::Players { data, .. } = &mut self.window_def {
            data.entity_id = self.entity_id_input.lines()[0].trim().to_string();
        }
    }

    pub fn get_window_def(&mut self) -> &WindowDef {
        self.sync_to_window_def();
        &self.window_def
    }

    /// Validate before saving (name required, no duplicates when creating/renaming).
    pub fn validate_before_save(&mut self, layout: &crate::config::Layout) -> bool {
        self.sync_to_window_def();

        // Trim the name and write it back to the model
        let trimmed = self.window_def.name().trim().to_string();
        self.window_def.base_mut().name = trimmed.clone();

        if trimmed.is_empty() {
            self.status_message = "Name is required to save".to_string();
            return false;
        }

        // If this is a new window or the name changed, ensure uniqueness
        let original_name = self.original_window_def.name();
        if self.is_new || !trimmed.eq_ignore_ascii_case(original_name) {
            if layout
                .windows
                .iter()
                .any(|w| w.name().eq_ignore_ascii_case(&trimmed))
            {
                self.status_message = format!("Name '{}' is already in use", trimmed);
                return false;
            }
        }

        // Clear any previous warning
        self.status_message = "Tab/Shift+Tab: Navigate | Ctrl+S: Save | Esc: Cancel".to_string();
        true
    }

    pub fn is_new(&self) -> bool {
        self.is_new
    }

    /// The name of the template/window the editor was created from
    pub fn original_name(&self) -> &str {
        self.original_window_def.name()
    }

    pub fn cancel(&mut self) {
        self.window_def = self.original_window_def.clone();
    }

    /// Get the current editor window position and size for persistence
    pub fn get_editor_geometry(&self) -> (u16, u16, u16, u16) {
        (self.popup_x, self.popup_y, self.popup_width, self.popup_height)
    }

    pub fn handle_mouse(&mut self, mouse_col: u16, mouse_row: u16, mouse_down: bool, area: Rect) -> WindowEditorMouseAction {
        if !mouse_down {
            self.dragging = false;
            return WindowEditorMouseAction::None;
        }

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: self.popup_width,
            height: self.popup_height,
        };

        // Check if mouse is on the title bar (for dragging)
        let on_title_bar = mouse_row == self.popup_y
            && mouse_col > popup_area.x
            && mouse_col < popup_area.x + popup_area.width.saturating_sub(1);

        // Start drag if on title bar
        if on_title_bar && !self.dragging {
            self.dragging = true;
            self.drag_offset_x = mouse_col.saturating_sub(self.popup_x);
            self.drag_offset_y = mouse_row.saturating_sub(self.popup_y);
        }

        // Handle dragging
        if self.dragging {
            self.popup_x = mouse_col.saturating_sub(self.drag_offset_x);
            self.popup_y = mouse_row.saturating_sub(self.drag_offset_y);
            self.popup_x = self.popup_x.min(area.width.saturating_sub(self.popup_width));
            self.popup_y = self.popup_y.min(area.height.saturating_sub(self.popup_height));
            return WindowEditorMouseAction::None; // Don't process field clicks while dragging
        }

        // Check if click is inside the popup (but not on title bar)
        let inside_popup = mouse_col >= popup_area.x
            && mouse_col < popup_area.x + popup_area.width
            && mouse_row > popup_area.y  // Skip title bar row
            && mouse_row < popup_area.y + popup_area.height;

        if !inside_popup {
            return WindowEditorMouseAction::None;
        }

        // Check if click is on the footer (bottom border with Save/Cancel)
        let footer_y = popup_area.y + popup_area.height.saturating_sub(1);
        if mouse_row == footer_y {
            // Footer text: "[Ctrl+S: Save] [Esc: Cancel]"
            // Check relative x position within the popup
            let rel_x = mouse_col.saturating_sub(popup_area.x);

            // Save is roughly in the left portion, Cancel in the right
            // "[Ctrl+S: Save]" starts around x=1, "Save" is around x=9-12
            // "[Esc: Cancel]" starts around x=16, "Cancel" is around x=22-27
            if rel_x >= 1 && rel_x <= 14 {
                return WindowEditorMouseAction::Save;
            } else if rel_x >= 16 && rel_x <= 28 {
                return WindowEditorMouseAction::Cancel;
            }
        }

        // Handle bar order editor clicks
        if let Some(ref mut editor) = self.bar_order_editor {
            if editor.handle_mouse_click(mouse_col, mouse_row) {
                return WindowEditorMouseAction::None;
            }
        }

        // Check if click is on a tracked field area
        // field_click_areas contains (y, x_start, field_ref)
        // We match by y (exact row) and distinguish side-by-side fields by x
        let left_column_end = self.popup_x + 37;  // Left column ends at x=37 relative to popup
        let geom_x2 = self.popup_x + 17;  // Divider for side-by-side geometry fields (Row/Col, etc.)

        // Find all fields on this row
        let fields_on_row: Vec<_> = self.field_click_areas.iter()
            .filter(|(y, _, _)| *y == mouse_row)
            .collect();

        for &(field_y, field_x, ref field_ref) in &self.field_click_areas {
            if mouse_row == field_y {
                // Check if there are multiple fields on this row (side-by-side)
                let multiple_on_row = fields_on_row.len() > 1;

                let in_field_region = if field_x >= left_column_end {
                    // Right column field: accept clicks in right half
                    mouse_col >= left_column_end
                } else if multiple_on_row && field_x < left_column_end {
                    // Left column with side-by-side fields (Row/Col, Rows/Cols, etc.)
                    // First field is at left_x, second at geom_x2
                    if field_x < geom_x2 {
                        // First field (Row, Rows, Min, Max): accept clicks up to geom_x2
                        mouse_col >= self.popup_x && mouse_col < geom_x2
                    } else {
                        // Second field (Col, Cols): accept clicks from geom_x2 onwards in left column
                        mouse_col >= geom_x2 && mouse_col < left_column_end
                    }
                } else {
                    // Single field in left column: accept clicks in left half
                    mouse_col >= self.popup_x && mouse_col < left_column_end
                };

                if in_field_region {
                    // Update focused field
                    self.focused_field = field_ref.legacy_field_id();
                    // Also update current_field_index to match
                    if let Some(idx) = self.field_order.iter().position(|f| f == field_ref) {
                        self.current_field_index = idx;
                    }

                    // Toggle checkboxes when clicked
                    if self.is_on_checkbox() {
                        self.toggle_field();
                    } else if self.is_on_content_align() {
                        self.cycle_content_align(false);
                    } else if self.is_on_title_position() {
                        self.cycle_title_position(false);
                    } else if self.is_on_border_style() {
                        self.cycle_border_style(false);
                    } else if self.is_on_dashboard_layout() {
                        self.cycle_dashboard_layout();
                    } else if self.is_on_edit_indicators() {
                        self.open_indicator_editor();
                    } else if self.is_on_edit_bar_order() {
                        self.open_bar_order_editor();
                    } else if self.is_on_perception_sort_direction() {
                        self.cycle_perception_sort_direction();
                    } else if self.is_on_perception_replacements() {
                        self.open_perception_replacements_editor();
                    }

                    return WindowEditorMouseAction::None;
                }
            }
        }

        WindowEditorMouseAction::None
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, theme: &EditorTheme) {
        // Center the popup on first render
        if self.popup_x == 0 && self.popup_y == 0 {
            self.popup_x = (area.width.saturating_sub(self.popup_width)) / 2;
            self.popup_y = (area.height.saturating_sub(self.popup_height)) / 2;
        }

        // Constrain position to screen bounds
        self.popup_x = self.popup_x.min(area.width.saturating_sub(self.popup_width));
        self.popup_y = self.popup_y.min(area.height.saturating_sub(self.popup_height));

        let popup_area = Rect {
            x: self.popup_x,
            y: self.popup_y,
            width: self.popup_width,
            height: self.popup_height,
        };

        Clear.render(popup_area, buf);

        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                if x < area.width && y < area.height {
                    let cell = &mut buf[(x, y)];
                    cell.set_char(' ').set_bg(Color::Black);
                }
            }
        }

        let title = if self.is_new {
            " Add Window "
        } else {
            " Edit Window "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .style(Style::default().bg(Color::Black).fg(crossterm_bridge::to_ratatui_color(theme.border_color)));
        block.render(popup_area, buf);

        // Draw combined bottom border with footer hints
        let inner_width = popup_area.width.saturating_sub(2);
        let help = self.footer_help_text();
        // Use chars().count() not len() - help contains multi-byte Unicode chars like "─"
        let pad_len = inner_width.saturating_sub(1 + help.chars().count() as u16) as usize;
        let pad = "─".repeat(pad_len);
        let mut interior = String::from("─");
        interior.push_str(help);
        interior.push_str(&pad);
        let mut footer_line = String::new();
        footer_line.push('└');
        footer_line.push_str(&interior.chars().take(inner_width as usize).collect::<String>());
        footer_line.push('┘');
        buf.set_string(
            popup_area.x,
            popup_area.y + popup_area.height.saturating_sub(1),
            footer_line,
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.border_color)),
        );

        let content = Rect {
            x: popup_area.x + 1,
            y: popup_area.y + 1,
            width: popup_area.width.saturating_sub(2),
            height: popup_area.height.saturating_sub(2),
        };

        if self.is_sub_editor_active() {
            self.render_sub_editor(content, buf, theme);
        } else {
            self.render_fields(content, buf, theme);
        }

    }

    fn render_sub_editor(&mut self, area: Rect, buf: &mut Buffer, theme: &EditorTheme) {
        if let Some(mut editor) = self.tab_editor.take() {
            self.render_tab_editor(area, buf, theme, &mut editor);
            self.tab_editor = Some(editor);
            return;
        }

        if let Some(mut editor) = self.indicator_editor.take() {
            self.render_indicator_editor(area, buf, theme, &mut editor);
            self.indicator_editor = Some(editor);
            return;
        }

        if let Some(mut editor) = self.performance_metrics_editor.take() {
            self.render_performance_metrics_editor(area, buf, theme, &mut editor);
            self.performance_metrics_editor = Some(editor);
            return;
        }

        if let Some(mut editor) = self.text_replacements_editor.take() {
            self.render_text_replacements_editor(area, buf, theme, &mut editor);
            self.text_replacements_editor = Some(editor);
            return;
        }

        if let Some(mut editor) = self.bar_order_editor.take() {
            self.render_bar_order_editor(area, buf, theme, &mut editor);
            self.bar_order_editor = Some(editor);
        }
    }

    fn render_bar_order_editor(
        &self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut BarOrderEditor,
    ) {
        let header_style =
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.section_header_color));
        let label_style = Style::default()
            .fg(crossterm_bridge::to_ratatui_color(theme.label_color));
        let focused_style = Style::default()
            .fg(crossterm_bridge::to_ratatui_color(theme.focused_label_color));
        let text_style = Style::default()
            .fg(crossterm_bridge::to_ratatui_color(theme.text_color));
        let cursor_style = Style::default()
            .fg(ratatui::style::Color::Black)
            .bg(crossterm_bridge::to_ratatui_color(theme.cursor_color));

        // Clear click areas for fresh population
        editor.click_areas.clear();

        // Title with enabled count indicator
        let title = format!(
            "Bar Order Editor ({}/{} enabled)",
            editor.enabled_count(),
            BarOrderEditor::MAX_ENABLED
        );
        buf.set_string(area.x + 1, area.y, &title, header_style);

        // Two-column layout:
        // Left column: "  [x] Health" (toggles)
        // Right column: "██ red_____" (color preview + input)
        let toggle_col_x = area.x + 1;
        let color_col_x = area.x + 22; // Start color column after toggle column
        let color_input_width = 12usize; // Width for color input

        let max_rows = area.height.saturating_sub(2);
        for (idx, bar) in editor.bars.iter().enumerate() {
            if idx as u16 >= max_rows {
                break;
            }
            let y = area.y + 1 + idx as u16;
            let is_sel = idx == editor.selected;
            let is_toggle_focus = is_sel && editor.focus == BarOrderEditorFocus::Toggle;
            let is_color_focus = is_sel && editor.focus == BarOrderEditorFocus::Color;

            // Left column: Toggle checkbox
            let prefix = if is_toggle_focus { "→ " } else { "  " };
            let checkbox = if bar.enabled { "[x]" } else { "[ ]" };
            let toggle_style = if is_toggle_focus { focused_style } else { label_style };
            let toggle_text = format!("{}{} {}", prefix, checkbox, bar.label);
            buf.set_string(toggle_col_x, y, &toggle_text, toggle_style);

            // Store toggle click area
            let toggle_rect = (toggle_col_x, y, 20, 1);

            // Right column: Color preview swatch + color input
            let color_str = if is_color_focus {
                // Show current input for selected bar
                editor.color_input.lines().join("")
            } else {
                bar.color.clone()
                    .unwrap_or_else(|| BarOrderEditor::default_color_for_id(&bar.id).to_string())
            };

            // Draw color preview swatch (2 chars)
            let preview_color_str = if color_str.trim().is_empty() {
                BarOrderEditor::default_color_for_id(&bar.id)
            } else {
                color_str.trim()
            };
            if let Some(ratatui_color) = super::colors::parse_color_to_ratatui(preview_color_str) {
                let swatch_style = Style::default().bg(ratatui_color);
                buf.set_string(color_col_x, y, "  ", swatch_style);
            } else {
                buf.set_string(color_col_x, y, "??", label_style);
            }

            // Draw color input field
            let input_x = color_col_x + 3;
            if is_color_focus {
                // Render with cursor for active editing
                let cursor_pos = editor.color_input.cursor().1;
                let chars: Vec<char> = color_str.chars().collect();

                for (i, ch) in chars.iter().enumerate().take(color_input_width) {
                    let x = input_x + i as u16;
                    if x < area.x + area.width {
                        if i == cursor_pos {
                            buf.set_string(x, y, &ch.to_string(), cursor_style);
                        } else {
                            buf.set_string(x, y, &ch.to_string(), focused_style);
                        }
                    }
                }
                // Cursor at end
                if cursor_pos >= chars.len() {
                    let x = input_x + chars.len() as u16;
                    if x < area.x + area.width {
                        buf.set_string(x, y, " ", cursor_style);
                    }
                }
                // Fill remaining with underscores
                let filled = chars.len().min(color_input_width);
                let start = if cursor_pos >= chars.len() { filled + 1 } else { filled };
                for i in start..color_input_width {
                    let x = input_x + i as u16;
                    if x < area.x + area.width {
                        buf.set_string(x, y, "_", text_style);
                    }
                }
            } else {
                // Just show the color value
                let display: String = color_str.chars().take(color_input_width).collect();
                let padded = format!("{:<width$}", display, width = color_input_width);
                let style = if is_sel { focused_style } else { text_style };
                buf.set_string(input_x, y, &padded, style);
            }

            // Store color click area
            let color_rect = (color_col_x, y, (3 + color_input_width) as u16, 1);

            // Add click areas for this bar
            editor.click_areas.push((idx, toggle_rect, color_rect));
        }
    }

    fn render_tab_editor(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut TabEditor,
    ) {
        let header_style =
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.section_header_color));
        buf.set_string(area.x + 1, area.y, "Tab Editor", header_style);

        match editor.mode {
            TabEditorMode::List => {
                let max_rows = area.height.saturating_sub(2);
                let available_width = area.width.saturating_sub(2) as usize;
                let name_col_width = available_width
                    .saturating_sub(6)
                    .min(24)
                    .max(available_width.min(8));
                let stream_col_width = available_width.saturating_sub(name_col_width + 4);
                for (idx, tab) in editor.tabs.iter().enumerate() {
                    if idx as u16 >= max_rows {
                        break;
                    }
                    let y = area.y + 1 + idx as u16;
                    let is_sel = idx == editor.selected;
                    let prefix = if is_sel { "> " } else { "  " };
                    let color = if is_sel {
                        crossterm_bridge::to_ratatui_color(theme.focused_label_color)
                    } else {
                        crossterm_bridge::to_ratatui_color(theme.label_color)
                    };
                    let stream_display = if tab.streams.is_empty() {
                        "-".to_string()
                    } else {
                        tab.streams.join(", ")
                    };
                    let name_text: String = tab.name.chars().take(name_col_width).collect();
                    let stream_text: String = stream_display
                        .chars()
                        .take(stream_col_width)
                        .collect();
                    let line = format!(
                        "{}{:name_width$} ->  {}",
                        prefix,
                        name_text,
                        stream_text,
                        name_width = name_col_width
                    );
                    buf.set_string(
                        area.x + 1,
                        y,
                        self.truncate_to_width(&line, available_width as u16),
                        Style::default().fg(color),
                    );
                }

            }
            TabEditorMode::Form => {
                let y = area.y + 2;
                self.render_tab_editor_input(
                    "Tab Name",
                    &editor.name_input,
                    area.x + 1,
                    y,
                    area.width.saturating_sub(2),
                    buf,
                    theme,
                    matches!(editor.form_field, TabEditorFormField::Name),
                );
                self.render_tab_editor_input(
                    "Stream",
                    &editor.streams_input,
                    area.x + 1,
                    y + 1,
                    area.width.saturating_sub(2),
                    buf,
                    theme,
                    matches!(editor.form_field, TabEditorFormField::Streams),
                );

                let ts_label = "Timestamps";
                self.render_tab_editor_checkbox(
                    ts_label,
                    editor.show_timestamps,
                    area.x + 1,
                    y + 2,
                    buf,
                    theme,
                    matches!(editor.form_field, TabEditorFormField::Timestamps),
                );

                let ignore_label = "Ignore Activity";
                self.render_tab_editor_checkbox(
                    ignore_label,
                    editor.ignore_activity,
                    area.x + 1,
                    y + 3,
                    buf,
                    theme,
                    matches!(editor.form_field, TabEditorFormField::IgnoreActivity),
                );
            }
        }
    }

    fn render_indicator_editor(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut IndicatorEditor,
    ) {
        let header_style =
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.section_header_color));
        buf.set_string(area.x + 1, area.y, "Indicator Selector", header_style);

        match editor.mode {
            IndicatorEditorMode::List => {
                let max_rows = area.height.saturating_sub(2);
                for (idx, ind) in editor.indicators.iter().enumerate() {
                    if idx as u16 >= max_rows {
                        break;
                    }
                    let y = area.y + 1 + idx as u16;
                    let is_sel = idx == editor.selected;
                    let prefix = if is_sel { "> " } else { "  " };
                    let color = if is_sel {
                        crossterm_bridge::to_ratatui_color(theme.focused_label_color)
                    } else {
                        crossterm_bridge::to_ratatui_color(theme.label_color)
                    };
                    let icon = if let Some(ch) = Self::parse_icon_char(&ind.icon) {
                        ch.to_string()
                    } else if ind.icon.is_empty() {
                        "?".to_string()
                    } else {
                        ind.icon.clone()
                    };
                    let enabled_marker = if ind.enabled { "[x]" } else { "[ ]" };
                    let mut line = format!("{}{} {} {}", prefix, enabled_marker, icon, ind.id);
                    let max_width = area.width.saturating_sub(2) as usize;
                    if line.chars().count() > max_width {
                        line = line.chars().take(max_width).collect();
                    }
                    let mut style = Style::default().fg(color);
                    if !ind.enabled {
                        style = style.add_modifier(Modifier::DIM);
                    }
                    buf.set_string(area.x + 1, y, line, style);
                }
            }
            IndicatorEditorMode::Form => {
                let y = area.y + 1;
                self.render_textarea_compact(
                    0,
                    "Id:",
                    &editor.id_input,
                    area.x + 1,
                    y,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, IndicatorFormField::Id),
                );
                self.render_textarea_compact(
                    0,
                    "Icon:",
                    &editor.icon_input,
                    area.x + 1,
                    y + 2,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, IndicatorFormField::Icon),
                );
                self.render_textarea_compact(
                    0,
                    "Colors:",
                    &editor.colors_input,
                    area.x + 1,
                    y + 4,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, IndicatorFormField::Colors),
                );
                let value = editor
                    .colors_input
                    .lines()
                    .get(0)
                    .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
                    .unwrap_or_default();
                let preview_x =
                    area.x + 1 + 2 + "Colors:".len() as u16 + 1 + 10;
                self.render_color_preview(&value, preview_x, y + 4, buf, theme);

                let footer = "Enter: Save | Esc: Cancel | Tab/Shift+Tab: Next/Prev";
                let footer_style =
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.label_color));
                buf.set_string(
                    area.x + 1,
                    area.y + area.height.saturating_sub(1),
                    self.truncate_to_width(footer, area.width.saturating_sub(2)),
                    footer_style,
                );
            }
        }
    }

    fn render_performance_metrics_editor(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut PerformanceMetricsEditor,
    ) {
        let header_style =
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.section_header_color));
        buf.set_string(area.x + 1, area.y, "Performance Metrics", header_style);

        let max_rows = area.height.saturating_sub(2);
        for (idx, item) in editor.items.iter().enumerate() {
            if idx as u16 >= max_rows {
                break;
            }
            let y = area.y + 1 + idx as u16;
            let is_sel = idx == editor.selected;
            let prefix = if is_sel { "> " } else { "  " };
            let marker = if item.enabled { "[x]" } else { "[ ]" };
            let color = if is_sel {
                crossterm_bridge::to_ratatui_color(theme.focused_label_color)
            } else {
                crossterm_bridge::to_ratatui_color(theme.label_color)
            };
            let line = format!("{}{} {}", prefix, marker, item.group.label());
            buf.set_string(
                area.x + 1,
                y,
                self.truncate_to_width(&line, area.width.saturating_sub(2)),
                Style::default().fg(color),
            );
        }
    }

    fn render_text_replacements_editor(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        theme: &EditorTheme,
        editor: &mut TextReplacementsEditor,
    ) {
        let header_style =
            Style::default().fg(crossterm_bridge::to_ratatui_color(theme.section_header_color));
        buf.set_string(area.x + 1, area.y, "Text Replacements Editor", header_style);

        match editor.mode {
            TextReplacementsEditorMode::List => {
                // Footer help is shown in the border, no need for separate footer here

                if editor.replacements.is_empty() {
                    let empty_msg = "(No replacements defined - press 'a' to add)";
                    let msg_style = Style::default()
                        .fg(crossterm_bridge::to_ratatui_color(theme.label_color))
                        .add_modifier(Modifier::DIM);
                    buf.set_string(area.x + 1, area.y + 2, empty_msg, msg_style);
                    return;
                }

                let max_rows = area.height.saturating_sub(3) as usize;
                let available_width = area.width.saturating_sub(4) as usize;
                let pattern_width = (available_width / 2).min(30);
                let replace_width = available_width.saturating_sub(pattern_width + 4);

                for (idx, item) in editor.replacements.iter().enumerate() {
                    if idx >= max_rows {
                        break;
                    }
                    let y = area.y + 1 + idx as u16;
                    let is_sel = idx == editor.selected;
                    let prefix = if is_sel { "> " } else { "  " };
                    let color = if is_sel {
                        crossterm_bridge::to_ratatui_color(theme.focused_label_color)
                    } else {
                        crossterm_bridge::to_ratatui_color(theme.label_color)
                    };

                    // Format: pattern → replace (or pattern → (remove) if replace is empty)
                    let pattern_display: String = item.pattern.chars().take(pattern_width).collect();
                    let replace_display = if item.replace.is_empty() {
                        "(remove)".to_string()
                    } else {
                        item.replace.chars().take(replace_width).collect()
                    };
                    let line = format!("{}{} → {}", prefix, pattern_display, replace_display);
                    buf.set_string(area.x + 1, y, &line, Style::default().fg(color));
                }
            }
            TextReplacementsEditorMode::Form => {
                let y = area.y + 1;
                self.render_textarea_compact(
                    0,
                    "Pattern:",
                    &editor.pattern_input,
                    area.x + 1,
                    y,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, TextReplacementsFormField::Pattern),
                );
                self.render_textarea_compact(
                    0,
                    "Replace:",
                    &editor.replace_input,
                    area.x + 1,
                    y + 2,
                    area.width as usize - 2,
                    buf,
                    theme,
                    matches!(editor.form_field, TextReplacementsFormField::Replace),
                );

                let hint = "(leave Replace empty to remove matched text)";
                let hint_style = Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.label_color))
                    .add_modifier(Modifier::DIM);
                buf.set_string(area.x + 1, y + 4, hint, hint_style);

                let footer = "Enter: Save | Esc: Cancel | Tab: Next Field";
                let footer_style =
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.label_color));
                buf.set_string(
                    area.x + 1,
                    area.y + area.height.saturating_sub(1),
                    self.truncate_to_width(footer, area.width.saturating_sub(2)),
                    footer_style,
                );
            }
        }
    }

    fn truncate_to_width(&self, text: &str, width: u16) -> String {
        if width == 0 {
            return String::new();
        }
        let width_usize = width as usize;
        if text.chars().count() <= width_usize {
            text.to_string()
        } else {
            text.chars().take(width_usize).collect()
        }
    }

    fn render_fields(&mut self, area: Rect, buf: &mut Buffer, theme: &EditorTheme) {
        // Clear click areas for fresh population
        self.field_click_areas.clear();

        let left_x = area.x + 1;
        let right_x = area.x + 38;
        let geom_x2 = left_x + 16;
        let column_width = 30;

        let mut left_y = area.y + 1;
        let mut right_y = area.y + 1;

        let is_focus = |f: FieldRef, focused: usize| focused == f.legacy_field_id();

        // Left column: Identity + geometry
        self.render_textarea_compact(
            FieldRef::Name.legacy_field_id(),
            " Name:",
            &self.name_input,
            left_x,
            left_y,
            24,
            buf,
            theme,
            is_focus(FieldRef::Name, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::Name));
        left_y += 1;

        self.render_textarea_compact(
            FieldRef::Title.legacy_field_id(),
            "Title:",
            &self.title_input,
            left_x,
            left_y,
            24,
            buf,
            theme,
            is_focus(FieldRef::Title, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::Title));
        left_y += 1;

        // Title align
        self.render_dropdown_compact(
            FieldRef::TitlePosition.legacy_field_id(),
            "Title Align:",
            self.title_position_input
                .lines()
                .get(0)
                .map(|s| s.as_str())
                .unwrap_or("top-left"),
            left_x,
            left_y,
            14,
            buf,
            theme,
            is_focus(FieldRef::TitlePosition, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::TitlePosition));
        left_y += 1;

        // Content align
        self.render_dropdown_compact(
            FieldRef::ContentAlign.legacy_field_id(),
            "Content Align:",
            self.current_content_align_value(),
            left_x,
            left_y,
            14,
            buf,
            theme,
            is_focus(FieldRef::ContentAlign, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::ContentAlign));
        left_y += 1;

        // Border style
        self.render_dropdown_compact(
            FieldRef::BorderStyle.legacy_field_id(),
            " Border Style:",
            &self.window_def.base().border_style,
            left_x,
            left_y,
            10,
            buf,
            theme,
            is_focus(FieldRef::BorderStyle, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::BorderStyle));
        left_y += 2;

        // Row / Col
        self.render_textarea_compact(
            FieldRef::Row.legacy_field_id(),
            "  Row:",
            &self.row_input,
            left_x,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::Row, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::Row));
        self.render_textarea_compact(
            FieldRef::Col.legacy_field_id(),
            "  Col:",
            &self.col_input,
            geom_x2,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::Col, self.focused_field),
        );
        self.field_click_areas.push((left_y, geom_x2, FieldRef::Col));
        left_y += 1;

        // Rows / Cols
        self.render_textarea_compact(
            FieldRef::Rows.legacy_field_id(),
            " Rows:",
            &self.rows_input,
            left_x,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::Rows, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::Rows));
        self.render_textarea_compact(
            FieldRef::Cols.legacy_field_id(),
            " Cols:",
            &self.cols_input,
            geom_x2,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::Cols, self.focused_field),
        );
        self.field_click_areas.push((left_y, geom_x2, FieldRef::Cols));
        left_y += 1;

        // Min/Max constraints
        self.render_textarea_compact(
            FieldRef::MinRows.legacy_field_id(),
            "  Min:",
            &self.min_rows_input,
            left_x,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::MinRows, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::MinRows));
        self.render_textarea_compact(
            FieldRef::MinCols.legacy_field_id(),
            "  Min:",
            &self.min_cols_input,
            geom_x2,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::MinCols, self.focused_field),
        );
        self.field_click_areas.push((left_y, geom_x2, FieldRef::MinCols));
        left_y += 1;

        self.render_textarea_compact(
            FieldRef::MaxRows.legacy_field_id(),
            "  Max:",
            &self.max_rows_input,
            left_x,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::MaxRows, self.focused_field),
        );
        self.field_click_areas.push((left_y, left_x, FieldRef::MaxRows));
        self.render_textarea_compact(
            FieldRef::MaxCols.legacy_field_id(),
            "  Max:",
            &self.max_cols_input,
            geom_x2,
            left_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::MaxCols, self.focused_field),
        );
        self.field_click_areas.push((left_y, geom_x2, FieldRef::MaxCols));
        left_y += 2;

        // Right column: appearance
        self.render_checkbox_compact(
            FieldRef::Locked.legacy_field_id(),
            "Lock Window",
            self.window_def.base().locked,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::Locked, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::Locked));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::ShowTitle.legacy_field_id(),
            "Show Title",
            self.window_def.base().show_title,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::ShowTitle, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::ShowTitle));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::TransparentBg.legacy_field_id(),
            "Transparent BG",
            self.window_def.base().transparent_background,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::TransparentBg, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::TransparentBg));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::ShowBorder.legacy_field_id(),
            "Show Border",
            self.window_def.base().show_border,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::ShowBorder, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::ShowBorder));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::BorderTop.legacy_field_id(),
            "Top Border",
            self.window_def.base().border_sides.top,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::BorderTop, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::BorderTop));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::BorderBottom.legacy_field_id(),
            "Bottom Border",
            self.window_def.base().border_sides.bottom,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::BorderBottom, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::BorderBottom));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::BorderLeft.legacy_field_id(),
            "Left Border",
            self.window_def.base().border_sides.left,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::BorderLeft, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::BorderLeft));
        right_y += 1;
        self.render_checkbox_compact(
            FieldRef::BorderRight.legacy_field_id(),
            "Right Border",
            self.window_def.base().border_sides.right,
            right_x,
            right_y,
            column_width,
            buf,
            theme,
            is_focus(FieldRef::BorderRight, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::BorderRight));
        right_y += 1;

        self.render_color_field(
            FieldRef::BgColor.legacy_field_id(),
            "BG Color",
            &self.bg_color_input,
            right_x,
            right_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::BgColor, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::BgColor));
        right_y += 1;

        self.render_color_field(
            FieldRef::BorderColor.legacy_field_id(),
            "Border",
            &self.border_color_input,
            right_x,
            right_y,
            8,
            buf,
            theme,
            is_focus(FieldRef::BorderColor, self.focused_field),
        );
        self.field_click_areas.push((right_y, right_x, FieldRef::BorderColor));

        // Special section
        let special_y = left_y.max(right_y) + 1;
        let mut special_row = special_y;
        match &self.window_def {
            WindowDef::CommandInput { .. } => {
                // Text color on first row, right column
                self.render_color_field(
                    FieldRef::TextColor.legacy_field_id(),
                    "Text",
                    &self.text_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TextColor, self.focused_field),
                );
                special_row += 1;

                // Icon text + cursor foreground
                self.render_textarea_compact(
                    FieldRef::PromptIcon.legacy_field_id(),
                    "Icon:",
                    &self.prompt_icon_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::PromptIcon, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::CursorColor.legacy_field_id(),
                    "Cursor FG",
                    &self.cursor_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CursorColor, self.focused_field),
                );
                special_row += 1;

                // Icon color + cursor background
                self.render_color_field(
                    FieldRef::PromptIconColor.legacy_field_id(),
                    "Icon",
                    &self.prompt_icon_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::PromptIconColor, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::CursorBg.legacy_field_id(),
                    "Cursor BG",
                    &self.cursor_bg_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CursorBg, self.focused_field),
                );
            }
            WindowDef::Text { .. } => {
                // Bounty window is special: hide Streams and BufferSize
                let is_bounty = self.window_def.base().name.eq_ignore_ascii_case("bounty");

                if is_bounty {
                    // Bounty layout: Wordwrap (left), Timestamps (right) on row 1
                    // TextCompact (left) on row 2
                    self.render_checkbox_compact(
                        FieldRef::Wordwrap.legacy_field_id(),
                        "Wordwrap",
                        self.text_wordwrap,
                        left_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::Wordwrap, self.focused_field),
                    );
                    self.field_click_areas.push((special_row, left_x, FieldRef::Wordwrap));
                    self.render_checkbox_compact(
                        FieldRef::Timestamps.legacy_field_id(),
                        "Timestamps",
                        self.text_show_timestamps,
                        right_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::Timestamps, self.focused_field),
                    );
                    self.field_click_areas.push((special_row, right_x, FieldRef::Timestamps));
                    special_row += 1;
                    self.render_checkbox_compact(
                        FieldRef::TextCompact.legacy_field_id(),
                        "Compact",
                        self.text_compact,
                        left_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::TextCompact, self.focused_field),
                    );
                    self.field_click_areas.push((special_row, left_x, FieldRef::TextCompact));
                } else {
                    // Standard text window layout
                    self.render_textarea_compact(
                        FieldRef::Streams.legacy_field_id(),
                        "Streams:",
                        &self.streams_input,
                        left_x,
                        special_row,
                        column_width as usize,
                        buf,
                        theme,
                        is_focus(FieldRef::Streams, self.focused_field),
                    );
                    self.field_click_areas.push((special_row, left_x, FieldRef::Streams));
                    self.render_checkbox_compact(
                        FieldRef::Wordwrap.legacy_field_id(),
                        "Wordwrap",
                        self.text_wordwrap,
                        right_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::Wordwrap, self.focused_field),
                    );
                    self.field_click_areas.push((special_row, right_x, FieldRef::Wordwrap));
                    special_row += 1;
                    self.render_textarea_compact(
                        FieldRef::BufferSize.legacy_field_id(),
                        "Buffer Size:",
                        &self.buffer_size_input,
                        left_x,
                        special_row,
                        8,
                        buf,
                        theme,
                        is_focus(FieldRef::BufferSize, self.focused_field),
                    );
                    self.field_click_areas.push((special_row, left_x, FieldRef::BufferSize));
                    self.render_checkbox_compact(
                        FieldRef::Timestamps.legacy_field_id(),
                        "Timestamps",
                        self.text_show_timestamps,
                        right_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::Timestamps, self.focused_field),
                    );
                    self.field_click_areas.push((special_row, right_x, FieldRef::Timestamps));
                    special_row += 1;
                    // Compact mode checkbox
                    self.render_checkbox_compact(
                        FieldRef::TextCompact.legacy_field_id(),
                        "Compact",
                        self.text_compact,
                        left_x,
                        special_row,
                        column_width,
                        buf,
                        theme,
                        is_focus(FieldRef::TextCompact, self.focused_field),
                    );
                    self.field_click_areas.push((special_row, left_x, FieldRef::TextCompact));
                }
            }
            WindowDef::Inventory { .. } => {
                self.render_textarea_compact(
                    FieldRef::Streams.legacy_field_id(),
                    "Streams:",
                    &self.streams_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::Streams, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::Streams));
                self.render_checkbox_compact(
                    FieldRef::Wordwrap.legacy_field_id(),
                    "Wordwrap",
                    self.text_wordwrap,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::Wordwrap, self.focused_field),
                );
                self.field_click_areas.push((special_row, right_x, FieldRef::Wordwrap));
                special_row += 1;
                self.render_textarea_compact(
                    FieldRef::BufferSize.legacy_field_id(),
                    "Buffer Size:",
                    &self.buffer_size_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::BufferSize, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::BufferSize));
                self.render_checkbox_compact(
                    FieldRef::Timestamps.legacy_field_id(),
                    "Timestamps",
                    self.text_show_timestamps,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::Timestamps, self.focused_field),
                );
                self.field_click_areas.push((special_row, right_x, FieldRef::Timestamps));
            }
            WindowDef::Targets { .. } => {
                self.render_textarea_compact(
                    FieldRef::EntityId.legacy_field_id(),
                    "Entity ID:",
                    &self.entity_id_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::EntityId, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::EntityId));
                self.render_checkbox_compact(
                    FieldRef::TargetsShowArmsCount.legacy_field_id(),
                    "Show 709 Arms",
                    self.targets_show_arms_count,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::TargetsShowArmsCount, self.focused_field),
                );
                self.field_click_areas.push((special_row, right_x, FieldRef::TargetsShowArmsCount));
            }
            WindowDef::Players { .. } => {
                self.render_textarea_compact(
                    FieldRef::EntityId.legacy_field_id(),
                    "Entity ID:",
                    &self.entity_id_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::EntityId, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::EntityId));
            }
            WindowDef::TabbedText { .. } => {
                let special_left_x = left_x + 2;
                self.render_dropdown_compact(
                    FieldRef::TabBarPosition.legacy_field_id(),
                    "Tab Bar Pos:",
            self.tab_bar_position_input
                .lines()
                .get(0)
                        .map(|s| s.as_str())
                        .unwrap_or("top"),
                    special_left_x,
                    special_row,
                    10,
                    buf,
                    theme,
                    is_focus(FieldRef::TabBarPosition, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::TabActiveColor.legacy_field_id(),
                    "Active",
                    &self.tab_active_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TabActiveColor, self.focused_field),
                );
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::TabSeparator.legacy_field_id(),
                    "Tab Separator",
                    self.tab_separator,
                    special_left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::TabSeparator, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::TabInactiveColor.legacy_field_id(),
                    "Inactive",
                    &self.tab_inactive_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TabInactiveColor, self.focused_field),
                );
                special_row += 1;
                self.render_textarea_compact(
                    FieldRef::TabUnreadPrefix.legacy_field_id(),
                    "New Msg Icon:",
                    &self.tab_unread_prefix_input,
                    special_left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TabUnreadPrefix, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::TabUnreadColor.legacy_field_id(),
                    "Unread",
                    &self.tab_unread_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TabUnreadColor, self.focused_field),
                );
                special_row += 1;
                self.render_button(
                    FieldRef::EditTabs.legacy_field_id(),
                    "[ Edit Tabs ]",
                    special_left_x,
                    special_row,
                    buf,
                    theme,
                    is_focus(FieldRef::EditTabs, self.focused_field),
                );
            }
            WindowDef::Room { .. } => {
                self.render_checkbox_compact(
                    FieldRef::ShowName.legacy_field_id(),
                    "Show Name",
                    self.show_name,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowName, self.focused_field),
                );
                self.render_checkbox_compact(
                    FieldRef::ShowDesc.legacy_field_id(),
                    "Show Desc",
                    self.show_desc,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowDesc, self.focused_field),
                );
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::ShowObjs.legacy_field_id(),
                    "Show Objects",
                    self.show_objs,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowObjs, self.focused_field),
                );
                self.render_checkbox_compact(
                    FieldRef::ShowPlayers.legacy_field_id(),
                    "Show Players",
                    self.show_players,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowPlayers, self.focused_field),
                );
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::ShowExits.legacy_field_id(),
                    "Show Exits",
                    self.show_exits,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ShowExits, self.focused_field),
                );
            }
            WindowDef::Progress { .. } => {
                self.render_textarea_compact(
                    FieldRef::ProgressId.legacy_field_id(),
                    "Progress ID:",
                    &self.progress_id_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::ProgressId, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::ProgressId));
                self.render_color_field(
                    FieldRef::TextColor.legacy_field_id(),
                    "Text Color",
                    &self.text_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::TextColor, self.focused_field),
                );
                self.field_click_areas.push((special_row, right_x, FieldRef::TextColor));
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::ProgressNumbersOnly.legacy_field_id(),
                    "Numbers Only",
                    self.progress_numbers_only,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ProgressNumbersOnly, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::ProgressNumbersOnly));
                self.render_color_field(
                    FieldRef::ProgressColor.legacy_field_id(),
                    "Bar Color",
                    &self.progress_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::ProgressColor, self.focused_field),
                );
                self.field_click_areas.push((special_row, right_x, FieldRef::ProgressColor));
                special_row += 1;
                self.render_checkbox_compact(
                    FieldRef::ProgressCurrentOnly.legacy_field_id(),
                    "Current Only",
                    self.progress_current_only,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::ProgressCurrentOnly, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::ProgressCurrentOnly));
            }
            WindowDef::Countdown { .. } => {
                self.render_textarea_compact(
                    FieldRef::CountdownIcon.legacy_field_id(),
                    "Icon:",
                    &self.countdown_icon_input,
                    left_x,
                    special_row,
                    4,
                    buf,
                    theme,
                    is_focus(FieldRef::CountdownIcon, self.focused_field),
                );
                self.render_textarea_compact(
                    FieldRef::CountdownId.legacy_field_id(),
                    "Countdown ID:",
                    &self.countdown_id_input,
                    right_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::CountdownId, self.focused_field),
                );
                special_row += 1;
                self.render_color_field(
                    FieldRef::CountdownColor.legacy_field_id(),
                    "Icon Color",
                    &self.countdown_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CountdownColor, self.focused_field),
                );
            }
            WindowDef::Compass { .. } => {
                // Clear left column row for a clean right-column layout
                buf.set_string(
                    left_x,
                    special_row,
                    " ".repeat(column_width as usize),
                    Style::default(),
                );
                self.render_color_field(
                    FieldRef::CompassActiveColor.legacy_field_id(),
                    "Active:",
                    &self.compass_active_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CompassActiveColor, self.focused_field),
                );
                special_row += 1;
                self.render_color_field(
                    FieldRef::CompassInactiveColor.legacy_field_id(),
                    "Inactive:",
                    &self.compass_inactive_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::CompassInactiveColor, self.focused_field),
                );
            }
            WindowDef::InjuryDoll { .. } => {
                self.render_color_field(
                    FieldRef::Injury1Color.legacy_field_id(),
                    "Wound1",
                    &self.injury1_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Injury1Color, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::Scar1Color.legacy_field_id(),
                    "Scar1",
                    &self.scar1_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Scar1Color, self.focused_field),
                );
                special_row += 1;
                self.render_color_field(
                    FieldRef::Injury2Color.legacy_field_id(),
                    "Wound2",
                    &self.injury2_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Injury2Color, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::Scar2Color.legacy_field_id(),
                    "Scar2",
                    &self.scar2_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Scar2Color, self.focused_field),
                );
                special_row += 1;
                self.render_color_field(
                    FieldRef::Injury3Color.legacy_field_id(),
                    "Wound3",
                    &self.injury3_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Injury3Color, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::Scar3Color.legacy_field_id(),
                    "Scar3",
                    &self.scar3_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::Scar3Color, self.focused_field),
                );
                special_row += 1;
                self.render_color_field(
                    FieldRef::InjuryDefaultColor.legacy_field_id(),
                    "Uninjured",
                    &self.injury_default_color_input,
                    left_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::InjuryDefaultColor, self.focused_field),
                );
            }
            WindowDef::Indicator { .. } => {
                self.render_textarea_compact(
                    FieldRef::IndicatorId.legacy_field_id(),
                    "Indicator ID:",
                    &self.indicator_id_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::IndicatorId, self.focused_field),
                );
                special_row += 1;
                self.render_textarea_compact(
                    FieldRef::IndicatorIcon.legacy_field_id(),
                    "Icon:",
                    &self.indicator_icon_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::IndicatorIcon, self.focused_field),
                );
                if let Some(icon_char) = Self::parse_icon_char(
                    self.indicator_icon_input
                        .lines()
                        .get(0)
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ) {
                    let preview_x = left_x
                        + 2
                        + "Icon:".len() as u16
                        + 1
                        + column_width
                        + 1;
                    if preview_x < buf.area().width && special_row < buf.area().height {
                        buf[(preview_x, special_row)].set_char(icon_char);
                        buf[(preview_x, special_row)]
                            .set_fg(crossterm_bridge::to_ratatui_color(theme.text_color));
                    }
                }
                self.render_color_field(
                    FieldRef::IndicatorActiveColor.legacy_field_id(),
                    "Active:",
                    &self.indicator_active_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::IndicatorActiveColor, self.focused_field),
                );
                special_row += 1;
                self.render_color_field(
                    FieldRef::IndicatorInactiveColor.legacy_field_id(),
                    "Inactive:",
                    &self.indicator_inactive_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::IndicatorInactiveColor, self.focused_field),
                );
            }
            WindowDef::Hand { .. } => {
                self.render_textarea_compact(
                    FieldRef::HandIcon.legacy_field_id(),
                    "Icon:",
                    &self.hand_icon_input,
                    left_x,
                    special_row,
                    6,
                    buf,
                    theme,
                    is_focus(FieldRef::HandIcon, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::HandIconColor.legacy_field_id(),
                    "Icon Color",
                    &self.hand_icon_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::HandIconColor, self.focused_field),
                );
                special_row += 1;
                self.render_color_field(
                    FieldRef::HandTextColor.legacy_field_id(),
                    "Text Color",
                    &self.hand_text_color_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::HandTextColor, self.focused_field),
                );
            }
            WindowDef::Dashboard { .. } => {
                self.render_dropdown_compact(
                    FieldRef::DashboardLayout.legacy_field_id(),
                    "Layout:",
                    self.dashboard_layout_input
                        .lines()
                        .get(0)
                        .map(|s| s.as_str())
                        .unwrap_or("horizontal"),
                    left_x,
                    special_row,
                    12,
                    buf,
                    theme,
                    is_focus(FieldRef::DashboardLayout, self.focused_field),
                );
                self.render_textarea_compact(
                    FieldRef::DashboardSpacing.legacy_field_id(),
                    "Spacing:",
                    &self.dashboard_spacing_input,
                    right_x,
                    special_row,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::DashboardSpacing, self.focused_field),
                );
                special_row += 1;
                self.render_button(
                    FieldRef::EditIndicators.legacy_field_id(),
                    "[ Edit Indicators ]",
                    left_x,
                    special_row,
                    buf,
                    theme,
                    is_focus(FieldRef::EditIndicators, self.focused_field),
                );
                self.render_checkbox_compact(
                    FieldRef::DashboardHideInactive.legacy_field_id(),
                    "Hide Inactive",
                    self.dashboard_hide_inactive,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::DashboardHideInactive, self.focused_field),
                );
            }
            WindowDef::ActiveEffects { .. } => {
                self.render_textarea_compact(
                    FieldRef::ActiveEffectsCategory.legacy_field_id(),
                    "Category:",
                    &self.active_effects_category_input,
                    left_x,
                    special_row,
                    column_width as usize,
                    buf,
                    theme,
                    is_focus(FieldRef::ActiveEffectsCategory, self.focused_field),
                );
            }
            WindowDef::Performance { .. } => {
                // Performance widget uses overlay-only system, no fields to render here
            }
            WindowDef::Perception { .. } => {
                // Only sort_direction is configurable (stream="percWindow", buffer_size=100 hardcoded)
                // Window clears on each update (<clearStream/>) so buffer size is irrelevant
                self.render_dropdown_compact(
                    FieldRef::PerceptionSortDirection.legacy_field_id(),
                    "Sort:",
                    self.perception_sort_direction_input
                        .lines()
                        .get(0)
                        .map(|s| s.as_str())
                        .unwrap_or("descending"),
                    left_x,
                    special_row,
                    12,
                    buf,
                    theme,
                    is_focus(FieldRef::PerceptionSortDirection, self.focused_field),
                );
                self.render_checkbox_compact(
                    FieldRef::PerceptionUseShortSpellNames.legacy_field_id(),
                    "Short Spell Names:",
                    self.perception_use_short_spell_names,
                    right_x,
                    special_row,
                    20,
                    buf,
                    theme,
                    is_focus(FieldRef::PerceptionUseShortSpellNames, self.focused_field),
                );
                self.render_button(
                    FieldRef::PerceptionTextReplacements.legacy_field_id(),
                    "[ Edit Replacements ]",
                    left_x,
                    special_row + 1,
                    buf,
                    theme,
                    is_focus(FieldRef::PerceptionTextReplacements, self.focused_field),
                );
            }
            WindowDef::GS4Experience { .. } => {
                // Row 1: Show toggles
                self.render_checkbox_compact(
                    FieldRef::GS4ExpShowLevel.legacy_field_id(),
                    "Show Level",
                    self.gs4_exp_show_level,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpShowLevel, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::GS4ExpShowLevel));
                self.render_checkbox_compact(
                    FieldRef::GS4ExpShowExpBar.legacy_field_id(),
                    "Show Exp Bar",
                    self.gs4_exp_show_exp_bar,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpShowExpBar, self.focused_field),
                );
                self.field_click_areas.push((special_row, right_x, FieldRef::GS4ExpShowExpBar));
                // Row 2: Bar colors
                self.render_color_field(
                    FieldRef::GS4ExpMindBarColor.legacy_field_id(),
                    "Mind",
                    &self.gs4_exp_mind_bar_color_input,
                    left_x,
                    special_row + 1,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpMindBarColor, self.focused_field),
                );
                self.field_click_areas.push((special_row + 1, left_x, FieldRef::GS4ExpMindBarColor));
                self.render_color_field(
                    FieldRef::GS4ExpExpBarColor.legacy_field_id(),
                    "Exp",
                    &self.gs4_exp_exp_bar_color_input,
                    right_x,
                    special_row + 1,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::GS4ExpExpBarColor, self.focused_field),
                );
                self.field_click_areas.push((special_row + 1, right_x, FieldRef::GS4ExpExpBarColor));
            }
            WindowDef::Encumbrance { .. } => {
                // Row 1: Show label checkbox
                self.render_checkbox_compact(
                    FieldRef::EncumShowLabel.legacy_field_id(),
                    "Show Label",
                    self.show_label_encum,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumShowLabel, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::EncumShowLabel));
                // Row 2: Light (0-20) and Moderate (21-50) colors
                self.render_color_field(
                    FieldRef::EncumColorLight.legacy_field_id(),
                    "Light",
                    &self.encum_color_light_input,
                    left_x,
                    special_row + 1,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumColorLight, self.focused_field),
                );
                self.field_click_areas.push((special_row + 1, left_x, FieldRef::EncumColorLight));
                self.render_color_field(
                    FieldRef::EncumColorModerate.legacy_field_id(),
                    "Moderate",
                    &self.encum_color_moderate_input,
                    right_x,
                    special_row + 1,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumColorModerate, self.focused_field),
                );
                self.field_click_areas.push((special_row + 1, right_x, FieldRef::EncumColorModerate));
                // Row 3: Heavy (51-80) and Critical (81-100) colors
                self.render_color_field(
                    FieldRef::EncumColorHeavy.legacy_field_id(),
                    "Heavy",
                    &self.encum_color_heavy_input,
                    left_x,
                    special_row + 2,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumColorHeavy, self.focused_field),
                );
                self.field_click_areas.push((special_row + 2, left_x, FieldRef::EncumColorHeavy));
                self.render_color_field(
                    FieldRef::EncumColorCritical.legacy_field_id(),
                    "Critical",
                    &self.encum_color_critical_input,
                    right_x,
                    special_row + 2,
                    8,
                    buf,
                    theme,
                    is_focus(FieldRef::EncumColorCritical, self.focused_field),
                );
                self.field_click_areas.push((special_row + 2, right_x, FieldRef::EncumColorCritical));
            }
            WindowDef::MiniVitals { .. } => {
                // Row 1: Display mode checkboxes
                self.render_checkbox_compact(
                    FieldRef::MiniVitalsNumbersOnly.legacy_field_id(),
                    "Numbers Only",
                    self.minivitals_numbers_only,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::MiniVitalsNumbersOnly, self.focused_field),
                );
                self.field_click_areas.push((special_row, left_x, FieldRef::MiniVitalsNumbersOnly));
                self.render_checkbox_compact(
                    FieldRef::MiniVitalsCurrentOnly.legacy_field_id(),
                    "Current Only",
                    self.minivitals_current_only,
                    right_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::MiniVitalsCurrentOnly, self.focused_field),
                );
                self.field_click_areas.push((special_row, right_x, FieldRef::MiniVitalsCurrentOnly));
                // Row 2: Bar order and colors editor button (handles all 5 bars)
                self.render_button(
                    FieldRef::MiniVitalsEditBarOrder.legacy_field_id(),
                    "[ Edit Bars & Colors ]",
                    left_x,
                    special_row + 1,
                    buf,
                    theme,
                    is_focus(FieldRef::MiniVitalsEditBarOrder, self.focused_field),
                );
                self.field_click_areas.push((special_row + 1, left_x, FieldRef::MiniVitalsEditBarOrder));
            }
            WindowDef::Betrayer { .. } => {
                // Betrayer widget: show_items toggle and bar color
                self.render_checkbox_compact(
                    FieldRef::BetrayerShowItems.legacy_field_id(),
                    "Show Items",
                    self.betrayer_show_items,
                    left_x,
                    special_row,
                    column_width,
                    buf,
                    theme,
                    is_focus(FieldRef::BetrayerShowItems, self.focused_field),
                );
                self.render_color_field(
                    FieldRef::BetrayerBarColor.legacy_field_id(),
                    "Bar Color",
                    &self.betrayer_bar_color_input,
                    left_x,
                    special_row + 1,
                    12,
                    buf,
                    theme,
                    is_focus(FieldRef::BetrayerBarColor, self.focused_field),
                );
            }
            _ => {
                buf.set_string(
                    left_x,
                    special_row,
                    "No special fields for this widget.",
                    Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_color)),
                );
            }
        }

    }

    /// Render a text input field (compact format for section-based layout)
    fn render_textarea_compact(
        &self,
        _field_id: usize,
        label: &str,
        textarea: &TextArea,
        x: u16,
        y: u16,
        width: usize,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        let prefix = if is_current { "→ " } else { "  " };
        buf.set_string(x, y, prefix, Style::default().fg(label_color));

        let label_x = x + 2;
        buf.set_string(label_x, y, label, Style::default().fg(label_color));

        let raw_value = if textarea.lines().is_empty() {
            ""
        } else {
            &textarea.lines()[0]
        };
        let truncated: String = raw_value.chars().take(width).collect();
        let padded = format!("{:<width$}", truncated, width = width);

        let text_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.cursor_color
        } else {
            theme.text_color
        });
        let input_x = label_x + label.len() as u16 + 1;
        buf.set_string(input_x, y, padded, Style::default().fg(text_color));
    }

    fn render_tab_editor_input(
        &self,
        label: &str,
        textarea: &TextArea,
        x: u16,
        y: u16,
        available_width: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });
        let text_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.cursor_color
        } else {
            theme.text_color
        });

        let prefix = "  ";
        let label_width: usize = 11;
        let usable_width = available_width as usize;
        let reserved = prefix.len() + label_width + 1; // space
        let input_width = usable_width.saturating_sub(reserved);

        let raw_value = if textarea.lines().is_empty() {
            ""
        } else {
            &textarea.lines()[0]
        };
        let truncated: String = raw_value.chars().take(input_width).collect();
        let padded_value = format!("{:<width$}", truncated, width = input_width);

        let start_x = x;
        buf.set_string(
            start_x,
            y,
            prefix,
            Style::default().fg(label_color),
        );
        buf.set_string(
            start_x + prefix.len() as u16,
            y,
            format!("{:<width$}", label, width = label_width),
            Style::default().fg(label_color),
        );
        let input_x = start_x + prefix.len() as u16 + label_width as u16 + 1;
        buf.set_string(
            input_x,
            y,
            padded_value,
            Style::default().fg(text_color),
        );
    }

    fn render_tab_editor_checkbox(
        &self,
        label: &str,
        checked: bool,
        x: u16,
        y: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });
        let prefix = "   "; // start at column 4 to align with text fields
        let checkbox = if checked { "[✓]" } else { "[ ]" };
        let start_x = x;
        buf.set_string(start_x, y, prefix, Style::default().fg(label_color));
        let checkbox_x = start_x + prefix.len() as u16;
        buf.set_string(checkbox_x, y, checkbox, Style::default().fg(label_color));
        buf.set_string(checkbox_x + 4, y, label, Style::default().fg(label_color));
    }

    /// Render a color field with preview
    fn render_color_field(
        &self,
        _field_id: usize,
        label: &str,
        textarea: &TextArea,
        x: u16,
        y: u16,
        input_width: usize,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });
        let prefix = if is_current { "→ " } else { "  " };
        buf.set_string(x, y, prefix, Style::default().fg(label_color));

        let value = if textarea.lines().is_empty() {
            ""
        } else {
            &textarea.lines()[0]
        };

        // Color swatch
        let swatch_x = x + 2;
        self.render_color_preview(value, swatch_x, y, buf, theme);

        // Label after swatch
        let label_x = swatch_x + 4 + 1;
        buf.set_string(label_x, y, label, Style::default().fg(label_color));

        // Input field
        let truncated: String = value.chars().take(input_width).collect();
        let padded = format!("{:<width$}", truncated, width = input_width);
        let text_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.cursor_color
        } else {
            theme.text_color
        });
        let input_x = label_x + label.len() as u16 + 1;
        buf.set_string(input_x, y, padded, Style::default().fg(text_color));
    }

    /// Render a checkbox field (compact format)
    fn render_checkbox_compact(
        &self,
        _field_id: usize,
        label: &str,
        checked: bool,
        x: u16,
        y: u16,
        _column_width: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current  {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        let prefix = if is_current { "→ " } else { "  " };
        buf.set_string(x, y, prefix, Style::default().fg(label_color));

        let label_x = x + 2;
        let label_width = usize::max(14, label.len());
        let padded_label = format!("{:<width$}", label, width = label_width);
        buf.set_string(label_x, y, padded_label, Style::default().fg(label_color));

        let checkbox = if checked { "[✓]" } else { "[ ]" };
        let checkbox_x = label_x + label_width as u16 + 2;
        buf.set_string(checkbox_x, y, checkbox, Style::default().fg(label_color));
    }

    /// Render a dropdown field (compact format)
    fn render_dropdown_compact(
        &self,
        _field_id: usize,
        label: &str,
        value: &str,
        x: u16,
        y: u16,
        width: usize,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current  {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        let prefix = if is_current { "→ " } else { "  " };
        buf.set_string(x, y, prefix, Style::default().fg(label_color));
        buf.set_string(x + 2, y, label, Style::default().fg(label_color));

        let display_value = format!("{} ▼", value);
        let truncated: String = display_value.chars().take(width).collect();
        let padded = format!("{:<width$}", truncated, width = width);
        let input_x = x + 2 + label.len() as u16 + 1;
        buf.set_string(input_x, y, &padded, Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_color)));
    }

    fn render_textarea(
        &self,
        field_id: usize,
        label: &str,
        textarea: &TextArea,
        x: u16,
        y: u16,
        _width: usize,
        spacing: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
    ) {
        let is_focused = self.focused_field == field_id;
        let label_color = crossterm_bridge::to_ratatui_color(if is_focused  {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        buf.set_string(x, y, label, Style::default().fg(label_color));
        let input_x = x + label.len() as u16 + spacing;

        // Render textarea content inline
        let value = if textarea.lines().is_empty() {
            ""
        } else {
            &textarea.lines()[0]
        };
        let text_color = crossterm_bridge::to_ratatui_color(if is_focused  {
            theme.cursor_color
        } else {
            theme.text_color
        });
        buf.set_string(input_x, y, value, Style::default().fg(text_color));
    }

    fn render_textarea_with_preview(
        &self,
        field_id: usize,
        label: &str,
        textarea: &TextArea,
        x: u16,
        y: u16,
        width: usize,
        spacing: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
    ) {
        self.render_textarea(field_id, label, textarea, x, y, width, spacing, buf, theme);
        let input_x = x + label.len() as u16 + spacing;
        let preview_x = input_x + width as u16 + 2;
        let value = if textarea.lines().is_empty() {
            ""
        } else {
            &textarea.lines()[0]
        };
        self.render_color_preview(value, preview_x, y, buf, theme);
    }

    fn render_color_preview(
        &self,
        color_str: &str,
        x: u16,
        y: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
    ) {
        // Use centralized mode-aware color parser
        let color = super::colors::parse_color_to_ratatui(color_str);

        buf.set_string(x, y, "[", Style::default().fg(crossterm_bridge::to_ratatui_color(theme.label_color)));
        if let Some(color) = color {
            let style = Style::default().bg(color);
            buf[(x + 1, y)].set_char(' ').set_style(style);
            buf[(x + 2, y)].set_char(' ').set_style(style);
        } else {
            buf[(x + 1, y)].set_char(' ').reset();
            buf[(x + 2, y)].set_char(' ').reset();
        }
        buf.set_string(x + 3, y, "]", Style::default().fg(crossterm_bridge::to_ratatui_color(theme.label_color)));
    }

    fn render_checkbox(
        &self,
        field_id: usize,
        label: &str,
        checked: bool,
        x: u16,
        y: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
    ) {
        let is_focused = self.focused_field == field_id;
        let label_color = crossterm_bridge::to_ratatui_color(if is_focused  {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        buf.set_string(x, y, label, Style::default().fg(label_color));
        let checkbox = if checked { "[✓]" } else { "[ ]" };
        let checkbox_x = x + 15;
        buf.set_string(checkbox_x, y, checkbox, Style::default().fg(label_color));
    }

    fn render_checkbox_row(
        &self,
        x: u16,
        y: u16,
        label: &str,
        checked: bool,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_current: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_current {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        buf.set_string(x, y, label, Style::default().fg(label_color));
        let checkbox = if checked { "[û]" } else { "[ ]" };
        let checkbox_x = x + label.len() as u16 + 2;
        buf.set_string(checkbox_x, y, checkbox, Style::default().fg(label_color));
    }

    fn parse_icon_char(value: &str) -> Option<char> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        let hex = trimmed
            .trim_start_matches("0x")
            .trim_start_matches("\\u{")
            .trim_start_matches("\\u")
            .trim_start_matches("\\U")
            .trim_start_matches("u+")
            .trim_start_matches("U+")
            .trim_start_matches('u')
            .trim_start_matches('U')
            .trim_end_matches('}');
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(codepoint) = u32::from_str_radix(hex, 16) {
                let mapped = match codepoint {
                    0xe231 | 0xf231 => 0x2620, // poison skull fallback
                    _ => codepoint,
                };
                if let Some(ch) = char::from_u32(mapped) {
                    return Some(ch);
                }
            }
        }

        trimmed.chars().next()
    }

    fn render_button(
        &self,
        _field_id: usize,
        label: &str,
        x: u16,
        y: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
        is_focused: bool,
    ) {
        let label_color = crossterm_bridge::to_ratatui_color(if is_focused {
            theme.focused_label_color
        } else {
            theme.text_color
        });

        buf.set_string(x, y, label, Style::default().fg(label_color).add_modifier(if is_focused { Modifier::BOLD } else { Modifier::empty() }));
    }

    fn render_dropdown(
        &self,
        field_id: usize,
        label: &str,
        value: &str,
        x: u16,
        y: u16,
        buf: &mut Buffer,
        theme: &EditorTheme,
    ) {
        let is_focused = self.focused_field == field_id;
        let label_color = crossterm_bridge::to_ratatui_color(if is_focused  {
            theme.focused_label_color
        } else {
            theme.label_color
        });

        buf.set_string(x, y, label, Style::default().fg(label_color));
        let input_x = x + label.len() as u16 + 1;
        let display = format!("{} ▼", value);
        buf.set_string(input_x, y, &display, Style::default().fg(crossterm_bridge::to_ratatui_color(theme.text_color)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Layout, SpacerWidgetData};

    #[test]
    fn test_new_window_spacer_auto_naming_empty_layout() {
        // RED: Test that new_window_with_layout generates auto-name for spacer in empty layout
        let layout = Layout {
            windows: vec![],
            terminal_width: None,
            terminal_height: None,
            base_layout: None,
            theme: None,
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
                row: 0,
                col: 0,
                rows: 2,
                cols: 5,
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
                visible: true,
                content_align: None,
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
}
