# MODULE KNOWLEDGE BASE

**Generated:** 2025-12-29T19:56:00Z
**Parent:** src/window_position/

## OVERVIEW
Cross-platform terminal window position persistence per character profile.

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Platform detection | `mod.rs` | `create_positioner()` returns platform-specific impl |
| Windows Terminal support | `windows.rs` | Process tree walking to find WT window handle |
| Linux X11 implementation | `linux.rs` | Uses xdotool, fails gracefully on Wayland |
| macOS AppleScript | `macos.rs` | Detects terminal app from $TERM_PROGRAM |
| Config persistence | `storage.rs` | Saves to `~/.vellum-fe/{character}/window.toml` |

## CONVENTIONS

- **Platform trait pattern**: `WindowPositioner` trait with `get_position()`, `set_position()`, `get_screen_bounds()`
- **Extension trait**: `WindowPositionerExt` provides visibility checking and screen clamping
- **Thread-local handles**: Positioner is NOT Send+Sync - main thread only
- **Graceful fallback**: Returns None if platform unsupported, doesn't crash
- **Minimum visibility**: 100x100 pixels required for window to be considered "visible"

## ANTI-PATTERNS

- **Wayland positioning**: Impossible - Wayland prohibits window positioning by design
- **Process tree depth**: Windows Terminal search limited to 10 parent levels to prevent infinite loops
- **Command dependency**: Linux requires xdotool, detects via shell command lookup
- **JSON parsing avoidance**: macOS display parsing uses simple string search instead of serde_json

## PLATFORM SPECIFICS

- **Windows**: Supports both ConHost and Windows Terminal via Win32 APIs
- **Linux**: X11 only, requires xdotool installation
- **macOS**: Uses osascript with AppleScript, detects from TERM_PROGRAM environment
- **Wayland**: Not supported by design