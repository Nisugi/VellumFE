# Terminal Position Persistence & Reapply (Design)

## Scope
- Persist terminal window position/size (pixels) and terminal size (rows/cols) into the autosaved layout.
- Reapply the saved position on startup where the host allows (classic Windows console). Skip safely when the host refuses (Windows Terminal/ConPTY).

## Capture (on exit / autosave)
- Query rows/cols via `crossterm::terminal::size()` (always captured).
- Query window rect:
  - Windows: `GetConsoleWindow` + `GetWindowRect` (only works on classic conhost; WT returns a hidden host).
  - Unix: CSI queries `ESC[13t` (position) + `ESC[14t` (pixel size) when supported.
- Windows-only monitor metadata:
  - `MonitorFromWindow` + `GetMonitorInfoW(MONITORINFOEXW)` to capture monitor device name (`\\.\DISPLAYn`) and monitor rect.
- Store in layout autosave under `[terminal_position]` (fields: x, y, width, height, cols, rows, monitor_device/rect*).

## Preservation
- Auto-resize path preserves any existing `terminal_position` when swapping in the baseline layout so autosave keeps the monitor/position info.
- `.reload` (`reload_windows`) keeps the existing `terminal_position` if the reloaded layout lacks one and applies it to both the live layout and baseline.

## Apply (startup)
- Called before entering raw mode.
- Host detection (Windows):
  - If `WT_SESSION`/`WT_PROFILE_ID` is set, skip reposition and log (WT/ConPTY cannot be moved from inside).
- Coordinate resolution (Windows):
  - Start with saved x/y/width/height.
  - If monitor info is present: compute relative offset to the saved monitor; clamp the target to that monitor’s bounds.
  - Restore the window (`ShowWindow(SW_RESTORE)`) to break snap/maximize.
  - Move/resize:
    - `SetWindowPos` to target coords/size.
    - Verify actual rect; if mismatch, try `MoveWindow` at target; if still wrong and target differs from raw saved coords, try raw saved coords.
- Unix: send CSI 3/4/8 to set position, pixel size (if available), and rows/cols.

## Failure handling
- Missing/invalid position: skip apply (no crash).
- WT/ConPTY: explicit skip with info log.
- If moves are blocked (snap/FancyZones/external WM), we log warnings; we cannot override an external manager that refuses moves.

## Data model (`terminal_position` fields)
- `x`, `y`, `width`, `height` (pixels; optional)
- `cols`, `rows` (chars; required)
- Windows-only (optional): `monitor_device`, `monitor_left`, `monitor_top`, `monitor_right`, `monitor_bottom`

## Limitations / Notes
- Only classic conhost is moveable; Windows Terminal is not controlled from the child process.
- Windows may reject moves that collide with snap/FancyZones; we try restore + clamp + raw fallback but cannot force it.
- When a window spans monitors, we clamp to the saved monitor; if the host still rejects, manual intervention may be needed.
