use anyhow::Result;
use std::path::Path;

use crate::app::App;
use crate::config::{Config, Layout};

#[derive(Debug, Clone)]
pub struct LayoutIssue {
    pub window: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub width: u16,
    pub height: u16,
    pub issues: Vec<LayoutIssue>,
}

fn fallback_min_size(widget_type: &str) -> (u16, u16) {
    match widget_type {
        "progress" | "countdown" | "indicator" | "hands" | "hand" => (10, 1),
        "compass" => (13, 5),
        "injury_doll" => (20, 10),
        "dashboard" => (15, 3),
        "command_input" => (20, 1),
        _ => (5, 3), // text, tabbed, etc.
    }
}

fn check_bounds_and_constraints(layout: &Layout, new_w: u16, new_h: u16) -> Vec<LayoutIssue> {
    let mut issues = Vec::new();

    // Bounds and constraints
    for w in &layout.windows {
        // Min/Max constraints
        let (min_cols_fallback, min_rows_fallback) = fallback_min_size(&w.widget_type);
        let min_rows = w.min_rows.unwrap_or(min_rows_fallback);
        let min_cols = w.min_cols.unwrap_or(min_cols_fallback);

        if w.rows < min_rows {
            issues.push(LayoutIssue { window: w.name.clone(), message: format!("rows {} < min_rows {}", w.rows, min_rows) });
        }
        if w.cols < min_cols {
            issues.push(LayoutIssue { window: w.name.clone(), message: format!("cols {} < min_cols {}", w.cols, min_cols) });
        }
        if let Some(max_r) = w.max_rows { if w.rows > max_r { issues.push(LayoutIssue { window: w.name.clone(), message: format!("rows {} > max_rows {}", w.rows, max_r) }); } }
        if let Some(max_c) = w.max_cols { if w.cols > max_c { issues.push(LayoutIssue { window: w.name.clone(), message: format!("cols {} > max_cols {}", w.cols, max_c) }); } }

        // Non-negative and inside terminal
        if w.row + w.rows > new_h {
            issues.push(LayoutIssue { window: w.name.clone(), message: format!("row+rows {} exceeds height {}", w.row + w.rows, new_h) });
        }
        if w.col + w.cols > new_w {
            issues.push(LayoutIssue { window: w.name.clone(), message: format!("col+cols {} exceeds width {}", w.col + w.cols, new_w) });
        }
    }

    // Optional: Overlap detection (warn)
    for i in 0..layout.windows.len() {
        let a = &layout.windows[i];
        let a_rect = (a.col, a.row, a.cols, a.rows);
        for j in (i + 1)..layout.windows.len() {
            let b = &layout.windows[j];
            let b_rect = (b.col, b.row, b.cols, b.rows);
            if rects_intersect(a_rect, b_rect) {
                issues.push(LayoutIssue {
                    window: a.name.clone(),
                    message: format!("overlaps with '{}'", b.name),
                });
            }
        }
    }

    issues
}

fn rects_intersect(a: (u16, u16, u16, u16), b: (u16, u16, u16, u16)) -> bool {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    let ax2 = ax.saturating_add(aw);
    let ay2 = ay.saturating_add(ah);
    let bx2 = bx.saturating_add(bw);
    let by2 = by.saturating_add(bh);
    !(ax2 <= bx || bx2 <= ax || ay2 <= by || by2 <= ay)
}

pub fn validate_layout_path(path: &Path, baseline: (u16, u16), sizes: &[(u16, u16)]) -> Result<Vec<ValidationResult>> {
    // Load layout file
    let layout = Layout::load_from_file(path)?;

    // Build a minimal app using current config, then swap layout/baseline
    let cfg = Config::load_with_options(None, 8000)?;
    let mut app = crate::app::App::new(cfg)?;

    // Use file layout as baseline
    app.set_layout_for_validation(layout.clone(), baseline);

    let mut results = Vec::new();

    for (w, h) in sizes.iter().copied() {
        // Reset layout to baseline before each run
        app.reset_layout_to_baseline();

        // Compute deltas from baseline
        let dw = w as i32 - baseline.0 as i32;
        let dh = h as i32 - baseline.1 as i32;

        // Apply resize
        app.apply_proportional_resize2(dw, dh);

        // Run checks
        let mut issues = check_bounds_and_constraints(app.current_layout(), w, h);

        // command_input anchoring check
        if let Some(cmd) = app.current_layout().windows.iter().find(|wd| wd.widget_type == "command_input") {
            let expected_row = h.saturating_sub(cmd.rows);
            if cmd.row != expected_row {
                issues.push(LayoutIssue { window: cmd.name.clone(), message: format!("command_input row {} != anchored {}", cmd.row, expected_row) });
            }
        }

        results.push(ValidationResult { width: w, height: h, issues });
    }

    Ok(results)
}
