//! UI State - Focus, selection, and interaction state
//!
//! This module contains UI state that is independent of rendering.
//! Both TUI and GUI frontends read from these structures.

use super::window::WindowState;
use crate::data::LinkData;
use crate::selection::SelectionState;
use std::collections::HashMap;

/// Application UI state
#[derive(Clone, Debug)]
pub struct UiState {
    /// All windows in the application
    pub windows: HashMap<String, WindowState>,

    /// Widget type index - cached mapping of widget types to window names
    /// Rebuilt when windows are added/removed
    widget_type_index: HashMap<super::window::WidgetType, Vec<String>>,

    /// Currently focused window name
    pub focused_window: Option<String>,

    /// Current input mode
    pub input_mode: InputMode,

    /// Search input (when in Search mode)
    pub search_input: String,
    pub search_cursor: usize,

    /// Popup menu state (main menu or level 1)
    pub popup_menu: Option<PopupMenu>,

    /// Submenu (level 2) - shown when clicking category in popup_menu
    pub submenu: Option<PopupMenu>,

    /// Nested submenu (level 3) - shown when clicking subcategory in submenu
    pub nested_submenu: Option<PopupMenu>,

    /// Deep submenu (level 4) - shown when clicking item in nested_submenu
    pub deep_submenu: Option<PopupMenu>,

    /// Interact-mode focus (Some only while InputMode::Interact)
    pub interact: Option<InteractState>,

    /// Status bar text
    pub status_text: String,

    /// Mouse drag state for window resize/move
    pub mouse_drag: Option<MouseDragState>,

    /// Text selection state
    pub selection_state: Option<SelectionState>,

    /// Mouse position when drag started (for detecting drag vs click)
    pub selection_drag_start: Option<(u16, u16)>,

    /// Link drag state (Ctrl+drag from link)
    pub link_drag_state: Option<LinkDragState>,

    /// Pending link click (released without drag = send _menu)
    pub pending_link_click: Option<PendingLinkClick>,

    /// Commands queued by dialog-panel widgets (rendered from an immutable
    /// AppCore borrow) for the app loop to send after rendering. Interior
    /// mutability so the panel renderer can push without a &mut AppCore.
    pub pending_panel_commands: std::cell::RefCell<Vec<String>>,

    /// Set true after layout reload to signal frontend to reset widget caches
    pub needs_widget_reset: bool,

    /// List of specific widget names to reset (used when widget type changes)
    /// More targeted than needs_widget_reset which clears ALL caches
    pub widgets_to_reset: Vec<String>,

    /// Set of ephemeral window names (session-only, not saved to layout)
    pub ephemeral_windows: std::collections::HashSet<String>,

    /// Container titles the user has opted to show this session (U3). A
    /// sighted container auto-(re)opens only if its title is in here;
    /// showing one via the Windows list adds it, hiding removes it.
    /// Session-only — never persisted (containers wipe on relog).
    pub shown_container_titles: std::collections::HashSet<String>,

    /// Dialog ids the user has opted to show as popups (U6). A game
    /// `openDialog` becomes a live popup only if its id is in here; empty by
    /// default = nothing pops up unless shown (replacing the blocklist).
    /// Kept in sync with layout visibility by AppCore.
    pub shown_dialog_ids: std::collections::HashSet<String>,

    /// Quickbar data keyed by id (e.g., "quick", "quick-combat")
    pub quickbars: HashMap<String, crate::data::QuickbarData>,

    /// Quickbar ids in encounter order (for switcher menu)
    pub quickbar_order: Vec<String>,

    /// Currently active quickbar id
    pub active_quickbar_id: Option<String>,

    /// Active dialog popup (dynamic openDialog payloads). This is the
    /// currently-DISPLAYED dialog; its full state also lives in
    /// `dialog_store` so it can be re-shown intact.
    pub active_dialog: Option<DialogState>,

    /// Every dialog the game has described this session, keyed by id,
    /// accumulated from dialogData regardless of show/hide policy. The
    /// game sends a dialog's full definition once (typically at login)
    /// then only deltas; ingesting into this store means enabling a
    /// dialog mid-session shows it fully formed rather than from whatever
    /// deltas happened to arrive after the user opted in.
    pub dialog_store: HashMap<String, DialogState>,

    /// Latest placement/persistence attributes a window-declaring tag
    /// carried, per game id, raw (location/resident/save/width/height…) —
    /// the parser's WindowHints. Ingested always, like the dialog store,
    /// so placement can honor hints whenever the window materializes
    /// (redesign Phase 3e).
    pub window_hints: HashMap<String, Vec<(String, String)>>,

    /// Active injuries popup (viewing another player's injuries)
    pub injuries_popup: Option<InjuriesPopupState>,

    /// Dialog drag state for move/resize operations
    pub dialog_drag: Option<DialogDragState>,

    /// Pending window additions (template names to add to layout)
    /// Set by message processor when openDialog has a matching template
    pub pending_window_additions: Vec<String>,

    /// Game-window discoveries the message processor observed (streamWindow,
    /// resident dialog panels, containers) that AppCore drains against the
    /// layout — the processor can't reach the layout itself. U3's unified
    /// discovery path, replacing the window_offers registry.
    pub pending_window_discoveries: Vec<WindowDiscovery>,
}

/// A game window the client just saw announced, to be registered as a
/// bound layout/ephemeral entry by AppCore (the message processor has no
/// layout access). All discoveries register Hidden-by-default (U6:
/// hidden-until-shown is the universal rule; the old blocklist is gone).
#[derive(Clone, Debug)]
pub struct WindowDiscovery {
    /// The game id (dialog/stream/container id).
    pub id: String,
    pub title: String,
    pub kind: WindowDiscoveryKind,
    /// The game asked to persist position (save='t').
    pub save: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowDiscoveryKind {
    /// A named text stream (thoughts/loot/bounty/…).
    Stream,
    /// A resident dialog panel with no dedicated widget (combat/befriend).
    DialogPanel,
    /// A non-resident dialog that pops up (bank).
    DialogPopup,
}

/// Mouse drag state for window operations
#[derive(Clone, Debug)]
pub struct MouseDragState {
    pub operation: DragOperation,
    pub window_name: String,
    pub start_pos: (u16, u16),
    pub original_window_pos: (u16, u16, u16, u16), // x, y, width, height
}

/// Type of mouse drag operation
#[derive(Clone, Debug, PartialEq)]
pub enum DragOperation {
    Move,
    ResizeRight,
    ResizeBottom,
    ResizeBottomRight,
}

/// Dialog drag state for move/resize operations
#[derive(Clone, Debug)]
pub struct DialogDragState {
    pub operation: DialogDragOperation,
    pub start_pos: (u16, u16),
    pub original_dialog_pos: (u16, u16),
    pub original_dialog_size: (u16, u16),
}

/// Type of dialog drag operation
#[derive(Clone, Debug, PartialEq)]
pub enum DialogDragOperation {
    Move,
    ResizeRight,
    ResizeBottom,
    ResizeBottomRight,
    ResizeLeft,
    ResizeTop,
    ResizeTopLeft,
    ResizeTopRight,
    ResizeBottomLeft,
}

/// Link drag state (Ctrl+drag on a link)
#[derive(Clone, Debug)]
pub struct LinkDragState {
    pub link_data: LinkData,
    pub start_pos: (u16, u16),
    pub current_pos: (u16, u16),
}

/// Pending link click (mouse down on link, waiting for mouse up to send _menu)
#[derive(Clone, Debug)]
pub struct PendingLinkClick {
    pub link_data: LinkData,
    pub click_pos: (u16, u16),
}

/// Input mode for the application
#[derive(Clone, Debug, PartialEq)]
pub enum InputMode {
    /// Normal command input
    Normal,
    /// Vi-style navigation mode
    Navigation,
    /// Scrolling through history
    History,
    /// Search mode (Ctrl+F)
    Search,
    /// Popup menu is active (Tab/Shift+Tab navigation)
    Menu,
    /// Dialog popup is active (openDialog type="dynamic")
    Dialog,
    /// Window editor is open
    WindowEditor,
    /// Highlight browser is open
    HighlightBrowser,
    /// Highlight form is open (create/edit highlight)
    HighlightForm,
    /// Keybind browser is open
    KeybindBrowser,
    /// Keybind form is open (create/edit keybind)
    KeybindForm,
    /// Hotbar editor is open (bars -> buttons -> button form)
    HotbarEditor,
    /// Color palette browser is open
    ColorPaletteBrowser,
    /// Color form is open (create/edit palette color)
    ColorForm,
    /// UI colors browser is open
    UIColorsBrowser,
    /// Spell colors browser is open
    SpellColorsBrowser,
    /// Spell color form is open (create/edit spell color)
    SpellColorForm,
    /// Menu keybind editor is open (edit the [menu] navigation/action keys)
    MenuKeybindEditor,
    /// Theme browser is open
    ThemeBrowser,
    /// Theme editor is open (create/edit theme)
    ThemeEditor,
    /// Settings editor is open
    SettingsEditor,
    /// Pack editor (.packs) is open
    PackEditor,
    /// Indicator template editor is open
    IndicatorTemplateEditor,
    /// Status abbreviation editor is open (edit the target_list.status_abbrev
    /// map: full status name -> short tag, shown in targets & players)
    StatusAbbrevEditor,
    /// Interact mode: arrow-key/controller focus cycling over room entities
    Interact,
}

/// Entity category the interact-mode focus is cycling through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractCategory {
    Creatures,
    Objects,
    Players,
    Exits,
}

