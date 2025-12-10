use anyhow::Result;
use std::path::Path;

use crate::config::{Config, Layout};

#[derive(Debug, Clone)]
pub enum IssueKind { Error, Warning }

#[derive(Debug, Clone)]
pub struct LayoutIssue {
    pub window: String,
    pub message: String,
    pub kind: IssueKind,
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
            issues.push(LayoutIssue { window: w.name.clone(), message: format!("rows {} < min_rows {}", w.rows, min_rows), kind: IssueKind::Error });
        }
        if w.cols < min_cols {
            issues.push(LayoutIssue { window: w.name.clone(), message: format!("cols {} < min_cols {}", w.cols, min_cols), kind: IssueKind::Error });
        }
        if let Some(max_r) = w.max_rows { if w.rows > max_r { issues.push(LayoutIssue { window: w.name.clone(), message: format!("rows {} > max_rows {}", w.rows, max_r), kind: IssueKind::Error }); } }
        if let Some(max_c) = w.max_cols { if w.cols > max_c { issues.push(LayoutIssue { window: w.name.clone(), message: format!("cols {} > max_cols {}", w.cols, max_c), kind: IssueKind::Error }); } }

        // Non-negative and inside terminal
        if w.row + w.rows > new_h {
            issues.push(LayoutIssue { window: w.name.clone(), message: format!("row+rows {} exceeds height {}", w.row + w.rows, new_h), kind: IssueKind::Error });
        }
        if w.col + w.cols > new_w {
            issues.push(LayoutIssue { window: w.name.clone(), message: format!("col+cols {} exceeds width {}", w.col + w.cols, new_w), kind: IssueKind::Error });
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
                    kind: IssueKind::Error,
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
    let mut app = crate::app::App::new(cfg, false, true)?; // nosound=true for validation

    // Use file layout as baseline
    app.set_layout_for_validation(layout.clone(), baseline);

    let mut results = Vec::new();

    // Analyze baseline top stack continuity to produce warnings (added to first result later)
    let baseline_layout = app.current_layout().clone();
    let top_stack_warnings = analyze_top_stack_warnings(&baseline_layout);

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
                issues.push(LayoutIssue { window: cmd.name.clone(), message: format!("command_input row {} != anchored {}", cmd.row, expected_row), kind: IssueKind::Error });
            }
        }

        results.push(ValidationResult { width: w, height: h, issues });
    }

    // Attach baseline top-stack warnings to the first result (if any sizes were requested)
    if let Some(first) = results.first_mut() {
        first.issues.extend(top_stack_warnings);
    }

    Ok(results)
}

// Build warnings about top-stack continuity: statics at row>0 overlapping row-0 chain only if each row has overlap with previous.
fn analyze_top_stack_warnings(layout: &Layout) -> Vec<LayoutIssue> {
    use std::collections::{BTreeMap, HashSet};
    let mut warnings = Vec::new();

    // Identify static-height windows (exclude command_input)
    let mut statics_by_row: BTreeMap<u16, Vec<(u16, u16, usize, &str)>> = BTreeMap::new();
    for (i, w) in layout.windows.iter().enumerate() {
        if w.widget_type == "command_input" { continue; }
        let is_static = match w.widget_type.as_str() {
            "compass" | "injury_doll" | "dashboard" | "indicator" | "progress" | "countdown" | "hands" | "hand" | "lefthand" | "righthand" | "spellhand" => true,
            _ => false,
        } || (w.min_rows.is_some() && w.max_rows.is_some() && w.min_rows == w.max_rows);
        if is_static {
            let s = w.col; let e = w.col.saturating_add(w.cols);
            statics_by_row.entry(w.row).or_default().push((s, e, i, w.name.as_str()));
        }
    }

    // Start chain from row 0
    let mut stack_indices: HashSet<usize> = HashSet::new();
    let mut last_row = 0u16;
    let mut last_spans: Vec<(u16, u16, usize, &str)> = statics_by_row.get(&0).cloned().unwrap_or_default();
    for (_, _, idx, _) in &last_spans { stack_indices.insert(*idx); }

    if !last_spans.is_empty() {
        loop {
            let next_row = last_row.saturating_add(1);
            let candidates = match statics_by_row.get(&next_row) { Some(v) => v, None => break };
            let mut next_spans = Vec::new();
            for (s, e, idx, name) in candidates.iter().copied() {
                let overlaps = last_spans.iter().any(|(ps, pe, _, _)| s < *pe && e > *ps);
                if overlaps { next_spans.push((s, e, idx, name)); stack_indices.insert(idx); }
            }
            if next_spans.is_empty() { break; }
            last_spans = next_spans; last_row = next_row;
        }
    }

    // Warn about row 1 statics that do not border any row 0 statics (they will shift)
    if let Some(row1) = statics_by_row.get(&1) {
        let row0 = statics_by_row.get(&0).cloned().unwrap_or_default();
        for (s, e, _idx, name) in row1 {
            let borders_top = row0.iter().any(|(ts, te, _, _)| *s < *te && *e > *ts);
            if !borders_top {
                warnings.push(LayoutIssue { window: (*name).to_string(), message: "static at row 1 does not border top stack; will shift on resize".to_string(), kind: IssueKind::Warning });
            }
        }
    }

    // Warn if a row overlaps a deeper row but not the immediately previous stacked row (discontinuity)
    for (row, spans) in &statics_by_row {
        if *row <= 1 { continue; }
        let prev = statics_by_row.get(&(*row - 1)).cloned().unwrap_or_default();
        let row0 = statics_by_row.get(&0).cloned().unwrap_or_default();
        let overlaps_prev = spans.iter().any(|(s, e, _, _)| prev.iter().any(|(ps, pe, _, _)| *s < *pe && *e > *ps));
        let overlaps_top = spans.iter().any(|(s, e, _, _)| row0.iter().any(|(ps, pe, _, _)| *s < *pe && *e > *ps));
        if overlaps_top && !overlaps_prev {
            warnings.push(LayoutIssue { window: format!("row {}", row), message: "top-stack discontinuity: overlaps row 0 but not preceding row".to_string(), kind: IssueKind::Warning });
        }
    }

    warnings
}
