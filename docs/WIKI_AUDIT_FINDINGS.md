# VellumFE Wiki Audit Findings

## Critical Issues (Incorrect Information)

### Home.md
- **Line 43**: Says "Highlights - Regex-based text highlighting (coming soon)"
  - **ACTUAL**: Highlights are FULLY implemented with Aho-Corasick optimization, sound support, and management UI
- **Line 44**: Says "Keybinds - Custom keyboard shortcuts (partial support)"
  - **ACTUAL**: Keybinds are FULLY implemented with 24 built-in actions, macro support, and management UI
- **Missing links**: Text-Selection, Highlight-Management, Keybind-Management pages not linked

### Configuration-Guide.md
- **Line 177**: Says "Highlighting is not yet fully implemented. This section describes the planned configuration format."
  - **ACTUAL**: Highlights ARE fully implemented - see defaults/config.toml and Highlight-Management.md
- **Line 211**: Says "Keybinds are not yet fully implemented. This section describes the planned configuration format."
  - **ACTUAL**: Keybinds ARE fully implemented - see defaults/config.toml and Keybind-Management.md
- **Line 73 & 86**: Still references `mouse_mode_toggle_key = "F11"` which is DEAD CODE
  - **ACTUAL**: This config option does nothing, F11 does nothing, mouse works immediately
- **Line 628**: Another reference to dead `mouse_mode_toggle_key`

### Quick-Start.md
- **Line 130**: Says "vellum-fe has 40+ pre-built widgets"
  - **ACTUAL**: 44 widgets (correct, no issue)
- **Line 144**: Says "Mouse mode is enabled by default - No need to toggle"
  - **ACTUAL**: Correct! But contradicts other wiki pages that mention F11

### Mouse-and-Keyboard.md
- Multiple F11 references throughout (at least 8 locations)
- **Line 51**: F11 listed as "Toggle mouse mode on/off"
  - **ACTUAL**: F11 does nothing, dead code
- **Line 55-57**: Says "Keybind support is planned but not yet implemented"
  - **ACTUAL**: Fully implemented
- **Line 63**: "Mouse operations require mouse mode to be enabled. Press F11"
  - **ACTUAL**: Mouse works immediately, no toggle needed
- **Lines 492-507**: Entire "Toggling Mouse Mode" section about F11
  - **ACTUAL**: Dead code section, should be deleted
- **Lines 508-519**: "When to Use Mouse Mode" section
  - **ACTUAL**: Irrelevant, mouse always works
- **Line 574**: Quick reference table lists F11
  - **ACTUAL**: Should show F12 for performance stats instead

### Troubleshooting.md
- Likely has F11 references (found 3 instances in grep)
- **Line 168**: Mentions "Press F11 to enable mouse mode"
- **Line 512 & 515**: More F11 references

## Dead Code in Codebase

### src/app.rs
- `mouse_mode_enabled: bool` field (line 52) - NEVER CHECKED, only toggled
- `toggle_mouse_mode()` function (lines 2248-2262) - NEVER CALLED
- `is_toggle_key()` function (lines 2264-2285) - NEVER CALLED

### src/config.rs
- `mouse_mode_toggle_key` field in UiConfig (line 203) - DEAD, never used
- `default_mouse_mode_toggle_key()` function (lines 558-560) - DEAD
- Call to `default_mouse_mode_toggle_key()` in defaults (line 2826)

### defaults/config.toml
- `mouse_mode_toggle_key = "F11"` (line 9) - DEAD CONFIG OPTION

## Incomplete/Missing Content

### Mouse-and-Keyboard.md
- **Line 470**: Says "Ctrl+R (not yet implemented)"
  - Need to verify: Is search implemented or not?

### Text-Selection.md
- **Line 60**: Says "No visual highlighting" is planned feature
  - Need to clarify: Is this actually needed/wanted?

## Recommendations

1. **Remove ALL F11/mouse_mode references** from:
   - Home.md
   - Configuration-Guide.md (2 locations)
   - Mouse-and-Keyboard.md (8+ locations)
   - Troubleshooting.md (3 locations)
   - Quick-Start.md (if any)

2. **Update feature status** in:
   - Home.md: Change highlights to "fully implemented"
   - Home.md: Change keybinds to "fully implemented"
   - Configuration-Guide.md: Remove "not yet implemented" notes for highlights & keybinds
   - Mouse-and-Keyboard.md: Remove "planned" status for keybinds

3. **Remove dead code**:
   - src/app.rs: Remove `mouse_mode_enabled`, `toggle_mouse_mode()`, `is_toggle_key()`
   - src/config.rs: Remove `mouse_mode_toggle_key` field and default function
   - defaults/config.toml: Remove `mouse_mode_toggle_key` line

4. **Add missing links** to Home.md:
   - Text-Selection.md
   - Highlight-Management.md
   - Keybind-Management.md

5. **Verify claims**:
   - Is search (Ctrl+R) actually implemented?
   - Widget count: 44 templates confirmed (40+ is accurate)
   - Are there other "planned" features that are actually done?

## Action Items

- [ ] Fix Home.md (highlights, keybinds status, missing links)
- [ ] Fix Configuration-Guide.md (remove "not implemented" notes, remove F11 refs, fix examples)
- [ ] Fix Mouse-and-Keyboard.md (remove entire F11 sections, update keybind status)
- [ ] Fix Troubleshooting.md (remove F11 references)
- [ ] Fix Quick-Start.md (verify accuracy)
- [ ] Remove dead code from src/app.rs
- [ ] Remove dead code from src/config.rs
- [ ] Remove dead config from defaults/config.toml
- [ ] Verify search implementation status
- [ ] Full pass on ALL wiki pages for other outdated content
