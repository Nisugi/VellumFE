//! GUI Window Editor - Floating panel for editing window configuration

use crate::config::{
    ActiveEffectsStyle, ActiveEffectsWidgetData, CommandInputWidgetData, CompassLayout,
    CompassWidgetData, CountdownFormat, CountdownWidgetData, DashboardWidgetData, HandWidgetData,
    IndicatorShape, IndicatorWidgetData, InjuryDollWidgetData, InjuryMarkerStyle,
    ProgressTextPosition, ProgressWidgetData, RoomWidgetData, TabbedTextWidgetData,
    TextWidgetData, TimerPosition, WindowDef,
};
use crate::core::app_core::AppCore;
use eframe::egui;

/// Action returned by editor rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    None,
    Apply,   // Preview changes in layout
    Cancel,  // Revert to original
    Save,    // Persist to memory + save config
    Close,   // Close editor panel
}

/// Field editors for ActiveEffects widgets
#[derive(Debug, Clone)]
pub struct ActiveEffectsEditorState {
    // Dropdown indices
    pub style_index: usize,  // 0-4 for Overlay/Separate/ThinBar/SideIndicator/Minimal
    pub timer_position_index: usize,  // 0-2 for Left/Right/Inline

    // Slider values (f32, directly editable)
    pub bar_height: f32,     // 10.0-40.0
    pub bar_opacity: f32,    // 0.0-1.0
    pub bar_rounding: f32,   // 0.0-10.0
    pub text_size: f32,      // 8.0-20.0
    pub spacing: f32,        // 0.0-10.0

    // Number input
    pub expiring_threshold: u32,  // seconds

    // Checkboxes (bool)
    pub show_timer: bool,
    pub show_percentage: bool,
    pub auto_contrast: bool,
    pub text_shadow: bool,
    pub animate_changes: bool,
    pub pulse_expiring: bool,
}

/// Field editors for Progress widgets (health/mana/stamina bars)
/// Field editors for Countdown widgets (RT/CT timers)
#[derive(Debug, Clone)]
pub struct CountdownEditorState {
    pub id: String,
    pub label: String,
    pub icon: String,  // Single character
    pub color: String,
    pub background_color: String,

    // NEW: Visual customization
    pub text_size: f32,
    pub max_time: u32,
    pub alert_threshold: u32,
    pub alert_color: String,
    pub pulse_when_ready: bool,
    pub format: usize,  // Index into CountdownFormat enum (0=Seconds, 1=MMss, 2=HHMMss)
}

/// Field editors for Text widgets
#[derive(Debug, Clone)]
pub struct TextEditorState {
    pub streams: String,  // Comma-separated list
    pub buffer_size: usize,
    pub wordwrap: bool,
    pub show_timestamps: bool,
    // NEW: Visual customization
    pub font_size: f32,
    pub line_spacing: f32,
    pub padding: f32,
    pub text_color: String,
    pub link_color: String,
    pub link_underline_on_hover: bool,
    pub auto_scroll: bool,
    pub timestamp_color: String,
    pub timestamp_format: String,
}

/// Field editors for Room widgets
#[derive(Debug, Clone)]
pub struct RoomEditorState {
    pub buffer_size: usize,
    pub show_desc: bool,
    pub show_objs: bool,
    pub show_players: bool,
    pub show_exits: bool,
    pub show_name: bool,
    // NEW: Visual customization
    pub name_text_size: f32,
    pub name_color: String,
    pub desc_text_size: f32,
    pub section_spacing: f32,
    pub section_separators: bool,
    pub separator_color: String,
    pub show_component_headers: bool,
    pub header_text_size: f32,
    pub header_color: String,
}

/// Field editors for CommandInput widgets
#[derive(Debug, Clone)]
pub struct CommandInputEditorState {
    pub text_color: String,
    pub cursor_color: String,
    pub cursor_background_color: String,
    pub prompt_icon: String,
    pub prompt_icon_color: String,

    // NEW: Visual customization
    pub text_size: f32,
    pub padding: f32,
    pub border_color: String,
    pub background_color: String,
    pub border_width: f32,
}

/// A single tab's editable properties
#[derive(Debug, Clone)]
pub struct TabEditItem {
    pub name: String,
    pub streams: Vec<String>,
    pub show_timestamps: bool,
    pub ignore_activity: bool,
}

/// Field editors for TabbedText widgets
#[derive(Debug, Clone)]
pub struct TabbedTextEditorState {
    pub buffer_size: usize,
    pub tab_bar_position: String,  // "top" or "bottom"
    pub tab_separator: bool,       // Show separators between tabs
    pub tab_active_color: String,
    pub tab_inactive_color: String,
    pub tab_unread_color: String,
    pub tab_text_size: f32,
    pub tab_bar_height: f32,
    pub tab_padding: f32,
    pub tab_rounding: f32,
    pub content_font_size: f32,

    // Tab list management
    pub tabs: Vec<TabEditItem>,
    pub selected_tab: usize,
    pub editing_tab: Option<usize>,  // None = list mode, Some(idx) = editing/adding tab
    pub is_adding_new_tab: bool,     // true when adding new, false when editing existing
    pub edit_tab_name: String,
    pub edit_tab_streams: String,
    pub edit_tab_show_timestamps: bool,
    pub edit_tab_ignore_activity: bool,
}

/// Field editors for Hand widgets
#[derive(Debug, Clone)]
pub struct HandEditorState {
    pub icon: String,
    pub icon_color: String,
    pub text_color: String,
    pub text_size: f32,
    pub icon_size: f32,
    pub spacing: f32,
    pub empty_text: String,
    pub empty_color: String,
    pub show_background: bool,
    pub background_color: String,
}

/// Field editors for Indicator widgets (status indicators)
#[derive(Debug, Clone)]
pub struct IndicatorEditorState {
    pub icon: String,
    pub indicator_id: String,
    pub inactive_color: String,
    pub active_color: String,
    // NEW: Visual customization
    pub text_size: f32,
    pub shape_index: usize,  // Index into IndicatorShape enum
    pub indicator_size: f32,
    pub glow_when_active: bool,
    pub glow_radius: f32,
    pub background_color: String,
    pub show_label: bool,
}

/// Field editors for Compass widgets (navigation compass)
#[derive(Debug, Clone)]
pub struct CompassEditorState {
    pub active_color: String,
    pub inactive_color: String,
    // NEW: Visual customization
    pub layout_index: usize,  // Index into CompassLayout enum (0=Grid3x3, 1=Horizontal, 2=Vertical)
    pub spacing: f32,
    pub text_size: f32,
    pub use_icons: bool,
    pub bold_active: bool,
}

/// Field editors for Progress widgets
#[derive(Debug, Clone)]
pub struct ProgressEditorState {
    pub id: String,
    pub label: String,
    pub color: String,
    pub numbers_only: bool,
    pub current_only: bool,
    pub bar_height: f32,
    pub text_size: f32,
    pub rounding: f32,
    pub text_position: usize,  // Index into ProgressTextPosition enum (0=Inside, 1=Above, 2=Below)
    pub text_shadow: bool,
    pub background_color: String,
    pub text_format: String,
}

#[derive(Debug, Clone)]
pub struct DashboardEditorState {
    pub layout: String,
    pub spacing: u16,
    pub hide_inactive: bool,
    pub text_size: f32,
    pub icon_size: f32,
    pub padding: f32,
    pub show_labels: bool,
    pub show_values: bool,
    pub label_color: String,
    pub value_color: String,
    pub grid_color: String,
}

/// Field editors for InjuryDoll widgets
#[derive(Debug, Clone)]
pub struct InjuryDollEditorState {
    pub image_path: String,
    pub scale: f32,
    pub greyscale: bool,
    pub tint_color: String,
    pub tint_strength: f32,
    pub marker_tint_strength: f32,
    pub marker_style_index: usize,  // 0=Circles, 1=CirclesOutline, 2=Numbers, 3=Icons
    pub marker_size: f32,
    pub show_numbers: bool,

    // Color fields
    pub injury1_color: String,
    pub injury2_color: String,
    pub injury3_color: String,
    pub scar1_color: String,
    pub scar2_color: String,
    pub scar3_color: String,
    pub background_color: String,

    // Nerve tint colors
    pub nerve_tint1_color: String,
    pub nerve_tint2_color: String,
    pub nerve_tint3_color: String,

    // Calibration state
    pub calibration_active: bool,
    pub calibration_index: usize,  // 0-15 for the 16 body parts
}

/// GUI window editor state for a single window
pub struct GuiWindowEditor {
    /// Window name being edited
    pub window_name: String,

    /// Original WindowDef before modifications (for Cancel)
    original_def: WindowDef,

    /// Currently modified WindowDef (for preview)
    pub modified_def: WindowDef,

    /// Editor panel position/size in pixels
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub visible: bool,

    /// Widget-specific editor states
    pub active_effects_editor: Option<ActiveEffectsEditorState>,
    pub progress_editor: Option<ProgressEditorState>,
    pub countdown_editor: Option<CountdownEditorState>,
    pub text_editor: Option<TextEditorState>,
    pub room_editor: Option<RoomEditorState>,
    pub command_input_editor: Option<CommandInputEditorState>,
    pub tabbed_text_editor: Option<TabbedTextEditorState>,
    pub hand_editor: Option<HandEditorState>,
    pub indicator_editor: Option<IndicatorEditorState>,
    pub compass_editor: Option<CompassEditorState>,
    pub dashboard_editor: Option<DashboardEditorState>,
    pub injury_doll_editor: Option<InjuryDollEditorState>,
}

