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
fn core_and_data_do_not_reference_egui() {
    // Rendering stays in frontends: core/, data/, and config/ must compile
    // without any UI toolkit.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for layer in ["core", "data", "config"] {
        scan_dir(
            &src.join(layer),
            &["egui", "eframe"],
            &mut violations,
        );
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
        ("src/frontend/gui/app.rs", 4800),
        ("src/core/app_core/state.rs", 3600),
        ("src/frontend/tui/window_editor.rs", 2100),
        ("src/core/messages.rs", 3400),
        ("src/frontend/gui/app/widgets.rs", 1200),
        ("src/parser.rs", 3600),
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
