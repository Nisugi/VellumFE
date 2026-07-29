//! `WindowDef` - the tagged enum tying widget types to their config data.
//!
//! Each variant pairs `WindowBase` geometry with a widget-specific data
//! struct from `widgets.rs`; serialized into layout.toml with
//! `widget_type` as the serde tag.

use super::*;

/// Default text buffer size for blank windows created via `.addwindow`.
/// Mirrors `default_buffer_size()` in config.rs (the serde default for
/// `TextWidgetData::buffer_size`).
const DEFAULT_BUFFER_SIZE: usize = 10_000;

/// Window definition - enum with widget-specific variants
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "widget_type")]
pub enum WindowDef {
    #[serde(rename = "text")]
    Text {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: TextWidgetData,
    },

    #[serde(rename = "tabbedtext")]
    TabbedText {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: TabbedTextWidgetData,
    },

    #[serde(rename = "room")]
    Room {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: RoomWidgetData,
    },

    #[serde(rename = "inventory")]
    Inventory {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: InventoryWidgetData,
    },

    #[serde(rename = "reserve")]
    Reserve {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: InventoryWidgetData,
    },

    #[serde(rename = "command_input")]
    CommandInput {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: CommandInputWidgetData,
    },

    #[serde(rename = "progress")]
    Progress {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: ProgressWidgetData,
    },

    #[serde(rename = "countdown")]
    Countdown {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: CountdownWidgetData,
    },

    #[serde(rename = "compass")]
    Compass {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: CompassWidgetData,
    },

    #[serde(rename = "map")]
    Map {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: MapWidgetData,
    },

    #[serde(rename = "injury_doll")]
    InjuryDoll {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: InjuryDollWidgetData,
    },

    #[serde(rename = "indicator")]
    Indicator {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: IndicatorWidgetData,
    },

    #[serde(rename = "dashboard")]
    Dashboard {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: DashboardWidgetData,
    },

    #[serde(rename = "hand")]
    Hand {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: HandWidgetData,
    },

    #[serde(rename = "active_effects")]
    ActiveEffects {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: ActiveEffectsWidgetData,
    },
    #[serde(rename = "performance")]
    Performance {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: PerformanceWidgetData,
    },

    #[serde(rename = "targets")]
    Targets {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: TargetsWidgetData,
    },

    #[serde(rename = "players")]
    Players {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: PlayersWidgetData,
    },

    #[serde(rename = "items")]
    Items {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: ItemsWidgetData,
    },

    /// Container window for displaying contents of bags, backpacks, etc.
    #[serde(rename = "container")]
    Container {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: ContainerWidgetData,
    },

    /// Resident dialog panel (combat, befriend, ...) rendered from the
    /// accumulated dialog store by id.
    #[serde(rename = "dialogpanel")]
    DialogPanel {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: DialogPanelWidgetData,
    },

    #[serde(rename = "spacer")]
    Spacer {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: SpacerWidgetData,
    },

    #[serde(rename = "quickbar")]
    Quickbar {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: QuickbarWidgetData,
    },

    /// Hotkey bar (buttons bound to game commands with condition-driven
    /// states; definitions live in hotbars.toml, referenced by name)
    #[serde(rename = "hotkeybar")]
    Hotkeybar {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: HotkeybarWidgetData,
    },

    #[serde(rename = "spells")]
    Spells {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: SpellsWidgetData,
    },

    #[serde(rename = "perception")]
    Perception {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: PerceptionWidgetData,
    },

    /// DragonRealms experience window (shows skill training status)
    #[serde(rename = "experience")]
    Experience {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: ExperienceWidgetData,
    },

    /// GS4 Experience window (shows level, mind state, experience)
    #[serde(rename = "gs4_experience")]
    GS4Experience {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: GS4ExperienceWidgetData,
    },

    /// Encumbrance window (shows progress bar + optional label)
    #[serde(rename = "encum")]
    Encumbrance {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: EncumbranceWidgetData,
    },

    /// MiniVitals window (horizontal 4-bar layout) - GS4 only
    #[serde(rename = "minivitals")]
    MiniVitals {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: MiniVitalsWidgetData,
    },