impl GuiWindowEditor {
    /// Create a new window editor
    pub fn new(
        window_name: String,
        window_def: WindowDef,
        position: [f32; 2],
        size: [f32; 2],
    ) -> Self {
        let original_def = window_def.clone();
        let modified_def = window_def.clone();

        // Initialize widget-specific editor states based on type
        let (
            active_effects_editor,
            progress_editor,
            countdown_editor,
            text_editor,
            room_editor,
            command_input_editor,
            tabbed_text_editor,
            hand_editor,
            indicator_editor,
            compass_editor,
            dashboard_editor,
            injury_doll_editor,
        ) = match &modified_def {
            WindowDef::ActiveEffects { data, .. } => {
                (Some(Self::init_active_effects_editor(data)), None, None, None, None, None, None, None, None, None, None, None)
            }
            WindowDef::Progress { data, .. } => {
                (None, Some(Self::init_progress_editor(data)), None, None, None, None, None, None, None, None, None, None)
            }
            WindowDef::Countdown { data, .. } => {
                (None, None, Some(Self::init_countdown_editor(data)), None, None, None, None, None, None, None, None, None)
            }
            WindowDef::Text { data, .. } => {
                (None, None, None, Some(Self::init_text_editor(data)), None, None, None, None, None, None, None, None)
            }
            WindowDef::Room { data, .. } => {
                (None, None, None, None, Some(Self::init_room_editor(data)), None, None, None, None, None, None, None)
            }
            WindowDef::CommandInput { data, .. } => {
                (None, None, None, None, None, Some(Self::init_command_input_editor(data)), None, None, None, None, None, None)
            }
            WindowDef::TabbedText { data, .. } => {
                (None, None, None, None, None, None, Some(Self::init_tabbed_text_editor(data)), None, None, None, None, None)
            }
            WindowDef::Hand { data, .. } => {
                (None, None, None, None, None, None, None, Some(Self::init_hand_editor(data)), None, None, None, None)
            }
            WindowDef::Indicator { data, .. } => {
                (None, None, None, None, None, None, None, None, Some(Self::init_indicator_editor(data)), None, None, None)
            }
            WindowDef::Compass { data, .. } => {
                (None, None, None, None, None, None, None, None, None, Some(Self::init_compass_editor(data)), None, None)
            }
            WindowDef::Dashboard { data, .. } => {
                (None, None, None, None, None, None, None, None, None, None, Some(Self::init_dashboard_editor(data)), None)
            }
            WindowDef::InjuryDoll { data, .. } => {
                (None, None, None, None, None, None, None, None, None, None, None, Some(Self::init_injury_doll_editor(data)))
            }
            _ => (None, None, None, None, None, None, None, None, None, None, None, None),
        };

        Self {
            window_name,
            original_def,
            modified_def,
            position,
            size,
            visible: true,
            active_effects_editor,
            progress_editor,
            countdown_editor,
            text_editor,
            room_editor,
            command_input_editor,
            tabbed_text_editor,
            hand_editor,
            indicator_editor,
            compass_editor,
            dashboard_editor,
            injury_doll_editor,
        }
    }

    /// Initialize ActiveEffects editor from WindowDef data
    fn init_active_effects_editor(data: &ActiveEffectsWidgetData) -> ActiveEffectsEditorState {
        // Convert enums to indices
        let style_index = match data.style {
            ActiveEffectsStyle::Overlay => 0,
            ActiveEffectsStyle::Separate => 1,
            ActiveEffectsStyle::ThinBar => 2,
            ActiveEffectsStyle::SideIndicator => 3,
            ActiveEffectsStyle::Minimal => 4,
        };

        let timer_position_index = match data.timer_position {
            TimerPosition::Left => 0,
            TimerPosition::Right => 1,
            TimerPosition::Inline => 2,
        };

        ActiveEffectsEditorState {
            style_index,
            timer_position_index,
            bar_height: data.bar_height,
            bar_opacity: data.bar_opacity,
            bar_rounding: data.bar_rounding,
            text_size: data.text_size,
            spacing: data.spacing,
            expiring_threshold: data.expiring_threshold,
            show_timer: data.show_timer,
            show_percentage: data.show_percentage,
            auto_contrast: data.auto_contrast,
            text_shadow: data.text_shadow,
            animate_changes: data.animate_changes,
            pulse_expiring: data.pulse_expiring,
        }
    }

    /// Initialize Progress editor from WindowDef data
    fn init_progress_editor(data: &ProgressWidgetData) -> ProgressEditorState {
        ProgressEditorState {
            id: data.id.clone().unwrap_or_default(),
            label: data.label.clone().unwrap_or_default(),
            color: data.color.clone().unwrap_or_default(),
            numbers_only: data.numbers_only,
            current_only: data.current_only,
            bar_height: data.bar_height,
            text_size: data.text_size,
            rounding: data.rounding,
            text_position: match data.text_position {
                ProgressTextPosition::Inside => 0,
                ProgressTextPosition::Above => 1,
                ProgressTextPosition::Below => 2,
            },
            text_shadow: data.text_shadow,
            background_color: data.background_color.clone().unwrap_or_default(),
            text_format: data.text_format.clone().unwrap_or_default(),
        }
    }

    /// Initialize Countdown editor from WindowDef data
    fn init_countdown_editor(data: &CountdownWidgetData) -> CountdownEditorState {
        CountdownEditorState {
            id: data.id.clone().unwrap_or_default(),
            label: data.label.clone().unwrap_or_default(),
            icon: data.icon.map(|c| c.to_string()).unwrap_or_default(),
            color: data.color.clone().unwrap_or_default(),
            background_color: data.background_color.clone().unwrap_or_default(),
            text_size: data.text_size,
            max_time: data.max_time,
            alert_threshold: data.alert_threshold,
            alert_color: data.alert_color.clone().unwrap_or_default(),
            pulse_when_ready: data.pulse_when_ready,
            format: match data.format {
                CountdownFormat::Seconds => 0,
                CountdownFormat::MMss => 1,
                CountdownFormat::HHMMss => 2,
            },
        }
    }

    /// Initialize Text editor from WindowDef data
    fn init_text_editor(data: &TextWidgetData) -> TextEditorState {
        TextEditorState {
            streams: data.streams.join(", "),
            buffer_size: data.buffer_size,
            wordwrap: data.wordwrap,
            show_timestamps: data.show_timestamps,
            font_size: data.font_size,
            line_spacing: data.line_spacing,
            padding: data.padding,
            text_color: data.text_color.clone().unwrap_or_default(),
            link_color: data.link_color.clone().unwrap_or_default(),
            link_underline_on_hover: data.link_underline_on_hover,
            auto_scroll: data.auto_scroll,
            timestamp_color: data.timestamp_color.clone().unwrap_or_default(),
            timestamp_format: data.timestamp_format.clone().unwrap_or_default(),
        }
    }

    /// Initialize Room editor from WindowDef data
    fn init_room_editor(data: &RoomWidgetData) -> RoomEditorState {
        RoomEditorState {
            buffer_size: data.buffer_size,
            show_desc: data.show_desc,
            show_objs: data.show_objs,
            show_players: data.show_players,
            show_exits: data.show_exits,
            show_name: data.show_name,
            name_text_size: data.name_text_size,
            name_color: data.name_color.clone().unwrap_or_default(),
            desc_text_size: data.desc_text_size,
            section_spacing: data.section_spacing,
            section_separators: data.section_separators,
            separator_color: data.separator_color.clone().unwrap_or_default(),
            show_component_headers: data.show_component_headers,
            header_text_size: data.header_text_size,
            header_color: data.header_color.clone().unwrap_or_default(),
        }
    }

    /// Initialize CommandInput editor from WindowDef data
    fn init_command_input_editor(data: &CommandInputWidgetData) -> CommandInputEditorState {
        CommandInputEditorState {
            text_color: data.text_color.clone().unwrap_or_default(),
            cursor_color: data.cursor_color.clone().unwrap_or_default(),
            cursor_background_color: data.cursor_background_color.clone().unwrap_or_default(),
            prompt_icon: data.prompt_icon.clone().unwrap_or_default(),
            prompt_icon_color: data.prompt_icon_color.clone().unwrap_or_default(),

            // NEW: Visual customization
            text_size: data.text_size,
            padding: data.padding,
            border_color: data.border_color.clone().unwrap_or_default(),
            background_color: data.background_color.clone().unwrap_or_default(),
            border_width: data.border_width,
        }
    }

    /// Initialize TabbedText editor from WindowDef data
    fn init_tabbed_text_editor(data: &TabbedTextWidgetData) -> TabbedTextEditorState {
        // Convert tabs from config to editor items
        let tabs: Vec<TabEditItem> = data.tabs.iter().map(|t| TabEditItem {
            name: t.name.clone(),
            streams: t.get_streams(),
            show_timestamps: t.show_timestamps.unwrap_or(false),
            ignore_activity: t.ignore_activity.unwrap_or(false),
        }).collect();

        TabbedTextEditorState {
            buffer_size: data.buffer_size,
            tab_bar_position: data.tab_bar_position.clone(),
            tab_separator: data.tab_separator,
            tab_active_color: data.tab_active_color.clone().unwrap_or_default(),
            tab_inactive_color: data.tab_inactive_color.clone().unwrap_or_default(),
            tab_unread_color: data.tab_unread_color.clone().unwrap_or_default(),
            tab_text_size: data.tab_text_size,
            tab_bar_height: data.tab_bar_height,
            tab_padding: data.tab_padding,
            tab_rounding: data.tab_rounding,
            content_font_size: data.content_font_size,

            // Tab list management
            tabs,
            selected_tab: 0,
            editing_tab: None,
            is_adding_new_tab: false,
            edit_tab_name: String::new(),
            edit_tab_streams: String::new(),
            edit_tab_show_timestamps: false,
            edit_tab_ignore_activity: false,
        }
    }

    /// Initialize Hand editor from WindowDef data
    fn init_hand_editor(data: &HandWidgetData) -> HandEditorState {
        HandEditorState {
            icon: data.icon.clone().unwrap_or_default(),
            icon_color: data.icon_color.clone().unwrap_or_default(),
            text_color: data.text_color.clone().unwrap_or_default(),
            text_size: data.text_size,
            icon_size: data.icon_size,
            spacing: data.spacing,
            empty_text: data.empty_text.clone().unwrap_or_else(|| "Empty".to_string()),
            empty_color: data.empty_color.clone().unwrap_or_default(),
            show_background: data.show_background,
            background_color: data.background_color.clone().unwrap_or_default(),
        }
    }

    /// Initialize Indicator editor from WindowDef data
    fn init_indicator_editor(data: &IndicatorWidgetData) -> IndicatorEditorState {
        let shape_index = match data.shape {
            IndicatorShape::Circle => 0,
            IndicatorShape::Square => 1,
            IndicatorShape::Icon => 2,
            IndicatorShape::Text => 3,
        };

        IndicatorEditorState {
            icon: data.icon.clone().unwrap_or_default(),
            indicator_id: data.indicator_id.clone().unwrap_or_default(),
            inactive_color: data.inactive_color.clone().unwrap_or_default(),
            active_color: data.active_color.clone().unwrap_or_default(),
            text_size: data.text_size,
            shape_index,
            indicator_size: data.indicator_size,
            glow_when_active: data.glow_when_active,
            glow_radius: data.glow_radius,
            background_color: data.background_color.clone().unwrap_or_default(),
            show_label: data.show_label,
        }
    }

    /// Initialize Compass editor from WindowDef data
    fn init_compass_editor(data: &CompassWidgetData) -> CompassEditorState {
        let layout_index = match data.layout {
            CompassLayout::Grid3x3 => 0,
            CompassLayout::Horizontal => 1,
            CompassLayout::Vertical => 2,
        };

        CompassEditorState {
            active_color: data.active_color.clone().unwrap_or_default(),
            inactive_color: data.inactive_color.clone().unwrap_or_default(),
            layout_index,
            spacing: data.spacing,
            text_size: data.text_size,
            use_icons: data.use_icons,
            bold_active: data.bold_active,
        }
    }

