# Lost Features - Reverted from backup-before-revert to 0e28511

## Current State
- **Current commit**: `0e28511` (Oct 13, 2025 6:58am) ✅ CLEAN - No artifacts
- **Backup branch**: `backup-before-revert` (contains all lost work)
- **Bug identified in**: `2574821` (Multi-command macro support) ❌ BUGGY

## Summary
- **13 commits lost** (after 0e28511)
- **2 new source files lost** (spell_color_browser.rs, spell_color_form.rs)
- **11 documentation files lost** (~6,200 lines)
- **Total code changes lost**: +11,084 lines added, -1,059 lines removed across 29 files

---

## Lost Commits (in chronological order, oldest to newest)

### 1. ❌ **2574821** - Multi-command macro support with ; separator and sleep/wait
**THIS COMMIT IS BUGGY - DO NOT CHERRY-PICK AS-IS**

Features:
- Multi-command macros with `;` separator (e.g., "ready weapon;attack")
- `sleep` and `wait` commands with delays (e.g., "incant 401;wait 3;cast")
- Async command execution with `tokio::spawn`
- Ctrl+C now clears command input instead of exiting
- Removed `\r` from default numpad movement macros

**Bug**: Introduces text artifacts during scrolling

Files changed: `src/app.rs`, `src/config.rs`

### 2. ✅ **15675e5** - UI consistency improvements for color and spell color forms
- Color form and spell color form consistency fixes
- Minor UI improvements

Files changed: `src/ui/color_form.rs`, `src/ui/highlight_browser.rs`

### 3. ✅ **8f5ad0b** - Normalize Active Spells category and clarify progress bar truncation
- Active Spells category normalization
- Progress bar truncation clarification

Files changed: `src/ui/progress_bar.rs`, `src/ui/settings_editor.rs`

### 4. ✅ **b669446** - Adaptive layout infrastructure - ratio-distributed positioning
⚠️ **IMPORTANT**: This is a RATIO-DISTRIBUTED system, NOT percentage-based!
- Ratio-distributed positioning system foundation
- Windows get space distributed based on ratios, not fixed percentages
- Layout conversion infrastructure
- Support for responsive layouts

Files changed: Multiple (config, app, window_manager)

**Implementation note**: When reimplementing, DO NOT use percentages. The system distributes available space based on ratios between windows.

### 5. ✅ **c546711** - CommandInputConfig ratio fields and LayoutInfo metadata
- CommandInputConfig supports ratio-distributed positioning
- LayoutInfo metadata for tracking layout state

Files changed: `src/config.rs`, `src/app.rs`

### 6. ✅ **bbc872f** - Ratio support to WindowConfig and conversion
- WindowConfig ratio-distributed positioning fields
- Layout conversion utilities between absolute and ratio

Files changed: `src/config.rs`

### 7. ✅ **4ae7a9e** - Command input ratio-distributed positioning
- Command input widget uses ratio-distributed positioning
- Responsive command input placement

Files changed: `src/app.rs`, `src/config.rs`

### 8. ✅ **303d425** - Command input mouse operations (resize/move)
- Command input can be resized with mouse
- Command input can be moved with mouse
- Click and drag support

Files changed: `src/app.rs`

### 9. ✅ **533f31e** - Terminal resize debouncing
- Debouncing for terminal resize events
- Performance improvement during window resizing
- Prevents layout thrashing

Files changed: `src/app.rs`

### 10. ✅ **56679d1** - Layout conversion commands
- `.convertlayout` command to convert absolute to percentage
- `.converttoabsolute` command
- `.converttopercent` command
- Dot commands for layout manipulation

Files changed: `src/app.rs`

### 11. ✅ **51115b8** - Command input mouse operations for full-width mode
- Fix mouse operations when command input is full-width
- Correct hit detection for full-width mode

Files changed: `src/app.rs`

### 12. ✅ **2159868** - Prevent layout overflow when using ratio positioning
- Layout overflow prevention
- Bounds checking for ratio-distributed layouts
- Prevents windows from going off-screen

Files changed: `src/app.rs`, `src/config.rs`

