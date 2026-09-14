//! Architecture rules enforced at test time.
//!
//! CLAUDE.md rule: `core/` and `data/` modules must not import from
//! `frontend/`. Frontends read from the data layer, never the reverse.
//! This test makes the rule mechanical instead of honor-system.

use std::fs;
use std::path::Path;

fn scan_dir(dir: &Path, needles: &[&str], violations: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {}", dir.display(), e)) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            scan_dir(&path, needles, violations);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
            for (idx, line) in content.lines().enumerate() {
                if needles.iter().any(|needle| line.contains(needle)) {
                    violations.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                }
            }
        }
    }
}

#[test]
fn core_and_data_do_not_reference_frontend() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for layer in ["core", "data"] {
        scan_dir(
            &src.join(layer),
            &["crate::frontend", "super::frontend"],
            &mut violations,
        );
    }
    assert!(
        violations.is_empty(),
        "core/ and data/ must not reference frontend/ (see CLAUDE.md architecture rules).\n\
         Pure input/event data types belong in data/ (e.g. data/input.rs).\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn gui_does_not_reference_tui() {
    // The GUI and TUI are peer frontends sharing core/, data/, and
    // frontend/common/. Anything both need belongs in one of those shared
    // layers (e.g. parse_color_flexible lives in frontend/common/color.rs),
    // never imported across frontends.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    scan_dir(
        &src.join("frontend/gui"),
        &["crate::frontend::tui", "super::tui", "ratatui", "crossterm"],
        &mut violations,
    );
    assert!(
        violations.is_empty(),
        "frontend/gui/ must not reference frontend/tui/ or terminal crates.\n\
         Move shared logic to core/, data/, or frontend/common/.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn gui_does_not_reference_tui_placement() {
    // TuiPlacement (src/config/widgets.rs) is TUI-only per-window state
    // (terminal-cell geometry, TUI show/hide selection), flattened into
    // WindowBase's on-disk shape but split out as its own type specifically
    // so the GUI cannot reach it — the GUI has its own selection/placement
    // (hidden_tabs, main_window_rects/zones/groups). Reaching `.placement`
    // or naming `TuiPlacement` from GUI code reintroduces the coupling that
    // caused loading a layout in one frontend to wreck the other's tab set.
    // Fields behind this boundary: row/col/rows/cols/min_rows/max_rows/
    // min_cols/max_cols/visibility/show_title/title_position.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    scan_dir(
        &src.join("frontend/gui"),
        &[".placement", "TuiPlacement"],
        &mut violations,
    );
    assert!(
        violations.is_empty(),
        "frontend/gui/ must not reference WindowBase.placement or TuiPlacement — \
         the GUI has its own selection/placement (hidden_tabs, main_window_rects/zones/groups).\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn gui_does_not_use_tui_show_hide_selection() {
    // `placement.visibility` is the TUI's SELECTION of which catalog windows
    // it shows; the GUI's selection is `hidden_tabs` in its own
    // layout_v1.json. Core's `set_known_window_shown` / `hide_window` /
    // `show_window` all write the TUI slot (plus a "Window 'X' hidden"
    // message and the ".savelayout" tip), so GUI code must not call them —
    // a GUI hide goes through `VellumGuiApp::gui_set_tab_shown`, and the
    // shared dialog-popup bookkeeping through
    // `AppCore::sync_dialog_popup_allow_set`.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    scan_dir(
        &src.join("frontend/gui"),
        &["set_known_window_shown(", ".hide_window(", ".show_window("],
        &mut violations,
    );
    assert!(
        violations.is_empty(),
        "frontend/gui/ must not drive the TUI's show/hide selection. \
         Use VellumGuiApp::gui_set_tab_shown (hidden_tabs) instead.\nViolations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn gui_does_not_assign_config_active_theme() {
    // Theme is presentation, so it is per-frontend: `config.active_theme`
    // (per-character config.toml) is the TUI's slot; `ui_settings
    // .active_theme` (layout_v1.json) is the GUI's. The GUI must never
    // ASSIGN the TUI's slot. Reads are allowed only in the single
    // first-run fallback `VellumGuiApp::gui_theme_id`, which lives in
    // src/frontend/gui/app/theme.rs — the one allowlisted file.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    scan_dir(
        &src.join("frontend/gui"),
        &["config.active_theme ="],
        &mut violations,
    );
    assert!(
        violations.is_empty(),
        "frontend/gui/ must not assign config.active_theme (the TUI's theme slot). \
         Write the GUI slot via VellumGuiApp::set_gui_theme_id instead.\nViolations:\n{}",
        violations.join("\n")
    );

    // And the read side: only the documented fallback file may read it, and
    // only in real code (prose mentions are fine anywhere).
    let mut reads = Vec::new();
    scan_dir(
        &src.join("frontend/gui"),
        &["config.active_theme"],
        &mut reads,
    );
    let stray: Vec<&String> = reads
        .iter()
        .filter(|v| {
            let code = v.rsplit_once(": ").map(|(_, c)| c).unwrap_or(v);
            !code.starts_with("//")
        })
        .filter(|v| !v.replace(char::from(92u8), "/").contains("app/theme.rs"))
        .collect();
    assert!(
        stray.is_empty(),
        "config.active_theme may only be read in the documented first-run \
         fallback (VellumGuiApp::gui_theme_id, src/frontend/gui/app/theme.rs).\
         \nViolations:\n{:#?}",
        stray
    );
}