impl InteractCategory {
    /// Cycle order for left/right category navigation.
    pub const ORDER: [InteractCategory; 4] = [
        InteractCategory::Creatures,
        InteractCategory::Objects,
        InteractCategory::Players,
        InteractCategory::Exits,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            InteractCategory::Creatures => "Creatures",
            InteractCategory::Objects => "Objects",
            InteractCategory::Players => "Players",
            InteractCategory::Exits => "Exits",
        }
    }
}

/// Interact-mode focus state. Present only while the mode is active
/// (`InputMode::Interact`). `focus_key` remembers the focused entity's
/// stable key (exist id, or exit direction) so focus survives room
/// updates rewriting the entity lists.
#[derive(Clone, Debug, PartialEq)]
pub struct InteractState {
    pub category: InteractCategory,
    pub index: usize,
    pub focus_key: Option<String>,
}

/// Dialog popup state
#[derive(Clone, Debug)]
pub struct DialogState {
    pub id: String,
    pub title: Option<String>,
    pub buttons: Vec<DialogButton>,
    pub selected: usize,
    pub fields: Vec<DialogField>,
    pub labels: Vec<DialogLabel>,
    pub focused_field: Option<usize>,
    /// Progress bars to display in the dialog
    pub progress_bars: Vec<DialogProgressBar>,
    /// Standalone display labels (not paired with input fields)
    pub display_labels: Vec<DialogLabel>,
    /// Option pickers (`<dropDownBox>`), e.g. combat stance/aim/spell.
    pub dropdowns: Vec<DialogDropDown>,
    /// Text-command links (`<link>`), e.g. combat's skin/search footer.
    pub links: Vec<DialogLink>,
    /// Icon buttons (`<image>`), e.g. combat's weapon-ready row.
    pub images: Vec<DialogImage>,
    /// Backdrop art (`<skin>`), e.g. UberBar's injury paperdoll + bar skins.
    pub skins: Vec<DialogSkin>,
    /// Integer spinners (`<upDownEditBox>`), e.g. quickstrike offset.
    pub spinboxes: Vec<DialogSpinBox>,
    /// Manual position override (None = auto-center)
    pub position: Option<(u16, u16)>,
    /// Manual size override (None = auto-size based on content)
    pub size: Option<(u16, u16)>,
    /// Whether to persist position/size across sessions (save='t' in XML)
    pub save_position: bool,
}

impl DialogState {
    /// An empty dialog with the given id/title, ready to accumulate
    /// controls from dialogData. `save_position` and geometry default off.
    pub fn empty(id: String, title: Option<String>) -> Self {
        DialogState {
            id,
            title,
            buttons: Vec::new(),
            selected: 0,
            fields: Vec::new(),
            labels: Vec::new(),
            focused_field: None,
            progress_bars: Vec::new(),
            display_labels: Vec::new(),
            dropdowns: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
            skins: Vec::new(),
            spinboxes: Vec::new(),
            position: None,
            size: None,
            save_position: false,
        }
    }

    /// Substitute `%id%` placeholders in a control command with the
    /// current field values and dropdown selections (the game's commands
    /// reference sibling controls: `cmd='prep %dDBSpell0%'`).
    pub fn command_with_placeholders(&self, command: &str) -> String {
        let mut resolved = command.to_string();
        for field in &self.fields {
            let token = format!("%{}%", field.id);
            resolved = resolved.replace(&token, &field.value);
        }
        for dropdown in &self.dropdowns {
            let token = format!("%{}%", dropdown.id);
            resolved = resolved.replace(&token, &dropdown.value);
        }
        for spinbox in &self.spinboxes {
            let token = format!("%{}%", spinbox.id);
            resolved = resolved.replace(&token, &spinbox.value.to_string());
        }
        resolved
    }

    /// Advance a dropdown to its next option (wrapping) and return the
    /// resolved command to send, if the dropdown carries one. The TUI's
    /// click-to-cycle interaction; the GUI uses a real combo box.
    pub fn cycle_dropdown(&mut self, index: usize) -> Option<String> {
        let dropdown = self.dropdowns.get_mut(index)?;
        if dropdown.options.is_empty() {
            return None;
        }
        let current = dropdown
            .options
            .iter()
            .position(|(_, value)| *value == dropdown.value)
            .unwrap_or(usize::MAX);
        let next = if current == usize::MAX {
            0
        } else {
            (current + 1) % dropdown.options.len()
        };
        dropdown.value = dropdown.options[next].1.clone();
        let command = dropdown.command.clone();
        if command.trim().is_empty() {
            return None;
        }
        Some(format!("{}\n", self.command_with_placeholders(&command)))
    }

    /// Activate a button by index, applying close/radio-group/autosend
    /// semantics. Returns (command to send, whether to close the dialog).
    pub fn activate_button(&mut self, index: usize) -> (Option<String>, bool) {
        let mut command_to_send: Option<String> = None;
        let mut close_dialog = false;

        if let Some(button) = self.buttons.get(index) {
            let button_id = button.id.clone();
            let button_cmd = button.command.clone();
            let button_autosend = button.autosend;
            let button_is_radio = button.is_radio;
            let button_is_close = button.is_close;
            let button_group = button.group.clone();

            if button_is_close {
                if !button_cmd.trim().is_empty() {
                    let resolved = self.command_with_placeholders(&button_cmd);
                    command_to_send = Some(format!("{}\n", resolved));
                }
                close_dialog = true;
            } else if button_is_radio {
                for other in self.buttons.iter_mut() {
                    if other.is_radio && other.group == button_group {
                        other.selected = other.id == button_id;
                    }
                }
                if button_autosend {
                    let resolved = self.command_with_placeholders(&button_cmd);
                    command_to_send = Some(format!("{}\n", resolved));
                }
            } else {
                let resolved = self.command_with_placeholders(&button_cmd);
                command_to_send = Some(format!("{}\n", resolved));
            }
        }

        (command_to_send, close_dialog)
    }
}

/// Pixel-space layout hints the game attaches to dialog controls:
/// absolute `top`/`left` (can be negative), size, compass `align`
/// (n/nw/ne/...), and anchors positioning a control relative to sibling
/// control ids (`anchor_left='cmdHide'`). Renderers translate these into
/// their own coordinate systems (GUI near-literally, TUI to cells).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialogControlLayout {
    pub top: Option<i32>,
    pub left: Option<i32>,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub align: Option<String>,
    pub anchor_top: Option<String>,
    pub anchor_left: Option<String>,
    pub anchor_right: Option<String>,
}

impl DialogControlLayout {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Dialog button definition
#[derive(Clone, Debug)]
pub struct DialogButton {
    pub id: String,
    pub label: String,
    pub command: String,
    pub is_close: bool,
    pub is_radio: bool,
    pub selected: bool,
    pub autosend: bool,
    pub group: Option<String>,
    /// Layout hints from the tag (None when the tag carried none).
    pub layout: Option<DialogControlLayout>,
}

/// A `<link>` inside dialogData: a text command (combat's skin/search/
/// grip/multistrike footer). Like a button but rendered as a link.
#[derive(Clone, Debug)]
pub struct DialogLink {
    pub id: String,
    pub label: String,
    pub command: String,
    pub layout: Option<DialogControlLayout>,
}

/// An `<image>` inside dialogData: a clickable icon by skin asset name
/// (combat's SwordBtn/ShieldBtn weapon-ready row). `name` keys into the
/// shared icon store; renderers fall back to the tooltip/label as text.
#[derive(Clone, Debug)]
pub struct DialogImage {
    pub id: String,
    /// Skin asset name (e.g. "SwordBtn"); resolved against the icon store.
    pub name: String,
    pub command: String,
    pub tooltip: Option<String>,
    pub layout: Option<DialogControlLayout>,
}

/// A `<skin>` inside dialogData: a named art asset drawn as a backdrop —
/// UberBar's `InjuriesPanel` paperdoll and its `healthBar`/`manaBar` bar
/// skins. `name` keys into the icon store (case-insensitive); `controls`
/// lists the ids this skin visually backs (informational for now). When the
/// asset isn't in the pool the renderer simply draws nothing (the numeric
/// progressBar / text still shows through).
#[derive(Clone, Debug)]
pub struct DialogSkin {
    pub id: String,
    /// Skin asset name (e.g. "InjuriesPanel", "healthBar").
    pub name: String,
    /// Comma-separated control ids this skin backs (may be empty).
    pub controls: Vec<String>,
    pub layout: Option<DialogControlLayout>,
}

/// An `<upDownEditBox>` inside dialogData: a bounded integer spinner
/// (combat's quickstrike offset). Its value feeds `%id%` substitution.
#[derive(Clone, Debug)]
pub struct DialogSpinBox {
    pub id: String,
    pub value: i32,
    pub min: i32,
    pub max: i32,
    pub layout: Option<DialogControlLayout>,
}

/// A `<dropDownBox>` inside dialogData: a labelled option picker whose
/// current value other controls' commands can reference via `%id%`
/// (e.g. `cmd='aim %dDBAim%'` on the dropdown itself, or a sibling
/// button's `cmd='prep %dDBSpell0%'`).
#[derive(Clone, Debug)]
pub struct DialogDropDown {
    pub id: String,
    /// Currently selected VALUE (matches an options entry's value).
    pub value: String,
    /// (display text, submit value) pairs from content_text/content_value.
    pub options: Vec<(String, String)>,
    /// Command template sent when the selection changes ("" = passive;
    /// other controls read the value via %id%).
    pub command: String,
    pub tooltip: Option<String>,
    pub layout: Option<DialogControlLayout>,
}

#[derive(Clone, Debug)]
pub struct DialogField {
    pub id: String,
    pub value: String,
    /// Caret position as a CHARACTER index into `value` (not a byte offset).
    /// `value` is UTF-8, so every edit converts to a byte offset before
    /// touching the String; indexing it by `cursor` directly would panic on
    /// any multibyte character.
    pub cursor: usize,
    pub enter_button: Option<String>,
    pub focused: bool,
}

impl DialogField {
    /// Number of characters in the value (the upper bound for `cursor`).
    fn char_count(&self) -> usize {
        self.value.chars().count()
    }

