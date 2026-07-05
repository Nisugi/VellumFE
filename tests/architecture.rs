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