#[test]
fn core_and_data_do_not_reference_egui() {
    // Rendering stays in frontends: core/, data/, and config/ must compile
    // without any UI toolkit.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for layer in ["core", "data", "config"] {
        scan_dir(&src.join(layer), &["egui", "eframe"], &mut violations);
    }
    assert!(
        violations.is_empty(),
        "core/, data/, and config/ must not reference egui/eframe.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn core_is_android_safe() {
    // The Android build is `--no-default-features`: core/, data/, and the
    // parser must never reference desktop-only crates directly. Platform
    // integrations go through the feature-gated wrapper modules instead:
    // crate::clipboard (arboard), crate::platform (open), crate::tts,
    // crate::sound (rodio). The remaining desktop crates (sysinfo, keyring,
    // rpassword) are gated at their designated sites: src/performance.rs,
    // src/config/profiles.rs, src/network.rs, src/frontend/web/server.rs.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let needles = &[
        "arboard::",
        "open::that",
        "sysinfo::",
        "keyring::",
        "rpassword::",
        "rodio::",
        "use tts::",
    ];
    let mut violations = Vec::new();
    for layer in ["core", "data"] {
        scan_dir(&src.join(layer), needles, &mut violations);
    }
    for (idx, line) in fs::read_to_string(src.join("parser.rs"))
        .expect("read parser.rs")
        .lines()
        .enumerate()
    {
        if needles.iter().any(|needle| line.contains(needle)) {
            violations.push(format!("src/parser.rs:{}: {}", idx + 1, line.trim()));
        }
    }
    // The parser was split into src/parser/ submodules; they carry the same
    // Android-safety obligation as the facade file.
    scan_dir(&src.join("parser"), needles, &mut violations);
    assert!(
        violations.is_empty(),
        "core/, data/, and parser.rs must not use desktop-only crates directly \
         (the Android build compiles them with --no-default-features).\n\
         Use the feature-gated wrappers: crate::clipboard, crate::platform, \
         crate::tts, crate::sound.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn config_root_stays_a_facade() {
    // config.rs was split into focused submodules (templates, widgets,
    // window_def, settings, layout, colors, paths, io). The root should
    // hold only the Config struct, shared glue (embedded defaults, serde
    // default fns, small enums), and the explicit pub use facade. If this
    // fails, put new code in the matching src/config/ submodule instead.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/config.rs");
    let lines = fs::read_to_string(&root)
        .unwrap_or_else(|e| panic!("read {}: {}", root.display(), e))
        .lines()
        .count();
    const MAX_CONFIG_ROOT_LINES: usize = 700;
    assert!(
        lines <= MAX_CONFIG_ROOT_LINES,
        "src/config.rs has {lines} lines (limit {MAX_CONFIG_ROOT_LINES}). \
         Move new types/impls into the appropriate src/config/ submodule."
    );
}

#[test]
fn split_parents_stay_facades() {
    // The 2026-08 monolith splits moved impl blocks into submodules
    // (frontend/gui/app/, core/app_core/state/, frontend/tui/window_editor/,
    // core/messages/, frontend/gui/app/widgets/, parser/). The residual
    // parents hold the type definitions, dispatchers, and (for now) the test
    // modules — new methods belong in the matching submodule. Limits are the
    // current size rounded up a little; if one trips, move code down into a
    // submodule instead of raising the cap.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let caps: &[(&str, usize)] = &[
        ("src/frontend/gui/app.rs", 3600),
        ("src/core/app_core/state.rs", 2100),
        ("src/frontend/tui/window_editor.rs", 1700),
        ("src/core/messages.rs", 800),
        ("src/frontend/gui/app/widgets.rs", 700),
        ("src/parser.rs", 1100),
    ];
    let mut violations = Vec::new();
    for (rel, cap) in caps {
        let path = root.join(rel);
        let lines = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e))
            .lines()
            .count();
        if lines > *cap {
            violations.push(format!("{rel}: {lines} lines (limit {cap})"));
        }
    }
    assert!(
        violations.is_empty(),
        "split parent files must stay facades — move new code into their submodules:\n{}",
        violations.join("\n")
    );
}

#[test]
fn catalog_access_goes_through_the_seams() {
    // Window-system redesign Phase 6: the template catalog's enumeration,
    // gating and id-mapping surface is reachable ONLY through the seams
    // (core::local_catalog, core::view_resolver) and inside config itself.
    // The three id-maps are deleted outright; referencing them anywhere is
    // an error. The rest are Config methods production code must reach via
    // the seam so catalog swaps stay provable.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let needles = [
        "dialog_id_to_template",
        "stream_id_to_template",
        "id_has_widget_template",
        "Config::list_window_templates",
        "Config::template_game_type",
        "Config::get_templates_by_category",
        "Config::get_addable_templates_by_category",
        "Config::get_visible_templates_by_category",
        "Config::get_layout_templates_by_category",
        "Config::get_window_template",
    ];
    let mut violations = Vec::new();
    scan_dir(&src, &needles, &mut violations);
    violations.retain(|v| {
        let path = v.split(".rs:").next().unwrap_or("").replace('\\', "/");
        let in_allowed = path.contains("/src/config")
            || path.ends_with("/src/core/local_catalog")
            || path.ends_with("/src/core/view_resolver");
        let line = v.rsplit(": ").next().unwrap_or("");
        let comment_only = line.trim_start().starts_with("//");
        !in_allowed && !comment_only
    });
    assert!(
        violations.is_empty(),
        "catalog access outside the seams (route through core::local_catalog / core::view_resolver):\n{}",
        violations.join("\n")
    );
}