### 13. ✅ **8d0f4fb** - Command input snap-to-bottom for ratio layouts
- Command input snaps to bottom correctly with ratio-distributed positioning
- Fix for command input positioning edge case

Files changed: `src/app.rs`

---

## Lost Uncommitted Work (from today, Oct 15)

### New Documentation Files (11 files, ~6,200 lines)
All lost - need to be regenerated:
- ❌ `documentation/Colors.md` (734 lines) - Color system docs
- ❌ `documentation/Commands.md` (705 lines) - Command reference
- ❌ `documentation/Development.md` (686 lines) - Dev guide
- ❌ `documentation/Highlights.md` (815 lines) - Highlights guide
- ❌ `documentation/Keybinds.md` (648 lines) - Keybinds reference
- ❌ `documentation/Mouse.md` (545 lines) - Mouse operations guide
- ❌ `documentation/Overview.md` (280 lines) - Project overview
- ❌ `documentation/Performance.md` (591 lines) - Performance guide
- ❌ `documentation/Quickstart.md` (401 lines) - Quick start guide
- ❌ `documentation/Settings.md` (496 lines) - Settings reference
- ❌ `documentation/Windows.md` (902 lines) - Window system guide

### New Source Files (2 files, ~821 lines)
- ❌ `src/ui/spell_color_browser.rs` (290 lines) - Spell color browser widget
- ❌ `src/ui/spell_color_form.rs` (531 lines) - Spell color form widget

### Other Lost Files
- ❌ `Claude's Plan` (679 lines) - Planning document
- ❌ `TODO_AUDIT_2025-10-13.md` (180 lines) - TODO audit from today

### Major Code Refactoring from Today
- ❌ `src/app.rs`: +1,165 lines (text selection, spell colors, refactoring)
- ❌ `src/config.rs`: +1,512 lines (restructuring, new features)
- ❌ `src/ui/mod.rs`: +101 lines (new widget exports)
- ❌ `src/ui/window_manager.rs`: +111 lines (selection support, spell colors)
- ❌ `src/ui/color_form.rs`: Major refactoring (~600 line changes)

### Text Selection Feature (Today's Main Work)
**Completely lost** - needs full re-implementation:
- Text selection with background color highlighting
- Mouse drag to select text
- Copy to clipboard on release
- Scroll-aware selection coordinates
- `SelectionState` tracking
- `create_spans_with_selection()` method
- `relative_row_to_absolute_line()` method
- Updated `render_with_focus()` signatures across all widgets
- Integration with window_manager and app event loop

### Cargo Dependencies Added
- ❌ 3 new dependencies in `Cargo.toml`
- ❌ 104 line changes in `Cargo.lock`

---

## Features Prioritized for Recovery

### ⚠️ CRITICAL - Has Bug - Needs Fix
1. **Multi-command macros** (commit 2574821)
   - Feature is valuable but commit introduces artifacts
   - Need to identify specific buggy change in app.rs
   - Re-implement without the bug

### 🔴 HIGH PRIORITY - User-Facing Features
2. **Text selection with highlighting** ✅ **COMPLETED** (commit cca5b24)
   - Major UX feature
   - Re-implemented with scroll-aware coordinates
   - Now on feature/text-selection branch

3. **Ratio-distributed layouts** (commits b669446, c546711, bbc872f, 4ae7a9e)
   - Major layout system upgrade using ratio distribution (NOT percentage-based)
   - Responsive layout support
   - 4 commits build on each other

4. **Command input mouse operations** (commits 303d425, 51115b8)
   - Resize and move command input with mouse
   - User-facing feature

5. **Layout conversion commands** (commit 56679d1)
   - `.convertlayout` and related commands
   - User-facing utility

6. **Terminal resize debouncing** (commit 533f31e)
   - Performance improvement
   - Prevents layout thrashing

