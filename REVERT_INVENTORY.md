# Revert Inventory - October 15, 2025

## Summary
Reverted entire codebase from commit `8d0f4fb` (latest) back to commit `7b98caa` (Oct 13 ~5-6am) to fix text artifact bug.

**Backup location**: `backup-before-revert` branch contains all reverted work.

---

## Lost Commits (14 total)

### 1. **0e28511** - feat: Add configurable compass colors and command input scrolling
**THIS COMMIT INTRODUCED THE BUG**

Features in this commit:
- ❌ Configurable compass colors (active/inactive exits)
- ❌ Horizontal scrolling for command input (long commands)
- ❌ Text color support for hands and progress bars
- ❌ Bloodpoints parsing fixes
- ❌ Border padding calculations (THE BUG SOURCE)
- ❌ Content alignment feature for text windows
- ❌ Window editor dynamic tab order
- ❌ Created 3 NEW FILES: color_form.rs, color_palette_browser.rs, color_picker.rs
- ❌ Added COMPANION_APP.md documentation (705 lines)

Files changed: 32 files, +5485/-402 lines

### 2. **2574821** - Feat: Multi-command macro support
- ❌ Multi-command macros with ; separator
- ❌ sleep/wait commands in macros with delays
- ❌ Cleaned up \r from movement macros

Files changed: src/app.rs, src/config.rs

### 3. **15675e5** - Fix: UI consistency improvements
- ❌ Color and spell color form consistency fixes

### 4. **8f5ad0b** - Fix: Normalize Active Spells category
- ❌ Active Spells category normalization
- ❌ Progress bar truncation clarification

### 5. **b669446** - feat: Add adaptive layout infrastructure
- ❌ Percentage-based positioning system
- ❌ Layout conversion infrastructure

### 6. **c546711** - feat: Add CommandInputConfig percentage fields
- ❌ CommandInputConfig percentage support
- ❌ LayoutInfo metadata

### 7. **bbc872f** - feat: Add percentage support to WindowConfig
- ❌ WindowConfig percentage positioning
- ❌ Layout conversion utilities

### 8. **4ae7a9e** - feat: Add command input percentage-based positioning
- ❌ Command input uses percentage positioning

### 9. **303d425** - feat: Add command input mouse operations
- ❌ Command input resize with mouse
- ❌ Command input move with mouse

### 10. **533f31e** - feat: Add terminal resize debouncing
- ❌ Debouncing for terminal resize events
- ❌ Performance improvement for resizing

### 11. **56679d1** - feat: Add layout conversion commands
- ❌ Dot commands for layout conversion
- ❌ .convertlayout and related commands

### 12. **51115b8** - fix: Command input mouse operations for full-width mode
- ❌ Mouse operations work correctly in full-width mode

### 13. **2159868** - fix: Prevent layout overflow
- ❌ Layout overflow prevention with percentage positioning

### 14. **8d0f4fb** - fix: Command input snap-to-bottom
- ❌ Command input snaps to bottom correctly with percentage layouts

---

## Uncommitted Work from Today (Oct 15)

### New Documentation (9 files, ~5500 lines)
- ❌ documentation/Colors.md (734 lines)
- ❌ documentation/Commands.md (705 lines)
- ❌ documentation/Development.md (686 lines)
- ❌ documentation/Highlights.md (815 lines)
- ❌ documentation/Keybinds.md (648 lines)
- ❌ documentation/Mouse.md (545 lines)
- ❌ documentation/Overview.md (280 lines)
- ❌ documentation/Performance.md (591 lines)
- ❌ documentation/Quickstart.md (401 lines)
- ❌ documentation/Settings.md (496 lines)
- ❌ documentation/Windows.md (902 lines)

### New Files/Features from Today
- ❌ TODO_AUDIT_2025-10-13.md (180 lines) - TODO audit
- ❌ Claude's Plan (679 lines) - Planning document
- ❌ spell_color_browser.rs (290 lines) - NEW FILE
- ❌ spell_color_form.rs (531 lines) - NEW FILE
- ❌ Updated Cargo.toml with 3 new dependencies

