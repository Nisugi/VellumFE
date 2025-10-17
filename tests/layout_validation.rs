use std::env;
use std::path::PathBuf;

use vellum_fe::validator::{validate_layout_path, ValidationResult};

fn default_layout_path() -> PathBuf {
    PathBuf::from("defaults/layout.toml")
}

fn sizes() -> Vec<(u16,u16)> { vec![(80,24), (100,30), (120,40), (140,40)] }

#[test]
fn validate_default_layout_bounds_and_constraints() {
    let path = default_layout_path();
    let baseline = (120u16, 40u16);
    let results = validate_layout_path(&path, baseline, &sizes()).expect("validation should run");

    // Fail test if any hard bounds/constraint issue occurs
    let mut hard_fail = 0usize;
    for ValidationResult { width, height, issues } in results {
        for iss in issues {
            // Overlap warnings are allowed in this default test
            if iss.message.contains("overlaps with") { continue; }
            eprintln!("{}x{}: [{}] {}", width, height, iss.window, iss.message);
            hard_fail += 1;
        }
    }
    assert_eq!(hard_fail, 0, "default layout must satisfy bounds/constraints at tested sizes");
}

// Optional: parameterized by env var TEST_LAYOUTS, comma-separated names relative to repo root.
// Run with: TEST_LAYOUTS="layouts/foo.toml,layouts/bar.toml" cargo test -- --nocapture
#[test]
fn validate_env_specified_layouts() {
    let list = match env::var("TEST_LAYOUTS") { Ok(v) if !v.trim().is_empty() => v, _ => return }; // skip if not set
    let baseline = (120u16, 40u16);
    for item in list.split(',') {
        let p = PathBuf::from(item.trim());
        let results = validate_layout_path(&p, baseline, &sizes()).expect("validation should run");
        let mut hard_fail = 0usize;
        for ValidationResult { width, height, issues } in results {
            for iss in issues {
                if iss.message.contains("overlaps with") { continue; }
                eprintln!("{} ({}x{}): [{}] {}", p.display(), width, height, iss.window, iss.message);
                hard_fail += 1;
            }
        }
        assert_eq!(hard_fail, 0, "layout {} failed validation", p.display());
    }
}