    /// Byte offset of character index `cursor` (or the end of the string).
    fn cursor_byte(&self) -> usize {
        self.value
            .char_indices()
            .nth(self.cursor)
            .map(|(idx, _)| idx)
            .unwrap_or(self.value.len())
    }

    /// Clamp the caret into `0..=char_count`. Called when (re)focusing a field
    /// whose value may have been set programmatically.
    pub fn clamp_cursor(&mut self) {
        let max = self.char_count();
        if self.cursor > max {
            self.cursor = max;
        }
    }

    /// Insert a character at the caret and advance it.
    pub fn insert_char(&mut self, ch: char) {
        let at = self.cursor_byte();
        self.value.insert(at, ch);
        self.cursor += 1;
    }

    /// Delete the character before the caret (Backspace). No-op at the start.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let at = self.cursor_byte();
        self.value.remove(at);
    }

    /// Delete the character at the caret (Delete). No-op at the end.
    pub fn delete_forward(&mut self) {
        if self.cursor >= self.char_count() {
            return;
        }
        let at = self.cursor_byte();
        self.value.remove(at);
    }

    /// Move the caret one character left. Returns whether it moved.
    pub fn move_left(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    /// Move the caret one character right. Returns whether it moved.
    pub fn move_right(&mut self) -> bool {
        if self.cursor < self.char_count() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// Move the caret to the start.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move the caret to the end.
    pub fn move_end(&mut self) {
        self.cursor = self.char_count();
    }
}

/// Horizontal text alignment decoded from Wrayth's `justify` attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug)]
pub struct DialogLabel {
    pub id: String,
    pub value: String,
    /// Anchor-grid layout hints (None when the tag carried none). Resident
    /// dynamic dialogs like UberBar position their label rows this way.
    pub layout: Option<DialogControlLayout>,
    /// Wrayth `justify`, a bitfield: the low two bits are the alignment
    /// (0 = left, 1 = center, 2 = right) and bit 4 is a flag Wrayth sets on
    /// some labels (so 4/5/6 are flagged left/center/right). Corpus census
    /// (11.4 GB of logs, 2026-08): 2 is by far the most common value —
    /// effect-duration columns in Buffs/Debuffs/Cooldowns/Active Spells —
    /// then 0 and 4; UberBar sends 4/6. Decode via [`DialogLabel::align`].
    pub justify: Option<u8>,
}

impl DialogLabel {
    /// Decode `justify` to a text alignment: low two bits carry it, the
    /// bit-4 flag is ignored. None (attribute absent) = left.
    pub fn align(&self) -> LabelAlign {
        match self.justify.map(|j| j & 3) {
            Some(1) => LabelAlign::Center,
            Some(2) => LabelAlign::Right,
            // 0 = explicit left; 3 is not a valid alignment (treat as left).
            _ => LabelAlign::Left,
        }
    }
}

/// Progress bar displayed in a dialog
#[derive(Clone, Debug)]
pub struct DialogProgressBar {
    pub id: String,
    pub value: u32,   // Percentage 0-100
    pub text: String, // Display text (e.g., "defensive (100%)")
    /// Anchor-grid layout hints (None when the tag carried none). Resident
    /// dynamic dialogs position their bars via anchor_top/anchor_left chains.
    pub layout: Option<DialogControlLayout>,
}

/// Which dialog control a resolved rect belongs to (index into the
/// corresponding DialogState vec).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionedControlKind {
    Button(usize),
    DropDown(usize),
    ProgressBar(usize),
    /// Index into `display_labels` — a positioned text row (UberBar's grid).
    Label(usize),
    /// Index into `skins` — a positioned backdrop art asset.
    Skin(usize),
    /// Index into `images` — an ANCHOR-ONLY point (never drawn as a control;
    /// e.g. UberBar's invisible `ubbars` background that vitals anchor to).
    Image(usize),
}

/// A dialog control resolved to a pixel-space rect by the anchor grid.
#[derive(Clone, Debug)]
pub struct PositionedControl {
    pub kind: PositionedControlKind,
    /// (x, y, width, height) in dialog-content pixels, origin top-left.
    pub rect: (f32, f32, f32, f32),
}

impl DialogState {
    /// Wrayth's right-panel dialogs are ~190px wide; centered/right
    /// alignment resolves against this canvas unless controls overflow it.
    const CANVAS_WIDTH: f32 = 190.0;

