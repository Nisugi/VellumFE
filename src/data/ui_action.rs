//! The typed UI-action protocol between core and the frontends.
//!
//! Core's command handling used to communicate frontend work via magic
//! `"action:*"` strings, hand-matched in every frontend — which is how
//! four commands silently died (emitter and consumer lists drifted; see
//! .claude/work-units/dot-command-parity-audit.md). [`UiAction`] replaces
//! that: core returns [`CommandOutcome::Ui`] and every frontend matches
//! the enum EXHAUSTIVELY, so an unhandled action is a compile error and
//! "not supported in this frontend" is always an explicit, user-visible
//! choice.
//!
//! The string forms remain as the wire/menu format: popup-menu items
//! (`PopupMenu::command`) carry strings, so [`UiAction::parse`] is the
//! single bridge from menu picks back into the typed world, and
//! [`std::fmt::Display`] writes the canonical string for menu builders.
//! Both directions round-trip (tested below); add new actions in BOTH
//! `parse` and `Display` — the round-trip test catches a miss.

/// A shell zone named by the zone dot-commands. Deliberately NOT the
/// GUI's `GuiShellZone` (data may not import from frontends); the GUI
/// maps this onto its own type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellZoneTarget {
    Header,
    Footer,
    LeftBar,
    RightBar,
}

impl ShellZoneTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            ShellZoneTarget::Header => "header",
            ShellZoneTarget::Footer => "footer",
            ShellZoneTarget::LeftBar => "leftbar",
            ShellZoneTarget::RightBar => "rightbar",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "header" => Some(ShellZoneTarget::Header),
            "footer" => Some(ShellZoneTarget::Footer),
            "leftbar" => Some(ShellZoneTarget::LeftBar),
            "rightbar" => Some(ShellZoneTarget::RightBar),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneOp {
    Toggle,
    On,
    Off,
}

impl ZoneOp {
    pub fn as_str(self) -> &'static str {
        match self {
            ZoneOp::Toggle => "toggle",
            ZoneOp::On => "on",
            ZoneOp::Off => "off",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "toggle" => Some(ZoneOp::Toggle),
            "on" => Some(ZoneOp::On),
            "off" => Some(ZoneOp::Off),
            _ => None,
        }
    }
}

/// Everything core can ask a frontend to do. One variant per distinct
/// user-visible behavior; payloads are the names/ids the behavior needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiAction {
    // Editors and browsers
    Settings,
    Highlights,
    AddHighlight,
    /// None = open the browser to pick; Some = edit that highlight.
    EditHighlight(Option<String>),
    Keybinds,
    AddKeybind,
    MenuKeybinds,
    Controller,
    Hotbars,
    Streams,
    Colors,
    AddColor,
    UiColors,
    SpellColors,
    AddSpellColor,
    Themes,
    SetTheme(String),
    EditTheme,
    SorterEdit,

    // Skins (GUI graphics)
    Skins,
    SetSkin(String),
    MakeSkin(String),
    ReloadSkin,

    // Terminal palette (TUI)
    SetPalette,
    ResetPalette,

    // Tab navigation
    NextTab,
    PrevTab,
    NextUnread,

    // Window management
    AddWindowPicker,
    /// None = open the picker; Some = edit that window.
    EditWindow(Option<String>),
    /// None = open the picker; Some = hide that window.
    HideWindow(Option<String>),
    ShowWindow(String),
    /// Open the window editor pre-set to create this widget type.
    CreateWindow(String),
    /// List windows to the main window (`.windows`).
    WindowList,
    CustomWindows,
    KnownWindows,

    // Stream routing menu entries (TUI `.streams` menu)
    StreamActions(String),
    StreamPickWindow(String),
    StreamRoute { kind: String, stream: String },
    StreamSubscribe { window: String, stream: String },
    StreamNewWindow(String),

    // TOML (TUI) layout load, from the Layouts menu
    LoadLayoutToml(String),

    // Layout & pack capability hooks (zone-free plan D3): core owns the
    // COMMANDS (.savelayout/.loadlayout/.layouts/.resize/.saveskin/
    // .uiexport/.uiimport), each frontend owns its persistence model —
    // TOML cell layouts in the TUI, window snapshots in the GUI.
    SaveLayout(Option<String>),
    LoadLayout(Option<String>),
    ListLayouts,
    ResizeLayout,
    SaveSkin(String),
    UiExport(Vec<String>),
    UiImport(Vec<String>),

    // GUI shell zones
    Zone { zone: ShellZoneTarget, op: ZoneOp },

    // Lich WebUI bridge (GUI)
    WebUiPicker,
    WebUiOff,
    WebUiOpen(String),

    // Diagnostics
    SnapDebug,
    /// Write a `.performance dump` report; each frontend appends its own
    /// sections (the GUI adds graphics-toolkit internals) before writing.
    PerformanceDump,
}

