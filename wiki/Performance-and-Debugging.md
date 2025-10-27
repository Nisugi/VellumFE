# Performance & Debugging

VellumFE ships with extensive instrumentation for diagnosing rendering, parsing, and layout issues. Use this page to understand the available tooling.

## Performance Overlay

- **Toggle**: Bind `toggle_performance_stats` (default `f12` in the stock keymap) or add a custom keybind.
- **Position**: Controlled by `[ui] perf_stats_x`, `perf_stats_y`, `perf_stats_width`, `perf_stats_height`.
- **Metrics** (from `PerformanceStats`):
  - Average/max render time and UI render time (ms).
  - Text wrapping average (µs).
  - Event processing average/max (µs) and total events processed.
  - Frames per second (inverse of frame time).
  - Bytes sent/received per second.
  - Elements parsed per second.
  - Estimated memory usage (lines buffered × 200 bytes heuristic).
  - Total windows & lines buffered.

Use this overlay to spot slowdowns when experimenting with large highlight sets or complex layouts.

## Logging

- Logs write to `~/.vellum-fe/<character>/debug.log` (or `debug_<character>.log` when using `--character`).
- Default log level is `DEBUG`; ratatui output never pollutes the TUI because logging is routed to a file.
- To add more detail, export `RUST_LOG=debug` before launching from a shell (Linux/macOS) or set `setx RUST_LOG debug` in PowerShell. The application uses `tracing` and `tracing-subscriber`.
- Key log messages:
  - Connection lifecycle: `Connecting to Lich`, `Connected successfully`, `Connection closed by server`.
  - Menu interactions: requests/responses for context menus.
  - Layout operations: baseline captures, resize deltas, autosave status.
  - Highlight and sound errors: regex compilation issues, missing sound files.

## Testing Commands

Use the built-in dot commands to simulate data without a live game session:

| Command | Purpose |
| --- | --- |
| `.randominjuries` / `.randomcountdowns` / `.randomprogress` / `.randomcompass` | Randomly populate widgets for visual inspection. |
| `.indicatoron` / `.indicatoroff` | Force status indicators to specific values. |
| `.setprogress <window> <current> <max>` | Manually drive a progress bar. |
| `.setcountdown <window> <seconds>` | Start a countdown timer. |
| `.testmenu <exist_id> [noun]` | Request a context menu from Lich to confirm clickable links. |
| `.testhighlight <name> <text>` | Validate highlight patterns and show match results. |

These commands never send data to the game unless explicitly noted (e.g., `.testmenu` sends `_menu` to Lich).

## Layout Validation

Run the validator when you add new layouts or modify existing ones:

```
vellum-fe.exe --validate-layout layouts/my-layout.toml --sizes 100x30,120x40
```

or run tests:

```
TEST_LAYOUTS="layouts/my-layout.toml" cargo test -- tests::validate_env_specified_layouts --nocapture
```

The validator reports overlaps, out-of-bounds placement, min/max violations, and top-stack continuity warnings.

## Sound Diagnostics

- Sound playback issues generate warnings in `debug.log`.
- Ensure the output device is available; Rodio uses the default system output.
- Sounds may be skipped silently if the cooldown is active—set a large `sound_cooldown_ms` to avoid rapid repeats.

## XML & Parser Insights

- Enable `RUST_LOG=debug` to watch parsed elements flow through the system; messages include event matches and link creation.
- `XmlParser` tracks nested tag state (colors, presets, styles) and logs when tags are unbalanced. Use this when debugging odd styling.

## Network Troubleshooting

- Confirm Lich is listening: from PowerShell, run `Test-NetConnection localhost -Port 8000`.
- Use `.menu` → Settings → Connection to verify host/port.
- `debug.log` records connection attempts and closures; repeated failures usually indicate a wrong port or mismatched `--links` expectation on the server side.

## Recovering From Broken Layouts

- Delete `~/.vellum-fe/<character>/layout.toml` to discard a problematic autosave.
- Re-run `.loadlayout layout` followed by `.resize`.
- If the UI refuses to draw, check `debug.log` for panic messages (Rust panics are rare thanks to error handling, but malicious layout edits can violate constraints).

## When To File Issues

Include these artifacts when opening an issue:

- `debug.log`
- `config.toml`, `colors.toml`, `highlights.toml`, and relevant `layouts/*.toml`
- Terminal size (columns × rows) and OS
- VellumFE version (`vellum-fe.exe --version`)

This data mirrors what the application itself relies on and accelerates reproduction.