    /// Resolve buttons/dropdowns/progress bars to pixel rects using the
    /// game's layout language: absolute `top`/`left` interpreted through
    /// compass `align` (nw = from left, n = from horizontal center,
    /// ne = from right), then `anchor_top`/`anchor_left`/`anchor_right`
    /// constraints re-positioning controls against resolved siblings
    /// (offsets add to the anchored edge; anchor_left + anchor_right
    /// together stretch the control between them). Returns None when no
    /// control carries position data — callers fall back to flow layout.
    /// Anchor references to controls we don't capture (images) are
    /// skipped, leaving that axis at its absolute placement.
    pub fn positioned_controls(&self) -> Option<(Vec<PositionedControl>, (f32, f32))> {
        struct Entry {
            id: String,
            layout: Option<DialogControlLayout>,
            kind: PositionedControlKind,
            rect: (f32, f32, f32, f32),
        }

        let mut entries: Vec<Entry> = Vec::new();
        // Skins first so the render loop paints these backdrops BEHIND the
        // controls that anchor to them (UberBar's InjuriesPanel + bar skins).
        for (i, skin) in self.skins.iter().enumerate() {
            entries.push(Entry {
                id: skin.id.clone(),
                layout: skin.layout.clone(),
                kind: PositionedControlKind::Skin(i),
                rect: (0.0, 0.0, 100.0, 100.0),
            });
        }
        for (i, button) in self.buttons.iter().enumerate() {
            entries.push(Entry {
                id: button.id.clone(),
                layout: button.layout.clone(),
                kind: PositionedControlKind::Button(i),
                rect: (0.0, 0.0, 55.0, 20.0),
            });
        }
        for (i, dropdown) in self.dropdowns.iter().enumerate() {
            entries.push(Entry {
                id: dropdown.id.clone(),
                layout: dropdown.layout.clone(),
                kind: PositionedControlKind::DropDown(i),
                rect: (0.0, 0.0, 80.0, 20.0),
            });
        }
        for (i, bar) in self.progress_bars.iter().enumerate() {
            // Positioned resident dialogs (UberBar) carry anchor geometry;
            // bars without it fall back to stacked rows at the top so combat's
            // stance bar still lands in roughly the right zone.
            entries.push(Entry {
                id: bar.id.clone(),
                layout: bar.layout.clone(),
                kind: PositionedControlKind::ProgressBar(i),
                rect: (0.0, i as f32 * 20.0, 130.0, 16.0),
            });
        }
        for (i, label) in self.display_labels.iter().enumerate() {
            entries.push(Entry {
                id: label.id.clone(),
                layout: label.layout.clone(),
                kind: PositionedControlKind::Label(i),
                rect: (0.0, 0.0, 60.0, 15.0),
            });
        }
        // Images participate as ANCHOR POINTS only (never drawn as controls):
        // UberBar's `ubbars`/PanelBackground is an invisible <image> that the
        // vitals bars hang from via anchor_top='ubbars'. Without it in the grid
        // those bars lose their vertical anchor and collapse to the top. Wound
        // images (width=0/height=0) resolve to zero-size points and affect
        // nothing.
        for (i, image) in self.images.iter().enumerate() {
            entries.push(Entry {
                id: image.id.clone(),
                layout: image.layout.clone(),
                kind: PositionedControlKind::Image(i),
                rect: (0.0, 0.0, 0.0, 0.0),
            });
        }

        let has_positions = entries.iter().any(|e| {
            e.layout.as_ref().is_some_and(|l| {
                l.top.is_some()
                    || l.left.is_some()
                    || l.anchor_top.is_some()
                    || l.anchor_left.is_some()
                    || l.anchor_right.is_some()
            })
        });
        if !has_positions {
            return None;
        }

        let canvas = Self::CANVAS_WIDTH;

        // Pass 1: absolute placement from align + top/left.
        for entry in entries.iter_mut() {
            let Some(layout) = entry.layout.clone() else {
                continue;
            };
            let w = layout.width.map(f32::from).unwrap_or(entry.rect.2);
            let h = layout.height.map(f32::from).unwrap_or(entry.rect.3);
            let left = layout.left.unwrap_or(0) as f32;
            let top = layout.top.unwrap_or(0) as f32;
            let align = layout.align.as_deref().unwrap_or("nw");
            let x = match align {
                "n" | "s" | "c" | "" => canvas / 2.0 - w / 2.0 + left,
                "ne" | "e" | "se" => canvas - w - left,
                _ => left, // nw/w/sw and anything unknown: from the left
            };
            entry.rect = (x, top, w, h);
        }

        // Which entries participate in implicit vertical flow: Wrayth stacks
        // siblings that share an `anchor_left` target down a column even when
        // a later one omits `anchor_top` (UberBar's `ubbars` vitals block
        // anchors_left to the doll with no anchor_top, yet sits BELOW the label
        // column that also anchors_left to the doll). An entry qualifies when
        // it has `anchor_left` but no explicit `anchor_top`; it then flows
        // below the lowest OTHER sibling sharing that anchor_left that does NOT
        // itself flow (so the anchoring label column, resolved by its own
        // anchor_top chain, is the floor — not a circular dependency). We skip
        // the column's own head (the topmost, which keeps its absolute top).
        // The set of ids that are some other entry's explicit anchor_top
        // target — these are column HEADS and must keep their absolute top
        // (they can't flow, or they'd chase the very siblings anchored to them
        // in a circular push-down).
        let anchor_top_targets: std::collections::HashSet<String> = entries
            .iter()
            .filter_map(|e| e.layout.as_ref().and_then(|l| l.anchor_top.clone()))
            .collect();
        let flows: Vec<Option<String>> = entries
            .iter()
            .map(|e| {
                let l = e.layout.as_ref()?;
                if l.anchor_top.is_some() {
                    return None;
                }
                // A column head (target of someone's anchor_top) stays put.
                if !e.id.is_empty() && anchor_top_targets.contains(&e.id) {
                    return None;
                }
                l.anchor_left.clone()
            })
            .collect();

        // Pass 2 (iterated): anchors against resolved siblings. A few
        // rounds lets chains (a anchored to b anchored to c) settle.
        for _ in 0..4 {
            for index in 0..entries.len() {
                let Some(layout) = entries[index].layout.clone() else {
                    continue;
                };
                let find = |id: &str| -> Option<(f32, f32, f32, f32)> {
                    entries
                        .iter()
                        .find(|e| !e.id.is_empty() && e.id == id)
                        .map(|e| e.rect)
                };
                let mut rect = entries[index].rect;
                if let Some(target) = layout.anchor_top.as_deref().and_then(find) {
                    rect.1 = target.1 + target.3 + layout.top.unwrap_or(2) as f32;
                } else if let Some(al) = flows[index].as_deref() {
                    // Implicit flow: stack below the lowest NON-flowing sibling
                    // that also anchors_left to `al` (the resolved label column
                    // is the floor; other flowing siblings don't count, so a
                    // block of flowing elements all land just below the column
                    // rather than cascading off each other).
                    let floor = entries
                        .iter()
                        .enumerate()
                        .filter(|(j, e)| {
                            *j != index
                                && flows[*j].is_none()
                                && e.layout
                                    .as_ref()
                                    .and_then(|l| l.anchor_left.as_deref())
                                    == Some(al)
                        })
                        .map(|(_, e)| e.rect.1 + e.rect.3)
                        .fold(f32::MIN, f32::max);
                    if floor > f32::MIN {
                        rect.1 = floor + layout.top.unwrap_or(2) as f32;
                    }
                }
                match (
                    layout.anchor_left.as_deref().and_then(find),
                    layout.anchor_right.as_deref().and_then(find),
                ) {
                    (Some(left_of), Some(right_of)) => {
                        rect.0 = left_of.0 + left_of.2 + layout.left.unwrap_or(2) as f32;
                        rect.2 = (right_of.0 - rect.0 - 2.0).max(10.0);
                    }
                    (Some(left_of), None) => {
                        rect.0 = left_of.0 + left_of.2 + layout.left.unwrap_or(2) as f32;
                    }
                    (None, Some(right_of)) => {
                        rect.0 = right_of.0 - rect.2 - layout.left.unwrap_or(2) as f32;
                    }
                    (None, None) => {}
                }
                entries[index].rect = rect;
            }
        }

        let mut max_x: f32 = canvas;
        let mut max_y: f32 = 0.0;
        for entry in &entries {
            max_x = max_x.max(entry.rect.0 + entry.rect.2);
            max_y = max_y.max(entry.rect.1 + entry.rect.3);
        }

        let controls = entries
            .into_iter()
            .map(|e| PositionedControl {
                kind: e.kind,
                rect: e.rect,
            })
            .collect();
        Some((controls, (max_x + 4.0, max_y + 4.0)))
    }
}

/// Injuries popup state for viewing another player's injuries
#[derive(Clone, Debug)]
pub struct InjuriesPopupState {
    /// Dialog ID (e.g., "injuries-10154507")
    pub dialog_id: String,
    /// Player name from dialog title (e.g., "Zoleta")
    pub player_name: String,
    /// Map of body part to injury level (0=none, 1-3=injury, 4-6=scar)
    pub injuries: std::collections::HashMap<String, u8>,
}

impl InjuriesPopupState {
    pub fn new(dialog_id: String, player_name: String) -> Self {
        Self {
            dialog_id,
            player_name,
            injuries: std::collections::HashMap::new(),
        }
    }

    /// Set injury level for a body part from image name
    pub fn set_injury_from_name(&mut self, body_part: &str, name: &str) {
        // Parse name like "Injury1", "Injury2", "Injury3", "Scar1", "Scar2", "Scar3"
        let level = match name {
            "Injury1" => 1,
            "Injury2" => 2,
            "Injury3" => 3,
            "Scar1" => 4,
            "Scar2" => 5,
            "Scar3" => 6,
            _ => 0, // Clear or unknown
        };
        self.injuries.insert(body_part.to_string(), level);
    }