impl UiAction {
    /// Parse the wire/menu string form. Returns None for anything that is
    /// not a recognized `action:*` string — callers should surface that
    /// as an "unknown action" to the user, not ignore it.
    pub fn parse(s: &str) -> Option<UiAction> {
        let body = s.strip_prefix("action:")?;
        // Prefixed (payload-carrying) forms first.
        if let Some(name) = body.strip_prefix("edithighlight:") {
            return Some(UiAction::EditHighlight(Some(name.to_string())));
        }
        if let Some(name) = body.strip_prefix("settheme:") {
            return Some(UiAction::SetTheme(name.to_string()));
        }
        if let Some(name) = body.strip_prefix("setskin:") {
            return Some(UiAction::SetSkin(name.to_string()));
        }
        if let Some(name) = body.strip_prefix("makeskin:") {
            return Some(UiAction::MakeSkin(name.to_string()));
        }
        if let Some(name) = body.strip_prefix("editwindow:") {
            return Some(UiAction::EditWindow(Some(name.to_string())));
        }
        if let Some(name) = body.strip_prefix("hidewindow:") {
            return Some(UiAction::HideWindow(Some(name.to_string())));
        }
        if let Some(name) = body.strip_prefix("showwindow:") {
            return Some(UiAction::ShowWindow(name.to_string()));
        }
        if let Some(widget_type) = body.strip_prefix("createwindow:") {
            return Some(UiAction::CreateWindow(widget_type.to_string()));
        }
        if let Some(stream) = body.strip_prefix("streamacts:") {
            return Some(UiAction::StreamActions(stream.to_string()));
        }
        if let Some(stream) = body.strip_prefix("streamwin:") {
            return Some(UiAction::StreamPickWindow(stream.to_string()));
        }
        if let Some(rest) = body.strip_prefix("streamroute:") {
            let (kind, stream) = rest.split_once(':')?;
            return Some(UiAction::StreamRoute {
                kind: kind.to_string(),
                stream: stream.to_string(),
            });
        }
        if let Some(rest) = body.strip_prefix("streamsub:") {
            let (window, stream) = rest.split_once(':')?;
            return Some(UiAction::StreamSubscribe {
                window: window.to_string(),
                stream: stream.to_string(),
            });
        }
        if let Some(stream) = body.strip_prefix("streamnew:") {
            return Some(UiAction::StreamNewWindow(stream.to_string()));
        }
        if let Some(name) = body.strip_prefix("loadlayout:") {
            return Some(UiAction::LoadLayoutToml(name.to_string()));
        }
        if let Some(name) = body.strip_prefix("layout:save:") {
            return Some(UiAction::SaveLayout(Some(name.to_string())));
        }
        if let Some(name) = body.strip_prefix("layout:load:") {
            return Some(UiAction::LoadLayout(Some(name.to_string())));
        }
        if let Some(name) = body.strip_prefix("saveskin:") {
            return Some(UiAction::SaveSkin(name.to_string()));
        }
        if let Some(rest) = body.strip_prefix("uiexport:") {
            return Some(UiAction::UiExport(
                rest.split_whitespace().map(str::to_string).collect(),
            ));
        }
        if let Some(rest) = body.strip_prefix("uiimport:") {
            return Some(UiAction::UiImport(
                rest.split_whitespace().map(str::to_string).collect(),
            ));
        }
        if let Some(rest) = body.strip_prefix("zone:") {
            let (zone, op) = rest.split_once(':')?;
            return Some(UiAction::Zone {
                zone: ShellZoneTarget::parse(zone)?,
                op: ZoneOp::parse(op)?,
            });
        }
        if let Some(page) = body.strip_prefix("webui:open:") {
            return Some(UiAction::WebUiOpen(page.to_string()));
        }
        Some(match body {
            "settings" => UiAction::Settings,
            "highlights" => UiAction::Highlights,
            "addhighlight" => UiAction::AddHighlight,
            "edithighlight" => UiAction::EditHighlight(None),
            "keybinds" => UiAction::Keybinds,
            "addkeybind" => UiAction::AddKeybind,
            "menukeybinds" => UiAction::MenuKeybinds,
            "controller" => UiAction::Controller,
            "hotbars" => UiAction::Hotbars,
            "streams" => UiAction::Streams,
            "colors" => UiAction::Colors,
            "addcolor" => UiAction::AddColor,
            "uicolors" => UiAction::UiColors,
            "spellcolors" => UiAction::SpellColors,
            "addspellcolor" => UiAction::AddSpellColor,
            "themes" => UiAction::Themes,
            "edittheme" => UiAction::EditTheme,
            "sorteredit" => UiAction::SorterEdit,
            "skins" => UiAction::Skins,
            "reloadskin" => UiAction::ReloadSkin,
            "setpalette" => UiAction::SetPalette,
            "resetpalette" => UiAction::ResetPalette,
            "nexttab" => UiAction::NextTab,
            "prevtab" => UiAction::PrevTab,
            "nextunread" => UiAction::NextUnread,
            "addwindow" => UiAction::AddWindowPicker,
            "editwindow" => UiAction::EditWindow(None),
            "hidewindow" => UiAction::HideWindow(None),
            // Two historical spellings, one behavior.
            "windows" | "listwindows" => UiAction::WindowList,
            "customwindows" => UiAction::CustomWindows,
            "knownwindows" => UiAction::KnownWindows,
            "webui" => UiAction::WebUiPicker,
            "webui:off" => UiAction::WebUiOff,
            "snapdebug" => UiAction::SnapDebug,
            "performance:dump" => UiAction::PerformanceDump,
            "layout:save" => UiAction::SaveLayout(None),
            "layout:load" => UiAction::LoadLayout(None),
            "layout:list" => UiAction::ListLayouts,
            "layout:resize" => UiAction::ResizeLayout,
            "uiexport" => UiAction::UiExport(Vec::new()),
            "uiimport" => UiAction::UiImport(Vec::new()),
            _ => return None,
        })
    }
}

