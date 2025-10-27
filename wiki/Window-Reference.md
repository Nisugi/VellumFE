# Window Reference

Each layout window instantiates a widget from `src/ui/`. This reference summarizes the purpose of every widget, the XML elements it consumes, and notable configuration fields from `WindowDef`.

## Text Window (`widget_type = "text"`)

- **Source**: `src/ui/text_window.rs`
- **Streams**: Any text stream listed in `streams`.
- **Features**:
  - Styled spans with layered foreground/background colors, bold, spell, monsterbold, and link semantics.
  - Timestamp rendering (per-window override via `show_timestamps`).
  - Highlight engine: regex (`highlight_regexes`) and literal (`Aho-Corasick`) patterns.
  - Mouse selection (`SelectionState`), copy-to-clipboard, clickable links (context menus via `CmdList`).
  - LaunchURL handling: `<LaunchURL>` tags open the official Play.net URL in your default browser.
  - Search mode with regex matches and navigation.
- **Key Config Fields**: `buffer_size`, `show_border`, `border_style/color/sides`, `title`, `content_align`, `background_color`, `transparent_background`.

## Tabbed Text Window (`"tabbed"`)

- **Source**: `src/ui/tabbed_text_window.rs`
- **Streams**: Defined per tab (`tabs = [{ name, stream, show_timestamps }]`).
- **Features**:
  - Independent scrollback per tab.
  - Unread badge (`tab_unread_prefix`, `tab_unread_color`).
  - Configurable tab bar position (`top`/`bottom`) and colors.
  - Per-tab timestamp toggle for mixing timestamped and plain streams.
  - Dot commands `.switchtab`, `.movetab`, `.tabcolors`, `.addtab`, `.removetab`.
- **Config Extras**: `tab_active_color`, `tab_inactive_color`, `tab_unread_color`, `tab_bar_position`.

## Command Input (`"command_input"`)

- **Source**: `src/ui/command_input.rs`
- **Streams**: N/A; optional to treat as standard window for layout editing.
- **Features**:
  - Horizontal scrolling for long commands.
  - Command history (`history.txt`) with prefix repeat.
  - Auto-completion for dot commands, window names, palette entries.
  - Tab cycling through completion candidates.
  - Border styling and background color.
- **Config Fields**: `title`, `show_border`, `border_style`, `border_color`, `background_color`, `min_command_length`.

## Progress Bar (`"progress"`)

- **Source**: `src/ui/progress_bar.rs`
- **XML**: `<progressBar id="health" value="325" max="346" text="...">`
- **Features**:
  - Optional transparent background.
  - `numbers_only` strips descriptive text, leaving `current/max`.
  - `set_progress` handles manual adjustments (`.setprogress`).
  - Color overrides via `bar_color`, `bar_background_color`, `text_color`.
- **Config**: `progress_id`, `bar_color`, `bar_background_color`, `text_color`, `content_align`, `numbers_only`.

## Countdown (`"countdown"`)

- **Source**: `src/ui/countdown.rs`
- **XML**: `<roundTime value="5">`, `<castTime value="7">`, event patterns.
- **Features**:
  - Character fill animation decreasing per second.
  - Icon (default `?`, per-window `countdown_icon`).
  - Colors match progress bars (`bar_color`, `bar_background_color`).
  - Dot command `.setcountdown`.

## Indicator (`"indicator"`)

- **Source**: `src/ui/indicator.rs`
- **XML**: `<statusIndicator id="poisoned" state="on/off">`
- **Features**:
  - Displays textual or bar indicator states.
  - `.indicatoron` / `.indicatoroff` for testing.
  - `indicator_colors` array (inactive + active palette).

## Compass (`"compass"`)

- **Source**: `src/ui/compass.rs`
- **XML**: `<compass><dir>N</dir> ...`
- **Features**:
  - Shows available exits with configurable active/inactive colors.
  - Supports content alignment and background color control.
- **Config**: `compass_active_color`, `compass_inactive_color`, `background_color`.

## Injury Doll (`"injury_doll"`)

- **Source**: `src/ui/injury_doll.rs`
- **XML**: `<injuryImage id="head" name="Injury2">`
- **Features**:
  - ASCII articulation of injury/scar severity per body part.
  - Custom palette per severity level: `injury_default_color`, `injury1_color`, `injury2_color`, `injury3_color`, `scar1_color`, `scar2_color`, `scar3_color`.
  - `.randominjuries` toggles random severities for testing.

## Hands (`"hands"` and `"hand"`)

