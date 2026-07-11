//! `WindowDef` - the tagged enum tying widget types to their config data.
//!
//! Each variant pairs `WindowBase` geometry with a widget-specific data
//! struct from `widgets.rs`; serialized into layout.toml with
//! `widget_type` as the serde tag.

use super::*;


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