impl std::fmt::Display for UiAction {
    /// The canonical wire/menu string. `parse(x.to_string()) == x` for
    /// every variant (round-trip tested).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiAction::Settings => write!(f, "action:settings"),
            UiAction::Highlights => write!(f, "action:highlights"),
            UiAction::AddHighlight => write!(f, "action:addhighlight"),
            UiAction::EditHighlight(None) => write!(f, "action:edithighlight"),
            UiAction::EditHighlight(Some(name)) => write!(f, "action:edithighlight:{name}"),
            UiAction::Keybinds => write!(f, "action:keybinds"),
            UiAction::AddKeybind => write!(f, "action:addkeybind"),
            UiAction::MenuKeybinds => write!(f, "action:menukeybinds"),
            UiAction::Controller => write!(f, "action:controller"),
            UiAction::Hotbars => write!(f, "action:hotbars"),
            UiAction::Streams => write!(f, "action:streams"),
            UiAction::Colors => write!(f, "action:colors"),
            UiAction::AddColor => write!(f, "action:addcolor"),
            UiAction::UiColors => write!(f, "action:uicolors"),
            UiAction::SpellColors => write!(f, "action:spellcolors"),
            UiAction::AddSpellColor => write!(f, "action:addspellcolor"),
            UiAction::Themes => write!(f, "action:themes"),
            UiAction::SetTheme(name) => write!(f, "action:settheme:{name}"),
            UiAction::EditTheme => write!(f, "action:edittheme"),
            UiAction::SorterEdit => write!(f, "action:sorteredit"),
            UiAction::Skins => write!(f, "action:skins"),
            UiAction::SetSkin(name) => write!(f, "action:setskin:{name}"),
            UiAction::MakeSkin(name) => write!(f, "action:makeskin:{name}"),
            UiAction::ReloadSkin => write!(f, "action:reloadskin"),
            UiAction::SetPalette => write!(f, "action:setpalette"),
            UiAction::ResetPalette => write!(f, "action:resetpalette"),
            UiAction::NextTab => write!(f, "action:nexttab"),
            UiAction::PrevTab => write!(f, "action:prevtab"),
            UiAction::NextUnread => write!(f, "action:nextunread"),
            UiAction::AddWindowPicker => write!(f, "action:addwindow"),
            UiAction::EditWindow(None) => write!(f, "action:editwindow"),
            UiAction::EditWindow(Some(name)) => write!(f, "action:editwindow:{name}"),
            UiAction::HideWindow(None) => write!(f, "action:hidewindow"),
            UiAction::HideWindow(Some(name)) => write!(f, "action:hidewindow:{name}"),
            UiAction::ShowWindow(name) => write!(f, "action:showwindow:{name}"),
            UiAction::CreateWindow(widget_type) => write!(f, "action:createwindow:{widget_type}"),
            UiAction::WindowList => write!(f, "action:windows"),
            UiAction::CustomWindows => write!(f, "action:customwindows"),
            UiAction::KnownWindows => write!(f, "action:knownwindows"),
            UiAction::StreamActions(stream) => write!(f, "action:streamacts:{stream}"),
            UiAction::StreamPickWindow(stream) => write!(f, "action:streamwin:{stream}"),
            UiAction::StreamRoute { kind, stream } => {
                write!(f, "action:streamroute:{kind}:{stream}")
            }
            UiAction::StreamSubscribe { window, stream } => {
                write!(f, "action:streamsub:{window}:{stream}")
            }
            UiAction::StreamNewWindow(stream) => write!(f, "action:streamnew:{stream}"),
            UiAction::LoadLayoutToml(name) => write!(f, "action:loadlayout:{name}"),
            UiAction::SaveLayout(None) => write!(f, "action:layout:save"),
            UiAction::SaveLayout(Some(name)) => write!(f, "action:layout:save:{name}"),
            UiAction::LoadLayout(None) => write!(f, "action:layout:load"),
            UiAction::LoadLayout(Some(name)) => write!(f, "action:layout:load:{name}"),
            UiAction::ListLayouts => write!(f, "action:layout:list"),
            UiAction::ResizeLayout => write!(f, "action:layout:resize"),
            UiAction::SaveSkin(name) => write!(f, "action:saveskin:{name}"),
            UiAction::UiExport(args) if args.is_empty() => write!(f, "action:uiexport"),
            UiAction::UiExport(args) => write!(f, "action:uiexport:{}", args.join(" ")),
            UiAction::UiImport(args) if args.is_empty() => write!(f, "action:uiimport"),
            UiAction::UiImport(args) => write!(f, "action:uiimport:{}", args.join(" ")),
            UiAction::Zone { zone, op } => {
                write!(f, "action:zone:{}:{}", zone.as_str(), op.as_str())
            }
            UiAction::WebUiPicker => write!(f, "action:webui"),
            UiAction::WebUiOff => write!(f, "action:webui:off"),
            UiAction::WebUiOpen(page) => write!(f, "action:webui:open:{page}"),
            UiAction::SnapDebug => write!(f, "action:snapdebug"),
            UiAction::PerformanceDump => write!(f, "action:performance:dump"),
        }
    }
}