    /// Betrayer window (blood pool progress bar + item list) - GS4 only
    #[serde(rename = "betrayer")]
    Betrayer {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: BetrayerWidgetData,
    },

    /// Lich WebUI panel (native rendering of a script's registered page)
    #[serde(rename = "webui")]
    WebUi {
        #[serde(flatten)]
        base: WindowBase,
        #[serde(flatten)]
        data: WebUiWidgetData,
    },
}

impl WindowDef {
    /// Build a blank `WindowDef` of the given widget type with default data.
    ///
    /// Used by `.addwindow` to persist a newly-created window to layout.toml.
    /// Previously the caller only handled text/room/command_input/webui and
    /// defaulted every other type to `WindowDef::Text`, so a progress /
    /// countdown / compass / indicator / hand window came back as an empty text
    /// box after reload — and, because the resize categorizer keys off the
    /// serialized `widget_type` tag, it was also placed in the wrong scaling
    /// bucket. This maps every supported type to its real variant.
    ///
    /// The per-widget `data` values mirror the serde `default` for each field
    /// so an `.addwindow` widget behaves identically to one authored by hand in
    /// the TOML. Volatile fields (progress value, countdown end_time, room
    /// contents, …) are re-derived from live game state on load regardless.
    ///
    /// Returns `None` for an unrecognized type string.
    pub fn blank(widget_type_str: &str, base: WindowBase) -> Option<WindowDef> {
        let def = match widget_type_str.to_lowercase().as_str() {
            "text" => WindowDef::Text {
                base,
                data: TextWidgetData {
                    streams: vec![],
                    buffer_size: DEFAULT_BUFFER_SIZE,
                    wordwrap: true,
                    show_timestamps: false,
                    timestamp_position: None,
                    compact: false,
                },
            },
            "tabbedtext" => WindowDef::TabbedText {
                base,
                data: TabbedTextWidgetData {
                    tabs: vec![],
                    buffer_size: DEFAULT_BUFFER_SIZE,
                    tab_bar_position: "top".to_string(),
                    tab_separator: false,
                    tab_active_color: None,
                    tab_inactive_color: None,
                    tab_unread_color: None,
                    tab_unread_prefix: None,
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
            "inventory" => WindowDef::Inventory {
                base,
                data: InventoryWidgetData {
                    streams: vec!["inv".to_string()],
                    buffer_size: 0,
                    wordwrap: true,
                    show_timestamps: false,
                },
            },
            "reserve" => WindowDef::Reserve {
                base,
                data: InventoryWidgetData {
                    streams: vec!["reserve".to_string()],
                    buffer_size: 0,
                    wordwrap: true,
                    show_timestamps: false,
                },
            },
            "command_input" | "commandinput" => WindowDef::CommandInput {
                base,
                data: CommandInputWidgetData::default(),
            },
            "progress" => WindowDef::Progress {
                base,
                data: ProgressWidgetData {
                    id: None,
                    label: None,
                    color: None,
                    numbers_only: false,
                    current_only: false,
                },
            },
            "countdown" => WindowDef::Countdown {
                base,
                data: CountdownWidgetData {
                    id: None,
                    label: None,
                    icon: None,
                    color: None,
                    background_color: None,
                },
            },
            "compass" => WindowDef::Compass {
                base,
                data: CompassWidgetData {
                    active_color: None,
                    inactive_color: None,
                },
            },
            "map" => WindowDef::Map {
                base,
                data: MapWidgetData::default(),
            },
            "injury_doll" | "injuries" => WindowDef::InjuryDoll {
                base,
                data: InjuryDollWidgetData {
                    injury_default_color: None,
                    injury1_color: None,
                    injury2_color: None,
                    injury3_color: None,
                    scar1_color: None,
                    scar2_color: None,
                    scar3_color: None,
                },
            },
            "indicator" => WindowDef::Indicator {
                base,
                data: IndicatorWidgetData {
                    icon: None,
                    indicator_id: None,
                    inactive_color: Some("#555555".to_string()),
                    active_color: Some("#00ff00".to_string()),
                    default_status: None,
                    default_color: None,
                },
            },
            "dashboard" => WindowDef::Dashboard {
                base,
                data: DashboardWidgetData {
                    layout: "horizontal".to_string(),
                    spacing: 1,
                    hide_inactive: false,
                    indicators: vec![],
                },
            },
            "hand" => WindowDef::Hand {
                base,
                data: HandWidgetData {
                    icon: None,
                    icon_color: None,
                    text_color: None,
                },
            },
            "active_effects" => WindowDef::ActiveEffects {
                base,
                data: ActiveEffectsWidgetData {
                    category: "Buffs".to_string(),
                },
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
            "targets" => WindowDef::Targets {
                base,
                data: TargetsWidgetData {
                    entity_id: "targetcount".to_string(),
                    show_body_part_count: false,
                    status_position: None,
                },
            },
            "players" => WindowDef::Players {
                base,
                data: PlayersWidgetData {
                    entity_id: "playercount".to_string(),
                },
            },
            "items" => WindowDef::Items {
                base,
                data: ItemsWidgetData {
                    entity_id: "items".to_string(),
                },
            },
            "container" => WindowDef::Container {
                base,
                data: ContainerWidgetData {
                    container_title: String::new(),
                },
            },
            "spacer" => WindowDef::Spacer {
                base,
                data: SpacerWidgetData {},
            },
            "quickbar" => WindowDef::Quickbar {
                base,
                data: QuickbarWidgetData {},
            },
            "hotkeybar" => {
                // A dot-command-created hotkeybar binds to the bar with the same
                // name as the window (matches the live-content path in add_window).
                let bar = base.name.clone();
                WindowDef::Hotkeybar {
                    base,
                    data: HotkeybarWidgetData {
                        bar,
                        orientation: "horizontal".to_string(),
                    },
                }
            }
            "spells" => WindowDef::Spells {
                base,
                data: SpellsWidgetData {},
            },
            "perception" => WindowDef::Perception {
                base,
                data: PerceptionWidgetData {
                    stream: "percWindow".to_string(),
                    buffer_size: 100,
                    sort_direction: SortDirection::default(),
                    text_replacements: vec![],
                    use_short_spell_names: false,
                },
            },
            "experience" => WindowDef::Experience {
                base,
                data: ExperienceWidgetData::default(),
            },
            "gs4_experience" => WindowDef::GS4Experience {
                base,
                data: GS4ExperienceWidgetData::default(),
            },
            "encum" => WindowDef::Encumbrance {
                base,
                data: EncumbranceWidgetData::default(),
            },
            "minivitals" => WindowDef::MiniVitals {
                base,
                data: MiniVitalsWidgetData::default(),
            },
            "betrayer" => WindowDef::Betrayer {
                base,
                data: BetrayerWidgetData::default(),
            },
            "webui" | "lichui" => WindowDef::WebUi {
                base,
                data: WebUiWidgetData::default(),
            },
            "dialogpanel" | "dialog_panel" => WindowDef::DialogPanel {
                base,
                data: DialogPanelWidgetData::default(),
            },
            _ => return None,
        };
        Some(def)
    }

    /// Get the window name
    pub fn name(&self) -> &str {
        match self {
            WindowDef::Text { base, .. } => &base.name,
            WindowDef::TabbedText { base, .. } => &base.name,
            WindowDef::Room { base, .. } => &base.name,
            WindowDef::Inventory { base, .. } => &base.name,
            WindowDef::Reserve { base, .. } => &base.name,
            WindowDef::CommandInput { base, .. } => &base.name,
            WindowDef::Progress { base, .. } => &base.name,
            WindowDef::Countdown { base, .. } => &base.name,
            WindowDef::Compass { base, .. } => &base.name,
            WindowDef::Map { base, .. } => &base.name,
            WindowDef::Indicator { base, .. } => &base.name,
            WindowDef::Dashboard { base, .. } => &base.name,
            WindowDef::InjuryDoll { base, .. } => &base.name,
            WindowDef::Hand { base, .. } => &base.name,
            WindowDef::ActiveEffects { base, .. } => &base.name,
            WindowDef::Performance { base, .. } => &base.name,
            WindowDef::Targets { base, .. } => &base.name,
            WindowDef::Players { base, .. } => &base.name,
            WindowDef::Items { base, .. } => &base.name,
            WindowDef::Container { base, .. } => &base.name,
            WindowDef::DialogPanel { base, .. } => &base.name,
            WindowDef::Spacer { base, .. } => &base.name,
            WindowDef::Quickbar { base, .. } => &base.name,
            WindowDef::Hotkeybar { base, .. } => &base.name,
            WindowDef::Spells { base, .. } => &base.name,
            WindowDef::Perception { base, .. } => &base.name,
            WindowDef::Experience { base, .. } => &base.name,
            WindowDef::GS4Experience { base, .. } => &base.name,
            WindowDef::Encumbrance { base, .. } => &base.name,
            WindowDef::MiniVitals { base, .. } => &base.name,
            WindowDef::Betrayer { base, .. } => &base.name,
            WindowDef::WebUi { base, .. } => &base.name,
        }
    }

    /// Get the widget type as a string
    pub fn widget_type(&self) -> &str {
        match self {
            WindowDef::Text { .. } => "text",
            WindowDef::TabbedText { .. } => "tabbedtext",
            WindowDef::Room { .. } => "room",
            WindowDef::Inventory { .. } => "inventory",
            WindowDef::Reserve { .. } => "reserve",
            WindowDef::CommandInput { .. } => "command_input",
            WindowDef::Progress { .. } => "progress",
            WindowDef::Countdown { .. } => "countdown",
            WindowDef::Compass { .. } => "compass",
            WindowDef::Map { .. } => "map",
            WindowDef::Indicator { .. } => "indicator",
            WindowDef::Dashboard { .. } => "dashboard",
            WindowDef::InjuryDoll { .. } => "injury_doll",
            WindowDef::Hand { .. } => "hand",
            WindowDef::ActiveEffects { .. } => "active_effects",
            WindowDef::Performance { .. } => "performance",
            WindowDef::Targets { .. } => "targets",
            WindowDef::Players { .. } => "players",
            WindowDef::Items { .. } => "items",
            WindowDef::Container { .. } => "container",
            WindowDef::DialogPanel { .. } => "dialogpanel",
            WindowDef::Spacer { .. } => "spacer",
            WindowDef::Quickbar { .. } => "quickbar",
            WindowDef::Hotkeybar { .. } => "hotkeybar",
            WindowDef::Spells { .. } => "spells",
            WindowDef::Perception { .. } => "perception",
            WindowDef::Experience { .. } => "experience",
            WindowDef::GS4Experience { .. } => "gs4_experience",
            WindowDef::Encumbrance { .. } => "encum",
            WindowDef::MiniVitals { .. } => "minivitals",
            WindowDef::Betrayer { .. } => "betrayer",
            WindowDef::WebUi { .. } => "webui",
        }
    }

    /// Get a reference to the base configuration
    pub fn base(&self) -> &WindowBase {
        match self {
            WindowDef::Text { base, .. } => base,
            WindowDef::TabbedText { base, .. } => base,
            WindowDef::Room { base, .. } => base,
            WindowDef::Inventory { base, .. } => base,
            WindowDef::Reserve { base, .. } => base,
            WindowDef::CommandInput { base, .. } => base,
            WindowDef::Progress { base, .. } => base,
            WindowDef::Countdown { base, .. } => base,
            WindowDef::Compass { base, .. } => base,
            WindowDef::Map { base, .. } => base,
            WindowDef::Indicator { base, .. } => base,
            WindowDef::Dashboard { base, .. } => base,
            WindowDef::InjuryDoll { base, .. } => base,
            WindowDef::Hand { base, .. } => base,
            WindowDef::ActiveEffects { base, .. } => base,
            WindowDef::Performance { base, .. } => base,
            WindowDef::Targets { base, .. } => base,
            WindowDef::Players { base, .. } => base,
            WindowDef::Items { base, .. } => base,
            WindowDef::Container { base, .. } => base,
            WindowDef::DialogPanel { base, .. } => base,
            WindowDef::Spacer { base, .. } => base,
            WindowDef::Quickbar { base, .. } => base,
            WindowDef::Hotkeybar { base, .. } => base,
            WindowDef::Spells { base, .. } => base,
            WindowDef::Perception { base, .. } => base,
            WindowDef::Experience { base, .. } => base,
            WindowDef::GS4Experience { base, .. } => base,
            WindowDef::Encumbrance { base, .. } => base,
            WindowDef::MiniVitals { base, .. } => base,
            WindowDef::Betrayer { base, .. } => base,
            WindowDef::WebUi { base, .. } => base,
        }
    }

    /// Get a mutable reference to the base configuration
    pub fn base_mut(&mut self) -> &mut WindowBase {
        match self {
            WindowDef::Text { base, .. } => base,
            WindowDef::TabbedText { base, .. } => base,
            WindowDef::Room { base, .. } => base,
            WindowDef::Inventory { base, .. } => base,
            WindowDef::Reserve { base, .. } => base,
            WindowDef::CommandInput { base, .. } => base,
            WindowDef::Progress { base, .. } => base,
            WindowDef::Countdown { base, .. } => base,
            WindowDef::Compass { base, .. } => base,
            WindowDef::Map { base, .. } => base,
            WindowDef::Indicator { base, .. } => base,
            WindowDef::Dashboard { base, .. } => base,
            WindowDef::InjuryDoll { base, .. } => base,
            WindowDef::Hand { base, .. } => base,
            WindowDef::ActiveEffects { base, .. } => base,
            WindowDef::Performance { base, .. } => base,
            WindowDef::Targets { base, .. } => base,
            WindowDef::Players { base, .. } => base,
            WindowDef::Items { base, .. } => base,
            WindowDef::Container { base, .. } => base,
            WindowDef::DialogPanel { base, .. } => base,
            WindowDef::Spacer { base, .. } => base,
            WindowDef::Quickbar { base, .. } => base,
            WindowDef::Hotkeybar { base, .. } => base,
            WindowDef::Spells { base, .. } => base,
            WindowDef::Perception { base, .. } => base,
            WindowDef::Experience { base, .. } => base,
            WindowDef::GS4Experience { base, .. } => base,
            WindowDef::Encumbrance { base, .. } => base,
            WindowDef::MiniVitals { base, .. } => base,
            WindowDef::Betrayer { base, .. } => base,
            WindowDef::WebUi { base, .. } => base,
        }
    }
}

#[cfg(test)]
mod blank_tests {
    use super::*;
    use crate::data::window::WidgetType;

    fn sample_base() -> WindowBase {
        WindowBase {
            name: "test".to_string(),
            row: crate::data::geometry::Row::new(0),
            col: crate::data::geometry::Col::new(0),
            rows: crate::data::geometry::Height::new(3),
            cols: crate::data::geometry::Width::new(20),
            show_border: true,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: true,
            title: Some("test".to_string()),
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
            tts_speak: false,
            text_size: None,
            font_family: None,
        }
    }

    /// Every widget type accepted by `.addwindow` (WidgetType::VALID_TYPES) must
    /// build a real WindowDef whose serialized `widget_type()` tag round-trips
    /// back to the SAME WidgetType. This pins the fix for the bug where
    /// non-text widgets were persisted as WindowDef::Text and reloaded empty.
    #[test]
    fn blank_covers_every_valid_widget_type_and_round_trips() {
        for &type_str in WidgetType::VALID_TYPES {
            let expected = WidgetType::try_from_str(type_str)
                .unwrap_or_else(|| panic!("VALID_TYPES entry '{type_str}' failed to parse"));

            let def = WindowDef::blank(type_str, sample_base()).unwrap_or_else(|| {
                panic!("WindowDef::blank returned None for valid type '{type_str}'")
            });

            // The serialized tag must map back to the same WidgetType — i.e. no
            // silent fallback to Text.
            let round_tripped = WidgetType::from_str(def.widget_type());
            assert_eq!(
                round_tripped, expected,
                "type '{type_str}' persisted as '{}' which parses to {:?}, not {:?}",
                def.widget_type(),
                round_tripped,
                expected
            );
        }
    }

    #[test]
    fn blank_returns_none_for_unknown_type() {
        assert!(WindowDef::blank("definitely_not_a_widget", sample_base()).is_none());
    }
}
