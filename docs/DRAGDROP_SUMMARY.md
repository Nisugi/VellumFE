# Drag-and-Drop Feature: Analysis and Recommendations

## Quick Summary

**Feasibility**: ✅ **Highly Feasible** - All required components are implementable with existing tech stack

**Complexity**: 🟡 **Medium-High** - Requires new subsystems but no fundamental blockers

**Risk**: 🟠 **Medium** - Primary concern is accidental item drops (mitigatable with good defaults)

**Recommendation**: **Implement in phases, starting with links and context menus**

---

## What You Asked For

> "It would be really cool to be able to emulate Wrayth's drag and drop functionality"

Wrayth's system provides:
1. **Clickable links** - Objects in game appear as colored/underlined links
2. **Context menus** - Right-click on object shows actions (look, get, attack, etc.)
3. **Drag-and-drop** - Drag object to object for interactions (pour potion on friend, give item to NPC)

---

## Key Findings

### 1. How It Works (Technical)

**Game Data**: The game already sends link data in XML:
```xml
<a exist="73772244" noun="pendant">stylized gold warcat pendant</a>
```

**Command Lookup**: Wrayth uses `cmdlist1.xml` (589 entries) to map actions:
```xml
<cli coord="2524,1613" menu="look @" command="look #" menu_cat="1"/>
<cli coord="2524,1581" menu="drop @" command="drop #" menu_cat="2"/>
<cli coord="2524,1578" menu="start dragging @" command="drag #" menu_cat="4"/>
```

**Interaction Flow**:
1. **Left-click** link (down + up, no drag) → sends `_menu #exist_id counter` to game
   - `counter` is a request correlation ID (verifies response is for your request)
2. Game responds: `<menu id="counter" path="" cat_list="1 2 3 4 5 6 7 8 9 10 11 12 13"><mi coord="2524,1613"/><mi coord="2524,1581"/>...`
3. Verify `id="counter"` matches our request counter
4. Extract all `<mi coord="..."/>` tags from the response
5. Look up each coord in cmdlist1.xml to get menu entries (menu text, command, category)
   - Skip coords not found (cmdlist may be outdated)