/// What `AppCore::send_command` did with a submitted command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandOutcome {
    /// Text for the network layer to transmit to the game.
    Game(String),
    /// Fully handled inside core; nothing further to do.
    Handled,
    /// The frontend must perform this action.
    Ui(UiAction),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One instance of every variant. A new variant that isn't added here
    /// still fails the round-trip test below via non_exhaustive match…
    /// which we can't have on a local enum — so keep this list complete;
    /// the compiler's exhaustiveness check on `Display` is the real net.
    fn every_variant() -> Vec<UiAction> {
        vec![
            UiAction::Settings,
            UiAction::Highlights,
            UiAction::AddHighlight,
            UiAction::EditHighlight(None),
            UiAction::EditHighlight(Some("bandits".into())),
            UiAction::Keybinds,
            UiAction::AddKeybind,
            UiAction::MenuKeybinds,
            UiAction::Controller,
            UiAction::Hotbars,
            UiAction::Streams,
            UiAction::Colors,
            UiAction::AddColor,
            UiAction::UiColors,
            UiAction::SpellColors,
            UiAction::AddSpellColor,
            UiAction::Themes,
            UiAction::SetTheme("gruvbox".into()),
            UiAction::EditTheme,
            UiAction::SorterEdit,
            UiAction::Skins,
            UiAction::SetSkin("wrayth".into()),
            UiAction::MakeSkin("mine".into()),
            UiAction::ReloadSkin,
            UiAction::SetPalette,
            UiAction::ResetPalette,
            UiAction::NextTab,
            UiAction::PrevTab,
            UiAction::NextUnread,
            UiAction::AddWindowPicker,
            UiAction::EditWindow(None),
            UiAction::EditWindow(Some("main".into())),
            UiAction::HideWindow(None),
            UiAction::HideWindow(Some("thoughts".into())),
            UiAction::ShowWindow("compass".into()),
            UiAction::CreateWindow("text".into()),
            UiAction::WindowList,
            UiAction::CustomWindows,
            UiAction::KnownWindows,
            UiAction::StreamActions("speech".into()),
            UiAction::StreamPickWindow("speech".into()),
            UiAction::StreamRoute {
                kind: "main".into(),
                stream: "speech".into(),
            },
            UiAction::StreamSubscribe {
                window: "thoughts".into(),
                stream: "speech".into(),
            },
            UiAction::StreamNewWindow("speech".into()),
            UiAction::LoadLayoutToml("combat".into()),
            UiAction::SaveLayout(None),
            UiAction::SaveLayout(Some("combat".into())),
            UiAction::LoadLayout(None),
            UiAction::LoadLayout(Some("combat".into())),
            UiAction::ListLayouts,
            UiAction::ResizeLayout,
            UiAction::SaveSkin("mine".into()),
            UiAction::UiExport(Vec::new()),
            UiAction::UiExport(vec!["mypack".into(), "highlights".into()]),
            UiAction::UiImport(vec!["mypack".into(), "apply".into()]),
            UiAction::Zone {
                zone: ShellZoneTarget::LeftBar,
                op: ZoneOp::Toggle,
            },
            UiAction::WebUiPicker,
            UiAction::WebUiOff,
            UiAction::WebUiOpen("bigshot".into()),
            UiAction::SnapDebug,
            UiAction::PerformanceDump,
        ]
    }

    #[test]
    fn every_variant_round_trips_through_the_string_form() {
        for action in every_variant() {
            let wire = action.to_string();
            assert_eq!(
                UiAction::parse(&wire),
                Some(action.clone()),
                "round-trip failed for {wire}"
            );
        }
    }

    #[test]
    fn legacy_spellings_and_junk() {
        // Both historical window-list spellings parse to one variant.
        assert_eq!(UiAction::parse("action:windows"), Some(UiAction::WindowList));
        assert_eq!(
            UiAction::parse("action:listwindows"),
            Some(UiAction::WindowList)
        );
        // Non-actions and unknown actions are None, not a panic.
        assert_eq!(UiAction::parse(".help"), None);
        assert_eq!(UiAction::parse("menu:windows"), None);
        assert_eq!(UiAction::parse("action:definitely-not-a-thing"), None);
        // Malformed payload forms.
        assert_eq!(UiAction::parse("action:zone:header:sideways"), None);
        assert_eq!(UiAction::parse("action:zone:attic:on"), None);
        assert_eq!(UiAction::parse("action:streamroute:no-colon"), None);
    }
}