### 🟡 MEDIUM PRIORITY - Nice to Have
7. **Spell color browser/form** (today's work)
   - New UI widgets (821 lines)
   - Nice-to-have feature

8. **Layout overflow prevention** (commit 2159868)
   - Bounds checking for layouts
   - Complements ratio-distributed layouts

9. **Command input snap-to-bottom** (commit 8d0f4fb)
   - Edge case fix
   - Low impact

10. **UI consistency improvements** (commits 15675e5, 8f5ad0b)
    - Minor polish
    - Low impact

### 📝 LOW PRIORITY - Documentation
11. **All documentation files** (11 files, ~6,200 lines)
    - Can be regenerated
    - Not blocking development

---

## Recovery Strategy

⚠️ **CRITICAL WARNING**: Commits 4-7 (b669446, c546711, bbc872f, 4ae7a9e) implement a **RATIO-DISTRIBUTED** layout system, NOT a percentage-based system. When reviewing these commits before cherry-picking, verify they use ratio distribution. If they mistakenly use percentages, they must be rewritten to use ratios instead.

### Phase 1: Cherry-pick Safe Commits (Order matters!)
```bash
# Currently on: 0e28511 (clean render, last good commit)

# 1. UI improvements (safe, small)
git cherry-pick 15675e5  # UI consistency
git cherry-pick 8f5ad0b  # Active Spells normalization

# 2. Percentage layout system (in order, dependencies)
git cherry-pick b669446  # Adaptive layout infrastructure
git cherry-pick c546711  # CommandInputConfig percentage
git cherry-pick bbc872f  # WindowConfig percentage
git cherry-pick 4ae7a9e  # Command input percentage positioning

# 3. Command input features
git cherry-pick 303d425  # Mouse operations (resize/move)
git cherry-pick 533f31e  # Terminal resize debouncing
git cherry-pick 56679d1  # Layout conversion commands
git cherry-pick 51115b8  # Mouse ops full-width fix

# 4. Percentage layout fixes
git cherry-pick 2159868  # Layout overflow prevention
git cherry-pick 8d0f4fb  # Command input snap-to-bottom

# SKIP 2574821 for now (buggy - multi-command macros)
```

### Phase 2: Fix Multi-Command Macro Bug
1. Analyze `git diff 0e28511 2574821 -- src/app.rs`
2. Identify the specific change causing artifacts
3. Re-implement multi-command macros WITHOUT the buggy change
4. Test thoroughly for artifacts

### Phase 3: Re-implement Text Selection
1. Re-implement selection feature from scratch
2. Test with current codebase
3. Ensure no artifacts introduced

### Phase 4: Re-implement Spell Color Features
1. Recreate `spell_color_browser.rs` and `spell_color_form.rs`
2. Integrate with app and window_manager

### Phase 5: Regenerate Documentation
1. Regenerate all 11 documentation files
2. Update with any changes from recovery process

---

## Testing Checklist After Each Cherry-Pick

After each `git cherry-pick`, test for artifacts:
1. ✅ Build succeeds
2. ✅ Run application
3. ✅ Trigger fast scrolling text
4. ✅ Look for partial words/text fragments appearing
5. ✅ Scroll up and down
6. ✅ Check borders remain intact
7. ✅ No visual glitches

If artifacts appear, revert that commit and investigate before proceeding.

---

## Root Cause Analysis

**Bug introduced in**: Commit `2574821`

**Files changed**:
- `src/app.rs` (131 insertions, 76 deletions)
- `src/config.rs` (30 insertions, 30 deletions)

**Suspected causes** (need investigation):
1. Async command execution with `tokio::spawn`
2. Ctrl+C now calls `self.command_input.clear()` instead of exiting
3. Command cloning and async iteration timing
4. Possible race condition between command execution and rendering
5. Terminal buffer interaction with command_input.clear()

**Next step**: Examine the exact diff to pinpoint the problematic change.

---

## Backup Location

All lost work is safely stored in the `backup-before-revert` branch.

To view lost work:
```bash
# View all changes
git diff 0e28511 backup-before-revert

# View specific file
git show backup-before-revert:src/ui/spell_color_browser.rs

# View commit list
git log 0e28511..backup-before-revert --oneline
```

---

## Tags Created

- `v0.1.0-clean-render` → commit `7b98caa` (Oct 13 5am, before any features)
- `v0.1.0-buggy-render` → commit `2574821` (first commit with artifacts)
- Currently on → commit `0e28511` (last good commit with features)