    fn init_dashboard_editor(data: &DashboardWidgetData) -> DashboardEditorState {
        DashboardEditorState {
            layout: data.layout.clone(),
            spacing: data.spacing,
            hide_inactive: data.hide_inactive,
            text_size: data.text_size,
            icon_size: data.icon_size,
            padding: data.padding,
            show_labels: data.show_labels,
            show_values: data.show_values,
            label_color: data.label_color.clone().unwrap_or_default(),
            value_color: data.value_color.clone().unwrap_or_default(),
            grid_color: data.grid_color.clone().unwrap_or_default(),
        }
    }

    /// Initialize InjuryDoll editor from WindowDef data
    fn init_injury_doll_editor(data: &InjuryDollWidgetData) -> InjuryDollEditorState {
        // Convert marker style enum to index
        let marker_style_index = match data.marker_style {
            InjuryMarkerStyle::Circles => 0,
            InjuryMarkerStyle::CirclesOutline => 1,
            InjuryMarkerStyle::Numbers => 2,
            InjuryMarkerStyle::Icons => 3,
        };

        InjuryDollEditorState {
            image_path: data.image_path.clone().unwrap_or_default(),
            scale: data.scale,
            greyscale: data.greyscale,
            tint_color: data.tint_color.clone().unwrap_or_default(),
            tint_strength: data.tint_strength,
            marker_tint_strength: data.marker_tint_strength,
            marker_style_index,
            marker_size: data.marker_size,
            show_numbers: data.show_numbers,
            injury1_color: data.injury1_color.clone().unwrap_or_default(),
            injury2_color: data.injury2_color.clone().unwrap_or_default(),
            injury3_color: data.injury3_color.clone().unwrap_or_default(),
            scar1_color: data.scar1_color.clone().unwrap_or_default(),
            scar2_color: data.scar2_color.clone().unwrap_or_default(),
            scar3_color: data.scar3_color.clone().unwrap_or_default(),
            background_color: data.background_color.clone().unwrap_or_default(),
            nerve_tint1_color: data.rank_indicators.nerve_tint1_color.clone(),
            nerve_tint2_color: data.rank_indicators.nerve_tint2_color.clone(),
            nerve_tint3_color: data.rank_indicators.nerve_tint3_color.clone(),
            calibration_active: false,
            calibration_index: 0,
        }
    }