6. **Substitute placeholders**: `@` = noun (display), `#` = "#exist_id" (command, **keep # symbol!**)
7. **Group by category** and build hierarchical menu:
   - Parse `menu_cat` for base and subcategory (e.g., "5_roleplay-swear")
   - ≤4 items: Show directly in main menu
   - 5+ items: Create submenu with ">" (e.g., "roleplay >")
   - Subcategories with `-`: Create nested submenu
8. Menu options are **clickable links** → click option sends command
9. Submenu triggers show ">" → click opens another popup
10. **Left-click + drag** link → sends `_drag #source_id #target_id`

**Important**: NO right-click involved! Click vs drag distinguished by movement threshold.

### 2. What's Needed

**New Components**:
- Link parser for `<a exist="...">` tags in XML
- cmdlist.xml parser and lookup system
- Context menu popup widget
- Link position tracking (line, column, window)
- Drag-and-drop state machine (start, drag, drop, cancel)
- Mouse click/drag detection on links

**Modified Components**:
- Parser: Add link detection
- TextWindow: Track link positions, render with styling
- App: Handle mouse events on links, manage menus/drag state
- Config: Add link/dragdrop settings

**Dependencies**:
- Already have: `quick-xml` for parsing, `crossterm` for mouse events
- May need: `arboard` for clipboard (if implementing text selection too)

### 3. Design Decisions

#### **Critical: Text Selection vs Drag-Drop**

You identified the key conflict:
> "We'd also need to consider the effects of drag and drop along with drag to select yeah? Maybe requiring a modifier key to do drag and drop and no modifier to select text. Or vice versa, probably shift? The reason I say default to text selection is don't want to have a bad time by accidentally dragging your $5000 weapon to the ground and losing it accidentally."

**Recommendation**: Default to text selection (SAFER)
- **No modifier** + Ctrl/Alt drag = text selection
- **Shift + drag on link** = drag-and-drop
- Config option to disable drag-drop entirely
- Config option to swap defaults (advanced users)

#### **Scrolling During Drag**

You asked:
> "We'd need to think about things like the text scrolling during drag and drop?"

**Solution**:
- Auto-scroll when mouse near window top/bottom edge
- Maintain drag state across scroll events
- Visual feedback (highlight source link)
- Cancel drag on Escape or window focus loss

---

## Implementation Strategy

### Phase 1: Links & Styling (Low Risk, High Value)
**Effort**: 1-2 days
**Value**: Players can see what objects are interactable

- Parse `<a exist="..." noun="...">` tags
- Render links with color/underline
- Track link positions per window
- Config: `link_color`, `link_underline`, `links_enabled`

**User Experience**:
```
You see a gold pendant and a silver ring here.
         ^^^^^^^^^^^^      ^^^^^^^^^^^^
         (colored/underlined links)
```

### Phase 2: cmdlist.xml Parsing (Foundation)
**Effort**: 1 day
**Value**: Enables context menus

- Bundle cmdlist1.xml in binary (embed with `include_bytes!`)
- Extract to `~/.vellum-fe/cmdlist1.xml` on first run
- Parse with quick-xml, build coord → command lookup table
- Handle placeholders (@ = display, # = exist_id, % = secondary)
- Re-extract if file missing/corrupted (self-healing)

### Phase 3: Context Menus (80% of Value)
**Effort**: 2-3 days
**Value**: Left-click (no drag) interactions work

- **Distinguish click from drag**: Movement threshold (~5 pixels)
- Create popup menu widget with **clickable link items**
- Detect left-click on links (down + up at same position)
- Generate correlation counter, send `_menu #exist_id counter` to game
- Parse response: `<menu id="counter" path="" cat_list="..."><mi coord="..."/><mi coord="..."/>...`
- Verify response `id` matches our `counter` (request/response correlation)
- Extract all `<mi coord="..."/>` tags from response
- Look up each coord in cmdlist1.xml to get menu entries
- Build context menu from matching entries, substitute placeholders
- Execute selected command by clicking menu item

**User Experience**:
```
Left-click (no drag) on "gold pendant":
┌─────────────────┐
│ look pendant    │ ← Clickable links!
│ get pendant     │
│ examine pendant │
│ appraise pendant│
└─────────────────┘
```

### Phase 4: Drag-and-Drop (Remaining 20%)
**Effort**: 2-3 days
**Value**: Drag interactions work

- Detect drag start: mouse down + movement beyond threshold (NOT a click)
- Track drag in progress (MouseEventKind::Drag events)
- Visual feedback during drag (highlight source link)
- Detect drop target (link or empty area)
- Send `_drag` command for link-to-link
- Send drop command for link-to-empty
- Handle scroll during drag (auto-scroll near edges)
- Optional modifier key (Shift) for safety

**User Experience**:
```
Left-click and DRAG "healing potion" to "wounded companion"
(mouse moves > threshold → drag mode, not click mode)
→ Sends: _drag #potion_id #companion_id
→ Game: "You pour a healing potion on your companion"
```

### Phase 5: Safety & Polish
**Effort**: 1-2 days
**Value**: Prevent accidents, smooth UX

- Default drag-drop disabled (opt-in)
- Require modifier key by default
- Config for each window (disable in combat log)
- Handle edge cases (malformed IDs, lag, wrapping)
- Performance optimization (lazy link detection)

---

## Configuration Approach

### Default (Safe for New Users)
```toml
[links]
enabled = true  # Low risk - read-only
link_color = "#5599ff"
link_underline = true
context_menu_enabled = true

[dragdrop]
enabled = false  # Disabled by default (safety)
require_modifier = true  # Must hold Shift to drag
modifier_key = "shift"

[selection]  # Text selection feature
selection_modifier = "ctrl"  # Ctrl+drag = select text
```

### Advanced (Experienced Users)
```toml
[links]
enabled = true
link_color = "#aaffff"
link_underline = false

[dragdrop]
enabled = true  # Opt-in after reading docs
require_modifier = true
modifier_key = "shift"
```

---

## Risks & Mitigations

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Accidental item drops | HIGH | Medium | Disabled by default, require modifier key |
| Performance degradation | Medium | Low | Lazy detection, limit cache size, per-window toggle |
| cmdlist.xml changes | Medium | Low | Graceful fallback, version detection |
| Conflict with text selection | Medium | High | Different modifier keys, clear config options |
| Scrolling bugs during drag | Low | Medium | Auto-scroll, maintain state, cancel on focus loss |

---

## Open Questions (Need Research)

1. **Menu Response Format**: How does game respond to `_menu #exist_id counter`?
   - May need to capture Wrayth network traffic
   - Alternative: Use cmdlist.xml exclusively (no game interaction)

2. **Drop Command**: What command for drag-to-empty-area?
   - Likely `drop #exist_id` but should verify

3. **Coordinate Meaning**: What is the "2524" prefix in cmdlist coords?
   - Appears constant, Y varies (1541-2236)
   - May be arbitrary IDs vs semantic meaning

---

## Recommendation: Phased Rollout

### Sprint 1 (MVP - Links Only)
**Goal**: Clickable links with visual styling
**Time**: 1-2 days
**Risk**: Low
**User Value**: Medium (see what's interactable)

### Sprint 2 (Context Menus)
**Goal**: Right-click links for command menu
**Time**: 3-4 days (cumulative)
**Risk**: Low-Medium
**User Value**: High (80% of Wrayth functionality)

**Stop here and gather feedback before proceeding**

### Sprint 3 (Drag-and-Drop)
**Goal**: Full drag-and-drop with safety controls
**Time**: 5-7 days (cumulative)
**Risk**: Medium
**User Value**: High (remaining 20% + quality of life)

### Sprint 4 (Text Selection Integration)
**Goal**: Coordinate with text selection feature
**Time**: 2-3 days additional
**Risk**: Low
**User Value**: High (prevents conflicts)

---

## Bottom Line

**Feasibility**: ✅ Totally doable with your current stack

**Complexity**: 🟡 Medium - Not trivial, but well-scoped work

**Safety**: 🟢 Can be made safe with good defaults (disabled by default, modifier keys)

**Value**: ⭐⭐⭐⭐⭐ High - This is a killer feature that sets VellumFE apart

**Recommendation**: **Start with Phases 1-3 (links + context menus)** to get 80% of the value with lower risk. Get user feedback before implementing full drag-and-drop in Phase 4.

---

## Next Steps

1. ✅ Research complete (this document)
2. ✅ Design document created (`DRAGDROP_DESIGN.md`)
3. ✅ TODO updated with comprehensive task breakdown
4. ⏭️ Decision: Start implementation or gather more info?
5. ⏭️ If starting: Begin with Phase 1 (link detection and rendering)

**Question for you**: Do you want to proceed with implementation, or investigate the open questions first (e.g., capture Wrayth network traffic to see `_menu` response format)?
