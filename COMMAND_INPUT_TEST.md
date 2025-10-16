# Command Input Migration Test Plan

## Implementation Summary
Command input has been migrated from `CommandInputConfig` to `WindowDef` like all other windows.

## Changes Made
1. Added command_input to `default_windows()` in `src/config.rs` with constraints:
   - `min_rows: Some(1)`
   - `max_rows: Some(5)`
   - `min_cols: Some(20)`

2. Added command_input to `defaults/layout.toml`

3. Updated `App::new()` to find command_input from windows array with fallback

4. Updated render loop to get command_input position from windows array (two locations)

5. Added deletion prevention in `.deletewindow` command

6. Added duplicate prevention in `.createwindow` command

7. Updated `.editinput` command to use windows array with fallback

8. Maintained backwards compatibility with fallback to old `command_input` field

## Test Cases

### ✓ Test 1: Application Launches
- **Action**: Run `cargo run`
- **Expected**: Application launches successfully with command_input rendered at bottom
- **Status**: NEEDS TESTING

### ✓ Test 2: Mouse Move Operation
- **Action**: Click and drag command_input title bar
- **Expected**: Command input window moves with mouse
- **Status**: NEEDS TESTING

### ✓ Test 3: Mouse Resize Operation
- **Action**: Click and drag command_input borders/corners
- **Expected**: Command input resizes (respecting min/max constraints)
- **Status**: NEEDS TESTING

### ✓ Test 4: .lockwindows Compatibility
- **Action**:
  1. Type `.lockwindows`
  2. Try to move/resize command_input
- **Expected**:
  - Command input should be locked
  - Mouse operations should be blocked
  - System message: "Window is locked - cannot move/resize"
- **Status**: NEEDS TESTING

### ✓ Test 5: .unlockwindows Compatibility
- **Action**:
  1. Type `.lockwindows`
  2. Type `.unlockwindows`
  3. Try to move/resize command_input
- **Expected**:
  - Command input should be unlocked
  - Mouse operations should work again
- **Status**: NEEDS TESTING

### ✓ Test 6: Deletion Prevention
- **Action**: Type `.deletewindow command_input`
- **Expected**: System message "Cannot delete command_input - it is required for the application"
- **Status**: NEEDS TESTING

### ✓ Test 7: Duplicate Prevention
- **Action**: Try to create another command_input window
- **Expected**: System message "Cannot create duplicate command_input - one already exists"
- **Status**: NEEDS TESTING

### ✓ Test 8: .editinput Command
- **Action**: Type `.editinput`
- **Expected**: Window editor opens with command_input configuration
- **Status**: NEEDS TESTING

### ✓ Test 9: Layout Save/Load
- **Action**:
  1. Move/resize command_input
  2. Type `.savelayout test_cmd`
  3. Restart application
  4. Type `.loadlayout test_cmd`
- **Expected**: Command input position/size restored from saved layout
- **Status**: NEEDS TESTING

### ✓ Test 10: .baseline and .resize Commands
- **Action**:
  1. Type `.baseline`
  2. Resize terminal
  3. Type `.resize`
- **Expected**: Command input resizes proportionally with other windows
- **Status**: NEEDS TESTING

## Next Steps After Testing
Once all tests pass:
1. Remove `CommandInputConfig` struct entirely from `src/config.rs`
2. Remove `command_input` field from `Layout` struct
3. Clean up all fallback code
4. Update CLAUDE.md to reflect the change
