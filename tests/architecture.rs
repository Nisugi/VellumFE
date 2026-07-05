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