    /// Get injury level for a body part (0 if not set)
    pub fn get_injury(&self, body_part: &str) -> u8 {
        self.injuries.get(body_part).copied().unwrap_or(0)
    }
}

/// Popup menu state
#[derive(Clone, Debug)]
pub struct PopupMenu {
    pub items: Vec<PopupMenuItem>,
    pub selected: usize,
    pub position: (u16, u16), // x, y position
}

/// A single popup menu item
#[derive(Clone, Debug)]
pub struct PopupMenuItem {
    pub text: String,
    pub command: String,
    pub disabled: bool,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            widget_type_index: HashMap::new(),
            focused_window: None,
            input_mode: InputMode::Normal,
            search_input: String::new(),
            search_cursor: 0,
            popup_menu: None,
            submenu: None,
            nested_submenu: None,
            deep_submenu: None,
            interact: None,
            status_text: String::from("Ready"),
            mouse_drag: None,
            selection_state: None,
            selection_drag_start: None,
            link_drag_state: None,
            pending_link_click: None,
            pending_panel_commands: std::cell::RefCell::new(Vec::new()),
            needs_widget_reset: false,
            widgets_to_reset: Vec::new(),
            ephemeral_windows: std::collections::HashSet::new(),
            shown_container_titles: std::collections::HashSet::new(),
            shown_dialog_ids: std::collections::HashSet::new(),
            quickbars: HashMap::new(),
            quickbar_order: Vec::new(),
            active_quickbar_id: None,
            active_dialog: None,
            dialog_store: HashMap::new(),
            window_hints: HashMap::new(),
            injuries_popup: None,
            dialog_drag: None,
            pending_window_additions: Vec::new(),
            pending_window_discoveries: Vec::new(),
        }
    }

    /// Get (creating if absent) the store entry for a dialog id. All
    /// dialogData ingestion writes here so a dialog can be re-shown intact.
    pub fn dialog_slot_mut(&mut self, id: &str) -> &mut DialogState {
        self.dialog_store
            .entry(id.to_string())
            .or_insert_with(|| DialogState::empty(id.to_string(), None))
    }

    /// Mirror a stored dialog into the visible `active_dialog` slot,
    /// preserving the live position/size/save flag if the same dialog is
    /// already showing (so an incoming delta doesn't yank a moved popup
    /// back to center). Switches input mode to Dialog and closes menus.
    pub fn show_dialog_from_store(&mut self, id: &str) {
        let Some(mut dialog) = self.dialog_store.get(id).cloned() else {
            return;
        };
        if let Some(current) = self.active_dialog.as_ref().filter(|d| d.id == id) {
            dialog.position = current.position;
            dialog.size = current.size;
            dialog.save_position = current.save_position;
            dialog.selected = current.selected.min(dialog.buttons.len().saturating_sub(1));
            dialog.focused_field = current.focused_field;
        }
        self.active_dialog = Some(dialog);
        self.input_mode = InputMode::Dialog;
        self.popup_menu = None;
        self.submenu = None;
        self.nested_submenu = None;
        self.deep_submenu = None;
    }

    /// Position of the deepest currently-open popup-menu level, falling back
    /// through deep → nested → submenu → popup, then the default anchor
    /// `(40, 12)` when no menu is open. The menu-command handlers place a new
    /// child level relative to this; extracting it collapses ~half a dozen
    /// hand-copied `.or_else(...).or_else(...)` cascades (which are easy to get
    /// subtly wrong per site) into one tested function.
    pub fn deepest_menu_pos(&self) -> (u16, u16) {
        self.deep_submenu
            .as_ref()
            .map(|m| m.get_position())
            .or_else(|| self.nested_submenu.as_ref().map(|m| m.get_position()))
            .or_else(|| self.submenu.as_ref().map(|m| m.get_position()))
            .or_else(|| self.popup_menu.as_ref().map(|m| m.get_position()))
            .unwrap_or((40, 12))
    }

    /// Where a new child menu level should open: two columns right of the
    /// deepest open level, same row.
    pub fn child_menu_pos(&self) -> (u16, u16) {
        let (x, y) = self.deepest_menu_pos();
        (x + 2, y)
    }

    /// Close every popup-menu level (popup + all three submenu depths). Does
    /// not touch `input_mode` — callers set that as they need.
    pub fn close_all_menus(&mut self) {
        self.popup_menu = None;
        self.submenu = None;
        self.nested_submenu = None;
        self.deep_submenu = None;
    }

    /// Get a window by name
    pub fn get_window(&self, name: &str) -> Option<&WindowState> {
        self.windows.get(name)
    }

    /// Get a mutable window by name
    pub fn get_window_mut(&mut self, name: &str) -> Option<&mut WindowState> {
        self.windows.get_mut(name)
    }

    /// Add or update a window
    pub fn set_window(&mut self, name: String, window: WindowState) {
        self.windows.insert(name, window);
        self.rebuild_widget_index();
    }

    /// Remove a window by name
    pub fn remove_window(&mut self, name: &str) -> Option<WindowState> {
        let result = self.windows.remove(name);
        if result.is_some() {
            self.rebuild_widget_index();
        }
        result
    }

    /// Rebuild the widget type index cache
    /// Called whenever windows are added/removed
    pub fn rebuild_widget_index(&mut self) {
        self.widget_type_index.clear();
        for (name, window) in &self.windows {
            self.widget_type_index
                .entry(window.widget_type.clone())
                .or_default()
                .push(name.clone());
        }
    }

    /// Get a window by widget type and optional name
    /// For singletons (Compass, InjuryDoll): pass None for name
    /// For multi-instance (Countdown, Text, etc): pass Some(name) to specify which one
    pub fn get_window_by_type(
        &self,
        widget_type: super::window::WidgetType,
        name: Option<&str>,
    ) -> Option<&WindowState> {
        let candidates = self.widget_type_index.get(&widget_type)?;

        match name {
            Some(specific_name) => {
                // Multi-instance: find the specific named window
                self.windows.get(specific_name)
            }
            None => {
                // Singleton: return the first (only) window of this type
                candidates.first().and_then(|n| self.windows.get(n))
            }
        }
    }

    /// Get a mutable window by widget type and optional name
    /// For singletons (Compass, InjuryDoll): pass None for name
    /// For multi-instance (Countdown, Text, etc): pass Some(name) to specify which one
    pub fn get_window_by_type_mut(
        &mut self,
        widget_type: super::window::WidgetType,
        name: Option<&str>,
    ) -> Option<&mut WindowState> {
        let candidates = self.widget_type_index.get(&widget_type)?;

        match name {
            Some(specific_name) => {
                // Multi-instance: find the specific named window
                self.windows.get_mut(specific_name)
            }
            None => {
                // Singleton: return the first (only) window of this type
                let window_name = candidates.first()?.clone();
                self.windows.get_mut(&window_name)
            }
        }
    }

    /// Set the focused window
    pub fn set_focus(&mut self, name: Option<String>) {
        // Clear old focus
        if let Some(old_name) = &self.focused_window {
            if let Some(window) = self.windows.get_mut(old_name) {
                window.focused = false;
            }
        }

        // Set new focus
        if let Some(new_name) = &name {
            if let Some(window) = self.windows.get_mut(new_name) {
                window.focused = true;
            }
        }

        self.focused_window = name;
    }

    /// Get the currently focused window
    pub fn focused_window(&self) -> Option<&WindowState> {
        self.focused_window
            .as_ref()
            .and_then(|name| self.windows.get(name))
    }

    /// Get the currently focused window mutably
    pub fn focused_window_mut(&mut self) -> Option<&mut WindowState> {
        let name = self.focused_window.clone();
        name.as_ref().and_then(|n| self.windows.get_mut(n))
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::new()
    }
}