### Selection Highlighting Feature (Today's main work)
- ❌ Text selection with background color highlighting
- ❌ Selection state tracking
- ❌ Scroll-aware selection coordinates
- ❌ create_spans_with_selection() method
- ❌ relative_row_to_absolute_line() method
- ❌ Updated render_with_focus() signatures across widgets

### Major Refactoring from Today
- ❌ src/app.rs: +1200 lines (major selection feature additions)
- ❌ src/config.rs: +600 lines (restructuring)
- ❌ src/ui/mod.rs: +100 lines (new widget exports)
- ❌ src/ui/window_manager.rs: +100 lines (selection support)

---

## Total Loss Summary

**Commits lost**: 14
**New files created (now lost)**: 6
- color_form.rs
- color_palette_browser.rs
- color_picker.rs
- spell_color_browser.rs
- spell_color_form.rs
- COMPANION_APP.md

**Documentation lost**: 11 comprehensive docs (~6200 lines total)
**Code changes lost**: ~16,000 lines added/modified across 51 files

---

## Features That Must Be Re-implemented

### Critical (Blocking features)
1. **Multi-command macros** - Users may depend on ; separator
2. **Command input horizontal scrolling** - UX improvement for long commands
3. **Percentage-based layouts** - Major layout system upgrade

### High Priority (Valuable features)
4. **Text selection with highlighting** - Today's main feature
5. **Terminal resize debouncing** - Performance improvement
6. **Layout conversion commands** - User-facing feature
7. **Compass configurable colors** - User customization
8. **Command input mouse operations** - UX improvement

### Medium Priority (Nice to have)
9. **Bloodpoints parsing fixes** - Game-specific fix
10. **Text color for hands/progress bars** - Customization
11. **Content alignment** - May be related to bug, skip for now
12. **Border padding changes** - Related to bug, skip
13. **Color palette browser** - New UI feature
14. **Spell color browser/form** - New UI features

### Documentation (Can wait)
15. All 11 documentation files - Comprehensive but can be regenerated

---

## Recovery Plan

1. ✅ **DONE**: Revert to clean state (commit 7b98caa)
2. Cherry-pick commits one by one, testing for artifacts after each
3. Skip commit 0e28511 entirely (contains the bug)
4. Re-implement features from 0e28511 individually without the buggy changes
5. Re-implement text selection highlighting from today
6. Regenerate documentation last

---

## How to Restore Work

All reverted work is in `backup-before-revert` branch:

```bash
# View what was lost
git diff 7b98caa backup-before-revert

# Cherry-pick specific commits (test after each!)
git cherry-pick <commit-hash>

# Or restore specific files
git show backup-before-revert:path/to/file > path/to/file

# View all commits on backup branch
git log 7b98caa..backup-before-revert
```

---

## Root Cause of Bug - IDENTIFIED! ✅

**ACTUAL CULPRIT**: Commit **2574821** (Multi-command macro support)

Testing results:
- ✅ `0e28511` - CLEAN (no artifacts) - Originally suspected, but innocent!
- ❌ `2574821` - BUGGY (artifacts appear) - This is the culprit!

**Files changed in buggy commit**:
- `src/app.rs` - Changed command execution to async, Ctrl+C behavior
- `src/config.rs` - Removed `\r` from default keybind macros

**Key changes that may have caused the bug**:
1. Async command execution with `tokio::spawn`
2. Ctrl+C now calls `self.command_input.clear()` instead of exiting
3. Command splitting and async iteration over command list

**The bug manifested as**: Text artifacts (partial words) appearing during scrolling, especially at the end of lines like "ths: east, south, northwest"

**Tags created**:
- `v0.1.0-clean-render` → 7b98caa (last known good)
- `v0.1.0-buggy-render` → 2574821 (first bad commit)