- **Source**: `src/ui/hands.rs`, `src/ui/hand.rs`
- **XML**: `<leftHand>`, `<rightHand>`, `<spellHand>`
- **Features**:
  - Combined widget (`hands`) shows both hands; individual `hand` displays a single slot.
  - `hand_icon` allows prefix labels (`L:`, `R:`, `S:`).
  - `text_color`, `background_color`, `content_align` customization.

## Dashboard (`"dashboard"`)

- **Source**: `src/ui/dashboard.rs`
- **XML**: `<component id="health" value="300/350">`, built-in indicator updates.
- **Features**:
  - Grid/horizontal/vertical layouts (`dashboard_layout`).
  - `dashboard_indicators` define which stats to display and their order.
  - Supports hiding inactive entries and adjusting spacing.
- **Config**: `dashboard_layout`, `dashboard_indicators`, `dashboard_spacing`, `dashboard_hide_inactive`.

## Active Effects (`"active_effects"`)

- **Source**: `src/ui/active_effects.rs`
- **XML**: `<activeEffect category="Buffs" id="601" text="...">`
- **Features**:
  - Scrollable list with durations and colored status bars.
  - `effect_category` selects which bucket to display (`ActiveSpells`, `Buffs`, `Debuffs`, `Cooldowns`).
  - `effect_default_color` sets fallback colors.
  - `.togglespellid` / `.toggleeffectid` flips between spell IDs and names.
  - When `visible_count` is omitted, the list auto-fits to the window height; set it explicitly to cap the number of rows.

## Targets (`"targets"`)

- **Source**: `src/ui/targets.rs`
- **XML**: `<streamPush id="target">`, `<component id="currentTarget" value="...">`
- **Features**:
  - Scrollable target panel with status, health estimate, stances.
  - Auto-sorts by priority.
  - `.randomprogress` seeds data for manual inspection.

## Players (`"players"`)

- **Source**: `src/ui/players.rs`
- **XML**: `<component id="roomPlayer" value="Name|Status">`
- **Features**:
  - Scrollable roster of players in the room.
  - Integrates with highlights and clickable links.

## Inventory (`"inventory"`)

- **Source**: `src/ui/inventory_window.rs`
- **Streams**: `inv`
- **Features**:
  - Buffers the entire inventory stream and only re-renders when the contents change, preventing flicker from repeated `inv` updates.
  - Each refresh replaces the snapshot—there is no scrollback beyond the current list of worn/carried items.
  - Manual word wrapping plus scrollback so long inventories remain readable; right-click menus continue to work via the link cache.
  - Honors border visibility/style/color; ignores `buffer_size` because history is not retained.

## Room (`"room"`)

- **Source**: `src/ui/room_window.rs`
- **Streams**: `room`
- **Features**:
  - Component-aware layout: description and objects share the lead line, while players and exits render on their own rows.
  - Maintains wrapped text and scrollback for the current room snapshot, resetting automatically on each `room` stream push.
  - Preserves clickable links inside components; parser state is isolated per component to prevent color bleed.
  - Window title updates with the latest navigation ID (`<nav rm>`), optional Lich room ID extracted from the main stream, and any subtitle sent via `<streamWindow>`.
  - Sprite data is accepted for future expansion but not displayed yet.

## Spacer (`"spacer"`)

- **Source**: `src/ui/spacer.rs`
- **Purpose**: Allocates empty space for layout balancing. Supports background colors and transparency but no content.

## Performance Stats Overlay

- **Widget**: `PerformanceStatsWidget` (not a layout window; toggled by keybind).
- **Metrics**: FPS, render timings, text wrap cost, network throughput, memory estimates, parser throughput.
- **Placement**: Controlled by `[ui] perf_stats_*` fields.
- **Keybind**: `toggle_performance_stats` action; assign via keybind editor if you want quick access.

## Popup Windows

The following widgets are popups, not layout windows, but they form part of the user experience:

- `HighlightFormWidget`, `HighlightBrowser`
- `KeybindFormWidget`, `KeybindBrowser`
- `WindowEditor`, `SettingsEditor`
- `ColorPaletteBrowser`, `ColorForm`, `UIColorsBrowser`
- `SpellColorBrowser`, `SpellColorFormWidget`
- `PopupMenu` (context menus for clickable links)
- `ColorPicker`

All popups share a consistent look (cyan border, draggable header, black background) defined in `POPUP_STYLE_GUIDE.md` and orchestrated via `InputMode`.
