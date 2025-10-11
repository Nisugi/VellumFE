# Drag-and-Drop Implementation Design

## Overview

This document outlines the design for implementing Wrayth-style clickable links, context menus, and drag-and-drop functionality in VellumFE. This system allows players to interact with game objects using mouse clicks and drag operations.

## Background

### How Wrayth's System Works

1. **Links in Game Data**: The game sends links in XML format:
   ```xml
   <a exist="73772244" noun="pendant">stylized gold warcat pendant</a>
   ```
   - `exist` attribute = unique object ID (exist_id)
   - `noun` attribute = object's noun for targeting
   - Text content = display name

2. **Command Lookup (cmdlist1.xml)**: Maps coordinates to commands:
   ```xml
   <cli coord="2524,1613" menu="look @" command="look #" menu_cat="1"/>
   <cli coord="2524,1581" menu="drop @" command="drop #" menu_cat="2"/>
   ```
   - `coord` = "mi coords" (magic number, y-coordinate) - identifies command type
   - `menu` = text shown in context menu (@ = display name placeholder)
   - `command` = command to send (# = exist_id placeholder, % = secondary item)
   - `menu_cat` = category for menu organization

3. **Context Menu**: **Left-click (down + up)** on link sends `_menu #exist_id counter` to game
   - `counter` is a request ID for correlation (like a password to verify the response is yours)
   - Game responds with: `<menu id="counter" path="" cat_list="..."><mi coord="2524,1613"/><mi coord="2524,1581"/>...`
   - Look up each `<mi coord="..."/>` value in cmdlist1.xml to get menu entries
   - **Placeholder substitution**: `@` = noun (display text), `#` = `#exist_id` (command with # symbol!)
   - **Menu structure**: Group by `menu_cat`
     - Categories with ≤4 items: Show directly in menu
     - Categories with 5+ items: Create submenu with ">" indicator (e.g., "roleplay >")
     - Subcategories with `-` separator: Create nested submenus (e.g., `5_roleplay-swear` → "swear >" under "roleplay")
     - Missing coords in cmdlist: Skip or show as generic entry
   - Display popup menu at mouse position
   - **Menu options are rendered as clickable links** (reuse link rendering!)
   - Clicking a menu option link sends the corresponding command

4. **Drag-and-Drop**: **Left-click + drag** on link (movement distinguishes from click)
   - Mouse down on link → track position
   - If mouse moves beyond threshold: **DRAG MODE** (not a click)
   - Drag link to link: `_drag #source_exist_id #target_exist_id`
   - Drag link to empty area: Send drop command for the item
   - Example use cases: "pour potion on companion", "give item to NPC", "combine items"

## Architecture

### New Components

#### 1. `src/links.rs` - Link Management

```rust
pub struct Link {
    pub exist_id: String,
    pub noun: String,
    pub display_text: String,
    pub window_name: String,
    pub line_index: usize,
    pub col_start: usize,
    pub col_end: usize,
}

pub struct LinkManager {
    links: HashMap<String, Vec<Link>>,  // window_name -> links in that window
    cmdlist: CmdList,
}

impl LinkManager {
    pub fn new(cmdlist_path: Option<PathBuf>) -> Result<Self>;
    pub fn add_link(&mut self, link: Link);
    pub fn get_link_at_position(&self, window: &str, line: usize, col: usize) -> Option<&Link>;
    pub fn clear_window_links(&mut self, window: &str);
    pub fn get_commands_for_link(&self, exist_id: &str) -> Vec<ContextMenuItem>;
}
```

#### 2. `src/cmdlist.rs` - Command List Parser

```rust
pub struct CmdListEntry {
    pub coord: String,  // "2524,1613"
    pub menu: String,   // "look @"
    pub command: String, // "look #"
    pub menu_cat: String, // "1"
}

pub struct CmdList {
    entries: Vec<CmdListEntry>,
    coord_map: HashMap<String, Vec<usize>>, // coord -> indices in entries
}

impl CmdList {
    pub fn load(path: &PathBuf) -> Result<Self>;
    pub fn get_entries_by_coord(&self, coord: &str) -> Vec<&CmdListEntry>;
    pub fn substitute_placeholders(&self, template: &str, display_name: &str, exist_id: &str) -> String;
}
```

#### 3. `src/ui/context_menu.rs` - Context Menu Widget

```rust
pub struct ContextMenuItem {
    pub display: String,        // Menu text (with @ substituted with noun)
    pub command: String,        // Command to send (with # substituted with #exist_id)
    pub category: String,       // Base category (e.g., "5_roleplay")
    pub subcategory: Option<String>, // Subcategory if present (e.g., "swear" from "5_roleplay-swear")
    pub bounds: Rect,           // Click detection bounds for this item
}

pub struct MenuCategory {
    pub name: String,           // Display name (e.g., "roleplay")
    pub items: Vec<ContextMenuItem>,
    pub is_submenu: bool,       // True if 5+ items (shows ">")
}

pub struct ContextMenu {
    categories: Vec<MenuCategory>,  // Top-level categories (sorted by cat number 0-13)
    position: (u16, u16),
    max_width: usize,
    active_submenu: Option<String>, // Currently open submenu category
    active_nested_submenu: Option<String>, // Currently open nested submenu (subcategory)
}

impl ContextMenu {
    pub fn new(items: Vec<ContextMenuItem>, position: (u16, u16)) -> Self;
    pub fn render(&self, frame: &mut Frame, area: Rect);

    // Menu items are links! Check if click position hits any item
    pub fn get_item_at_position(&self, x: u16, y: u16) -> Option<&ContextMenuItem>;

    // Handle submenu clicks
    pub fn get_submenu_at_position(&self, x: u16, y: u16) -> Option<&str>;

    // Open submenu (renders another popup)
    pub fn open_submenu(&mut self, category: &str);

    // Close active submenus
    pub fn close_submenus(&mut self);
}
```

#### 4. Drag-Drop State in `app.rs`

```rust
pub struct DragDropState {
    pub source_link: Link,
    pub start_position: (u16, u16),
    pub current_position: (u16, u16),
}

pub struct App {
    // ... existing fields ...
    link_manager: LinkManager,
    context_menu: Option<ContextMenu>,
    drag_drop_state: Option<DragDropState>,
    config: Config,  // includes link/dragdrop settings
}
```

### Modified Components

#### `src/parser.rs`

Add link detection to XML parser:

```rust
pub enum ParsedElement {
    // ... existing variants ...
    Link {
        exist_id: String,
        noun: String,
        text: String,
    },
    LinkEnd,
}

impl XmlParser {
    fn handle_link_tag(&mut self, tag: &BytesStart) -> Result<Vec<ParsedElement>> {
        let exist_id = Self::get_attribute(tag, b"exist")?;
        let noun = Self::get_attribute(tag, b"noun")?;

        Ok(vec![ParsedElement::Link {
            exist_id: exist_id.to_string(),
            noun: noun.to_string(),
            text: String::new(),
        }])
    }
}
```

#### `src/ui/text_window.rs`

Track link positions while rendering:

```rust
pub struct TextWindow {
    // ... existing fields ...
    current_link: Option<(String, String, usize)>, // (exist_id, noun, start_col)
}

impl TextWindow {
    pub fn start_link(&mut self, exist_id: String, noun: String) {
        self.current_link = Some((exist_id, noun, self.current_line_buffer.len()));
    }

    pub fn end_link(&mut self) -> Option<Link> {
        if let Some((exist_id, noun, start_col)) = self.current_link.take() {
            let end_col = self.current_line_buffer.len();
            let display_text = self.current_line_buffer[start_col..end_col]
                .iter()
                .map(|seg| &seg.text)
                .collect::<String>();

            Some(Link {
                exist_id,
                noun,
                display_text,
                window_name: self.name.clone(),
                line_index: self.lines.len(),
                col_start: start_col,
                col_end: end_col,
            })
        } else {
            None
        }
    }

    pub fn render_with_links(&self, area: Rect, buf: &mut Buffer, link_config: &LinkConfig) {
        // Render text as normal, but apply link styling (color, underline)
        // to characters within link ranges
    }
}
```

#### `src/config.rs`

Add link/dragdrop configuration:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkConfig {
    #[serde(default = "default_links_enabled")]
    pub enabled: bool,

    // No cmdlist_path needed - always uses ~/.vellum-fe/cmdlist1.xml
    // (extracted from embedded default on first run)

    #[serde(default = "default_link_color")]
    pub link_color: String,  // Hex color for links

    #[serde(default = "default_link_underline")]
    pub link_underline: bool,

    #[serde(default = "default_context_menu_enabled")]
    pub context_menu_enabled: bool,

    #[serde(default = "default_click_drag_threshold")]
    pub click_drag_threshold: u16,  // Pixels to distinguish click from drag (default: 5)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DragDropConfig {
    #[serde(default = "default_dragdrop_enabled")]
    pub enabled: bool,

    // Drag-and-drop requires Ctrl by default (safety)
    // No modifier needed for context menu clicks
    // No modifier needed for text selection (default drag behavior)
    // Shift+drag = native terminal selection (passthrough)
}

pub struct Config {
    // ... existing fields ...
    pub links: LinkConfig,
    pub dragdrop: DragDropConfig,
}
```

## Implementation Phases

### Phase 1: Link Detection and Rendering

**Goal**: Parse and display clickable links with styling

1. Add `Link` parsing to `parser.rs`:
   - Detect `<a exist="..." noun="...">` tags
   - Emit `ParsedElement::Link` and `ParsedElement::LinkEnd`

2. Modify `TextWindow` to track link positions:
   - `start_link()` / `end_link()` methods
   - Store link metadata with line/column positions

3. Render links with styling:
   - Apply link color and underline to link text
   - Use existing styled text rendering infrastructure

4. Add `LinkConfig` to configuration system

**Testing**: Links should be visually distinguishable from regular text

### Phase 2: cmdlist.xml Parsing

**Goal**: Load and parse command lookup table

1. Create `src/cmdlist.rs`:
   - Parse XML with `quick-xml`
   - Build `coord_map` for fast lookups
   - Implement placeholder substitution (@ = display, # = exist_id, % = secondary)

2. Auto-detect cmdlist.xml location:
   - Check common paths (e.g., `C:\Gemstone\SIMU\Wrayth\settings\GS4\`)
   - Fall back to config-specified path
   - Handle missing file gracefully (disable context menus)

3. Create `LinkManager` to bridge links and cmdlist

**Testing**: Verify cmdlist.xml loads correctly and lookups work

### Phase 3: Context Menus (Left-Click, No Drag)

**Goal**: Display context menu on left-click (without drag)

1. **Distinguish click from drag**:
   - Mouse down on link → start tracking position
   - If mouse up at same position (or within small threshold): **CLICK** → show menu
   - If mouse moves beyond threshold: **DRAG** → start drag-and-drop (Phase 4)
   - Threshold: ~5 pixels or ~1 character cell (configurable)

2. Create `src/ui/context_menu.rs`:
   - Render popup menu at mouse position
   - **Menu items are clickable links** (reuse link rendering code!)
   - Track bounds for each menu item for click detection
   - Calculate menu position (avoid screen edges)

3. Send `_menu` command on click:
   - Format: `_menu #exist_id counter` (counter = request ID for correlation)
   - Parse response: `<menu id="counter" path="" cat_list="1 2 3 4 5 6 7 8 9 10 11 12 13"><mi coord="2524,1613"/><mi coord="2524,1581"/>...`
   - Verify `id` matches our `counter` (ensures response is for our request)
   - Extract all `<mi coord="..."/>` tags from response
   - Look up each coord in cmdlist1.xml to get menu entries (menu text, command, category)
   - **Handle missing coords**: Skip entries not found in cmdlist (game adds new commands faster than cmdlist updates)
   - Build menu items from matching cmdlist entries
   - **Substitute placeholders correctly**:
     - `@` = noun (e.g., "look @" → "look pendant")
     - `#` = "#exist_id" **WITH the # symbol** (e.g., "look #" → "look #73772244")
   - **Group by category**:
     - Parse `menu_cat` for base category and subcategory (e.g., "5_roleplay-swear" → base=5_roleplay, sub=swear)
     - Categories with ≤4 items: Show all directly in main menu
     - Categories with 5+ items: Create submenu (e.g., "roleplay >")
     - Subcategories (with `-`): Create nested submenu under parent

4. Handle menu rendering and selection:
   - Render categories in order (0-13, top to bottom)
   - Categories with ≤4 items: Show all items directly
   - Categories with 5+ items: Show as "{category_name} >" (clickable submenu trigger)
   - **All menu items are clickable links** (reuse link rendering!)
   - Track bounds for each item and submenu trigger
   - **Handle submenu clicks**: Open submenu popup at appropriate position
   - **Handle item clicks**: Send command to game server
   - **Handle nested submenus**: If subcategory clicked, open another nested popup
   - Close menu on final selection, click outside, or Escape key

**Testing**: Click on links shows menu, clicking menu options executes commands

### Phase 4: Drag-and-Drop

**Goal**: Drag link to link or empty area

1. **Detect drag start** (from Phase 3 click/drag detection):
   - Mouse down on link → start tracking position
   - Mouse moves beyond threshold (NOT a click) → enter drag mode
   - Store source link and start position
   - Optional: Check for modifier key if `require_modifier = true`

2. Track drag in progress:
   - Update current mouse position on MouseEventKind::Drag
   - Render visual feedback (highlight source link, show "dragging" cursor/indicator)
   - Handle window scrolling during drag (auto-scroll near top/bottom edges)
   - Cancel on Escape key

3. Detect drop target on mouse up:
   - Check if drop position intersects another link
   - If yes: `_drag #source_exist_id #target_exist_id`
   - If no: Send drop command (need to research command format - likely `drop #exist_id`)
   - Clear drag state

4. Modifier key handling (REQUIRED for safety):
   - **Ctrl must be held** to drag-and-drop links (prevents accidental drops)
   - If Ctrl not held: drag = text selection (not drag-and-drop)
   - Visual indicator when Ctrl held and hovering over link (cursor change?)
   - Clear feedback that drag-and-drop is active

**Testing**: Drag link to link sends `_drag` command, drag to empty drops item

### Phase 5: Text Selection Integration

**Goal**: Coordinate drag-drop with text selection

**Modifier Key Strategy** (SIMPLIFIED!):
- **No modifier + click** = Context menu (most common)
- **No modifier + drag** = Text selection (VellumFE-aware, respects window boundaries)
- **Ctrl + drag on link** = Drag-and-drop (requires deliberate action - prevents accidents!)
- **Shift + drag** = Native terminal selection (passthrough to terminal)

1. Implement interaction logic:
   - Mouse down on link → check if Ctrl is held
   - If Ctrl held + drag: **drag-and-drop mode** (Phase 4)
   - If no Ctrl + drag: **text selection mode** (window-aware)
   - If no Ctrl + no drag: **context menu** (Phase 3)
   - If Shift held: **disable VellumFE handling** (let terminal handle selection)

2. Prevent conflicts:
   - Ctrl+drag on link = drag-drop only (no selection)
   - No modifier drag = selection only (no drag-drop)
   - Shift = terminal selection (VellumFE ignores mouse events)

3. Config options:
   - `dragdrop.enabled` = false by default (opt-in for safety)
   - `selection.enabled` = true by default
   - `selection.respect_window_boundaries` = true by default

**Testing**: All three modes work independently with correct triggers

### Phase 6: Performance and Polish

**Goal**: Optimize and handle edge cases

1. Lazy link detection:
   - Only track links in active/focused window
   - Clear old links on window scroll/resize

2. Limit link cache size:
   - Max N links per window (e.g., 1000)
   - LRU eviction when limit reached

3. Handle edge cases:
   - Malformed exist_id values
   - Missing cmdlist.xml
   - Network lag during menu operations
   - Very long link text (wrapping)
   - Multiple items with same noun

4. Config defaults:
   - Disable drag-drop by default (safety)
   - Require modifier key by default
   - Enable links by default (read-only, low risk)

**Testing**: Stress test with many links, various terminal sizes, lag

## Configuration Examples

### Default Configuration

```toml
[links]
enabled = true
link_color = "#5599ff"
link_underline = true
context_menu_enabled = true
# cmdlist_path auto-detected

[dragdrop]
enabled = false  # Disabled by default for safety
modifier_key = "shift"
selection_modifier = "ctrl"
require_modifier = true  # Must hold Shift to drag links
```

### Experienced User Configuration

```toml
[links]
enabled = true
link_color = "#aaffff"
link_underline = false
context_menu_enabled = true
cmdlist_path = "C:\\Gemstone\\SIMU\\Wrayth\\settings\\GS4\\cmdlist1.xml"

[dragdrop]
enabled = true
modifier_key = "shift"
selection_modifier = "ctrl"
require_modifier = true
```

## Risks and Mitigations

### Risk: Accidental Item Drops

**Impact**: High - Players could accidentally drop expensive items

**Mitigation**:
- Disable drag-drop by default
- Require modifier key (Shift) by default
- Config option to disable entirely
- Visual confirmation before drop (future enhancement)

### Risk: Performance Impact

**Impact**: Medium - Link tracking could slow rendering

**Mitigation**:
- Lazy link detection (active window only)
- Limit link cache size
- Option to disable links in specific windows
- Benchmark and optimize hot paths

### Risk: cmdlist.xml Compatibility

**Impact**: Medium - Game might change format or coords

**Mitigation**:
- Graceful handling of missing/malformed cmdlist.xml
- Version detection (use `timestamp` attribute?)
- Fall back to basic link support without context menus

### Risk: Scrolling During Drag

**Impact**: Low - Could be disorienting or buggy

**Mitigation**:
- Auto-scroll when mouse near window edge
- Maintain drag state across scroll events
- Visual feedback during drag
- Cancel drag if window loses focus

## Open Questions

1. ✅ **Menu Response Format**: RESOLVED
   - Send: `_menu #exist_id counter` (counter is request correlation ID)
   - Response: `<menu id="counter" path="" cat_list="1 2 3 4 5 6 7 8 9 10 11 12 13"><mi coord="2524,1613"/><mi coord="2524,1581"/>...`
   - `id` attribute matches the `counter` we sent (request/response correlation)
   - `cat_list` appears to be a list of available categories
   - Each `<mi coord="..."/>` is looked up in cmdlist1.xml
   - `path` attribute purpose unclear (may be for nested menus or breadcrumbs)

3. **Click vs Drag Threshold**: What's a good movement threshold?
   - **Proposed**: 5 pixels or 1 character cell width
   - Should be small enough for steady clicking but large enough to prevent accidental drags
   - Should be configurable in config file

4. **Drop Command Format**: What command is sent when dragging to empty area?
   - Cmdlist has: `<cli coord="2524,1581" menu="drop @" command="drop #" menu_cat="2"/>`
   - So likely just `drop #exist_id` substituting the exist_id
   - May be context-dependent (e.g., in shop = sell, in container = put) - needs verification

5. ✅ **Menu Categories**: RESOLVED
   - Categories sorted by number (0-13, top to bottom)
   - Parse `menu_cat` for base and subcategory (e.g., "5_roleplay-swear" → base="5_roleplay", sub="swear")
   - **≤4 items in category**: Show all directly in main menu
   - **5+ items in category**: Create submenu with ">" indicator (e.g., "roleplay >")
   - Subcategories with `-` separator: Create nested submenu under parent
   - Category display names extracted from suffix (e.g., "5_roleplay" → "roleplay")
   - Missing coords: Skip gracefully (cmdlist may be outdated)

6. **Placeholder Substitution**: CLARIFIED
   - ✅ `@` = noun (display text in menu: "look @" → "look pendant")
   - ✅ `#` = "#exist_id" **INCLUDING the # symbol** (command: "look #" → "look #73772244")

7. ✅ **Missing cmdlist Entries**: RESOLVED
   - Game evolves faster than cmdlist updates
   - **Solution**: Skip entries not found in cmdlist, log warning
   - Example: `analyze` is always present but not in older cmdlist files

8. ✅ **Dialog Commands**: RESOLVED (Phase 3: skip, later phase: implement)
   - Some commands trigger `_dialog` (e.g., `speak to`, `sing to`, `recite to`)
   - Pattern: `_dialog #exist_id <dialog_type>` opens input dialog
   - Game responds with `<openDialog>` containing input box UI
   - User enters text, clicks OK, sends formatted command
   - **Phase 3**: Skip dialog commands (filter out `_dialog` from menu)
   - **Later Phase**: Implement dialog widget for these commands
   - **All dialogs follow same pattern**: Title, prompt label, input field, OK/Cancel buttons
   - Examples with screenshots documented:
     - `speak to` → `_dialog #exist_id say` → Dialog: "Speech to say:" → `say ::#exist_id <text>`
     - `sing to` → `_dialog #exist_id sing` → Dialog: "Text to sing:" → `sing ::#exist_id <text>`
     - `recite to` → `_dialog #exist_id recite` → Dialog: "Text to recite:" → `recite ::#exist_id <text>`
     - `submit bug report` → `_dialog #exist_id bugitem` → Dialog with instructions + multi-line input → `bugitem #exist_id;<text>`
   - Dialog implementation is simpler than expected - all follow same structure

## Future Enhancements

- **Drag Confirmation**: Show popup "Drop X on Y?" before executing
- **Drop History**: Track recent drops for undo functionality
- **Custom Commands**: Allow users to add custom context menu items
- **Macro Integration**: Bind drag-drop operations to macros
- **Multi-Select**: Drag multiple items at once (Ctrl+Click to select)
- **Visual Feedback**: Animate drag-drop with particle effects
- **Smart Targeting**: Auto-suggest drop targets based on item type

## Priority Assessment

Based on user needs and implementation complexity:

**P1 - High Priority** (Core functionality):
- Phase 1: Link detection and rendering
- Phase 2: cmdlist.xml parsing
- Phase 3: Context menus

**P2 - Medium Priority** (Full feature):
- Phase 4: Drag-and-drop
- Phase 5: Text selection integration

**P3 - Low Priority** (Polish):
- Phase 6: Performance and edge cases

**Recommendation**: Implement phases 1-3 first to provide clickable links and context menus. This gives 80% of the value with lower risk than full drag-and-drop. Add phases 4-5 in a later iteration after user feedback.