    /// Get body part name by index (0-15)
    pub fn get_body_part_name(index: usize) -> &'static str {
        match index {
            0 => "head",
            1 => "neck",
            2 => "chest",
            3 => "abdomen",
            4 => "back",
            5 => "leftArm",
            6 => "rightArm",
            7 => "leftHand",
            8 => "rightHand",
            9 => "leftLeg",
            10 => "rightLeg",
            11 => "leftEye",
            12 => "rightEye",
            13 => "nsys",
            14 => "leftFoot",
            15 => "rightFoot",
            _ => "unknown",
        }
    }

    /// Apply changes to layout for preview
    pub fn apply(&mut self, app_core: &mut AppCore) {
        // Find the window by name in the Vec
        if let Some(window) = app_core.layout.windows.iter_mut()
            .find(|w| w.name() == self.window_name)
        {
            *window = self.modified_def.clone();
        }

        // For TabbedText windows, also sync tabs to ui_state content
        // This ensures tabs show up immediately without needing to restart
        if let WindowDef::TabbedText { data, .. } = &self.modified_def {
            if let Some(window_state) = app_core.ui_state.windows.get_mut(&self.window_name) {
                if let crate::data::window::WindowContent::TabbedText(content) = &mut window_state.content {
                    // Rebuild tabs from config while preserving any existing text content
                    let new_tabs: Vec<crate::data::widget::TabState> = data.tabs.iter().map(|tab_def| {
                        // Try to find existing tab with same name to preserve content
                        let existing_content = content.tabs.iter()
                            .find(|t| t.definition.name == tab_def.name)
                            .map(|t| t.content.clone());

                        let tab_content = existing_content.unwrap_or_else(|| {
                            crate::data::widget::TextContent::new(&tab_def.name, data.buffer_size)
                        });

                        crate::data::widget::TabState {
                            definition: crate::data::widget::TabDefinition {
                                name: tab_def.name.clone(),
                                streams: tab_def.get_streams(),
                                show_timestamps: tab_def.show_timestamps.unwrap_or(false),
                                ignore_activity: tab_def.ignore_activity.unwrap_or(false),
                            },
                            content: tab_content,
                        }
                    }).collect();

                    content.tabs = new_tabs;
                    // Clamp active_tab_index if it's out of bounds
                    if content.active_tab_index >= content.tabs.len() && !content.tabs.is_empty() {
                        content.active_tab_index = 0;
                    }
                }
            }
        }
    }

    /// Revert to original settings
    pub fn cancel(&mut self, app_core: &mut AppCore) {
        // Restore original def to layout
        if let Some(window) = app_core.layout.windows.iter_mut()
            .find(|w| w.name() == self.window_name)
        {
            *window = self.original_def.clone();
        }
        // Reset modified_def to original
        self.modified_def = self.original_def.clone();
        // Re-initialize editor states based on type
        match &self.modified_def {
            WindowDef::ActiveEffects { data, .. } => {
                self.active_effects_editor = Some(Self::init_active_effects_editor(data));
            }
            WindowDef::Progress { data, .. } => {
                self.progress_editor = Some(Self::init_progress_editor(data));
            }
            WindowDef::Countdown { data, .. } => {
                self.countdown_editor = Some(Self::init_countdown_editor(data));
            }
            WindowDef::Text { data, .. } => {
                self.text_editor = Some(Self::init_text_editor(data));
            }
            WindowDef::Room { data, .. } => {
                self.room_editor = Some(Self::init_room_editor(data));
            }
            WindowDef::CommandInput { data, .. } => {
                self.command_input_editor = Some(Self::init_command_input_editor(data));
            }
            WindowDef::TabbedText { data, .. } => {
                self.tabbed_text_editor = Some(Self::init_tabbed_text_editor(data));
            }
            WindowDef::Hand { data, .. } => {
                self.hand_editor = Some(Self::init_hand_editor(data));
            }
            WindowDef::Indicator { data, .. } => {
                self.indicator_editor = Some(Self::init_indicator_editor(data));
            }
            WindowDef::Compass { data, .. } => {
                self.compass_editor = Some(Self::init_compass_editor(data));
            }
            WindowDef::Dashboard { data, .. } => {
                self.dashboard_editor = Some(Self::init_dashboard_editor(data));
            }
            WindowDef::InjuryDoll { data, .. } => {
                self.injury_doll_editor = Some(Self::init_injury_doll_editor(data));
            }
            _ => {}
        }
    }

    /// Save changes to memory and config
    pub fn save(&mut self, app_core: &mut AppCore) {
        // Update layout with modified def
        if let Some(window) = app_core.layout.windows.iter_mut()
            .find(|w| w.name() == self.window_name)
        {
            *window = self.modified_def.clone();
        }

        // Update original_def to match (so Cancel won't revert)
        self.original_def = self.modified_def.clone();

        // Mark layout as modified
        app_core.layout_modified_since_save = true;

        // Save panel position/size to config
        app_core.config.window_editor.panel_positions.insert(
            self.window_name.clone(),
            self.position,
        );
        app_core.config.window_editor.panel_sizes.insert(
            self.window_name.clone(),
            self.size,
        );

        // TODO: Write config to disk (currently only in memory)
    }

    /// Render editor panel, return action to take
    pub fn render(&mut self, ctx: &egui::Context, _app_core: &AppCore) -> EditorAction {
        let mut action = EditorAction::None;

        // Check for Escape key to close
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return EditorAction::Close;
        }

        let mut open = self.visible;

        egui::Window::new(format!("Edit Window: {}", self.window_name))
            .id(egui::Id::new(format!("window_editor_{}", self.window_name)))
            .open(&mut open)
            .default_pos(self.position)
            .default_size(self.size)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                // Track position/size changes
                if let Some(rect) = ui.ctx().memory(|mem| {
                    mem.area_rect(egui::Id::new(format!("window_editor_{}", self.window_name)))
                }) {
                    self.position = [rect.min.x, rect.min.y];
                    self.size = [rect.width(), rect.height()];
                }

                // Base window settings (rows, cols, title, border, etc.)
                ui.heading("Window Settings");
                egui::Grid::new("window_base_settings")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        // Rows
                        ui.label("Rows:");
                        let mut rows = self.modified_def.base().rows;
                        ui.add(egui::Slider::new(&mut rows, 1..=100).suffix(" rows"));
                        self.modified_def.base_mut().rows = rows;
                        ui.end_row();

                        // Cols
                        ui.label("Cols:");
                        let mut cols = self.modified_def.base().cols;
                        ui.add(egui::Slider::new(&mut cols, 1..=200).suffix(" cols"));
                        self.modified_def.base_mut().cols = cols;
                        ui.end_row();

                        // Title
                        ui.label("Title:");
                        let mut title = self.modified_def.base().title.clone().unwrap_or_default();
                        ui.text_edit_singleline(&mut title);
                        self.modified_def.base_mut().title = if title.is_empty() { None } else { Some(title) };
                        ui.end_row();

                        // Font Family
                        ui.label("Font:");
                        let current_font = self.modified_def.base().font_family.clone().unwrap_or_else(|| "default".to_string());
                        let mut selected_font = current_font.clone();
                        egui::ComboBox::from_id_salt("font_family_selector")
                            .selected_text(&selected_font)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut selected_font, "default".to_string(), "Default");
                                ui.selectable_value(&mut selected_font, "monospace".to_string(), "Monospace");
                                ui.selectable_value(&mut selected_font, "proportional".to_string(), "Proportional");
                            });
                        self.modified_def.base_mut().font_family = if selected_font == "default" { None } else { Some(selected_font) };
                        ui.end_row();
                    });

                ui.checkbox(&mut self.modified_def.base_mut().show_border, "Show Border");

                ui.separator();

                // Render widget-specific controls
                match &mut self.modified_def {
                    WindowDef::ActiveEffects { data, .. } => {
                        if let Some(editor) = &mut self.active_effects_editor {
                            Self::render_active_effects_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::Progress { data, .. } => {
                        if let Some(editor) = &mut self.progress_editor {
                            Self::render_progress_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::Countdown { data, .. } => {
                        if let Some(editor) = &mut self.countdown_editor {
                            Self::render_countdown_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::Text { data, .. } => {
                        if let Some(editor) = &mut self.text_editor {
                            Self::render_text_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::Room { data, .. } => {
                        if let Some(editor) = &mut self.room_editor {
                            Self::render_room_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::CommandInput { data, .. } => {
                        if let Some(editor) = &mut self.command_input_editor {
                            Self::render_command_input_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::TabbedText { data, .. } => {
                        if let Some(editor) = &mut self.tabbed_text_editor {
                            Self::render_tabbed_text_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::Hand { data, .. } => {
                        if let Some(editor) = &mut self.hand_editor {
                            Self::render_hand_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::Indicator { data, .. } => {
                        if let Some(editor) = &mut self.indicator_editor {
                            Self::render_indicator_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::Compass { data, .. } => {
                        if let Some(editor) = &mut self.compass_editor {
                            Self::render_compass_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::Dashboard { data, .. } => {
                        if let Some(editor) = &mut self.dashboard_editor {
                            Self::render_dashboard_controls_static(ui, editor, data);
                        }
                    }
                    WindowDef::InjuryDoll { data, .. } => {
                        if let Some(editor) = &mut self.injury_doll_editor {
                            Self::render_injury_doll_controls_static(ui, editor, data);
                        }
                    }
                    _ => {
                        ui.label("Editor not yet implemented for this widget type");
                    }
                }

                ui.separator();

                // Bottom button row
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        action = EditorAction::Apply;
                    }
                    if ui.button("Cancel").clicked() {
                        action = EditorAction::Cancel;
                    }
                    if ui.button("Save").clicked() {
                        action = EditorAction::Save;
                    }
                    if ui.button("Close").clicked() {
                        action = EditorAction::Close;
                    }
                });
            });

        if !open {
            action = EditorAction::Close;
        }

        action
    }

    /// Render ActiveEffects controls (static method to avoid borrow issues)
    fn render_active_effects_controls_static(
        ui: &mut egui::Ui,
        editor: &mut ActiveEffectsEditorState,
        data: &mut ActiveEffectsWidgetData,
    ) {
        use egui::Grid;

        Grid::new("active_effects_controls")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {

                // Style dropdown
                ui.label("Style:");
                egui::ComboBox::from_id_salt("style_combo")
                    .selected_text(["Overlay", "Separate", "Thin Bar", "Side Indicator", "Minimal"][editor.style_index])
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut editor.style_index, 0, "Overlay");
                        ui.selectable_value(&mut editor.style_index, 1, "Separate");
                        ui.selectable_value(&mut editor.style_index, 2, "Thin Bar");
                        ui.selectable_value(&mut editor.style_index, 3, "Side Indicator");
                        ui.selectable_value(&mut editor.style_index, 4, "Minimal");
                    });
                ui.end_row();

                // Bar Height slider
                ui.label("Bar Height:");
                ui.add(egui::Slider::new(&mut editor.bar_height, 10.0..=40.0).suffix(" px"));
                ui.end_row();

                // Bar Opacity slider
                ui.label("Bar Opacity:");
                ui.add(egui::Slider::new(&mut editor.bar_opacity, 0.0..=1.0));
                ui.end_row();

                // Bar Rounding slider
                ui.label("Bar Rounding:");
                ui.add(egui::Slider::new(&mut editor.bar_rounding, 0.0..=10.0));
                ui.end_row();

                // Text Size slider
                ui.label("Text Size:");
                ui.add(egui::Slider::new(&mut editor.text_size, 8.0..=20.0).suffix(" px"));
                ui.end_row();

                // Spacing slider
                ui.label("Spacing:");
                ui.add(egui::Slider::new(&mut editor.spacing, 0.0..=10.0).suffix(" px"));
                ui.end_row();

                // Expiring Threshold
                ui.label("Expiring Threshold:");
                ui.add(egui::DragValue::new(&mut editor.expiring_threshold).suffix(" sec"));
                ui.end_row();
            });

        ui.separator();
        ui.heading("Timer Position");
        ui.horizontal(|ui| {
            ui.radio_value(&mut editor.timer_position_index, 0, "Left");
            ui.radio_value(&mut editor.timer_position_index, 1, "Right");
            ui.radio_value(&mut editor.timer_position_index, 2, "Inline");
        });

        ui.separator();
        ui.heading("Display Options");
        ui.checkbox(&mut editor.show_timer, "Show Timer");
        ui.checkbox(&mut editor.show_percentage, "Show Percentage");
        ui.checkbox(&mut editor.auto_contrast, "Auto Contrast");
        ui.checkbox(&mut editor.text_shadow, "Text Shadow");
        ui.checkbox(&mut editor.animate_changes, "Animate Changes");
        ui.checkbox(&mut editor.pulse_expiring, "Pulse When Expiring");

        // Sync editor state back to WindowDef data (inlined to avoid borrow issues)
        data.style = match editor.style_index {
            0 => ActiveEffectsStyle::Overlay,
            1 => ActiveEffectsStyle::Separate,
            2 => ActiveEffectsStyle::ThinBar,
            3 => ActiveEffectsStyle::SideIndicator,
            4 => ActiveEffectsStyle::Minimal,
            _ => ActiveEffectsStyle::Overlay,
        };
        data.timer_position = match editor.timer_position_index {
            0 => TimerPosition::Left,
            1 => TimerPosition::Right,
            2 => TimerPosition::Inline,
            _ => TimerPosition::Right,
        };
        data.bar_height = editor.bar_height;
        data.bar_opacity = editor.bar_opacity;
        data.bar_rounding = editor.bar_rounding;
        data.text_size = editor.text_size;
        data.spacing = editor.spacing;
        data.expiring_threshold = editor.expiring_threshold;
        data.show_timer = editor.show_timer;
        data.show_percentage = editor.show_percentage;
        data.auto_contrast = editor.auto_contrast;
        data.text_shadow = editor.text_shadow;
        data.animate_changes = editor.animate_changes;
        data.pulse_expiring = editor.pulse_expiring;
    }

    /// Render Progress controls (static method to avoid borrow issues)
    fn render_progress_controls_static(
        ui: &mut egui::Ui,
        editor: &mut ProgressEditorState,
        data: &mut ProgressWidgetData,
    ) {
        use egui::Grid;

        ui.heading("Data Settings");
        Grid::new("progress_data")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // ID text input
                ui.label("ID:");
                ui.text_edit_singleline(&mut editor.id);
                ui.end_row();

                // Label text input
                ui.label("Label:");
                ui.text_edit_singleline(&mut editor.label);
                ui.end_row();

                // Color picker
                ui.label("Color:");
                Self::render_color_field(ui, &mut editor.color, "#00FF00");
                ui.end_row();
            });

        ui.checkbox(&mut editor.numbers_only, "Numbers Only");
        ui.checkbox(&mut editor.current_only, "Current Only");

        ui.separator();
        ui.heading("Visual Customization");

        Grid::new("progress_visual")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Bar Height
                ui.label("Bar Height:");
                ui.add(egui::Slider::new(&mut editor.bar_height, 10.0..=40.0).suffix(" px"));
                ui.end_row();

                // Text Size
                ui.label("Text Size:");
                ui.add(egui::Slider::new(&mut editor.text_size, 8.0..=20.0).suffix(" px"));
                ui.end_row();

                // Rounding
                ui.label("Corner Rounding:");
                ui.add(egui::Slider::new(&mut editor.rounding, 0.0..=10.0).suffix(" px"));
                ui.end_row();

                // Text Position
                ui.label("Text Position:");
                egui::ComboBox::from_id_salt("progress_text_position")
                    .selected_text(match editor.text_position {
                        0 => "Inside",
                        1 => "Above",
                        2 => "Below",
                        _ => "Inside",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut editor.text_position, 0, "Inside");
                        ui.selectable_value(&mut editor.text_position, 1, "Above");
                        ui.selectable_value(&mut editor.text_position, 2, "Below");
                    });
                ui.end_row();

                // Background Color
                ui.label("Background Color:");
                Self::render_color_field(ui, &mut editor.background_color, "#333333");
                ui.end_row();

                // Text Format
                ui.label("Text Format:");
                ui.text_edit_singleline(&mut editor.text_format);
                ui.end_row();
            });

        ui.checkbox(&mut editor.text_shadow, "Text Shadow");

        // Sync editor state back to WindowDef data
        data.id = if editor.id.is_empty() { None } else { Some(editor.id.clone()) };
        data.label = if editor.label.is_empty() { None } else { Some(editor.label.clone()) };
        data.color = if editor.color.is_empty() { None } else { Some(editor.color.clone()) };
        data.numbers_only = editor.numbers_only;
        data.current_only = editor.current_only;
        data.bar_height = editor.bar_height;
        data.text_size = editor.text_size;
        data.rounding = editor.rounding;
        data.text_position = match editor.text_position {
            0 => ProgressTextPosition::Inside,
            1 => ProgressTextPosition::Above,
            2 => ProgressTextPosition::Below,
            _ => ProgressTextPosition::Inside,
        };
        data.text_shadow = editor.text_shadow;
        data.background_color = if editor.background_color.is_empty() {
            None
        } else {
            Some(editor.background_color.clone())
        };
        data.text_format = if editor.text_format.is_empty() {
            None
        } else {
            Some(editor.text_format.clone())
        };
    }

    /// Render Countdown controls (static method to avoid borrow issues)
    fn render_countdown_controls_static(
        ui: &mut egui::Ui,
        editor: &mut CountdownEditorState,
        data: &mut CountdownWidgetData,
    ) {
        use egui::Grid;

        ui.heading("Data Settings");
        Grid::new("countdown_data")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // ID text input
                ui.label("ID:");
                ui.text_edit_singleline(&mut editor.id);
                ui.end_row();

                // Label text input
                ui.label("Label:");
                ui.text_edit_singleline(&mut editor.label);
                ui.end_row();

                // Icon text input (single character)
                ui.label("Icon:");
                ui.text_edit_singleline(&mut editor.icon);
                ui.end_row();

                // Color picker
                ui.label("Color:");
                Self::render_color_field(ui, &mut editor.color, "#FFFF00");
                ui.end_row();

                // Background color picker
                ui.label("Background Color:");
                Self::render_color_field(ui, &mut editor.background_color, "#333333");
                ui.end_row();
            });

        ui.separator();
        ui.heading("Visual Customization");

        Grid::new("countdown_visual")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Text Size
                ui.label("Text Size:");
                ui.add(egui::Slider::new(&mut editor.text_size, 8.0..=24.0).suffix(" px"));
                ui.end_row();

                // Max Time
                ui.label("Max Time (for bar scale):");
                ui.add(egui::Slider::new(&mut editor.max_time, 5..=300).suffix(" sec"));
                ui.end_row();

                // Alert Threshold
                ui.label("Alert Threshold:");
                ui.add(egui::Slider::new(&mut editor.alert_threshold, 1..=60).suffix(" sec"));
                ui.end_row();

                // Alert Color picker
                ui.label("Alert Color:");
                Self::render_color_field(ui, &mut editor.alert_color, "#FF0000");
                ui.end_row();

                // Format dropdown
                ui.label("Time Format:");
                egui::ComboBox::from_id_salt("countdown_format")
                    .selected_text(match editor.format {
                        0 => "Seconds (5s, 30s)",
                        1 => "MM:ss (01:30, 00:05)",
                        2 => "HH:MM:ss (00:01:30)",
                        _ => "Seconds",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut editor.format, 0, "Seconds (5s, 30s)");
                        ui.selectable_value(&mut editor.format, 1, "MM:ss (01:30, 00:05)");
                        ui.selectable_value(&mut editor.format, 2, "HH:MM:ss (00:01:30)");
                    });
                ui.end_row();
            });

        ui.checkbox(&mut editor.pulse_when_ready, "Pulse when ready/complete");

        // Sync editor state back to WindowDef data
        data.id = if editor.id.is_empty() { None } else { Some(editor.id.clone()) };
        data.label = if editor.label.is_empty() { None } else { Some(editor.label.clone()) };
        data.icon = editor.icon.chars().next();
        data.color = if editor.color.is_empty() { None } else { Some(editor.color.clone()) };
        data.background_color = if editor.background_color.is_empty() { None } else { Some(editor.background_color.clone()) };
        data.text_size = editor.text_size;
        data.max_time = editor.max_time;
        data.alert_threshold = editor.alert_threshold;
        data.alert_color = if editor.alert_color.is_empty() {
            None
        } else {
            Some(editor.alert_color.clone())
        };
        data.pulse_when_ready = editor.pulse_when_ready;
        data.format = match editor.format {
            0 => CountdownFormat::Seconds,
            1 => CountdownFormat::MMss,
            2 => CountdownFormat::HHMMss,
            _ => CountdownFormat::Seconds,
        };
    }

    /// Render Text controls (static method to avoid borrow issues)
    fn render_text_controls_static(
        ui: &mut egui::Ui,
        editor: &mut TextEditorState,
        data: &mut TextWidgetData,
    ) {
        use egui::Grid;

        ui.heading("Data Settings");
        Grid::new("text_data")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Streams (comma-separated)
                ui.label("Streams:");
                ui.text_edit_singleline(&mut editor.streams);
                ui.end_row();

                // Buffer size
                ui.label("Buffer Size:");
                ui.add(egui::DragValue::new(&mut editor.buffer_size).range(100..=100000));
                ui.end_row();
            });

        ui.separator();
        ui.heading("Visual Customization");
        Grid::new("text_visual")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Font size slider
                ui.label("Font Size:");
                ui.add(egui::Slider::new(&mut editor.font_size, 8.0..=20.0).suffix(" px"));
                ui.end_row();

                // Line spacing slider
                ui.label("Line Spacing:");
                ui.add(egui::Slider::new(&mut editor.line_spacing, 0.0..=10.0).suffix(" px"));
                ui.end_row();

                // Padding slider
                ui.label("Padding:");
                ui.add(egui::Slider::new(&mut editor.padding, 0.0..=20.0).suffix(" px"));
                ui.end_row();

                // Text color picker
                ui.label("Text Color:");
                Self::render_color_field(ui, &mut editor.text_color, "#FFFFFF");
                ui.end_row();

                // Link color picker
                ui.label("Link Color:");
                Self::render_color_field(ui, &mut editor.link_color, "#00FFFF");
                ui.end_row();

                // Timestamp color picker
                ui.label("Timestamp Color:");
                Self::render_color_field(ui, &mut editor.timestamp_color, "#888888");
                ui.end_row();

                // Timestamp format
                ui.label("Timestamp Format:");
                ui.text_edit_singleline(&mut editor.timestamp_format);
                ui.end_row();
            });

        ui.separator();
        ui.heading("Display Options");
        ui.checkbox(&mut editor.wordwrap, "Word Wrap");
        ui.checkbox(&mut editor.show_timestamps, "Show Timestamps");
        ui.checkbox(&mut editor.link_underline_on_hover, "Link Underline on Hover");
        ui.checkbox(&mut editor.auto_scroll, "Auto Scroll");

        // Sync editor state back to WindowDef data
        data.streams = editor.streams
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        data.buffer_size = editor.buffer_size;
        data.wordwrap = editor.wordwrap;
        data.show_timestamps = editor.show_timestamps;
        data.font_size = editor.font_size;
        data.line_spacing = editor.line_spacing;
        data.padding = editor.padding;
        data.text_color = if editor.text_color.is_empty() { None } else { Some(editor.text_color.clone()) };
        data.link_color = if editor.link_color.is_empty() { None } else { Some(editor.link_color.clone()) };
        data.link_underline_on_hover = editor.link_underline_on_hover;
        data.auto_scroll = editor.auto_scroll;
        data.timestamp_color = if editor.timestamp_color.is_empty() { None } else { Some(editor.timestamp_color.clone()) };
        data.timestamp_format = if editor.timestamp_format.is_empty() { None } else { Some(editor.timestamp_format.clone()) };
    }

    /// Render Room controls (static method to avoid borrow issues)
    fn render_room_controls_static(
        ui: &mut egui::Ui,
        editor: &mut RoomEditorState,
        data: &mut RoomWidgetData,
    ) {
        use egui::Grid;

        ui.heading("Data Settings");
        Grid::new("room_data")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Buffer size
                ui.label("Buffer Size:");
                ui.add(egui::DragValue::new(&mut editor.buffer_size).range(0..=100000));
                ui.end_row();
            });

        ui.separator();
        ui.heading("Component Visibility");
        ui.checkbox(&mut editor.show_name, "Show Room Name");
        ui.checkbox(&mut editor.show_desc, "Show Description");
        ui.checkbox(&mut editor.show_objs, "Show Objects");
        ui.checkbox(&mut editor.show_players, "Show Players");
        ui.checkbox(&mut editor.show_exits, "Show Exits");

        ui.separator();
        ui.heading("Visual Customization");
        Grid::new("room_visual")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Name text size
                ui.label("Name Text Size:");
                ui.add(egui::Slider::new(&mut editor.name_text_size, 14.0..=24.0).suffix(" px"));
                ui.end_row();

                // Name color picker
                ui.label("Name Color:");
                Self::render_color_field(ui, &mut editor.name_color, "#FFFFFF");
                ui.end_row();

                // Description text size
                ui.label("Description Text Size:");
                ui.add(egui::Slider::new(&mut editor.desc_text_size, 10.0..=18.0).suffix(" px"));
                ui.end_row();

                // Section spacing
                ui.label("Section Spacing:");
                ui.add(egui::Slider::new(&mut editor.section_spacing, 4.0..=16.0).suffix(" px"));
                ui.end_row();

                // Separator color picker
                ui.label("Separator Color:");
                Self::render_color_field(ui, &mut editor.separator_color, "#444444");
                ui.end_row();

                // Header text size
                ui.label("Header Text Size:");
                ui.add(egui::Slider::new(&mut editor.header_text_size, 10.0..=16.0).suffix(" px"));
                ui.end_row();

                // Header color picker
                ui.label("Header Color:");
                Self::render_color_field(ui, &mut editor.header_color, "#888888");
                ui.end_row();
            });

        ui.separator();
        ui.heading("Display Options");
        ui.checkbox(&mut editor.section_separators, "Section Separators");
        ui.checkbox(&mut editor.show_component_headers, "Show Component Headers");

        // Sync editor state back to WindowDef data
        data.buffer_size = editor.buffer_size;
        data.show_desc = editor.show_desc;
        data.show_objs = editor.show_objs;
        data.show_players = editor.show_players;
        data.show_exits = editor.show_exits;
        data.show_name = editor.show_name;
        data.name_text_size = editor.name_text_size;
        data.name_color = if editor.name_color.is_empty() { None } else { Some(editor.name_color.clone()) };
        data.desc_text_size = editor.desc_text_size;
        data.section_spacing = editor.section_spacing;
        data.section_separators = editor.section_separators;
        data.separator_color = if editor.separator_color.is_empty() { None } else { Some(editor.separator_color.clone()) };
        data.show_component_headers = editor.show_component_headers;
        data.header_text_size = editor.header_text_size;
        data.header_color = if editor.header_color.is_empty() { None } else { Some(editor.header_color.clone()) };
    }

    /// Render CommandInput controls (static method to avoid borrow issues)
    fn render_command_input_controls_static(
        ui: &mut egui::Ui,
        editor: &mut CommandInputEditorState,
        data: &mut CommandInputWidgetData,
    ) {
        use egui::Grid;

        Grid::new("command_input_controls")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Text color picker
                ui.label("Text Color:");
                Self::render_color_field(ui, &mut editor.text_color, "#FFFFFF");
                ui.end_row();

                // Cursor color picker
                ui.label("Cursor Color:");
                Self::render_color_field(ui, &mut editor.cursor_color, "#FFFFFF");
                ui.end_row();

                // Cursor background color picker
                ui.label("Cursor Background:");
                Self::render_color_field(ui, &mut editor.cursor_background_color, "#444444");
                ui.end_row();

                // Prompt icon
                ui.label("Prompt Icon:");
                ui.text_edit_singleline(&mut editor.prompt_icon);
                ui.end_row();

                // Prompt icon color picker
                ui.label("Prompt Icon Color:");
                Self::render_color_field(ui, &mut editor.prompt_icon_color, "#888888");
                ui.end_row();

                // Visual customization controls
                ui.label("Text Size:");
                ui.add(egui::Slider::new(&mut editor.text_size, 10.0..=20.0).suffix(" px"));
                ui.end_row();

                ui.label("Padding:");
                ui.add(egui::Slider::new(&mut editor.padding, 2.0..=10.0).suffix(" px"));
                ui.end_row();

                // Border color picker
                ui.label("Border Color:");
                Self::render_color_field(ui, &mut editor.border_color, "#555555");
                ui.end_row();

                // Background color picker
                ui.label("Background Color:");
                Self::render_color_field(ui, &mut editor.background_color, "#222222");
                ui.end_row();

                ui.label("Border Width:");
                ui.add(egui::Slider::new(&mut editor.border_width, 0.0..=3.0).suffix(" px"));
                ui.end_row();
            });

        // Sync editor state back to WindowDef data
        data.text_color = if editor.text_color.is_empty() { None } else { Some(editor.text_color.clone()) };
        data.cursor_color = if editor.cursor_color.is_empty() { None } else { Some(editor.cursor_color.clone()) };
        data.cursor_background_color = if editor.cursor_background_color.is_empty() { None } else { Some(editor.cursor_background_color.clone()) };
        data.prompt_icon = if editor.prompt_icon.is_empty() { None } else { Some(editor.prompt_icon.clone()) };
        data.prompt_icon_color = if editor.prompt_icon_color.is_empty() { None } else { Some(editor.prompt_icon_color.clone()) };

        // NEW: Sync visual customization
        data.text_size = editor.text_size;
        data.padding = editor.padding;
        data.border_color = if editor.border_color.is_empty() { None } else { Some(editor.border_color.clone()) };
        data.background_color = if editor.background_color.is_empty() { None } else { Some(editor.background_color.clone()) };
        data.border_width = editor.border_width;
    }

    /// Render TabbedText controls (static method to avoid borrow issues)
    fn render_tabbed_text_controls_static(
        ui: &mut egui::Ui,
        editor: &mut TabbedTextEditorState,
        data: &mut TabbedTextWidgetData,
    ) {
        use egui::Grid;

        // === TAB MANAGEMENT SECTION ===
        ui.heading("Tabs");

        // Show edit form if editing/adding a tab
        if editor.editing_tab.is_some() || editor.is_adding_new_tab {
            let title = if editor.is_adding_new_tab { "Add New Tab" } else { "Edit Tab" };
            ui.group(|ui| {
                ui.label(egui::RichText::new(title).strong());

                Grid::new("tab_edit_form")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut editor.edit_tab_name);
                        ui.end_row();

                        ui.label("Streams:");
                        ui.text_edit_singleline(&mut editor.edit_tab_streams);
                        ui.end_row();
                    });

                ui.label("(comma-separated stream names, e.g. 'room,combat,death')");

                ui.horizontal(|ui| {
                    ui.checkbox(&mut editor.edit_tab_show_timestamps, "Show Timestamps");
                    ui.checkbox(&mut editor.edit_tab_ignore_activity, "Ignore Activity");
                });

                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        // Parse streams from comma-separated string
                        let streams: Vec<String> = editor.edit_tab_streams
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();

                        if !editor.edit_tab_name.is_empty() {
                            if editor.is_adding_new_tab {
                                // Add new tab
                                editor.tabs.push(TabEditItem {
                                    name: editor.edit_tab_name.clone(),
                                    streams,
                                    show_timestamps: editor.edit_tab_show_timestamps,
                                    ignore_activity: editor.edit_tab_ignore_activity,
                                });
                            } else if let Some(idx) = editor.editing_tab {
                                // Update existing tab
                                if let Some(tab) = editor.tabs.get_mut(idx) {
                                    tab.name = editor.edit_tab_name.clone();
                                    tab.streams = streams;
                                    tab.show_timestamps = editor.edit_tab_show_timestamps;
                                    tab.ignore_activity = editor.edit_tab_ignore_activity;
                                }
                            }
                        }
                        // Clear form
                        editor.editing_tab = None;
                        editor.is_adding_new_tab = false;
                        editor.edit_tab_name.clear();
                        editor.edit_tab_streams.clear();
                        editor.edit_tab_show_timestamps = false;
                        editor.edit_tab_ignore_activity = false;
                    }

                    if ui.button("Cancel").clicked() {
                        editor.editing_tab = None;
                        editor.is_adding_new_tab = false;
                        editor.edit_tab_name.clear();
                        editor.edit_tab_streams.clear();
                        editor.edit_tab_show_timestamps = false;
                        editor.edit_tab_ignore_activity = false;
                    }
                });
            });
        } else {
            // Show tab list
            let mut tab_to_delete: Option<usize> = None;
            let mut tab_to_edit: Option<usize> = None;
            let mut tab_to_move_up: Option<usize> = None;
            let mut tab_to_move_down: Option<usize> = None;

            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    for (idx, tab) in editor.tabs.iter().enumerate() {
                        ui.horizontal(|ui| {
                            // Selection indicator
                            let is_selected = editor.selected_tab == idx;
                            if ui.selectable_label(is_selected, &tab.name).clicked() {
                                editor.selected_tab = idx;
                            }

                            // Stream summary
                            let streams_str = if tab.streams.is_empty() {
                                "(no streams)".to_string()
                            } else {
                                format!("({})", tab.streams.join(", "))
                            };
                            ui.weak(&streams_str);

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // Delete button (only if more than 1 tab)
                                if editor.tabs.len() > 1 {
                                    if ui.small_button("🗑").on_hover_text("Delete tab").clicked() {
                                        tab_to_delete = Some(idx);
                                    }
                                }

                                // Edit button
                                if ui.small_button("✏").on_hover_text("Edit tab").clicked() {
                                    tab_to_edit = Some(idx);
                                }

                                // Move down button
                                if idx < editor.tabs.len() - 1 {
                                    if ui.small_button("↓").on_hover_text("Move down").clicked() {
                                        tab_to_move_down = Some(idx);
                                    }
                                }

                                // Move up button
                                if idx > 0 {
                                    if ui.small_button("↑").on_hover_text("Move up").clicked() {
                                        tab_to_move_up = Some(idx);
                                    }
                                }
                            });
                        });
                    }
                });

            // Process actions after iteration
            if let Some(idx) = tab_to_delete {
                editor.tabs.remove(idx);
                if editor.selected_tab >= editor.tabs.len() && editor.selected_tab > 0 {
                    editor.selected_tab -= 1;
                }
            }

            if let Some(idx) = tab_to_edit {
                let tab = &editor.tabs[idx];
                editor.editing_tab = Some(idx);
                editor.is_adding_new_tab = false;
                editor.edit_tab_name = tab.name.clone();
                editor.edit_tab_streams = tab.streams.join(", ");
                editor.edit_tab_show_timestamps = tab.show_timestamps;
                editor.edit_tab_ignore_activity = tab.ignore_activity;
            }

            if let Some(idx) = tab_to_move_up {
                editor.tabs.swap(idx, idx - 1);
                if editor.selected_tab == idx {
                    editor.selected_tab = idx - 1;
                } else if editor.selected_tab == idx - 1 {
                    editor.selected_tab = idx;
                }
            }

            if let Some(idx) = tab_to_move_down {
                editor.tabs.swap(idx, idx + 1);
                if editor.selected_tab == idx {
                    editor.selected_tab = idx + 1;
                } else if editor.selected_tab == idx + 1 {
                    editor.selected_tab = idx;
                }
            }

            // Add tab button
            if ui.button("+ Add Tab").clicked() {
                editor.is_adding_new_tab = true;
                editor.editing_tab = None;
                editor.edit_tab_name.clear();
                editor.edit_tab_streams.clear();
                editor.edit_tab_show_timestamps = false;
                editor.edit_tab_ignore_activity = false;
            }
        }

        ui.separator();

        // === STYLING SECTION ===
        ui.heading("Tab Bar Styling");
        Grid::new("tabbed_text_controls")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Buffer size
                ui.label("Buffer Size:");
                ui.add(egui::DragValue::new(&mut editor.buffer_size).range(100..=100000));
                ui.end_row();

                // Tab bar position
                ui.label("Tab Bar Position:");
                ui.text_edit_singleline(&mut editor.tab_bar_position);
                ui.end_row();

                // Tab active color
                ui.label("Tab Active Color:");
                Self::render_color_field(ui, &mut editor.tab_active_color, "#FFFF00");
                ui.end_row();

                // Tab inactive color
                ui.label("Tab Inactive Color:");
                Self::render_color_field(ui, &mut editor.tab_inactive_color, "#808080");
                ui.end_row();

                // Tab unread color
                ui.label("Tab Unread Color:");
                Self::render_color_field(ui, &mut editor.tab_unread_color, "#FFFFFF");
                ui.end_row();
            });

        ui.separator();
        ui.heading("Visual Customization");
        Grid::new("tabbed_text_visual")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Tab text size slider
                ui.label("Tab Text Size:");
                ui.add(egui::Slider::new(&mut editor.tab_text_size, 10.0..=18.0).suffix(" px"));
                ui.end_row();

                // Tab bar height slider
                ui.label("Tab Bar Height:");
                ui.add(egui::Slider::new(&mut editor.tab_bar_height, 20.0..=40.0).suffix(" px"));
                ui.end_row();

                // Tab padding slider
                ui.label("Tab Padding:");
                ui.add(egui::Slider::new(&mut editor.tab_padding, 2.0..=10.0).suffix(" px"));
                ui.end_row();

                // Tab rounding slider
                ui.label("Tab Rounding:");
                ui.add(egui::Slider::new(&mut editor.tab_rounding, 0.0..=8.0).suffix(" px"));
                ui.end_row();

                // Content font size slider
                ui.label("Content Font Size:");
                ui.add(egui::Slider::new(&mut editor.content_font_size, 8.0..=20.0).suffix(" px"));
                ui.end_row();
            });

        ui.separator();
        ui.heading("Display Options");
        ui.checkbox(&mut editor.tab_separator, "Show separators between tabs");

        // Sync editor state back to WindowDef data
        data.buffer_size = editor.buffer_size;
        data.tab_bar_position = editor.tab_bar_position.clone();
        data.tab_separator = editor.tab_separator;
        data.tab_active_color = if editor.tab_active_color.is_empty() { None } else { Some(editor.tab_active_color.clone()) };
        data.tab_inactive_color = if editor.tab_inactive_color.is_empty() { None } else { Some(editor.tab_inactive_color.clone()) };
        data.tab_unread_color = if editor.tab_unread_color.is_empty() { None } else { Some(editor.tab_unread_color.clone()) };
        data.tab_text_size = editor.tab_text_size;
        data.tab_bar_height = editor.tab_bar_height;
        data.tab_padding = editor.tab_padding;
        data.tab_rounding = editor.tab_rounding;
        data.content_font_size = editor.content_font_size;

        // Sync tabs back to data
        data.tabs = editor.tabs.iter().map(|t| crate::config::TabbedTextTab {
            name: t.name.clone(),
            stream: None,  // Use streams array, not single stream
            streams: t.streams.clone(),
            show_timestamps: Some(t.show_timestamps),
            ignore_activity: Some(t.ignore_activity),
        }).collect();
    }

    /// Render Hand controls (static method to avoid borrow issues)
    fn render_hand_controls_static(
        ui: &mut egui::Ui,
        editor: &mut HandEditorState,
        data: &mut HandWidgetData,
    ) {
        use egui::Grid;

        Grid::new("hand_controls")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Icon text input
                ui.label("Icon:");
                ui.text_edit_singleline(&mut editor.icon);
                ui.end_row();

                // Icon color picker
                ui.label("Icon Color:");
                Self::render_color_field(ui, &mut editor.icon_color, "#FFFF00");
                ui.end_row();

                // Text color picker
                ui.label("Text Color:");
                Self::render_color_field(ui, &mut editor.text_color, "#FFFFFF");
                ui.end_row();

                // Text size slider
                ui.label("Text Size:");
                ui.add(egui::Slider::new(&mut editor.text_size, 10.0..=20.0).suffix(" px"));
                ui.end_row();

                // Icon size slider
                ui.label("Icon Size:");
                ui.add(egui::Slider::new(&mut editor.icon_size, 10.0..=24.0).suffix(" px"));
                ui.end_row();

                // Spacing slider
                ui.label("Spacing:");
                ui.add(egui::Slider::new(&mut editor.spacing, 2.0..=8.0).suffix(" px"));
                ui.end_row();

                // Empty text
                ui.label("Empty Text:");
                ui.text_edit_singleline(&mut editor.empty_text);
                ui.end_row();

                // Empty color picker
                ui.label("Empty Color:");
                Self::render_color_field(ui, &mut editor.empty_color, "#888888");
                ui.end_row();

                // Background color picker
                ui.label("Background Color:");
                Self::render_color_field(ui, &mut editor.background_color, "#333333");
                ui.end_row();
            });

        ui.separator();
        ui.heading("Display Options");
        ui.checkbox(&mut editor.show_background, "Show Background");

        // Sync editor state back to WindowDef data
        data.icon = if editor.icon.is_empty() { None } else { Some(editor.icon.clone()) };
        data.icon_color = if editor.icon_color.is_empty() { None } else { Some(editor.icon_color.clone()) };
        data.text_color = if editor.text_color.is_empty() { None } else { Some(editor.text_color.clone()) };
        data.text_size = editor.text_size;
        data.icon_size = editor.icon_size;
        data.spacing = editor.spacing;
        data.empty_text = if editor.empty_text.is_empty() { None } else { Some(editor.empty_text.clone()) };
        data.empty_color = if editor.empty_color.is_empty() { None } else { Some(editor.empty_color.clone()) };
        data.show_background = editor.show_background;
        data.background_color = if editor.background_color.is_empty() { None } else { Some(editor.background_color.clone()) };
    }

    fn render_indicator_controls_static(
        ui: &mut egui::Ui,
        editor: &mut IndicatorEditorState,
        data: &mut IndicatorWidgetData,
    ) {
        use egui::Grid;

        ui.heading("Basic Settings");
        Grid::new("indicator_basic")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Icon text input
                ui.label("Icon:");
                ui.text_edit_singleline(&mut editor.icon);
                ui.end_row();

                // Indicator ID
                ui.label("Indicator ID:");
                ui.text_edit_singleline(&mut editor.indicator_id);
                ui.end_row();

                // Inactive color picker
                ui.label("Inactive Color:");
                Self::render_color_field(ui, &mut editor.inactive_color, "#888888");
                ui.end_row();

                // Active color picker
                ui.label("Active Color:");
                Self::render_color_field(ui, &mut editor.active_color, "#00FF00");
                ui.end_row();
            });

        ui.separator();
        ui.heading("Visual Customization");
        Grid::new("indicator_visual")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Text size slider
                ui.label("Text Size:");
                ui.add(egui::Slider::new(&mut editor.text_size, 10.0..=24.0).suffix(" px"));
                ui.end_row();

                // Shape dropdown
                ui.label("Shape:");
                egui::ComboBox::from_id_salt("indicator_shape")
                    .selected_text(["Circle", "Square", "Icon", "Text"][editor.shape_index])
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut editor.shape_index, 0, "Circle");
                        ui.selectable_value(&mut editor.shape_index, 1, "Square");
                        ui.selectable_value(&mut editor.shape_index, 2, "Icon");
                        ui.selectable_value(&mut editor.shape_index, 3, "Text");
                    });
                ui.end_row();

                // Indicator size slider
                ui.label("Indicator Size:");
                ui.add(egui::Slider::new(&mut editor.indicator_size, 10.0..=40.0).suffix(" px"));
                ui.end_row();

                // Glow radius slider
                ui.label("Glow Radius:");
                ui.add(egui::Slider::new(&mut editor.glow_radius, 0.0..=10.0).suffix(" px"));
                ui.end_row();

                // Background color picker
                ui.label("Background Color:");
                Self::render_color_field(ui, &mut editor.background_color, "#333333");
                ui.end_row();
            });

        ui.separator();
        ui.heading("Display Options");
        ui.checkbox(&mut editor.glow_when_active, "Glow When Active");
        ui.checkbox(&mut editor.show_label, "Show Label");

        // Sync editor state back to WindowDef data
        data.icon = if editor.icon.is_empty() { None } else { Some(editor.icon.clone()) };
        data.indicator_id = if editor.indicator_id.is_empty() { None } else { Some(editor.indicator_id.clone()) };
        data.inactive_color = if editor.inactive_color.is_empty() { None } else { Some(editor.inactive_color.clone()) };
        data.active_color = if editor.active_color.is_empty() { None } else { Some(editor.active_color.clone()) };
        data.text_size = editor.text_size;
        data.shape = match editor.shape_index {
            0 => IndicatorShape::Circle,
            1 => IndicatorShape::Square,
            2 => IndicatorShape::Icon,
            3 => IndicatorShape::Text,
            _ => IndicatorShape::Circle,
        };
        data.indicator_size = editor.indicator_size;
        data.glow_when_active = editor.glow_when_active;
        data.glow_radius = editor.glow_radius;
        data.background_color = if editor.background_color.is_empty() { None } else { Some(editor.background_color.clone()) };
        data.show_label = editor.show_label;
    }

    fn render_compass_controls_static(
        ui: &mut egui::Ui,
        editor: &mut CompassEditorState,
        data: &mut CompassWidgetData,
    ) {
        use egui::Grid;

        ui.heading("Color Settings");
        Grid::new("compass_colors")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Active color picker
                ui.label("Active Color:");
                Self::render_color_field(ui, &mut editor.active_color, "#FFFF00");
                ui.end_row();

                // Inactive color picker
                ui.label("Inactive Color:");
                Self::render_color_field(ui, &mut editor.inactive_color, "#888888");
                ui.end_row();
            });

        ui.separator();
        ui.heading("Visual Customization");
        Grid::new("compass_visual")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Layout dropdown
                ui.label("Layout:");
                egui::ComboBox::from_id_salt("compass_layout")
                    .selected_text(["Grid 3x3", "Horizontal", "Vertical"][editor.layout_index])
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut editor.layout_index, 0, "Grid 3x3");
                        ui.selectable_value(&mut editor.layout_index, 1, "Horizontal");
                        ui.selectable_value(&mut editor.layout_index, 2, "Vertical");
                    });
                ui.end_row();

                // Spacing slider
                ui.label("Spacing:");
                ui.add(egui::Slider::new(&mut editor.spacing, 2.0..=10.0).suffix(" px"));
                ui.end_row();

                // Text size slider
                ui.label("Text Size:");
                ui.add(egui::Slider::new(&mut editor.text_size, 10.0..=20.0).suffix(" px"));
                ui.end_row();
            });

        ui.separator();
        ui.heading("Display Options");
        ui.checkbox(&mut editor.use_icons, "Use Direction Icons");
        ui.checkbox(&mut editor.bold_active, "Bold Active Directions");

        // Sync editor state back to WindowDef data
        data.active_color = if editor.active_color.is_empty() { None } else { Some(editor.active_color.clone()) };
        data.inactive_color = if editor.inactive_color.is_empty() { None } else { Some(editor.inactive_color.clone()) };
        data.layout = match editor.layout_index {
            0 => CompassLayout::Grid3x3,
            1 => CompassLayout::Horizontal,
            2 => CompassLayout::Vertical,
            _ => CompassLayout::Grid3x3,
        };
        data.spacing = editor.spacing;
        data.text_size = editor.text_size;
        data.use_icons = editor.use_icons;
        data.bold_active = editor.bold_active;
    }

    fn render_dashboard_controls_static(
        ui: &mut egui::Ui,
        editor: &mut DashboardEditorState,
        data: &mut DashboardWidgetData,
    ) {
        use egui::Grid;

        ui.heading("Layout Settings");
        Grid::new("dashboard_layout")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Layout text input
                ui.label("Layout:");
                ui.text_edit_singleline(&mut editor.layout);
                ui.end_row();

                // Spacing slider
                ui.label("Spacing:");
                ui.add(egui::Slider::new(&mut editor.spacing, 0..=10).suffix(" chars"));
                ui.end_row();
            });

        ui.separator();
        ui.heading("Visual Customization");
        Grid::new("dashboard_visual")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Text size slider
                ui.label("Text Size:");
                ui.add(egui::Slider::new(&mut editor.text_size, 10.0..=20.0).suffix(" px"));
                ui.end_row();

                // Icon size slider
                ui.label("Icon Size:");
                ui.add(egui::Slider::new(&mut editor.icon_size, 12.0..=32.0).suffix(" px"));
                ui.end_row();

                // Padding slider
                ui.label("Padding:");
                ui.add(egui::Slider::new(&mut editor.padding, 2.0..=10.0).suffix(" px"));
                ui.end_row();

                // Label color picker
                ui.label("Label Color:");
                Self::render_color_field(ui, &mut editor.label_color, "#888888");
                ui.end_row();

                // Value color picker
                ui.label("Value Color:");
                Self::render_color_field(ui, &mut editor.value_color, "#FFFFFF");
                ui.end_row();

                // Grid color picker
                ui.label("Grid Color:");
                Self::render_color_field(ui, &mut editor.grid_color, "#444444");
                ui.end_row();
            });

        ui.separator();
        ui.heading("Display Options");
        ui.checkbox(&mut editor.show_labels, "Show Labels");
        ui.checkbox(&mut editor.show_values, "Show Values");
        ui.checkbox(&mut editor.hide_inactive, "Hide Inactive Indicators");

        // Sync editor state back to WindowDef data
        data.layout = editor.layout.clone();
        data.spacing = editor.spacing;
        data.hide_inactive = editor.hide_inactive;
        data.text_size = editor.text_size;
        data.icon_size = editor.icon_size;
        data.padding = editor.padding;
        data.show_labels = editor.show_labels;
        data.show_values = editor.show_values;
        data.label_color = if editor.label_color.is_empty() { None } else { Some(editor.label_color.clone()) };
        data.value_color = if editor.value_color.is_empty() { None } else { Some(editor.value_color.clone()) };
        data.grid_color = if editor.grid_color.is_empty() { None } else { Some(editor.grid_color.clone()) };
    }

    /// Render InjuryDoll widget editor controls
    fn render_injury_doll_controls_static(
        ui: &mut egui::Ui,
        editor: &mut InjuryDollEditorState,
        data: &mut InjuryDollWidgetData,
    ) {
        use egui::Grid;

        // Helper function to parse hex color to RGB float array
        fn hex_to_rgb(hex: &str) -> [f32; 3] {
            let hex = hex.trim_start_matches('#');
            if hex.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    return [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
                }
            }
            [1.0, 1.0, 1.0] // Default to white
        }

        // Helper function to convert RGB float array to hex string
        fn rgb_to_hex(rgb: [f32; 3]) -> String {
            format!(
                "#{:02x}{:02x}{:02x}",
                (rgb[0] * 255.0) as u8,
                (rgb[1] * 255.0) as u8,
                (rgb[2] * 255.0) as u8
            )
        }

        ui.heading("Image Settings");
        Grid::new("injury_doll_image")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Image path
                ui.label("Image Path:");
                ui.text_edit_singleline(&mut editor.image_path);
                ui.end_row();

                // Scale slider
                ui.label("Scale:");
                ui.add(egui::Slider::new(&mut editor.scale, 0.5..=3.0).step_by(0.1));
                ui.end_row();

                // Greyscale checkbox
                ui.label("Greyscale:");
                ui.checkbox(&mut editor.greyscale, "");
                ui.end_row();

                // Tint color with color picker
                ui.label("Tint Color:");
                let mut tint_rgb = hex_to_rgb(&editor.tint_color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut tint_rgb).changed() {
                    editor.tint_color = rgb_to_hex(tint_rgb);
                }
                ui.end_row();

                // Tint strength slider
                ui.label("Tint Strength:");
                ui.add(egui::Slider::new(&mut editor.tint_strength, 0.0..=1.0));
                ui.end_row();
            });

        ui.separator();
        ui.heading("Marker Settings");
        Grid::new("injury_doll_markers")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Marker size slider
                ui.label("Marker Size:");
                ui.add(egui::Slider::new(&mut editor.marker_size, 4.0..=30.0).suffix(" px"));
                ui.end_row();

                // Marker tint strength slider
                ui.label("Marker Tint Strength:");
                ui.add(egui::Slider::new(&mut editor.marker_tint_strength, 0.0..=1.0));
                ui.end_row();
            });

        ui.separator();
        ui.heading("Injury Colors");
        ui.label("Configure tint colors for each injury and scar rank:");
        ui.add_space(4.0);

        Grid::new("injury_doll_colors")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Wound Rank 1 color
                ui.label("Wound Rank 1:");
                let mut injury1_rgb = hex_to_rgb(&editor.injury1_color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut injury1_rgb).changed() {
                    editor.injury1_color = rgb_to_hex(injury1_rgb);
                }
                ui.end_row();

                // Wound Rank 2 color
                ui.label("Wound Rank 2:");
                let mut injury2_rgb = hex_to_rgb(&editor.injury2_color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut injury2_rgb).changed() {
                    editor.injury2_color = rgb_to_hex(injury2_rgb);
                }
                ui.end_row();

                // Wound Rank 3 color
                ui.label("Wound Rank 3:");
                let mut injury3_rgb = hex_to_rgb(&editor.injury3_color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut injury3_rgb).changed() {
                    editor.injury3_color = rgb_to_hex(injury3_rgb);
                }
                ui.end_row();

                // Scar Rank 1 color
                ui.label("Scar Rank 1:");
                let mut scar1_rgb = hex_to_rgb(&editor.scar1_color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut scar1_rgb).changed() {
                    editor.scar1_color = rgb_to_hex(scar1_rgb);
                }
                ui.end_row();

                // Scar Rank 2 color
                ui.label("Scar Rank 2:");
                let mut scar2_rgb = hex_to_rgb(&editor.scar2_color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut scar2_rgb).changed() {
                    editor.scar2_color = rgb_to_hex(scar2_rgb);
                }
                ui.end_row();

                // Scar Rank 3 color
                ui.label("Scar Rank 3:");
                let mut scar3_rgb = hex_to_rgb(&editor.scar3_color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut scar3_rgb).changed() {
                    editor.scar3_color = rgb_to_hex(scar3_rgb);
                }
                ui.end_row();

                // Background color with clear button
                ui.label("Background Color:");
                ui.horizontal(|ui| {
                    let mut bg_rgb = hex_to_rgb(&editor.background_color);
                    if egui::color_picker::color_edit_button_rgb(ui, &mut bg_rgb).changed() {
                        editor.background_color = rgb_to_hex(bg_rgb);
                    }

                    if ui.button("× Clear").clicked() {
                        editor.background_color = String::new();  // Clear to empty
                    }
                });
                ui.end_row();
            });

        ui.separator();
        ui.heading("Nerve Damage Tints");

        egui::Grid::new("nerve_tints_grid")
            .num_columns(2)
            .show(ui, |ui| {
                // Nerve Severity 1 tint (yellow)
                ui.label("Nerve Rank 1 (Yellow):");
                ui.horizontal(|ui| {
                    let mut nerve1_rgb = hex_to_rgb(&editor.nerve_tint1_color);
                    if egui::color_picker::color_edit_button_rgb(ui, &mut nerve1_rgb).changed() {
                        editor.nerve_tint1_color = rgb_to_hex(nerve1_rgb);
                    }
                    if ui.button("Reset").clicked() {
                        editor.nerve_tint1_color = "#FFFF00".to_string();
                    }
                });
                ui.end_row();

                // Nerve Severity 2 tint (orange)
                ui.label("Nerve Rank 2 (Orange):");
                ui.horizontal(|ui| {
                    let mut nerve2_rgb = hex_to_rgb(&editor.nerve_tint2_color);
                    if egui::color_picker::color_edit_button_rgb(ui, &mut nerve2_rgb).changed() {
                        editor.nerve_tint2_color = rgb_to_hex(nerve2_rgb);
                    }
                    if ui.button("Reset").clicked() {
                        editor.nerve_tint2_color = "#FFA500".to_string();
                    }
                });
                ui.end_row();

                // Nerve Severity 3 tint (red)
                ui.label("Nerve Rank 3 (Red):");
                ui.horizontal(|ui| {
                    let mut nerve3_rgb = hex_to_rgb(&editor.nerve_tint3_color);
                    if egui::color_picker::color_edit_button_rgb(ui, &mut nerve3_rgb).changed() {
                        editor.nerve_tint3_color = rgb_to_hex(nerve3_rgb);
                    }
                    if ui.button("Reset").clicked() {
                        editor.nerve_tint3_color = "#FF0000".to_string();
                    }
                });
                ui.end_row();
            });

        ui.separator();
        ui.heading("Body Part Calibration");

        if !editor.calibration_active {
            // Show start calibration button when not calibrating
            ui.label("Click 'Start Calibration' to position body part markers on your character image.");
            if ui.button("Start Calibration").clicked() {
                editor.calibration_active = true;
                editor.calibration_index = 0;
            }
        } else {
            // Show calibration UI when active
            let current_part = Self::get_body_part_name(editor.calibration_index);
            let progress_text = format!("Calibrating: {} ({}/16)", current_part, editor.calibration_index + 1);

            ui.label(&progress_text);
            ui.label("Click on the injury doll image to set the position for this body part.");

            ui.horizontal(|ui| {
                // Previous button (disabled on first item)
                if ui.add_enabled(editor.calibration_index > 0, egui::Button::new("◀ Previous")).clicked() {
                    if editor.calibration_index > 0 {
                        editor.calibration_index -= 1;
                    }
                }

                // Next button (disabled on last item)
                if ui.add_enabled(editor.calibration_index < 15, egui::Button::new("Next ▶")).clicked() {
                    if editor.calibration_index < 15 {
                        editor.calibration_index += 1;
                    }
                }

                // Finish button (only on last item)
                if editor.calibration_index == 15 {
                    if ui.button("✓ Finish").clicked() {
                        editor.calibration_active = false;
                        editor.calibration_index = 0;
                    }
                }

                // Cancel button (always available)
                if ui.button("✗ Cancel").clicked() {
                    editor.calibration_active = false;
                    editor.calibration_index = 0;
                }
            });
        }

        // Sync editor state back to WindowDef data
        data.image_path = if editor.image_path.is_empty() { None } else { Some(editor.image_path.clone()) };
        data.scale = editor.scale;
        data.greyscale = editor.greyscale;
        data.tint_color = if editor.tint_color.is_empty() { None } else { Some(editor.tint_color.clone()) };
        data.tint_strength = editor.tint_strength;
        data.marker_tint_strength = editor.marker_tint_strength;

        data.marker_size = editor.marker_size;

        // Sync injury colors
        data.injury1_color = if editor.injury1_color.is_empty() { None } else { Some(editor.injury1_color.clone()) };
        data.injury2_color = if editor.injury2_color.is_empty() { None } else { Some(editor.injury2_color.clone()) };
        data.injury3_color = if editor.injury3_color.is_empty() { None } else { Some(editor.injury3_color.clone()) };
        data.scar1_color = if editor.scar1_color.is_empty() { None } else { Some(editor.scar1_color.clone()) };
        data.scar2_color = if editor.scar2_color.is_empty() { None } else { Some(editor.scar2_color.clone()) };
        data.scar3_color = if editor.scar3_color.is_empty() { None } else { Some(editor.scar3_color.clone()) };
        data.background_color = if editor.background_color.is_empty() { None } else { Some(editor.background_color.clone()) };

        // Sync nerve tints from editor to config
        data.rank_indicators.nerve_tint1_color = editor.nerve_tint1_color.clone();
        data.rank_indicators.nerve_tint2_color = editor.nerve_tint2_color.clone();
        data.rank_indicators.nerve_tint3_color = editor.nerve_tint3_color.clone();
    }

    /// Render a color field with color picker preview and clear button
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `color_hex` - Mutable reference to the hex color string (e.g., "#FF0000" or empty)
    /// * `default_preview` - Default color to show in preview when empty (e.g., "GRAY" for no color)
    ///
    /// # Returns
    /// true if the color was changed
    fn render_color_field(ui: &mut egui::Ui, color_hex: &mut String, default_preview: &str) -> bool {
        let mut changed = false;

        // Helper to parse hex to RGB floats
        fn hex_to_rgb(hex: &str) -> Option<[f32; 3]> {
            let hex = hex.trim_start_matches('#');
            if hex.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    return Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]);
                }
            } else if hex.len() == 3 {
                // Short format #RGB
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..1], 16),
                    u8::from_str_radix(&hex[1..2], 16),
                    u8::from_str_radix(&hex[2..3], 16),
                ) {
                    return Some([r as f32 * 17.0 / 255.0, g as f32 * 17.0 / 255.0, b as f32 * 17.0 / 255.0]);
                }
            }
            None
        }

        // Helper to convert RGB floats to hex
        fn rgb_to_hex(rgb: [f32; 3]) -> String {
            format!(
                "#{:02X}{:02X}{:02X}",
                (rgb[0] * 255.0) as u8,
                (rgb[1] * 255.0) as u8,
                (rgb[2] * 255.0) as u8
            )
        }

        ui.horizontal(|ui| {
            // Get current color or default for preview
            let current_rgb = if color_hex.is_empty() {
                hex_to_rgb(default_preview).unwrap_or([0.5, 0.5, 0.5])
            } else {
                hex_to_rgb(color_hex).unwrap_or([0.5, 0.5, 0.5])
            };

            // Color picker button with preview
            let mut rgb = current_rgb;
            let color_response = egui::color_picker::color_edit_button_rgb(ui, &mut rgb);
            if color_response.changed() {
                *color_hex = rgb_to_hex(rgb);
                changed = true;
            }

            // Text edit for hex value (smaller width)
            let text_edit = egui::TextEdit::singleline(color_hex)
                .desired_width(80.0)
                .hint_text("#RRGGBB");
            if ui.add(text_edit).changed() {
                changed = true;
            }

            // Clear button (only show if there's a value)
            if !color_hex.is_empty() {
                if ui.small_button("✕").on_hover_text("Clear color").clicked() {
                    color_hex.clear();
                    changed = true;
                }
            }
        });

        changed
    }

}