impl PopupMenu {
    pub fn new(items: Vec<PopupMenuItem>, position: (u16, u16)) -> Self {
        Self {
            items,
            selected: 0,
            position,
        }
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.items.is_empty() {
            self.selected = if self.selected == 0 {
                self.items.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    pub fn selected_item(&self) -> Option<&PopupMenuItem> {
        self.items.get(self.selected)
    }

    pub fn get_selected(&self) -> Option<&PopupMenuItem> {
        self.items.get(self.selected)
    }

    pub fn get_items(&self) -> &[PopupMenuItem] {
        &self.items
    }

    pub fn get_position(&self) -> (u16, u16) {
        self.position
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected
    }

    /// Check if a mouse click at (x, y) hits a menu item
    /// Returns the index of the clicked item if any
    ///
    /// # Arguments
    /// * `area` - Tuple of (x, y, width, height) representing the menu area
    pub fn check_click(&self, x: u16, y: u16, area: (u16, u16, u16, u16)) -> Option<usize> {
        let (area_x, area_y, area_width, area_height) = area;

        // Check if click is within the menu area
        if x < area_x || x >= area_x + area_width || y < area_y || y >= area_y + area_height {
            return None;
        }

        // Calculate which item was clicked (accounting for border and title)
        let relative_y = (y - area_y) as usize;

        // Border takes 1 row at top and bottom
        if relative_y == 0 || relative_y >= area_height as usize - 1 {
            return None; // Clicked on border
        }

        let item_index = relative_y - 1; // Subtract top border

        if item_index < self.items.len() {
            Some(item_index)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== DialogLabel justify decode Tests ====================

    fn label_with_justify(justify: Option<u8>) -> DialogLabel {
        DialogLabel {
            id: "l".to_string(),
            value: "x".to_string(),
            layout: None,
            justify,
        }
    }

    #[test]
    fn justify_decodes_as_bitfield_low_two_bits() {
        // Real wire values (11.4 GB corpus census 2026-08): 2 is the most
        // common (×10.5M — effect-duration labels in Buffs/Debuffs/
        // Cooldowns/Active Spells), then 0 (mapMaster) and 4 (expr's
        // PTPs/MTPs columns); 5 is rare; UberBar sends 4/6. Bit 4 is a
        // flag: 4/5/6 are flagged left/center/right.
        assert_eq!(label_with_justify(Some(0)).align(), LabelAlign::Left);
        assert_eq!(label_with_justify(Some(1)).align(), LabelAlign::Center);
        assert_eq!(label_with_justify(Some(2)).align(), LabelAlign::Right);
        assert_eq!(label_with_justify(Some(4)).align(), LabelAlign::Left);
        assert_eq!(label_with_justify(Some(5)).align(), LabelAlign::Center);
        assert_eq!(label_with_justify(Some(6)).align(), LabelAlign::Right);
    }

    #[test]
    fn justify_absent_or_invalid_defaults_left() {
        assert_eq!(label_with_justify(None).align(), LabelAlign::Left);
        // 3 is not a valid alignment in the low bits; don't guess.
        assert_eq!(label_with_justify(Some(3)).align(), LabelAlign::Left);
        assert_eq!(label_with_justify(Some(7)).align(), LabelAlign::Left);
    }

    // ==================== DialogField edit (UTF-8 safety) Tests ====================

    fn field(value: &str, cursor: usize) -> DialogField {
        DialogField {
            id: "f".to_string(),
            value: value.to_string(),
            cursor,
            enter_button: None,
            focused: false,
        }
    }

    #[test]
    fn dialog_field_insert_ascii() {
        let mut f = field("ab", 1);
        f.insert_char('X');
        assert_eq!(f.value, "aXb");
        assert_eq!(f.cursor, 2);
    }

    #[test]
    fn dialog_field_insert_after_multibyte_does_not_panic() {
        // Regression: 'é' is 2 bytes; a char-stepped cursor used as a byte
        // index would land mid-codepoint and panic on the next insert.
        let mut f = field("é", 1);
        f.insert_char('z');
        assert_eq!(f.value, "éz");
        assert_eq!(f.cursor, 2);
    }

    #[test]
    fn dialog_field_insert_multibyte_between_multibyte() {
        let mut f = field("áé", 1);
        f.insert_char('ñ');
        assert_eq!(f.value, "áñé");
        assert_eq!(f.cursor, 2);
    }

    #[test]
    fn dialog_field_backspace_on_prefilled_multibyte() {
        // A game-prefilled field ending in a multibyte char, cursor at end.
        let mut f = field("café", 4);
        f.backspace();
        assert_eq!(f.value, "caf");
        assert_eq!(f.cursor, 3);
    }

    #[test]
    fn dialog_field_backspace_at_start_is_noop() {
        let mut f = field("é", 0);
        f.backspace();
        assert_eq!(f.value, "é");
        assert_eq!(f.cursor, 0);
    }

    #[test]
    fn dialog_field_delete_forward_multibyte() {
        let mut f = field("aéb", 1);
        f.delete_forward();
        assert_eq!(f.value, "ab");
        assert_eq!(f.cursor, 1);
    }

    #[test]
    fn dialog_field_delete_forward_at_end_is_noop() {
        let mut f = field("é", 1);
        f.delete_forward();
        assert_eq!(f.value, "é");
        assert_eq!(f.cursor, 1);
    }

    #[test]
    fn dialog_field_move_bounds_are_char_counts() {
        let mut f = field("é€", 0);
        assert!(f.move_right());
        assert!(f.move_right());
        assert!(!f.move_right(), "cursor must stop at char count, not byte len");
        assert_eq!(f.cursor, 2);
        f.move_end();
        assert_eq!(f.cursor, 2);
        f.move_home();
        assert_eq!(f.cursor, 0);
        assert!(!f.move_left());
    }

    #[test]
    fn dialog_field_clamp_cursor_uses_char_count() {
        // cursor set past the char count (e.g. stale from a longer value)
        // must clamp to the char count, never leave a byte-sized overshoot.
        let mut f = field("é", 5);
        f.clamp_cursor();
        assert_eq!(f.cursor, 1);
    }

    #[test]
    fn dialog_field_full_edit_sequence_multibyte() {
        // Type "naïve", then backspace twice, then insert — exercises the
        // whole path the panic used to blow up.
        let mut f = field("", 0);
        for ch in "naïve".chars() {
            f.insert_char(ch);
        }
        assert_eq!(f.value, "naïve");
        assert_eq!(f.cursor, 5);
        f.backspace();
        f.backspace();
        assert_eq!(f.value, "naï");
        f.insert_char('l');
        assert_eq!(f.value, "naïl");
    }

    // ==================== UiState Tests ====================

    #[test]
    fn test_ui_state_new() {
        let state = UiState::new();
        assert!(state.windows.is_empty());
        assert!(state.focused_window.is_none());
        assert_eq!(state.input_mode, InputMode::Normal);
        assert!(state.search_input.is_empty());
        assert_eq!(state.search_cursor, 0);
        assert!(state.popup_menu.is_none());
        assert!(state.submenu.is_none());
        assert!(state.nested_submenu.is_none());
        assert_eq!(state.status_text, "Ready");
        assert!(state.mouse_drag.is_none());
        assert!(state.selection_state.is_none());
    }

    #[test]
    fn test_ui_state_default() {
        let state = UiState::default();
        assert!(state.windows.is_empty());
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn deepest_menu_pos_falls_through_levels() {
        let mut state = UiState::new();
        // No menu open: the default anchor.
        assert_eq!(state.deepest_menu_pos(), (40, 12));
        // popup only.
        state.popup_menu = Some(PopupMenu::new(vec![], (10, 20)));
        assert_eq!(state.deepest_menu_pos(), (10, 20));
        // submenu wins over popup.
        state.submenu = Some(PopupMenu::new(vec![], (12, 20)));
        assert_eq!(state.deepest_menu_pos(), (12, 20));
        // nested wins over submenu.
        state.nested_submenu = Some(PopupMenu::new(vec![], (14, 20)));
        assert_eq!(state.deepest_menu_pos(), (14, 20));
        // deep wins over all.
        state.deep_submenu = Some(PopupMenu::new(vec![], (16, 20)));
        assert_eq!(state.deepest_menu_pos(), (16, 20));
    }

    #[test]
    fn child_menu_pos_is_two_right_same_row() {
        let mut state = UiState::new();
        state.popup_menu = Some(PopupMenu::new(vec![], (40, 12)));
        assert_eq!(state.child_menu_pos(), (42, 12));
    }

    #[test]
    fn close_all_menus_clears_every_level_but_not_mode() {
        let mut state = UiState::new();
        state.popup_menu = Some(PopupMenu::new(vec![], (1, 1)));
        state.submenu = Some(PopupMenu::new(vec![], (2, 2)));
        state.nested_submenu = Some(PopupMenu::new(vec![], (3, 3)));
        state.deep_submenu = Some(PopupMenu::new(vec![], (4, 4)));
        state.input_mode = InputMode::Menu;
        state.close_all_menus();
        assert!(state.popup_menu.is_none());
        assert!(state.submenu.is_none());
        assert!(state.nested_submenu.is_none());
        assert!(state.deep_submenu.is_none());
        // input_mode is the caller's responsibility, not touched here.
        assert_eq!(state.input_mode, InputMode::Menu);
    }

    #[test]
    fn test_ui_state_get_nonexistent_window() {
        let state = UiState::new();
        assert!(state.get_window("nonexistent").is_none());
    }

    #[test]
    fn test_ui_state_focused_window_none() {
        let state = UiState::new();
        assert!(state.focused_window().is_none());
    }

    // ==================== InputMode Tests ====================

    #[test]
    fn test_input_mode_equality() {
        assert_eq!(InputMode::Normal, InputMode::Normal);
        assert_ne!(InputMode::Normal, InputMode::Navigation);
        assert_ne!(InputMode::History, InputMode::Search);
    }

    #[test]
    fn test_input_mode_clone() {
        let mode = InputMode::WindowEditor;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_input_mode_debug() {
        let debug_str = format!("{:?}", InputMode::HighlightBrowser);
        assert!(debug_str.contains("HighlightBrowser"));
    }

    #[test]
    fn test_all_input_modes_distinct() {
        let modes = vec![
            InputMode::Normal,
            InputMode::Navigation,
            InputMode::History,
            InputMode::Search,
            InputMode::Menu,
            InputMode::Dialog,
            InputMode::WindowEditor,
            InputMode::HighlightBrowser,
            InputMode::HighlightForm,
            InputMode::KeybindBrowser,
            InputMode::KeybindForm,
            InputMode::ColorPaletteBrowser,
            InputMode::ColorForm,
            InputMode::UIColorsBrowser,
            InputMode::SpellColorsBrowser,
            InputMode::SpellColorForm,
            InputMode::MenuKeybindEditor,
            InputMode::ThemeBrowser,
            InputMode::ThemeEditor,
            InputMode::SettingsEditor,
            InputMode::IndicatorTemplateEditor,
        ];

        // All modes should be distinct
        for i in 0..modes.len() {
            for j in i + 1..modes.len() {
                assert_ne!(modes[i], modes[j]);
            }
        }
    }

    // ==================== DragOperation Tests ====================

    #[test]
    fn test_drag_operation_equality() {
        assert_eq!(DragOperation::Move, DragOperation::Move);
        assert_ne!(DragOperation::Move, DragOperation::ResizeRight);
        assert_ne!(
            DragOperation::ResizeBottom,
            DragOperation::ResizeBottomRight
        );
    }

    #[test]
    fn test_drag_operation_clone() {
        let op = DragOperation::ResizeBottomRight;
        let cloned = op.clone();
        assert_eq!(op, cloned);
    }

    #[test]
    fn test_drag_operation_debug() {
        let debug_str = format!("{:?}", DragOperation::Move);
        assert!(debug_str.contains("Move"));
    }

    // ==================== PopupMenuItem Tests ====================

    #[test]
    fn test_popup_menu_item_creation() {
        let item = PopupMenuItem {
            text: "Look".to_string(),
            command: "look".to_string(),
            disabled: false,
        };
        assert_eq!(item.text, "Look");
        assert_eq!(item.command, "look");
        assert!(!item.disabled);
    }

    #[test]
    fn test_popup_menu_item_disabled() {
        let item = PopupMenuItem {
            text: "Disabled Action".to_string(),
            command: "disabled".to_string(),
            disabled: true,
        };
        assert!(item.disabled);
    }

    #[test]
    fn test_popup_menu_item_clone() {
        let item = PopupMenuItem {
            text: "Get".to_string(),
            command: "get".to_string(),
            disabled: false,
        };
        let cloned = item.clone();
        assert_eq!(cloned.text, item.text);
        assert_eq!(cloned.command, item.command);
        assert_eq!(cloned.disabled, item.disabled);
    }

    // ==================== PopupMenu Tests ====================

    fn create_test_menu() -> PopupMenu {
        let items = vec![
            PopupMenuItem {
                text: "Look".to_string(),
                command: "look".to_string(),
                disabled: false,
            },
            PopupMenuItem {
                text: "Get".to_string(),
                command: "get".to_string(),
                disabled: false,
            },
            PopupMenuItem {
                text: "Drop".to_string(),
                command: "drop".to_string(),
                disabled: false,
            },
        ];
        PopupMenu::new(items, (10, 20))
    }

    #[test]
    fn test_popup_menu_new() {
        let menu = create_test_menu();
        assert_eq!(menu.items.len(), 3);
        assert_eq!(menu.selected, 0);
        assert_eq!(menu.position, (10, 20));
    }

    #[test]
    fn test_popup_menu_empty() {
        let menu = PopupMenu::new(vec![], (0, 0));
        assert!(menu.items.is_empty());
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_select_next() {
        let mut menu = create_test_menu();
        assert_eq!(menu.selected, 0);

        menu.select_next();
        assert_eq!(menu.selected, 1);

        menu.select_next();
        assert_eq!(menu.selected, 2);

        // Should wrap around
        menu.select_next();
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_select_next_empty() {
        let mut menu = PopupMenu::new(vec![], (0, 0));
        menu.select_next(); // Should not panic
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_select_prev() {
        let mut menu = create_test_menu();
        assert_eq!(menu.selected, 0);

        // Should wrap to last item
        menu.select_prev();
        assert_eq!(menu.selected, 2);

        menu.select_prev();
        assert_eq!(menu.selected, 1);

        menu.select_prev();
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_select_prev_empty() {
        let mut menu = PopupMenu::new(vec![], (0, 0));
        menu.select_prev(); // Should not panic
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn test_popup_menu_selected_item() {
        let menu = create_test_menu();
        let item = menu.selected_item().unwrap();
        assert_eq!(item.text, "Look");
    }

    #[test]
    fn test_popup_menu_selected_item_after_navigation() {
        let mut menu = create_test_menu();
        menu.select_next();
        let item = menu.selected_item().unwrap();
        assert_eq!(item.text, "Get");
    }

    #[test]
    fn test_popup_menu_selected_item_empty() {
        let menu = PopupMenu::new(vec![], (0, 0));
        assert!(menu.selected_item().is_none());
    }

    #[test]
    fn test_popup_menu_get_selected() {
        let menu = create_test_menu();
        let item = menu.get_selected().unwrap();
        assert_eq!(item.command, "look");
    }

    #[test]
    fn test_popup_menu_get_items() {
        let menu = create_test_menu();
        let items = menu.get_items();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "Look");
        assert_eq!(items[1].text, "Get");
        assert_eq!(items[2].text, "Drop");
    }

    #[test]
    fn test_popup_menu_get_position() {
        let menu = create_test_menu();
        assert_eq!(menu.get_position(), (10, 20));
    }

    #[test]
    fn test_popup_menu_get_selected_index() {
        let mut menu = create_test_menu();
        assert_eq!(menu.get_selected_index(), 0);

        menu.select_next();
        assert_eq!(menu.get_selected_index(), 1);
    }

    // ==================== PopupMenu::check_click Tests ====================

    #[test]
    fn test_check_click_outside_left() {
        let menu = create_test_menu();
        // Area starts at x=10, click at x=5 is outside
        let result = menu.check_click(5, 22, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_outside_right() {
        let menu = create_test_menu();
        // Area is x=10 to x=30 (10+20), click at x=35 is outside
        let result = menu.check_click(35, 22, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_outside_top() {
        let menu = create_test_menu();
        // Area starts at y=20, click at y=15 is outside
        let result = menu.check_click(15, 15, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_outside_bottom() {
        let menu = create_test_menu();
        // Area is y=20 to y=25 (20+5), click at y=30 is outside
        let result = menu.check_click(15, 30, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_on_top_border() {
        let menu = create_test_menu();
        // y=20 is the top border (relative_y=0)
        let result = menu.check_click(15, 20, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_on_bottom_border() {
        let menu = create_test_menu();
        // y=24 is the bottom border (area_height-1 = 4)
        let result = menu.check_click(15, 24, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_first_item() {
        let menu = create_test_menu();
        // y=21 is the first item (relative_y=1, item_index=0)
        let result = menu.check_click(15, 21, (10, 20, 20, 5));
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_check_click_second_item() {
        let menu = create_test_menu();
        // y=22 is the second item (relative_y=2, item_index=1)
        let result = menu.check_click(15, 22, (10, 20, 20, 5));
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_check_click_third_item() {
        let menu = create_test_menu();
        // y=23 is the third item (relative_y=3, item_index=2)
        let result = menu.check_click(15, 23, (10, 20, 20, 5));
        assert_eq!(result, Some(2));
    }

    #[test]
    fn test_check_click_beyond_items() {
        // Menu with only 2 items, but area has room for more
        let items = vec![
            PopupMenuItem {
                text: "A".to_string(),
                command: "a".to_string(),
                disabled: false,
            },
            PopupMenuItem {
                text: "B".to_string(),
                command: "b".to_string(),
                disabled: false,
            },
        ];
        let menu = PopupMenu::new(items, (0, 0));

        // Click on what would be item 3 (but menu only has 2 items)
        // Area height = 6, so relative_y=3 gives item_index=2
        let result = menu.check_click(5, 3, (0, 0, 20, 6));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_click_at_area_boundary() {
        let menu = create_test_menu();
        // Click at the exact right edge (x=29, just inside x=10+20-1)
        let result = menu.check_click(29, 21, (10, 20, 20, 5));
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_check_click_at_area_corner() {
        let menu = create_test_menu();
        // Click at top-left corner (border)
        let result = menu.check_click(10, 20, (10, 20, 20, 5));
        assert!(result.is_none());
    }

    // ==================== MouseDragState Tests ====================

    #[test]
    fn test_mouse_drag_state_creation() {
        let drag = MouseDragState {
            operation: DragOperation::Move,
            window_name: "main".to_string(),
            start_pos: (100, 200),
            original_window_pos: (10, 20, 80, 40),
        };
        assert_eq!(drag.operation, DragOperation::Move);
        assert_eq!(drag.window_name, "main");
        assert_eq!(drag.start_pos, (100, 200));
        assert_eq!(drag.original_window_pos, (10, 20, 80, 40));
    }

    #[test]
    fn test_mouse_drag_state_clone() {
        let drag = MouseDragState {
            operation: DragOperation::ResizeRight,
            window_name: "story".to_string(),
            start_pos: (50, 60),
            original_window_pos: (0, 0, 100, 50),
        };
        let cloned = drag.clone();
        assert_eq!(cloned.operation, drag.operation);
        assert_eq!(cloned.window_name, drag.window_name);
    }

    // ==================== PopupMenu Clone Tests ====================

    #[test]
    fn test_popup_menu_clone() {
        let mut menu = create_test_menu();
        menu.select_next();

        let cloned = menu.clone();
        assert_eq!(cloned.items.len(), menu.items.len());
        assert_eq!(cloned.selected, menu.selected);
        assert_eq!(cloned.position, menu.position);
    }

    // ==================== UiState Clone Tests ====================

    #[test]
    fn test_ui_state_clone() {
        let state = UiState::new();
        let cloned = state.clone();
        assert_eq!(cloned.input_mode, state.input_mode);
        assert_eq!(cloned.status_text, state.status_text);
    }

    fn dialog_with(buttons: Vec<DialogButton>, dropdowns: Vec<DialogDropDown>) -> DialogState {
        DialogState {
            buttons,
            dropdowns,
            ..DialogState::empty("combat".to_string(), None)
        }
    }

    fn button(id: &str, layout: DialogControlLayout) -> DialogButton {
        DialogButton {
            id: id.to_string(),
            label: id.to_string(),
            command: String::new(),
            is_close: false,
            is_radio: false,
            selected: false,
            autosend: false,
            group: None,
            layout: Some(layout),
        }
    }

    #[test]
    fn anchor_grid_resolves_combat_stance_row() {
        // Real combat-window row: [defense (nw)] [stance dropdown,
        // anchored between] [offense (ne)], all at top=70.
        use crate::data::ui_state::PositionedControlKind;
        let defense = button(
            "cmdDefStance",
            DialogControlLayout {
                top: Some(70),
                left: Some(0),
                width: Some(55),
                height: Some(20),
                align: Some("nw".to_string()),
                ..Default::default()
            },
        );
        let offense = button(
            "cmdOffStance",
            DialogControlLayout {
                top: Some(70),
                left: Some(0),
                width: Some(50),
                height: Some(20),
                align: Some("ne".to_string()),
                ..Default::default()
            },
        );
        let stance = DialogDropDown {
            id: "dDBStance".to_string(),
            value: "defensive".to_string(),
            options: vec![("offensive".into(), "offensive".into())],
            command: "_stance %dDBStance%".to_string(),
            tooltip: None,
            layout: Some(DialogControlLayout {
                top: Some(70),
                left: Some(0),
                width: Some(80),
                height: Some(20),
                align: Some("n".to_string()),
                anchor_left: Some("cmdDefStance".to_string()),
                anchor_right: Some("cmdOffStance".to_string()),
                ..Default::default()
            }),
        };

        let dialog = dialog_with(vec![defense, offense], vec![stance]);
        let (controls, (w, h)) = dialog.positioned_controls().expect("positioned");

        let rect_of = |kind: PositionedControlKind| {
            controls
                .iter()
                .find(|c| c.kind == kind)
                .map(|c| c.rect)
                .unwrap()
        };
        let defense = rect_of(PositionedControlKind::Button(0));
        let offense = rect_of(PositionedControlKind::Button(1));
        let stance = rect_of(PositionedControlKind::DropDown(0));

        // Defense flush left, offense flush right of the 190px canvas.
        assert_eq!(defense.0, 0.0);
        assert_eq!(offense.0, 190.0 - 50.0);
        // Stance starts at defense's right edge and stretches to offense.
        assert_eq!(stance.0, defense.0 + defense.2);
        assert!((stance.0 + stance.2 - offense.0).abs() <= 2.0 + f32::EPSILON);
        // Whole row at y=70; content bounds cover it.
        assert_eq!(defense.1, 70.0);
        assert_eq!(stance.1, 70.0);
        assert!(w >= 190.0 && h >= 90.0);
    }

    #[test]
    fn cycle_dropdown_advances_and_resolves_command() {
        let stance = DialogDropDown {
            id: "dDBStance".to_string(),
            value: "defensive".to_string(),
            options: vec![
                ("offensive".into(), "offensive".into()),
                ("defensive".into(), "defensive".into()),
            ],
            command: "_stance %dDBStance%".to_string(),
            tooltip: None,
            layout: None,
        };
        let mut dialog = dialog_with(Vec::new(), vec![stance]);

        // defensive (index 1) wraps to offensive (index 0).
        let cmd = dialog.cycle_dropdown(0);
        assert_eq!(dialog.dropdowns[0].value, "offensive");
        assert_eq!(cmd.as_deref(), Some("_stance offensive\n"));

        // And back again.
        let cmd = dialog.cycle_dropdown(0);
        assert_eq!(dialog.dropdowns[0].value, "defensive");
        assert_eq!(cmd.as_deref(), Some("_stance defensive\n"));

        // Out-of-range index is a no-op.
        assert!(dialog.cycle_dropdown(5).is_none());
    }

    #[test]
    fn anchor_grid_resolves_uberbar_label_and_bar_rows() {
        // UberBar's grid: a value label anchored to the right of its header
        // label, and two vitals bars stacked via anchor_top (mana below
        // health). Proves display_labels + progress_bars now flow through the
        // same anchor resolver as combat's controls.
        use crate::data::ui_state::PositionedControlKind;
        let label = |id: &str, layout: DialogControlLayout| DialogLabel {
            id: id.to_string(),
            value: id.to_string(),
            layout: Some(layout),
            justify: None,
        };
        let bar = |id: &str, layout: DialogControlLayout| DialogProgressBar {
            id: id.to_string(),
            value: 50,
            text: id.to_string(),
            layout: Some(layout),
        };

        let header = label(
            "ublog",
            DialogControlLayout {
                top: Some(5),
                left: Some(5),
                width: Some(50),
                height: Some(15),
                align: Some("nw".to_string()),
                ..Default::default()
            },
        );
        let value = label(
            "ublogv",
            DialogControlLayout {
                top: Some(0),
                left: Some(0),
                width: Some(50),
                height: Some(15),
                anchor_left: Some("ublog".to_string()),
                ..Default::default()
            },
        );
        // health anchors to a bare "ubbars" reference we don't capture — that
        // axis stays at its absolute top; mana then stacks under health.
        let health = bar(
            "health",
            DialogControlLayout {
                top: Some(3),
                left: Some(4),
                width: Some(100),
                height: Some(15),
                ..Default::default()
            },
        );
        let mana = bar(
            "mana",
            DialogControlLayout {
                top: Some(3),
                left: Some(4),
                width: Some(100),
                height: Some(15),
                anchor_top: Some("health".to_string()),
                ..Default::default()
            },
        );

        let dialog = DialogState {
            display_labels: vec![header, value],
            progress_bars: vec![health, mana],
            ..DialogState::empty("UberBar".to_string(), None)
        };
        let (controls, _) = dialog.positioned_controls().expect("positioned");
        let rect_of = |kind: PositionedControlKind| {
            controls.iter().find(|c| c.kind == kind).map(|c| c.rect).unwrap()
        };

        let header = rect_of(PositionedControlKind::Label(0));
        let value = rect_of(PositionedControlKind::Label(1));
        let health = rect_of(PositionedControlKind::ProgressBar(0));
        let mana = rect_of(PositionedControlKind::ProgressBar(1));

        // The value label sits immediately right of the header (its own
        // left=0 is the offset added past the header's right edge).
        assert_eq!(value.0, header.0 + header.2);
        // Mana stacks directly beneath health (anchor_top + its own top=3).
        assert_eq!(mana.1, health.1 + health.3 + 3.0);
    }

    #[test]
    fn implicit_flow_stacks_vitals_below_the_label_column() {
        // UberBar's real bug: a label column and the vitals background BOTH
        // anchor_left to the doll, but the vitals block has NO anchor_top, so
        // raw anchors piled it on top of the labels. Wrayth flows same-column
        // siblings in document order — the vitals must land BELOW the labels.
        use crate::data::ui_state::PositionedControlKind;
        let lbl = |id: &str, anchor_top: Option<&str>| DialogLabel {
            id: id.to_string(),
            value: id.to_string(),
            justify: None,
            layout: Some(DialogControlLayout {
                top: Some(5),
                left: Some(5),
                width: Some(50),
                height: Some(15),
                anchor_left: Some("ubinjury".to_string()),
                anchor_top: anchor_top.map(str::to_string),
                ..Default::default()
            }),
        };
        // Doll skin (the shared anchor_left target), 150 tall.
        let doll = DialogSkin {
            id: "ubinjury".to_string(),
            name: "InjuriesPanel".to_string(),
            controls: vec![],
            layout: Some(DialogControlLayout {
                top: Some(5),
                left: Some(5),
                width: Some(100),
                height: Some(150),
                align: Some("nw".to_string()),
                ..Default::default()
            }),
        };
        // Two-row label column (ublog top, ubhour anchor_top=ublog), then a
        // vitals background with anchor_left=doll but NO anchor_top.
        let ubbars = DialogProgressBar {
            id: "ubbars".to_string(),
            value: 0,
            text: String::new(),
            layout: Some(DialogControlLayout {
                top: Some(3),
                left: Some(5),
                width: Some(100),
                height: Some(15),
                anchor_left: Some("ubinjury".to_string()),
                ..Default::default()
            }),
        };

        let dialog = DialogState {
            skins: vec![doll],
            display_labels: vec![lbl("ublog", None), lbl("ubhour", Some("ublog"))],
            progress_bars: vec![ubbars],
            ..DialogState::empty("UberBar".to_string(), None)
        };
        let (controls, _) = dialog.positioned_controls().expect("positioned");
        let rect_of = |k: PositionedControlKind| {
            controls.iter().find(|c| c.kind == k).map(|c| c.rect).unwrap()
        };
        let ublog = rect_of(PositionedControlKind::Label(0));
        let ubhour = rect_of(PositionedControlKind::Label(1));
        let ubbars = rect_of(PositionedControlKind::ProgressBar(0));

        // ubhour stacks below ublog (explicit chain).
        assert!(ubhour.1 > ublog.1, "ubhour below ublog");
        // The vitals bar flows below the LAST label (ubhour), not at the top.
        assert!(
            ubbars.1 >= ubhour.1 + ubhour.3,
            "vitals ({}) must sit below the label column bottom ({})",
            ubbars.1,
            ubhour.1 + ubhour.3
        );
    }

    #[test]
    fn no_layout_means_flow_mode() {
        let plain = DialogButton {
            id: "ok".to_string(),
            label: "OK".to_string(),
            command: String::new(),
            is_close: true,
            is_radio: false,
            selected: false,
            autosend: false,
            group: None,
            layout: None,
        };
        let dialog = dialog_with(vec![plain], Vec::new());
        assert!(dialog.positioned_controls().is_none());
    }
}
