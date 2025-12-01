# VellumFE GUI Frontend Implementation Plan

**Author:** Planning session with Claude Code
**Date:** 2025-11-09
**Status:** Planning / Not Started
**Purpose:** Add GUI frontend using egui while maintaining TUI for SSH/remote usage

---

## Table of Contents
1. [Executive Summary](#executive-summary)
2. [Motivation](#motivation)
3. [Architecture Overview](#architecture-overview)
4. [Implementation Phases](#implementation-phases)
5. [Technical Details](#technical-details)
6. [Performance Analysis](#performance-analysis)
7. [Risk Assessment](#risk-assessment)
8. [Success Criteria](#success-criteria)
9. [Timeline](#timeline)
10. [References](#references)

---

## Executive Summary

Add a GUI frontend using **egui** (Rust GUI framework) while maintaining the existing TUI. Both frontends will share 95% of the codebase (parser, network, config, window management logic). Users can choose between TUI and GUI modes via `--gui` command-line flag.

**Key Benefits:**
- Use proportional fonts (Verdana, Arial, etc.) for better readability
- Native window management (drag, resize, minimize, maximize)
- Modern GUI look and feel
- Maintain TUI for SSH/remote usage
- Share all core logic (no duplication)

**Total Estimated Time:** 12-16 weeks of focused development

---

## Motivation

### Current Pain Points
- **TUI requires monospaced fonts** - Character grid limitations prevent use of proportional fonts like Verdana
- **Eye strain** - Long gaming sessions with monospaced fonts can be uncomfortable for some users
- **Wrayth comparison** - Wrayth uses GUI with proportional fonts, better readability for extended play

### Why Not Just Switch to GUI?
- **TUI has value** - SSH/remote play, tmux/screen integration, terminal purists
- **Both can coexist** - Share 95% of codebase, minimal maintenance burden
- **User choice** - Different use cases prefer different interfaces

### Why egui?
| Framework | Pros | Cons |
|-----------|------|------|
| **egui** | Immediate mode (simple), GPU-accelerated, pure Rust, proven performance | Younger ecosystem |
| **iced** | Retained mode, Elm architecture | More complex mental model |
| **Tauri** | Web tech (HTML/CSS) | Heavier, web rendering overhead |
| **Qt** | Mature, feature-rich | C++ bindings, complex, large |

**Verdict:** egui offers best balance of simplicity, performance, and Rust integration.

---

## Architecture Overview

### Current Architecture (TUI-Only)
```
┌─────────────────────────────────────────┐
│  main.rs                                │
│  - Parse args                           │
│  - Initialize logging                   │
│  - Load config                          │
│  - Create App                           │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│  App (src/app.rs)                       │
│  - Event loop (crossterm)               │
│  - Rendering (ratatui)                  │
│  - Business logic (parser, windows)     │
│  - Network handling                     │
└──────────────┬──────────────────────────┘
               │
    ┌──────────┼──────────┐
    │          │          │
┌───▼───┐  ┌──▼────┐  ┌──▼─────┐
│Parser │  │Network│  │Windows │
└───────┘  └───────┘  └────────┘
```

**Problem:** Rendering (ratatui) is tightly coupled with business logic.

### Target Architecture (TUI + GUI)
```
┌─────────────────────────────────────────┐
│  main.rs                                │
│  - Parse args (--gui flag)              │
│  - Choose frontend (TUI or GUI)         │
│  - Load config                          │
└──────────────┬──────────────────────────┘
               │
        ┌──────┴───────┐
        │              │
┌───────▼────┐   ┌─────▼──────┐
│ TUI        │   │ GUI        │
│ Frontend   │   │ Frontend   │
│ (ratatui)  │   │ (egui)     │
└───────┬────┘   └─────┬──────┘
        │              │
        └──────┬───────┘
               │ implements Frontend trait
┌──────────────▼──────────────────────────┐
│  AppCore (shared business logic)        │
│  - Parser                               │
│  - WindowManager                        │
│  - Config                               │
│  - Network                              │
│  - Input handling                       │
│  - State management                     │
└─────────────────────────────────────────┘
```

**Solution:** Abstract rendering layer, shared core logic.

### Frontend Trait
```rust
pub trait Frontend {
    /// Poll for user input events (keyboard, mouse, resize, etc.)
    fn poll_events(&mut self) -> Result<Vec<FrontendEvent>>;

    /// Render current application state
    fn render(&mut self, core: &AppCore) -> Result<()>;

    /// Cleanup (restore terminal, close window, etc.)
    fn cleanup(&mut self) -> Result<()>;
}

pub enum FrontendEvent {
    Key { code: KeyCode, modifiers: KeyModifiers },
    Mouse { kind: MouseEventKind, x: u16, y: u16 },
    Resize { width: u16, height: u16 },
    Quit,
}
```

---

## Implementation Phases

### Phase 1: Architecture Refactoring (Foundation)
**Goal:** Decouple rendering from core logic
**Time Estimate:** 2-3 weeks

#### Tasks
1. **Create rendering abstraction layer**
   - New module: `src/frontend/mod.rs`
   - Define `Frontend` trait
   - Define `FrontendEvent` enum
   - Move crossterm-specific code

2. **Extract core state machine**
   - New module: `src/core/mod.rs`
   - Create `AppCore` struct (contains window_manager, parser, config, etc.)
   - Move `handle_input()` logic (make frontend-agnostic)
   - Move `handle_server_message()` logic
   - Move state update logic

3. **Refactor widget system**
   - Create `src/widgets/` module
   - Define `WidgetState` trait (data only, no rendering)
   - Separate state from rendering:
     - `TextWindowState` (data: lines, scroll position)
     - `ProgressBarState` (data: current, max, colors)
     - `CountdownState` (data: end_time, type)
   - Keep existing ratatui rendering in separate impls

#### Deliverables
- TUI still works exactly as before
- Code is now abstracted and ready for multiple frontends
- Tests pass

#### Migration Strategy
- Do this incrementally (one widget type at a time)
- Keep `App` struct temporarily as glue layer
- Write tests to ensure TUI behavior unchanged

---

### Phase 2: TUI Frontend Implementation
**Goal:** Wrap existing Ratatui code in new abstraction
**Time Estimate:** 1 week

#### Tasks
1. **Create TUI frontend module**
   - `src/frontend/tui/mod.rs` - Implements `Frontend` trait
   - `src/frontend/tui/app.rs` - TUI-specific app wrapper
   - `src/frontend/tui/widgets/` - Ratatui widget renderers

2. **Migrate terminal setup**
   - Move `enable_raw_mode()`, `EnterAlternateScreen` to TUI frontend
   - Convert crossterm events to `FrontendEvent`
   - Move terminal cleanup to `Frontend::cleanup()`

3. **Update main.rs**
   - Add `--tui` flag (default)
   - Add `--gui` flag (future)
   - Route to TUI frontend
   - Pass `AppCore` to frontend

#### Deliverables
- TUI works exactly as before
- All existing features functional
- Clean separation between frontend and core

---

### Phase 3: GUI Frontend - Core (Prototype)
**Goal:** Basic working GUI with text windows
**Time Estimate:** 1-2 weeks

#### Tasks
1. **Add egui dependencies**
   ```toml
   [dependencies]
   egui = "0.29"
   eframe = "0.29"  # Application framework
   egui_extras = "0.29"  # Additional widgets
   ```

2. **Create GUI frontend structure**
   - `src/frontend/gui/mod.rs` - Implements `Frontend` trait
   - `src/frontend/gui/app.rs` - egui::App wrapper
   - `src/frontend/gui/widgets/` - egui widget renderers

3. **Implement basic rendering**
   - Text windows with **proportional fonts** (Verdana support!)
   - Command input box
   - Window borders and titles
   - Basic mouse/keyboard input

4. **Event handling**
   - Convert egui events to `FrontendEvent`
   - Keyboard input
   - Mouse clicks
   - Window resize

#### Deliverables
- Can launch with `--gui` flag
- Connects to game server
- See text in GUI window with Verdana font
- Can type commands and send to game
- Basic scrolling works

#### Proof of Concept Code
```rust
// src/frontend/gui/app.rs
pub struct GuiApp {
    core: AppCore,
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll events
        let events = self.poll_egui_events(ctx);

        // Update core
        for event in events {
            self.core.handle_event(event);
        }

        // Render windows
        for window_state in &self.core.windows {
            self.render_window(ctx, window_state);
        }
    }
}

fn render_window(&self, ctx: &egui::Context, window: &WindowState) {
    egui::Window::new(&window.title)
        .default_pos([window.x as f32, window.y as f32])
        .default_size([window.width as f32, window.height as f32])
        .show(ctx, |ui| {
            // Use proportional font!
            let font = egui::FontId::proportional(14.0);
            ui.label(egui::RichText::new(&window.text).font(font));
        });
}
```

---

### Phase 4: GUI Frontend - Window Management
**Goal:** Full window functionality
**Time Estimate:** 1-2 weeks

#### Tasks
1. **Window system**
   - Draggable windows (egui::Window native support)
   - Resizable windows with handles
   - Z-ordering (click to bring to front)
   - Minimize/maximize buttons
   - Window snapping to edges (optional)
   - Remember positions across sessions

2. **Tabbed windows**
   - Native tab bars using egui widgets
   - Unread indicators (colored tabs)
   - Tab switching (mouse + keyboard)
   - Drag tabs to reorder

3. **Progress bars & countdown timers**
   - Visual progress bars with colored fills
   - Animated countdowns
   - Custom styling to match config colors
   - Smooth animations (not just character blocks)

4. **Command input**
   - Syntax highlighting (future)
   - Command history (up/down arrows)
   - Auto-complete (future)
   - Multi-line support

#### Deliverables
- Full window management in GUI
- All window types rendering correctly
- Window positions saveable
- Tabbed windows fully functional

---

### Phase 5: GUI Frontend - Advanced Features
**Goal:** Feature parity with TUI
**Time Estimate:** 2-3 weeks

#### Tasks
1. **Clickable links & context menus**
   - Hover highlighting (change cursor, underline)
   - Right-click context menus (native egui menus)
   - Hierarchical menu support (submenus)
   - Link click handling (send commands to game)
   - Recent links cache

2. **Highlights & colors**
   - Text highlighting with configurable colors
   - Regex-based highlights (reuse existing Aho-Corasick)
   - Color pickers in settings (egui::color_picker)
   - Sound playback (reuse existing rodio code)
   - Highlight profiles (save/load)

3. **Specialized widgets**
   - **Compass widget** - Visual compass rose with clickable exits
   - **Injury doll** - Graphical body diagram with colored limbs
   - **Hands display** - Show left/right hand items with icons
   - **Dashboard** - Compact stats display
   - **Active effects** - Scrollable buff/debuff list
   - **Map widget** - Visual map with current room highlighted
   - **Inventory window** - Searchable, sortable item list
   - **Room window** - Room description with clickable objects
   - **Spells window** - Active spells with durations

4. **Settings editor**
   - Native GUI settings panel (no more TOML editing!)
   - Font selection dropdown (with preview)
   - Color pickers for all color settings
   - Live preview of changes
   - Save/load layouts
   - Import/export config

#### Deliverables
- GUI has all TUI features
- Specialized widgets look better than TUI versions
- Settings editor is user-friendly

---

### Phase 6: GUI Polish & Quality of Life
**Goal:** Production-ready GUI
**Time Estimate:** 1-2 weeks

#### Tasks
1. **UI improvements**
   - Smooth scrolling (not instant jumps)
   - Smooth animations (progress bar fills, countdown ticks)
   - Tooltips on hover (explain icons, settings, etc.)
   - Keyboard shortcuts overlay (Ctrl+? shows all keybinds)
   - Status bar (connection status, server time, FPS counter)
   - Loading screen on startup

2. **Window layouts**
   - Save/load layout system (reuse existing)
   - Preset layouts dropdown (Tank, Healer, Caster, etc.)
   - Layout templates (shareable files)
   - Auto-save on exit
   - Layout editor (drag-and-drop window positioning)

3. **Themes**
   - Dark theme (default)
   - Light theme (optional)
   - Custom theme colors
   - Theme save/load

4. **Accessibility**
   - Font size controls (Ctrl+Plus/Minus)
   - High contrast mode
   - Screen reader support (future)
   - Colorblind-friendly palette options

#### Deliverables
- Polished, professional-looking GUI
- User-friendly settings
- Good UX for new users

---

### Phase 7: Testing & Documentation
**Goal:** Stable release
**Time Estimate:** 1 week

#### Tasks
1. **Testing**
   - Test all widgets in GUI mode
   - Test TUI still works (regression testing)
   - Test switching between modes (--gui vs --tui)
   - Performance testing (10k+ lines/sec)
   - Multi-character testing
   - Layout save/load testing
   - Edge cases (tiny windows, huge windows, etc.)

2. **Documentation**
   - Update README.md with GUI instructions
   - Add GUI screenshots
   - Document `--gui` flag
   - Font selection guide
   - Layout customization guide
   - Troubleshooting section
   - Video tutorial (optional)

3. **Packaging**
   - Build Windows installer (.msi)
   - Include font recommendations in installer
   - Create desktop shortcut
   - Add to Windows Start Menu
   - Include example layouts
   - Bundle default fonts (if licensing allows)

#### Deliverables
- Ready for public release
- Documentation complete
- Installer tested

---

## Technical Details

### Directory Structure (Final)
```
src/
├── main.rs                  # Entry point, chooses frontend
├── core/                    # Shared business logic
│   ├── mod.rs
│   ├── app_core.rs          # AppCore struct
│   ├── input_handler.rs     # Input processing
│   └── state.rs             # State management
├── widgets/                 # Widget state (no rendering)
│   ├── mod.rs
│   ├── text_window.rs       # TextWindowState
│   ├── progress_bar.rs      # ProgressBarState
│   ├── countdown.rs         # CountdownState
│   └── ...
├── frontend/
│   ├── mod.rs               # Frontend trait
│   ├── events.rs            # FrontendEvent enum
│   ├── tui/
│   │   ├── mod.rs           # TuiFrontend impl
│   │   ├── app.rs
│   │   └── widgets/         # Ratatui rendering
│   │       ├── text_window.rs
│   │       ├── progress_bar.rs
│   │       └── ...
│   └── gui/
│       ├── mod.rs           # GuiFrontend impl
│       ├── app.rs
│       └── widgets/         # egui rendering
│           ├── text_window.rs
│           ├── progress_bar.rs
│           └── ...
├── config.rs                # Unchanged
├── network.rs               # Unchanged
├── parser.rs                # Unchanged
├── selection.rs             # Unchanged (or abstracted)
├── sound.rs                 # Unchanged
└── ...
```

### Command-Line Interface
```bash
# TUI mode (default)
vellum-fe --character Zoleta --port 8000

# TUI mode (explicit)
vellum-fe --tui --character Zoleta --port 8000

# GUI mode
vellum-fe --gui --character Zoleta --port 8000

# GUI mode with custom font
vellum-fe --gui --font "Verdana" --font-size 14

# Validate layout (works for both TUI and GUI)
vellum-fe --validate-layout layouts/mytank.toml
```

### Configuration Changes

**New section in config.toml:**
```toml
[gui]
# GUI-specific settings (ignored in TUI mode)
default_font = "Verdana"
font_size = 14
enable_antialiasing = true
enable_animations = true
window_opacity = 1.0
theme = "dark"  # or "light"
smooth_scrolling = true
fps_limit = 60
vsync = true

# Window behavior
remember_positions = true
snap_to_edges = false
snap_threshold = 10  # pixels

# Performance
text_cache_size = 10000  # lines
max_fps = 60
```

**Backward compatibility:**
- Existing configs work as-is
- GUI settings are optional
- TUI ignores GUI section

### Widget State Examples

**Before (TUI-coupled):**
```rust
// src/ui/text_window.rs
pub struct TextWindow {
    lines: Vec<Vec<TextSegment>>,  // Data
    scroll_position: usize,         // Data
    inner_width: u16,               // Layout (TUI-specific)
    // ... ratatui rendering code mixed in
}

impl Widget for TextWindow {
    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        // Ratatui rendering code
    }
}
```

**After (Abstracted):**
```rust
// src/widgets/text_window.rs
pub struct TextWindowState {
    lines: Vec<Vec<TextSegment>>,
    scroll_position: usize,
    max_lines: usize,
}

impl TextWindowState {
    pub fn add_line(&mut self, segments: Vec<TextSegment>) { ... }
    pub fn scroll_up(&mut self, lines: usize) { ... }
    pub fn scroll_down(&mut self, lines: usize) { ... }
}

// src/frontend/tui/widgets/text_window.rs
impl TuiWidget for TextWindowState {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Ratatui-specific rendering
    }
}

// src/frontend/gui/widgets/text_window.rs
impl GuiWidget for TextWindowState {
    fn render(&self, ui: &mut egui::Ui) {
        // egui-specific rendering
        for line in &self.lines {
            ui.label(line_to_richtext(line));
        }
    }
}
```

---

## Performance Analysis

### Current TUI Performance
- **Frame time:** ~1-2ms average
- **Large chunks:** Handles 10k+ lines/sec
- **Memory:** ~50-100MB typical

### Expected GUI Performance

#### What Stays Fast
- **XML parsing** - No change (same parser)
- **Stream routing** - No change (same window manager)
- **Highlight matching** - No change (same Aho-Corasick)
- **Network I/O** - No change (same tokio)
- **Data structures** - No change (same core)

**Conclusion:** 90% of performance is unchanged.

#### What Might Be Slower
1. **Text rendering**
   - Proportional fonts + antialiasing is heavier than monospaced grid
   - **Mitigation:** egui uses glyph cache + GPU acceleration
   - **Expected:** 5-10ms per frame (still 100+ FPS)

2. **Layout calculation**
   - More complex than terminal grid (floating windows)
   - **Mitigation:** Cache layouts, only recalc on resize/move
   - **Expected:** Negligible (egui handles this efficiently)

3. **Redraws**
   - More pixels to push (1920x1080 vs 120x40 chars)
   - **Mitigation:** Dirty rectangles, GPU acceleration, vsync
   - **Expected:** 60 FPS cap is plenty

#### Performance Targets
- **Target FPS:** 60 FPS (16.6ms per frame)
- **Acceptable:** 30 FPS (33ms per frame) under extreme load
- **Large chunk handling:** 10k+ lines/sec (same as TUI)
- **Memory:** <200MB typical
- **Startup time:** <1 second

#### Benchmarking Plan
1. **Baseline:** Measure TUI performance (existing)
2. **GUI prototype:** Measure basic text rendering
3. **Full GUI:** Measure with all widgets
4. **Stress test:** 50k lines/sec, 100 windows, etc.
5. **Profile:** Use `cargo flamegraph` to find bottlenecks

### Real-World Examples
- **Rerun (egui app):** Handles real-time 3D visualization at 60 FPS
- **COSMIC Desktop (iced):** Entire desktop environment, very smooth
- **Wrayth (Qt):** Proof that GUI can handle GS4 streams

**Verdict:** GUI will maintain excellent performance with proper implementation.

---

## Risk Assessment

### Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| egui learning curve | Medium | Low | Prototype first, read docs, ask community |
| Font rendering performance | Low | Medium | GPU acceleration, profile early, fallback to simpler rendering |
| Breaking TUI during refactor | Medium | High | Incremental changes, keep tests, CI checks |
| Scope creep | High | Medium | Stick to feature parity first, polish later |
| egui limitations | Low | Medium | Research egui capabilities upfront, have fallback plan |
| Maintenance burden | Medium | Low | 95% shared code, frontend abstraction makes it manageable |

### User Impact Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| TUI users angry about breaking changes | Low | High | **Zero changes to TUI** - explicit goal |
| GUI users expect Wrayth parity | High | Medium | Set expectations: "feature parity with TUI, not Wrayth" |
| Config migration issues | Low | Medium | Backward compatible configs, migration guide |
| Performance regression | Low | High | Benchmark early and often, target 60 FPS |

### Project Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Abandonment (hobby project) | Medium | Low | Incremental approach, each phase adds value |
| Time estimate too optimistic | High | Low | Phases can be done independently, no rush |
| Community expectations | Medium | Medium | Clear communication: hobby project, no timeline |

---

## Success Criteria

### Must Have (MVP)
- ✅ TUI works exactly as before (zero regressions)
- ✅ GUI supports proportional fonts (Verdana, Arial, etc.)
- ✅ GUI has basic text windows + command input
- ✅ GUI can connect to game and display text
- ✅ Performance: <16ms render time (60 FPS)
- ✅ Works on Windows (primary target)

### Should Have (Feature Parity)
- ✅ GUI has all TUI features (windows, tabs, progress, countdown, etc.)
- ✅ GUI has clickable links + context menus
- ✅ GUI has highlights + sounds
- ✅ GUI has settings editor
- ✅ GUI has layout save/load
- ✅ Binary size <50MB
- ✅ User can switch modes without reconfiguring

### Nice to Have (Polish)
- ⭐ Smooth animations
- ⭐ Themes (dark/light)
- ⭐ Layout editor (drag-and-drop)
- ⭐ Installer for Windows
- ⭐ Video tutorial
- ⭐ Cross-platform (Mac/Linux)

---

## Timeline

### Conservative Estimate (Hobby Project Pace)
**Total: 12-16 weeks of focused development**

| Phase | Duration | Calendar Time (1-2 hrs/day) |
|-------|----------|------------------------------|
| Phase 1: Refactoring | 2-3 weeks | 1-2 months |
| Phase 2: TUI Migration | 1 week | 2-3 weeks |
| Phase 3: GUI Core | 1-2 weeks | 3-4 weeks |
| Phase 4: Window Mgmt | 1-2 weeks | 3-4 weeks |
| Phase 5: Advanced | 2-3 weeks | 1-2 months |
| Phase 6: Polish | 1-2 weeks | 3-4 weeks |
| Phase 7: Testing | 1 week | 2-3 weeks |

**Calendar time (hobby pace):** 6-9 months

### Aggressive Estimate (Full-Time Pace)
**Total: 8-12 weeks of focused development**

Assumes 6-8 hours/day, no interruptions, experienced with Rust/egui.

**Calendar time (full-time):** 2-3 months

### Recommended Approach
1. **Start small:** Phase 1 only (refactoring)
   - This adds value even without GUI (cleaner code)
   - Can be done incrementally (1-2 hrs at a time)
   - Low risk

2. **Prototype early:** Phase 3 (GUI core)
   - Build basic prototype ASAP
   - Validate assumptions (egui performance, font rendering, etc.)
   - Get user feedback

3. **Iterate:** Phases 4-6
   - Add features incrementally
   - Release alpha versions for testing
   - Gather feedback, adjust priorities

4. **Polish later:** Phase 6
   - Don't over-polish before users test
   - Let user feedback drive polish priorities

---

## References

### egui Resources
- **egui docs:** https://docs.rs/egui/latest/egui/
- **eframe docs:** https://docs.rs/eframe/latest/eframe/
- **egui demo:** https://www.egui.rs/ (try in browser!)
- **egui examples:** https://github.com/emilk/egui/tree/master/examples
- **egui template:** https://github.com/emilk/eframe_template

### Similar Projects
- **Rerun:** https://github.com/rerun-io/rerun (egui for 3D visualization)
- **COSMIC Desktop:** https://github.com/pop-os/cosmic-epoch (iced for DE)
- **Halloy:** https://github.com/squidowl/halloy (iced IRC client - similar to VellumFE!)

### Rust GUI Comparison
- **"Are we GUI yet?"**: https://areweguiyet.com/
- **egui vs iced:** https://blog.logrocket.com/comparing-rust-gui-libraries/

### Performance
- **egui performance:** https://github.com/emilk/egui/blob/master/CHANGELOG.md#performance
- **GPU acceleration in egui:** https://docs.rs/egui/latest/egui/#backends

---

## Next Steps

### Before Starting
1. **Try egui demo:** Visit https://www.egui.rs/ and play with widgets
2. **Read egui docs:** Understand immediate mode paradigm
3. **Clone eframe_template:** Build a "Hello World" egui app
4. **Profile current TUI:** Get baseline performance numbers

### When Ready to Start
1. **Create feature branch:** `git checkout -b feature/gui-frontend`
2. **Start Phase 1:** Begin refactoring incrementally
3. **Write tests:** Ensure TUI behavior unchanged
4. **Commit often:** Small, focused commits
5. **Ask for help:** egui community is friendly on GitHub Discussions

### Questions to Answer First
- [ ] Do you want to support macOS/Linux, or Windows-only?
- [ ] What's your minimum target Windows version? (egui requires OpenGL 3.3+)
- [ ] Do you want to bundle fonts, or rely on system fonts?
- [ ] Do you want installer, or just portable .exe?
- [ ] Do you want themes (dark/light), or just dark?

---

## Conclusion

Adding a GUI frontend is **feasible and worthwhile** for VellumFE:

**Pros:**
- ✅ Solves eye strain issue (proportional fonts!)
- ✅ Modern, native look and feel
- ✅ Minimal code duplication (95% shared)
- ✅ Maintains TUI for SSH/remote usage
- ✅ egui is proven, performant, and Rust-native

**Cons:**
- ⚠️ Significant development time (3-6 months hobby pace)
- ⚠️ Learning curve for egui
- ⚠️ Maintenance of two frontends (though mostly shared)

**Recommendation:** **Proceed, but incrementally.**

Start with Phase 1 (refactoring), which provides value even without GUI. Then build a Phase 3 prototype to validate assumptions. If prototype looks good, continue with remaining phases.

This is a **hobby project** - no rush. Each phase adds value independently. Can be done in small chunks (1-2 hours at a time).

---

**Good luck, and enjoy the journey!**
