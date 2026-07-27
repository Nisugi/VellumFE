use std::collections::{HashMap, HashSet};

use crate::config::{Config, Layout};
use crate::data::{WindowContent, WindowPosition};

use super::AppCore;

/// One resizable track along a single axis (a window as seen by one column of
/// the height pass, or one row of the width pass).
#[derive(Debug, Clone)]
struct Track {
    /// Window name (identity, for mapping results back).
    name: String,
    /// Baseline size along this axis (rows for height, cols for width).
    base_size: u16,
    /// Minimum size this track may shrink to.
    min: u16,
    /// Maximum size this track may grow to (None = unbounded).
    max: Option<u16>,
    /// Static tracks never scale — they keep `base_size` exactly.
    is_static: bool,
}

/// Distribute a size `delta` across `tracks` along one axis, returning the new
/// size for each track (indexed the same as the input).
///
/// Guarantees, in order of priority:
/// 1. Static tracks keep `base_size`.
/// 2. Every size stays within `[min, max]`.
/// 3. CONSERVATION: the sizes sum to `Σ base_size + delta` whenever that total
///    is feasible under the clamps (i.e. within `[Σmin, Σmax]` over the
///    scalable tracks). When infeasible, the sum is clamped to the nearest
///    feasible bound — no units are silently lost.
///
/// This unifies what used to be three separate steps (floor-proportional
/// share, leftover correction, and min/max remainder recoupling) into ONE
/// reconciliation loop. Keeping them separate let a floor-overshoot and a
/// clamp interact so a unit went missing (breaking conservation); a single
/// loop that pushes ±1 onto any track with headroom until the delta is exactly
/// consumed cannot have that failure mode.
fn distribute_1d(tracks: &[Track], delta: i32) -> Vec<u16> {
    // Start everyone at their baseline size.
    let mut sizes: Vec<i32> = tracks.iter().map(|t| t.base_size as i32).collect();

    // Total baseline size of the scalable tracks — the denominator for the
    // proportional first pass. Static tracks are excluded entirely.
    let scalable_total: i32 = tracks
        .iter()
        .filter(|t| !t.is_static)
        .map(|t| t.base_size as i32)
        .sum();

    // Effective clamp bounds per track. Static tracks are pinned to their
    // baseline (min == max == base_size) so the reconciliation loop never
    // touches them.
    let lo: Vec<i32> = tracks
        .iter()
        .map(|t| if t.is_static { t.base_size as i32 } else { t.min as i32 })
        .collect();
    let hi: Vec<i32> = tracks
        .iter()
        .map(|t| {
            if t.is_static {
                t.base_size as i32
            } else {
                t.max.map(|m| m as i32).unwrap_or(i32::MAX)
            }
        })
        .collect();

    // First pass: floor-proportional share for each scalable track (only when
    // there is something to scale). This is just a good starting point; the
    // reconciliation loop below is what actually guarantees conservation.
    if scalable_total > 0 && delta != 0 {
        for (i, t) in tracks.iter().enumerate() {
            if t.is_static {
                continue;
            }
            let proportion = t.base_size as f64 / scalable_total as f64;
            let share = (proportion * delta as f64).floor() as i32;
            sizes[i] = (sizes[i] + share).clamp(lo[i], hi[i]);
        }
    }

    // Reconciliation: push ±1 onto any track that still has headroom in the
    // needed direction until the total change equals `delta`, or no track can
    // absorb more (target infeasible under the clamps). One unit at a time,
    // round-robin, so the result is balanced and can never drop a unit.
    let target_total: i32 = scalable_total
        + tracks
            .iter()
            .filter(|t| t.is_static)
            .map(|t| t.base_size as i32)
            .sum::<i32>()
        + delta;
    loop {
        let current_total: i32 = sizes.iter().sum();
        let diff = target_total - current_total;
        if diff == 0 {
            break;
        }
        let step = if diff > 0 { 1 } else { -1 };
        let mut applied = false;
        for i in 0..tracks.len() {
            if tracks[i].is_static {
                continue;
            }
            let candidate = sizes[i] + step;
            if candidate >= lo[i] && candidate <= hi[i] {
                sizes[i] = candidate;
                applied = true;
                // Re-check the running total after each unit so we stop exactly
                // on target rather than overshooting.
                if sizes.iter().sum::<i32>() == target_total {
                    break;
                }
            }
        }
        if !applied {
            // No scalable track can absorb more in this direction: the target
            // is infeasible under the clamps. Stop at the nearest feasible sum
            // rather than looping forever.
            break;
        }
    }

    sizes.iter().map(|&s| s.max(0) as u16).collect()
}

impl AppCore {
    /// Apply a layout-provided theme, returning the applied theme (if changed)
    pub(super) fn apply_layout_theme(
        &mut self,
        theme_name: Option<&str>,
    ) -> Option<(String, crate::theme::AppTheme)> {
        let theme_id = theme_name?;
        if theme_id == self.config.active_theme {
            return None;
        }

        let theme_presets =
            crate::theme::ThemePresets::all_with_custom(self.config.character.as_deref());

        if let Some(theme) = theme_presets.get(theme_id) {
            self.config.active_theme = theme_id.to_string();
            if let Err(e) = self.config.save(self.config.character.as_deref()) {
                tracing::warn!("Failed to save config after applying layout theme: {}", e);
            }
            Some((theme_id.to_string(), theme.clone()))
        } else {
            tracing::warn!(
                "Layout requested unknown theme '{}', keeping current theme '{}'",
                theme_id,
                self.config.active_theme
            );
            None
        }
    }

    /// Load a saved layout and update window positions/configs
    ///
    /// Loads layout at exact positions specified in file.
    /// Use .resize command for delta-based proportional scaling after loading.
    pub fn load_layout(
        &mut self,
        name: &str,
        terminal_width: u16,
        terminal_height: u16,
    ) -> Option<(String, crate::theme::AppTheme)> {
        tracing::info!("========== LOAD LAYOUT: '{}' START ==========", name);
        tracing::info!(
            "Current terminal size: {}x{}",
            terminal_width,
            terminal_height
        );
        tracing::info!("Current layout has {} windows", self.layout.windows.len());
        tracing::info!(
            "Current UI state has {} windows",
            self.ui_state.windows.len()
        );

        let layout_path = match Config::layout_path(name) {
            Ok(path) => path,
            Err(e) => {
                tracing::error!("Failed to get layout path for '{}': {}", name, e);
                self.add_system_message(&format!("Failed to get layout path: {}", e));
                return None;
            }
        };

        tracing::info!("Loading layout from: {}", layout_path.display());

        match Layout::load_from_file(&layout_path) {
            Ok(new_layout) => {
                let theme_update = self.apply_layout_theme(new_layout.theme.as_deref());
                tracing::info!("Layout file loaded successfully");
                tracing::info!("Loaded layout has {} windows", new_layout.windows.len());
                tracing::info!(
                    "Loaded layout terminal size: {}x{}",
                    new_layout.terminal_width.unwrap_or(0),
                    new_layout.terminal_height.unwrap_or(0)
                );

                // Log all windows in the loaded layout
                for (idx, window_def) in new_layout.windows.iter().enumerate() {
                    let base = window_def.base();
                    tracing::info!(
                        "  [{}] Window '{}' ({}): pos=({},{}) size={}x{}",
                        idx,
                        window_def.name(),
                        window_def.widget_type(),
                        base.col.get(),
                        base.row.get(),
                        base.cols.get(),
                        base.rows.get()
                    );
                }

                // Check if terminal is too small for any window
                let mut terminal_too_small = false;
                for window_def in &new_layout.windows {
                    let base = window_def.base();
                    let required_width = base.col.get().saturating_add(base.cols.get());
                    let required_height = base.row.get().saturating_add(base.rows.get());
                    if terminal_width < required_width || terminal_height < required_height {
                        terminal_too_small = true;
                        tracing::error!(
                            "Window '{}' ({}) requires {}x{} at position ({},{}), but terminal is {}x{}",
                            window_def.name(),
                            window_def.widget_type(),
                            required_width,
                            required_height,
                            base.col.get(),
                            base.row.get(),
                            terminal_width,
                            terminal_height
                        );
                    }
                }

                if terminal_too_small {
                    tracing::error!("Terminal too small to load layout '{}'", name);
                    self.add_system_message(&format!(
                        "Cannot load layout '{}': terminal too small",
                        name
                    ));
                    self.add_system_message("Increase terminal size or use a different layout");
                    return None;
                }

                // Store new layout
                let old_layout = std::mem::replace(&mut self.layout, new_layout.clone());
                self.baseline_layout = Some(new_layout);

                tracing::info!("Calling sync_layout_to_ui_state to apply changes...");

                // Update positions for existing windows, create new ones, remove old ones
                self.sync_layout_to_ui_state(terminal_width, terminal_height, &old_layout);

                tracing::info!(
                    "After sync: UI state now has {} windows",
                    self.ui_state.windows.len()
                );
                tracing::info!("========== LOAD LAYOUT: '{}' SUCCESS ==========", name);

                self.add_system_message(&format!("Layout '{}' loaded", name));

                // Clear modified flag and update base layout name
                self.layout_modified_since_save = false;
                self.base_layout_name = Some(name.to_string());
                self.needs_render = true;
                return theme_update;
            }
            Err(e) => {
                tracing::error!("Failed to load layout file '{}': {}", name, e);
                tracing::info!("========== LOAD LAYOUT: '{}' FAILED ==========", name);
                self.add_system_message(&format!("Failed to load layout: {}", e));
            }
        }

        None
    }

    /// Resize all windows proportionally based on current terminal size (VellumFE algorithm)
    ///
    /// This command resets to the baseline layout and applies delta-based proportional distribution.
    /// This is the ONLY place (besides initial load) that should perform scaling operations.
    pub fn resize_windows(&mut self, terminal_width: u16, terminal_height: u16) {
        tracing::info!("========== RESIZE WINDOWS START (VellumFE algorithm) ==========");
        tracing::info!(
            "Target terminal size: {}x{}",
            terminal_width,
            terminal_height
        );

        // Get baseline layout (the original, unscaled layout)
        let baseline_layout = if let Some(ref bl) = self.baseline_layout {
            bl.clone()
        } else {
            tracing::error!("No baseline layout available");
            self.add_system_message("Error: No baseline layout - cannot resize");
            self.add_system_message("Load a layout first with .loadlayout");
            return;
        };

        let baseline_width = baseline_layout.terminal_width.unwrap_or(terminal_width);
        let baseline_height = baseline_layout.terminal_height.unwrap_or(terminal_height);

        tracing::info!(
            "Baseline terminal size: {}x{}",
            baseline_width,
            baseline_height
        );

        // Calculate deltas (not scale factors!)
        let width_delta = terminal_width as i32 - baseline_width as i32;
        let height_delta = terminal_height as i32 - baseline_height as i32;

        tracing::info!("Delta: width={:+}, height={:+}", width_delta, height_delta);

        if width_delta == 0 && height_delta == 0 {
            tracing::info!("No resize needed - terminal size matches baseline");
            self.add_system_message("Already at baseline size - no resize needed");
            return;
        }

        // Reset layout to baseline (critical - prevents cumulative scaling errors)
        self.layout = baseline_layout;

        tracing::info!("Reset to baseline layout - now applying proportional distribution...");

        // Categorize widgets by scaling behavior
        let mut static_both = HashSet::new();
        let mut static_height = HashSet::new();
        for window_def in self.layout.windows.iter().filter(|w| w.base().visible) {
            let base = window_def.base();
            match window_def.widget_type() {
                "indicator" => {
                    static_both.insert(base.name.clone());
                }
                "progress" | "countdown" | "hands" | "hand" | "left" | "right" | "spell"
                | "lefthand" | "righthand" | "spellhand" | "command_input" => {
                    static_height.insert(base.name.clone());
                }
                _ => {}
            }
        }

        // Snapshot baseline row positions BEFORE any resizing
        // This ensures width distribution uses original row groupings
        let baseline_rows: Vec<(String, u16, u16)> =
            if let Some(ref baseline) = self.baseline_layout {
                baseline
                    .windows
                    .iter()
                    .filter(|w| w.base().visible)
                    .map(|w| (w.name().to_string(), w.base().row.get(), w.base().rows.get()))
                    .collect()
            } else {
                self.layout
                    .windows
                    .iter()
                    .filter(|w| w.base().visible)
                    .map(|w| (w.base().name.clone(), w.base().row.get(), w.base().rows.get()))
                    .collect()
            };

        // Apply VellumFE's proportional distribution algorithm
        self.apply_height_resize(height_delta, &static_both, &static_height);
        self.apply_width_resize(width_delta, &static_both, &baseline_rows);

        // Update layout terminal size to current
        self.layout.terminal_width = Some(terminal_width);
        self.layout.terminal_height = Some(terminal_height);

        // Apply resized positions to UI state
        for window_def in &self.layout.windows {
            if let Some(window_state) = self.ui_state.windows.get_mut(window_def.name()) {
                let base = window_def.base();
                window_state.position = WindowPosition {
                    x: base.col,
                    y: base.row,
                    width: base.cols,
                    height: base.rows,
                };
                match &mut window_state.content {
                    WindowContent::Text(text) => {
                        text.title = base.title.clone().unwrap_or_default();
                    }
                    WindowContent::Inventory(text) => {
                        text.title = base.title.clone().unwrap_or_default();
                    }
                    WindowContent::Reserve(text) => {
                        text.title = base.title.clone().unwrap_or_default();
                    }
                    WindowContent::Spells(text) => {
                        text.title = base.title.clone().unwrap_or_default();
                    }
                    _ => {}
                }
                tracing::debug!(
                    "Applied to UI: '{}' @ ({},{}) size {}x{}",
                    base.name,
                    base.col.get(),
                    base.row.get(),
                    base.cols.get(),
                    base.rows.get()
                );
            }
        }

        self.needs_render = true;
        self.add_system_message(&format!(
            "Resized to {}x{} - use .savelayout to save",
            terminal_width, terminal_height
        ));
        tracing::info!("========== RESIZE WINDOWS COMPLETE ==========");
    }

    /// Helper to get minimum widget size based on widget type (from VellumFE)
    fn widget_min_size(&self, widget_type: &str) -> (u16, u16) {
        match widget_type {
            "indicator" => (2, 1),
            "progress" | "countdown" | "hands" | "hand" => (10, 1),
            "compass" => (13, 5),
            "map" => (10, 5),
            "injury_doll" => (20, 10),
            "dashboard" => (15, 3),
            "command_input" => (20, 1),
            "quickbar" => (20, 1),
            "hotkeybar" => (20, 1),
            _ => (5, 3), // text, room, tabbed, etc.
        }
    }

    pub fn window_min_size(&self, window_name: &str) -> (u16, u16) {
        if let Some(window_def) = self.layout.windows.iter().find(|w| w.name() == window_name) {
            let (default_min_cols, default_min_rows) =
                self.widget_min_size(window_def.widget_type());
            let base = window_def.base();
            let min_cols = base.min_cols.unwrap_or(default_min_cols);
            let min_rows = base.min_rows.unwrap_or(default_min_rows);
            (min_cols, min_rows)
        } else {
            self.widget_min_size("text")
        }
    }

    /// Apply proportional height resize (from VellumFE apply_height_resize)
    /// Adapted for WindowDef enum structure
    fn apply_height_resize(
        &mut self,
        height_delta: i32,
        static_both: &HashSet<String>,
        static_height: &HashSet<String>,
    ) {
        if height_delta == 0 {
            return;
        }

        tracing::debug!("--- HEIGHT SCALING (VellumFE COLUMN-BY-COLUMN) ---");
        tracing::debug!("height_delta={}", height_delta);

        // Snapshot baseline rows for calculation (so repeated columns don't amplify deltas)
        let baseline_rows: HashMap<String, (u16, u16)> = self
            .layout
            .windows
            .iter()
            .filter(|w| w.base().visible)
            .map(|w| {
                let base = w.base();
                (base.name.clone(), (base.row.get(), base.rows.get()))
            })
            .collect();

        // Find max column
        let max_col = self
            .layout
            .windows
            .iter()
            .filter(|w| w.base().visible)
            .map(|w| {
                let base = w.base();
                base.col.get().saturating_add(base.cols.get())
            })
            .max()
            .unwrap_or(0);

        tracing::debug!("Processing columns 0..{}", max_col);

        // Track which windows have already had their delta applied
        let mut height_applied = HashSet::new();

        // Column-by-column: Calculate and immediately apply height deltas
        for current_col in 0..max_col {
            // Find all windows that occupy this column
            let mut windows_at_col: Vec<String> = self
                .layout
                .windows
                .iter()
                .filter(|w| w.base().visible)
                .filter_map(|w| {
                    let base = w.base();
                    if base.col.get() <= current_col
                        && base.col.get().saturating_add(base.cols.get()) > current_col
                    {
                        Some(base.name.clone())
                    } else {
                        None
                    }
                })
                .collect();

            if windows_at_col.is_empty() {
                continue;
            }

            tracing::debug!(
                "Column {}: {} windows present",
                current_col,
                windows_at_col.len()
            );

            // Build the axis tracks for this column (in row order) and let the
            // shared distribute_1d compute the new sizes. This replaces the
            // former hand-rolled proportional + leftover + remainder logic with
            // one conserving pass (see distribute_1d). Sort by baseline row so
            // proportional order matches the cascade order below.
            windows_at_col.sort_by_key(|name| baseline_rows.get(name).map(|(r, _)| *r).unwrap_or(0));

            let tracks: Vec<Track> = windows_at_col
                .iter()
                .map(|name| {
                    let (_, base_rows) = baseline_rows.get(name).copied().unwrap_or((0, 0));
                    let is_static = static_both.contains(name.as_str())
                        || static_height.contains(name.as_str());
                    let (min_c, max_c) = if let Some(w) =
                        self.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let base = w.base();
                        let (_, min_rows) = self.widget_min_size(w.widget_type());
                        (base.min_rows.unwrap_or(min_rows), base.max_rows)
                    } else {
                        (0, None)
                    };
                    Track {
                        name: name.clone(),
                        base_size: base_rows,
                        min: min_c,
                        max: max_c,
                        is_static,
                    }
                })
                .collect();

            // If nothing in this column can scale, skip it.
            if tracks.iter().all(|t| t.is_static) {
                continue;
            }

            let new_sizes = distribute_1d(&tracks, height_delta);
            let col_height_deltas: HashMap<String, i32> = tracks
                .iter()
                .zip(new_sizes.iter())
                .map(|(t, &new_size)| (t.name.clone(), new_size as i32 - t.base_size as i32))
                .collect();

            // Cascade and apply (discarding deltas for already-applied windows)
            let mut windows_at_col_with_meta: Vec<(String, u16, u16)> = self
                .layout
                .windows
                .iter()
                .filter(|w| w.base().visible)
                .filter_map(|w| {
                    let base = w.base();
                    if base.col.get() <= current_col
                        && base.col.get().saturating_add(base.cols.get()) > current_col
                    {
                        let (orig_row, orig_rows) = baseline_rows
                            .get(&base.name)
                            .copied()
                            .unwrap_or((base.row.get(), base.rows.get()));
                        Some((base.name.clone(), orig_row, orig_rows))
                    } else {
                        None
                    }
                })
                .collect();

            windows_at_col_with_meta.sort_by_key(|(_, row, _)| *row);

            let mut current_row = windows_at_col_with_meta[0].1;
            let win_count = windows_at_col_with_meta.len();

            // Position pass: distribute_1d already produced final, clamped,
            // conserved sizes, so this loop is a pure cascade — assign each
            // window its new size (baseline + delta) and stack it directly
            // below the previous one. A window spanning multiple columns is
            // sized once (tracked in height_applied); at later columns we only
            // advance the cascade cursor past it.
            for idx in 0..win_count {
                let (window_name, _original_row, original_rows) =
                    windows_at_col_with_meta[idx].clone();

                if height_applied.contains(&window_name) {
                    // Already sized at an earlier column: keep it, advance cursor.
                    if let Some(w) = self.layout.windows.iter().find(|w| w.name() == window_name) {
                        current_row = w.base().row.get().saturating_add(w.base().rows.get());
                    }
                    continue;
                }

                let assigned_delta = *col_height_deltas.get(&window_name).unwrap_or(&0);
                let new_rows = (original_rows as i32 + assigned_delta).max(0) as u16;

                if let Some(w) = self
                    .layout
                    .windows
                    .iter_mut()
                    .find(|w| w.name() == window_name)
                {
                    let base = w.base_mut();
                    base.row = crate::data::geometry::Row::new(current_row);
                    base.rows = crate::data::geometry::Height::new(new_rows);
                    height_applied.insert(window_name.clone());
                }

                current_row += new_rows;
            }
        }

        tracing::debug!("Height resize complete");
    }

    /// Apply proportional width resize (from VellumFE apply_width_resize)
    /// Adapted for WindowDef enum structure
    /// baseline_rows: Vec of (name, baseline_row, baseline_rows) for grouping windows by original row
    fn apply_width_resize(
        &mut self,
        width_delta: i32,
        static_both: &HashSet<String>,
        _baseline_rows: &[(String, u16, u16)],
    ) {
        if width_delta == 0 {
            return;
        }

        tracing::debug!("--- WIDTH SCALING (VellumFE ROW-BY-ROW) ---");
        tracing::debug!("width_delta={}", width_delta);

        // Snapshot baseline cols for calculation (freeze widths during distribution)
        let baseline_cols: HashMap<String, (u16, u16)> = self
            .layout
            .windows
            .iter()
            .filter(|w| w.base().visible)
            .map(|w| {
                let base = w.base();
                (base.name.clone(), (base.col.get(), base.cols.get()))
            })
            .collect();

        // Find max row
        let max_row = self
            .layout
            .windows
            .iter()
            .filter(|w| w.base().visible)
            .map(|w| {
                let base = w.base();
                base.row.get().saturating_add(base.rows.get())
            })
            .max()
            .unwrap_or(0);

        tracing::debug!("Processing rows 0..{}", max_row);

        // Track which windows have already had their delta applied
        let mut width_applied = HashSet::new();

        // Row-by-row: Calculate and immediately apply width deltas
        for current_row in 0..max_row {
            // Find all windows that occupy this row
            let mut windows_at_row: Vec<String> = self
                .layout
                .windows
                .iter()
                .filter(|w| w.base().visible)
                .filter_map(|w| {
                    let base = w.base();
                    if base.row.get() <= current_row
                        && base.row.get().saturating_add(base.rows.get()) > current_row
                    {
                        Some(base.name.clone())
                    } else {
                        None
                    }
                })
                .collect();

            if windows_at_row.is_empty() {
                continue;
            }

            tracing::debug!(
                "Row {}: {} windows present",
                current_row,
                windows_at_row.len()
            );

            // Build the axis tracks for this row (in column order) and let the
            // shared distribute_1d compute the new widths — the transpose of the
            // height pass. Replaces the former proportional + leftover +
            // remainder logic with one conserving pass.
            windows_at_row.sort_by_key(|name| baseline_cols.get(name).map(|(c, _)| *c).unwrap_or(0));

            let tracks: Vec<Track> = windows_at_row
                .iter()
                .map(|name| {
                    let (_, base_cols) = baseline_cols.get(name).copied().unwrap_or((0, 0));
                    // Width has only the `static_both` category (no static_width).
                    let is_static = static_both.contains(name.as_str());
                    let (min_c, max_c) = if let Some(w) =
                        self.layout.windows.iter().find(|w| w.name() == name)
                    {
                        let base = w.base();
                        let (min_cols, _) = self.widget_min_size(w.widget_type());
                        (base.min_cols.unwrap_or(min_cols), base.max_cols)
                    } else {
                        (0, None)
                    };
                    Track {
                        name: name.clone(),
                        base_size: base_cols,
                        min: min_c,
                        max: max_c,
                        is_static,
                    }
                })
                .collect();

            if tracks.iter().all(|t| t.is_static) {
                continue;
            }

            let new_sizes = distribute_1d(&tracks, width_delta);
            let row_width_deltas: HashMap<String, i32> = tracks
                .iter()
                .zip(new_sizes.iter())
                .map(|(t, &new_size)| (t.name.clone(), new_size as i32 - t.base_size as i32))
                .collect();

            // Cascade and apply (discarding deltas for already-applied windows)
            let mut windows_at_row_with_meta: Vec<(String, u16, u16)> = self
                .layout
                .windows
                .iter()
                .filter(|w| w.base().visible)
                .filter_map(|w| {
                    let base = w.base();
                    if base.row.get() <= current_row
                        && base.row.get().saturating_add(base.rows.get()) > current_row
                    {
                        let (orig_col, orig_cols) = baseline_cols
                            .get(&base.name)
                            .copied()
                            .unwrap_or((base.col.get(), base.cols.get()));
                        Some((base.name.clone(), orig_col, orig_cols))
                    } else {
                        None
                    }
                })
                .collect();

            windows_at_row_with_meta.sort_by_key(|(_, col, _)| *col);

            let mut current_col_pos = windows_at_row_with_meta[0].1;
            let win_count = windows_at_row_with_meta.len();

            // Position pass: distribute_1d already produced final, clamped,
            // conserved widths, so this is a pure cascade (transpose of the
            // height positioner). A window spanning multiple rows is sized once
            // (width_applied); at later rows we only advance the cursor.
            for idx in 0..win_count {
                let (window_name, _original_col, original_cols) =
                    windows_at_row_with_meta[idx].clone();

                if width_applied.contains(&window_name) {
                    if let Some(w) = self.layout.windows.iter().find(|w| w.name() == window_name) {
                        current_col_pos = w.base().col.get().saturating_add(w.base().cols.get());
                    }
                    continue;
                }

                let assigned_delta = *row_width_deltas.get(&window_name).unwrap_or(&0);
                let new_cols = (original_cols as i32 + assigned_delta).max(0) as u16;

                if let Some(w) = self
                    .layout
                    .windows
                    .iter_mut()
                    .find(|w| w.name() == window_name)
                {
                    let base = w.base_mut();
                    base.col = crate::data::geometry::Col::new(current_col_pos);
                    base.cols = crate::data::geometry::Width::new(new_cols);
                    width_applied.insert(window_name.clone());
                }

                current_col_pos += new_cols;
            }
        }

        tracing::debug!("Width resize complete");
    }

    /// Sync layout WindowDefs to ui_state WindowStates without destroying content
    ///
    /// Uses exact positions from layout file.
    /// Use .resize command for delta-based proportional scaling.
    pub fn sync_layout_to_ui_state(
        &mut self,
        terminal_width: u16,
        terminal_height: u16,
        _old_layout: &Layout,
    ) {
        tracing::info!("--- sync_layout_to_ui_state START ---");
        tracing::info!("Terminal size: {}x{}", terminal_width, terminal_height);
        tracing::info!("New layout has {} windows", self.layout.windows.len());

        // Use exact positions from layout file
        tracing::debug!("Using exact positions from layout file");

        // Track which windows are in the new layout AND visible
        let new_window_names: std::collections::HashSet<String> = self
            .layout
            .windows
            .iter()
            .filter(|w| w.base().visible)
            .map(|w| w.name().to_string())
            .collect();

        tracing::info!("Visible windows in new layout: {:?}", new_window_names);

        let current_window_names: std::collections::HashSet<String> =
            self.ui_state.windows.keys().cloned().collect();
        tracing::info!("Windows currently in UI state: {:?}", current_window_names);

        // Collect windows to create (can't create while iterating due to borrow checker)
        let mut windows_to_create: Vec<crate::config::WindowDef> = Vec::new();
        let mut windows_to_update = 0;

        // Update existing windows' positions
        for window_def in &self.layout.windows {
            let window_name = window_def.name().to_string();
            let base = window_def.base();

            // Skip hidden windows
            if !base.visible {
                tracing::debug!("Skipping hidden window '{}'", window_name);
                continue;
            }

            // Use exact position from layout file
            let position = WindowPosition {
                x: base.col,
                y: base.row,
                width: base.cols,
                height: base.rows,
            };

            tracing::debug!(
                "Processing window '{}': exact pos=({},{}) size={}x{}",
                window_name,
                position.x.get(),
                position.y.get(),
                position.width.get(),
                position.height.get()
            );

            if let Some(window_state) = self.ui_state.windows.get_mut(&window_name) {
                // Window exists - just update position (preserve content!)
                let old_pos = window_state.position.clone();
                window_state.position = position.clone();
                windows_to_update += 1;
                tracing::info!(
                    "UPDATING window '{}': pos ({},{})ΓåÆ({},{}) size {}x{}ΓåÆ{}x{}",
                    window_name,
                    old_pos.x.get(),
                    old_pos.y.get(),
                    position.x.get(),
                    position.y.get(),
                    old_pos.width.get(),
                    old_pos.height.get(),
                    position.width.get(),
                    position.height.get()
                );
            } else {
                // Window doesn't exist - queue for creation
                tracing::info!(
                    "Window '{}' not in UI state - queuing for creation",
                    window_name
                );
                windows_to_create.push(window_def.clone());
            }
        }

        tracing::info!(
            "Summary: {} windows to update, {} windows to create",
            windows_to_update,
            windows_to_create.len()
        );

        // Create new windows
        if !windows_to_create.is_empty() {
            tracing::info!("Creating {} new windows...", windows_to_create.len());
            for window_def in windows_to_create {
                let window_name = window_def.name().to_string();
                tracing::info!(
                    "CREATING window '{}' ({})",
                    window_name,
                    window_def.widget_type()
                );
                self.add_new_window(&window_def, terminal_width, terminal_height);
            }
        }

        // Remove windows that are no longer in the layout
        let windows_to_remove: Vec<String> = self
            .ui_state
            .windows
            .keys()
            .filter(|name| !new_window_names.contains(*name))
            .cloned()
            .collect();

        if !windows_to_remove.is_empty() {
            tracing::info!(
                "Removing {} windows not in new layout: {:?}",
                windows_to_remove.len(),
                windows_to_remove
            );
            for window_name in windows_to_remove {
                self.ui_state.remove_window(&window_name);
                tracing::info!("REMOVED window '{}'", window_name);
            }
        } else {
            tracing::info!("No windows to remove");
        }

        tracing::info!("--- sync_layout_to_ui_state COMPLETE ---");
    }

    /// Load a saved layout with terminal size for immediate reinitialization
    pub fn load_layout_with_size(&mut self, name: &str, width: u16, height: u16) {
        let layout_path = match Config::layout_path(name) {
            Ok(path) => path,
            Err(e) => {
                self.add_system_message(&format!("Failed to get layout path: {}", e));
                return;
            }
        };

        match Layout::load_from_file(&layout_path) {
            Ok(new_layout) => {
                self.apply_layout_theme(new_layout.theme.as_deref());
                self.layout = new_layout.clone();
                self.baseline_layout = Some(new_layout);
                self.add_system_message(&format!("Layout '{}' loaded", name));

                // Clear modified flag and update base layout name
                self.layout_modified_since_save = false;
                self.base_layout_name = Some(name.to_string());

                // Reinitialize windows from new layout with actual terminal size
                self.init_windows(width, height);
                self.needs_render = true;

                // Signal frontend to reset widget caches
                self.ui_state.needs_widget_reset = true;
            }
            Err(e) => self.add_system_message(&format!("Failed to load layout: {}", e)),
        }
    }

    /// List all saved layouts
    pub(super) fn list_layouts(&mut self) {
        match Config::list_layouts() {
            Ok(layouts) => {
                if layouts.is_empty() {
                    self.add_system_message("No saved layouts");
                } else {
                    self.add_system_message(&format!("=== Saved Layouts ({}) ===", layouts.len()));
                    for layout in layouts {
                        self.add_system_message(&format!("  {}", layout));
                    }
                }
            }
            Err(e) => self.add_system_message(&format!("Failed to list layouts: {}", e)),
        }
    }

    /// Fallback for the `.resize` dot-command in frontends without a cell grid
    /// (GUI, headless). Resizes the layout to its own stored terminal size —
    /// a zero delta in practice — routed through the single resize algorithm.
    /// The TUI's real `.resize` handler calls `resize_windows` directly with
    /// the actual terminal size (see frontend/tui/input_handlers.rs).
    pub(super) fn resize_to_current_terminal(&mut self) {
        let width = self.layout.terminal_width.unwrap_or(80);
        let height = self.layout.terminal_height.unwrap_or(24);
        self.resize_windows(width, height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BorderSides, Layout, SpacerWidgetData, WindowBase, WindowDef};

    // ========== Test helpers ==========

    /// Helper to create a minimal WindowBase for testing
    fn test_window_base(name: &str, col: u16, row: u16, cols: u16, rows: u16) -> WindowBase {
        WindowBase {
            name: name.to_string(),
            row: crate::data::geometry::Row::new(row),
            col: crate::data::geometry::Col::new(col),
            rows: crate::data::geometry::Height::new(rows),
            cols: crate::data::geometry::Width::new(cols),
            show_border: false,
            border_style: "single".to_string(),
            border_sides: BorderSides::default(),
            border_color: None,
            show_title: false,
            title: None,
            background_color: None,
            text_color: None,
            transparent_background: false,
            locked: false,
            min_rows: None,
            max_rows: None,
            min_cols: None,
            max_cols: None,
            visible: true,
            content_align: None,
            tts_speak: false,
            text_size: None,
            font_family: None,
            title_position: "top-left".to_string(),
        }
    }

    fn test_layout_empty() -> Layout {
        Layout {
            windows: vec![],
            terminal_width: Some(80),
            terminal_height: Some(24),
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
        }
    }

    fn test_layout_with_windows(windows: Vec<WindowDef>) -> Layout {
        Layout {
            windows,
            terminal_width: Some(80),
            terminal_height: Some(24),
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
        }
    }

    // ========== Widget min size tests ==========

    /// Helper that replicates widget_min_size logic for testing
    fn widget_min_size_standalone(widget_type: &str) -> (u16, u16) {
        match widget_type {
            "indicator" => (2, 1),
            "progress" | "countdown" | "hands" | "hand" => (10, 1),
            "compass" => (13, 5),
            "map" => (10, 5),
            "injury_doll" => (20, 10),
            "dashboard" => (15, 3),
            "command_input" => (20, 1),
            "quickbar" => (20, 1),
            "hotkeybar" => (20, 1),
            _ => (5, 3), // text, room, tabbed, etc.
        }
    }

    /// Guard: the standalone replica above must stay in sync with the real
    /// `AppCore::widget_min_size`. This copy silently drifted once already
    /// (it was missing the "map" arm), so pin every type against the source.
    #[test]
    fn widget_min_size_standalone_matches_real_impl() {
        let core = AppCore::new_for_test();
        for wt in [
            "indicator",
            "progress",
            "countdown",
            "hands",
            "hand",
            "compass",
            "map",
            "injury_doll",
            "dashboard",
            "command_input",
            "quickbar",
            "hotkeybar",
            "text",
            "room",
            "tabbedtext",
            "unknown_type",
        ] {
            assert_eq!(
                widget_min_size_standalone(wt),
                core.widget_min_size(wt),
                "widget_min_size mismatch for '{wt}' — standalone replica drifted from the real impl"
            );
        }
    }

    #[test]
    fn test_widget_min_size_indicator() {
        let (cols, rows) = widget_min_size_standalone("indicator");
        assert_eq!(cols, 2);
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_widget_min_size_progress() {
        let (cols, rows) = widget_min_size_standalone("progress");
        assert_eq!(cols, 10);
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_widget_min_size_countdown() {
        let (cols, rows) = widget_min_size_standalone("countdown");
        assert_eq!(cols, 10);
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_widget_min_size_hands() {
        let (cols, rows) = widget_min_size_standalone("hands");
        assert_eq!(cols, 10);
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_widget_min_size_hand() {
        let (cols, rows) = widget_min_size_standalone("hand");
        assert_eq!(cols, 10);
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_widget_min_size_compass() {
        let (cols, rows) = widget_min_size_standalone("compass");
        assert_eq!(cols, 13);
        assert_eq!(rows, 5);
    }

    #[test]
    fn test_widget_min_size_injury_doll() {
        let (cols, rows) = widget_min_size_standalone("injury_doll");
        assert_eq!(cols, 20);
        assert_eq!(rows, 10);
    }

    #[test]
    fn test_widget_min_size_dashboard() {
        let (cols, rows) = widget_min_size_standalone("dashboard");
        assert_eq!(cols, 15);
        assert_eq!(rows, 3);
    }

    #[test]
    fn test_widget_min_size_command_input() {
        let (cols, rows) = widget_min_size_standalone("command_input");
        assert_eq!(cols, 20);
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_widget_min_size_quickbar() {
        let (cols, rows) = widget_min_size_standalone("quickbar");
        assert_eq!(cols, 20);
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_widget_min_size_text() {
        let (cols, rows) = widget_min_size_standalone("text");
        assert_eq!(cols, 5);
        assert_eq!(rows, 3);
    }

    #[test]
    fn test_widget_min_size_room() {
        let (cols, rows) = widget_min_size_standalone("room");
        assert_eq!(cols, 5);
        assert_eq!(rows, 3);
    }

    #[test]
    fn test_widget_min_size_tabbed() {
        let (cols, rows) = widget_min_size_standalone("tabbed");
        assert_eq!(cols, 5);
        assert_eq!(rows, 3);
    }

    #[test]
    fn test_widget_min_size_unknown() {
        let (cols, rows) = widget_min_size_standalone("unknown_type");
        assert_eq!(cols, 5);
        assert_eq!(rows, 3);
    }

    // ========== Static widget categorization tests ==========

    /// Widget types that should not resize (both dimensions)
    fn is_static_both(widget_type: &str) -> bool {
        matches!(widget_type, "indicator")
    }

    /// Widget types that should not resize height
    fn is_static_height(widget_type: &str) -> bool {
        matches!(
            widget_type,
            "progress"
                | "countdown"
                | "hands"
                | "hand"
                | "left"
                | "right"
                | "spell"
                | "lefthand"
                | "righthand"
                | "spellhand"
                | "command_input"
                | "quickbar"
        )
    }

    #[test]
    fn test_compass_is_scalable() {
        assert!(!is_static_both("compass"));
    }

    #[test]
    fn test_injury_doll_is_scalable() {
        assert!(!is_static_both("injury_doll"));
    }

    #[test]
    fn test_dashboard_is_scalable() {
        // Dashboard should scale with terminal resize (not static)
        assert!(!is_static_both("dashboard"));
    }

    #[test]
    fn test_static_both_indicator() {
        assert!(is_static_both("indicator"));
    }

    #[test]
    fn test_static_both_text_is_scalable() {
        assert!(!is_static_both("text"));
    }

    #[test]
    fn test_static_both_room_is_scalable() {
        assert!(!is_static_both("room"));
    }

    #[test]
    fn test_static_height_progress() {
        assert!(is_static_height("progress"));
    }

    #[test]
    fn test_static_height_countdown() {
        assert!(is_static_height("countdown"));
    }

    #[test]
    fn test_static_height_hands() {
        assert!(is_static_height("hands"));
        assert!(is_static_height("hand"));
        assert!(is_static_height("lefthand"));
        assert!(is_static_height("righthand"));
        assert!(is_static_height("spellhand"));
    }

    #[test]
    fn test_static_height_command_input() {
        assert!(is_static_height("command_input"));
    }

    #[test]
    fn test_static_height_quickbar() {
        assert!(is_static_height("quickbar"));
    }

    #[test]
    fn test_static_height_text_is_scalable() {
        assert!(!is_static_height("text"));
    }

    // ========== Layout structure tests ==========

    #[test]
    fn test_empty_layout() {
        let layout = test_layout_empty();
        assert!(layout.windows.is_empty());
        assert_eq!(layout.terminal_width, Some(80));
        assert_eq!(layout.terminal_height, Some(24));
    }

    #[test]
    fn test_layout_with_theme() {
        let mut layout = test_layout_empty();
        layout.theme = Some("dark".to_string());
        assert_eq!(layout.theme, Some("dark".to_string()));
    }

    #[test]
    fn test_layout_terminal_size() {
        let mut layout = test_layout_empty();
        layout.terminal_width = Some(120);
        layout.terminal_height = Some(40);
        assert_eq!(layout.terminal_width, Some(120));
        assert_eq!(layout.terminal_height, Some(40));
    }

    // ========== Delta calculation tests ==========

    #[test]
    fn test_delta_calculation_grow() {
        let baseline_width: u16 = 80;
        let baseline_height: u16 = 24;
        let terminal_width: u16 = 100;
        let terminal_height: u16 = 30;

        let width_delta = terminal_width as i32 - baseline_width as i32;
        let height_delta = terminal_height as i32 - baseline_height as i32;

        assert_eq!(width_delta, 20);
        assert_eq!(height_delta, 6);
    }

    #[test]
    fn test_delta_calculation_shrink() {
        let baseline_width: u16 = 100;
        let baseline_height: u16 = 40;
        let terminal_width: u16 = 80;
        let terminal_height: u16 = 24;

        let width_delta = terminal_width as i32 - baseline_width as i32;
        let height_delta = terminal_height as i32 - baseline_height as i32;

        assert_eq!(width_delta, -20);
        assert_eq!(height_delta, -16);
    }

    #[test]
    fn test_delta_calculation_no_change() {
        let baseline_width: u16 = 80;
        let baseline_height: u16 = 24;
        let terminal_width: u16 = 80;
        let terminal_height: u16 = 24;

        let width_delta = terminal_width as i32 - baseline_width as i32;
        let height_delta = terminal_height as i32 - baseline_height as i32;

        assert_eq!(width_delta, 0);
        assert_eq!(height_delta, 0);
    }

    // ========== Proportional distribution tests ==========

    #[test]
    fn test_proportional_share_calculation() {
        // If window is 40 cols out of 80 total, it gets 50% of delta
        let window_cols: u16 = 40;
        let total_cols: u16 = 80;
        let width_delta: i32 = 20;

        let proportion = window_cols as f64 / total_cols as f64;
        let share = (proportion * width_delta as f64).floor() as i32;

        assert_eq!(proportion, 0.5);
        assert_eq!(share, 10);
    }

    #[test]
    fn test_proportional_share_uneven() {
        // If window is 30 cols out of 80 total (37.5%)
        let window_cols: u16 = 30;
        let total_cols: u16 = 80;
        let width_delta: i32 = 16;

        let proportion = window_cols as f64 / total_cols as f64;
        let share = (proportion * width_delta as f64).floor() as i32;

        assert_eq!(proportion, 0.375);
        assert_eq!(share, 6); // floor(0.375 * 16) = floor(6) = 6
    }

    #[test]
    fn test_proportional_share_negative_delta() {
        // Shrinking: 40 cols out of 80 total, -20 delta
        let window_cols: u16 = 40;
        let total_cols: u16 = 80;
        let width_delta: i32 = -20;

        let proportion = window_cols as f64 / total_cols as f64;
        let share = (proportion * width_delta as f64).floor() as i32;

        assert_eq!(share, -10);
    }

    // ========== Min constraint tests ==========

    #[test]
    fn test_min_constraint_applied() {
        let original_cols: u16 = 20;
        let delta: i32 = -25; // Would make it -5
        let min_cols: u16 = 5;

        let new_cols = ((original_cols as i32 + delta).max(min_cols as i32)) as u16;

        assert_eq!(new_cols, 5); // Clamped to minimum
    }

    #[test]
    fn test_min_constraint_not_needed() {
        let original_cols: u16 = 20;
        let delta: i32 = -10; // Would make it 10
        let min_cols: u16 = 5;

        let new_cols = ((original_cols as i32 + delta).max(min_cols as i32)) as u16;

        assert_eq!(new_cols, 10); // No clamping needed
    }

    // ========== Max constraint tests ==========

    #[test]
    fn test_max_constraint_applied() {
        let original_cols: u16 = 20;
        let delta: i32 = 30; // Would make it 50
        let max_cols: Option<u16> = Some(40);

        let mut new_cols = (original_cols as i32 + delta) as u16;
        if let Some(max) = max_cols {
            new_cols = new_cols.min(max);
        }

        assert_eq!(new_cols, 40); // Clamped to maximum
    }

    #[test]
    fn test_max_constraint_not_needed() {
        let original_cols: u16 = 20;
        let delta: i32 = 10; // Would make it 30
        let max_cols: Option<u16> = Some(40);

        let mut new_cols = (original_cols as i32 + delta) as u16;
        if let Some(max) = max_cols {
            new_cols = new_cols.min(max);
        }

        assert_eq!(new_cols, 30); // No clamping needed
    }

    #[test]
    fn test_no_max_constraint() {
        let original_cols: u16 = 20;
        let delta: i32 = 100; // Would make it 120
        let max_cols: Option<u16> = None;

        let mut new_cols = (original_cols as i32 + delta) as u16;
        if let Some(max) = max_cols {
            new_cols = new_cols.min(max);
        }

        assert_eq!(new_cols, 120); // No maximum constraint
    }

    // ========== Terminal size validation tests ==========

    #[test]
    fn test_terminal_size_sufficient() {
        let window_col: u16 = 0;
        let window_cols: u16 = 80;
        let window_row: u16 = 0;
        let window_rows: u16 = 24;
        let terminal_width: u16 = 80;
        let terminal_height: u16 = 24;

        let required_width = window_col + window_cols;
        let required_height = window_row + window_rows;

        let fits = terminal_width >= required_width && terminal_height >= required_height;
        assert!(fits);
    }

    #[test]
    fn test_terminal_size_too_small_width() {
        let window_col: u16 = 0;
        let window_cols: u16 = 100;
        let terminal_width: u16 = 80;

        let required_width = window_col + window_cols;
        let fits = terminal_width >= required_width;
        assert!(!fits);
    }

    #[test]
    fn test_terminal_size_too_small_height() {
        let window_row: u16 = 10;
        let window_rows: u16 = 20;
        let terminal_height: u16 = 24;

        let required_height = window_row + window_rows;
        let fits = terminal_height >= required_height;
        assert!(!fits); // 10 + 20 = 30 > 24
    }

    #[test]
    fn test_terminal_size_offset_window() {
        // Window at position (20, 10) with size 40x10
        let window_col: u16 = 20;
        let window_cols: u16 = 40;
        let window_row: u16 = 10;
        let window_rows: u16 = 10;
        let terminal_width: u16 = 80;
        let terminal_height: u16 = 24;

        let required_width = window_col + window_cols; // 20 + 40 = 60
        let required_height = window_row + window_rows; // 10 + 10 = 20

        let fits = terminal_width >= required_width && terminal_height >= required_height;
        assert!(fits); // 60 <= 80 and 20 <= 24
    }

    // ========== WindowDef extraction tests ==========

    #[test]
    fn test_window_def_name() {
        let base = test_window_base("main", 0, 0, 80, 24);
        let spacer = WindowDef::Spacer {
            base,
            data: SpacerWidgetData {},
        };
        assert_eq!(spacer.name(), "main");
    }

    #[test]
    fn test_window_def_widget_type_spacer() {
        let base = test_window_base("spacer_1", 0, 0, 5, 1);
        let spacer = WindowDef::Spacer {
            base,
            data: SpacerWidgetData {},
        };
        assert_eq!(spacer.widget_type(), "spacer");
    }

    #[test]
    fn test_window_def_base_position() {
        let base = test_window_base("test", 10, 20, 40, 15);
        let spacer = WindowDef::Spacer {
            base,
            data: SpacerWidgetData {},
        };
        let b = spacer.base();
        assert_eq!(b.col.get(), 10);
        assert_eq!(b.row.get(), 20);
        assert_eq!(b.cols.get(), 40);
        assert_eq!(b.rows.get(), 15);
    }

    #[test]
    fn test_window_base_visibility() {
        let mut base = test_window_base("test", 0, 0, 10, 10);
        assert!(base.visible);

        base.visible = false;
        assert!(!base.visible);
    }

    // ========== Layout window collection tests ==========

    #[test]
    fn test_layout_window_names() {
        let windows = vec![
            WindowDef::Spacer {
                base: test_window_base("main", 0, 0, 60, 20),
                data: SpacerWidgetData {},
            },
            WindowDef::Spacer {
                base: test_window_base("sidebar", 60, 0, 20, 20),
                data: SpacerWidgetData {},
            },
        ];
        let layout = test_layout_with_windows(windows);

        let names: Vec<&str> = layout.windows.iter().map(|w| w.name()).collect();
        assert_eq!(names, vec!["main", "sidebar"]);
    }

    #[test]
    fn test_layout_visible_windows() {
        let mut hidden_base = test_window_base("hidden", 0, 0, 10, 10);
        hidden_base.visible = false;

        let windows = vec![
            WindowDef::Spacer {
                base: test_window_base("visible1", 0, 0, 40, 20),
                data: SpacerWidgetData {},
            },
            WindowDef::Spacer {
                base: hidden_base,
                data: SpacerWidgetData {},
            },
            WindowDef::Spacer {
                base: test_window_base("visible2", 40, 0, 40, 20),
                data: SpacerWidgetData {},
            },
        ];
        let layout = test_layout_with_windows(windows);

        let visible_count = layout.windows.iter().filter(|w| w.base().visible).count();
        assert_eq!(visible_count, 2);
    }

    // ========== Window position tests ==========

    #[test]
    fn test_window_position_clone() {
        let pos = WindowPosition {
            x: crate::data::geometry::Col::new(10),
            y: crate::data::geometry::Row::new(20),
            width: crate::data::geometry::Width::new(80),
            height: crate::data::geometry::Height::new(24),
        };
        let cloned = pos.clone();
        assert_eq!(pos.x, cloned.x);
        assert_eq!(pos.y, cloned.y);
        assert_eq!(pos.width, cloned.width);
        assert_eq!(pos.height, cloned.height);
    }

    #[test]
    fn test_window_position_from_base() {
        let base = test_window_base("test", 15, 5, 50, 18);
        let pos = WindowPosition {
            x: base.col,
            y: base.row,
            width: base.cols,
            height: base.rows,
        };
        assert_eq!(pos.x.get(), 15);
        assert_eq!(pos.y.get(), 5);
        assert_eq!(pos.width.get(), 50);
        assert_eq!(pos.height.get(), 18);
    }

    // ========== Remainder distribution tests ==========

    #[test]
    fn test_remainder_positive() {
        let total_delta: i32 = 17;
        let distributed: i32 = 15; // Proportional distribution gave 15
        let remainder = total_delta - distributed;
        assert_eq!(remainder, 2); // 2 extra columns to distribute
    }

    #[test]
    fn test_remainder_negative() {
        let total_delta: i32 = -17;
        let distributed: i32 = -15; // Proportional distribution gave -15
        let remainder = total_delta - distributed;
        assert_eq!(remainder, -2); // 2 columns still to remove
    }

    #[test]
    fn test_remainder_zero() {
        let total_delta: i32 = 20;
        let distributed: i32 = 20;
        let remainder = total_delta - distributed;
        assert_eq!(remainder, 0); // Perfect distribution
    }

    // ================================================================
    // Characterization tests for the REAL resize cascade.
    //
    // Unlike the arithmetic-in-a-vacuum tests above, these build a real
    // AppCore + Layout, set a baseline, call the actual resize_windows(),
    // and assert on the resulting window geometry. They pin the cascade
    // behaviors that were previously untested (proportional distribution,
    // row cascade, min-clamp remainder recoupling, delta-from-baseline
    // idempotence, multi-column apply-once, static widgets).
    // ================================================================

    use crate::core::AppCore;

    /// Build an AppCore whose `layout` and `baseline_layout` are the given
    /// windows at the given baseline terminal size, with ui_state populated
    /// so the write-back path runs. Returns the ready-to-resize core.
    fn core_with_baseline(windows: Vec<WindowDef>, base_w: u16, base_h: u16) -> AppCore {
        let layout = Layout {
            windows,
            terminal_width: Some(base_w),
            terminal_height: Some(base_h),
            base_layout: None,
            theme: None,
            unknown_windows: Vec::new(),
        };
        let mut core = AppCore::new_for_test();
        for def in &layout.windows {
            core.ui_state.set_window(
                def.name().to_string(),
                crate::data::WindowState::new_text(def.name().to_string(), 100),
            );
        }
        core.baseline_layout = Some(layout.clone());
        core.layout = layout;
        core
    }

    fn text_def(name: &str, col: u16, row: u16, cols: u16, rows: u16) -> WindowDef {
        WindowDef::Text {
            base: test_window_base(name, col, row, cols, rows),
            data: crate::config::TextWidgetData {
                streams: vec![],
                buffer_size: 100,
                wordwrap: true,
                show_timestamps: false,
                timestamp_position: None,
                compact: false,
            },
        }
    }

    /// (row, rows) of a window in the current (post-resize) layout.
    fn row_rows(core: &AppCore, name: &str) -> (u16, u16) {
        let b = core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == name)
            .unwrap()
            .base();
        (b.row.get(), b.rows.get())
    }

    /// (col, cols).
    fn col_cols(core: &AppCore, name: &str) -> (u16, u16) {
        let b = core
            .layout
            .windows
            .iter()
            .find(|w| w.name() == name)
            .unwrap()
            .base();
        (b.col.get(), b.cols.get())
    }

    /// Two stacked full-width text windows grow proportionally and the
    /// bottom one cascades to sit directly below the resized top one.
    /// Pins: proportional floor() distribution, leftover-to-first, cascade.
    #[test]
    fn resize_grows_stacked_windows_proportionally_and_cascades() {
        // Baseline 80x24: top rows 0..10, bottom rows 10..24. Grow +10 -> 34.
        let mut core = core_with_baseline(
            vec![
                text_def("top", 0, 0, 80, 10),
                text_def("bottom", 0, 10, 80, 14),
            ],
            80,
            24,
        );
        core.resize_windows(80, 34);

        // top: floor(10/24 * 10) = 4, +1 leftover (top is first) -> 15.
        // bottom: floor(14/24 * 10) = 5 -> 19. Cascade: bottom.row = 15.
        assert_eq!(row_rows(&core, "top"), (0, 15));
        assert_eq!(row_rows(&core, "bottom"), (15, 19));
        // Conservation: heights sum to the new terminal height.
        assert_eq!(15 + 19, 34);
    }

    /// The resize is always computed from the pristine baseline, never the
    /// current layout, so resizing in two steps equals resizing in one.
    /// Pins the delta-from-baseline invariant (layout.rs:206) — the single
    /// most load-bearing line in the cascade.
    #[test]
    fn resize_is_idempotent_from_baseline() {
        let make = || {
            core_with_baseline(
                vec![
                    text_def("top", 0, 0, 80, 10),
                    text_def("bottom", 0, 10, 80, 14),
                ],
                80,
                24,
            )
        };

        let mut twice = make();
        twice.resize_windows(80, 34);
        twice.resize_windows(80, 44);

        let mut once = make();
        once.resize_windows(80, 44);

        assert_eq!(row_rows(&twice, "top"), row_rows(&once, "top"));
        assert_eq!(row_rows(&twice, "bottom"), row_rows(&once, "bottom"));
        assert_eq!(row_rows(&once, "top"), (0, 19));
        assert_eq!(row_rows(&once, "bottom"), (19, 25));
    }

    /// Two side-by-side windows split a width increase evenly.
    /// Pins the width transpose + column-span predicate.
    #[test]
    fn resize_grows_side_by_side_windows_in_width() {
        let mut core = core_with_baseline(
            vec![
                text_def("left", 0, 0, 40, 24),
                text_def("right", 40, 0, 40, 24),
            ],
            80,
            24,
        );
        core.resize_windows(100, 24); // +20 width

        assert_eq!(col_cols(&core, "left"), (0, 50));
        assert_eq!(col_cols(&core, "right"), (50, 50));
    }

    /// A static widget (indicator) keeps its height on a height resize; the
    /// scalable window absorbs the entire delta and cascades below it.
    /// Pins the static categorization coupling.
    #[test]
    fn resize_leaves_static_widget_height_unchanged() {
        let ind = WindowDef::Indicator {
            base: test_window_base("ind", 0, 0, 80, 1),
            data: crate::config::IndicatorWidgetData {
                icon: None,
                indicator_id: None,
                inactive_color: None,
                active_color: None,
                default_status: None,
                default_color: None,
            },
        };
        let mut core = core_with_baseline(vec![ind, text_def("body", 0, 1, 80, 23)], 80, 24);
        core.resize_windows(80, 34); // +10

        // Indicator height stays 1; body takes the full +10 and stays below it.
        assert_eq!(row_rows(&core, "ind"), (0, 1));
        assert_eq!(row_rows(&core, "body"), (1, 33));
    }

    /// When shrinking would push a window below its min_rows, the window
    /// stops at min and the ENTIRE unused delta is recoupled onto later
    /// windows in the column, preserving size conservation. Pins invariant E.
    #[test]
    fn resize_min_clamp_stops_at_min_and_recouples_to_sibling() {
        let mut top = text_def("top", 0, 0, 80, 6);
        top.base_mut().min_rows = Some(5);
        let mut core = core_with_baseline(vec![top, text_def("bottom", 0, 6, 80, 18)], 80, 24);
        core.resize_windows(80, 12); // -12

        // top wanted -3 (6 -> 3) but min_rows=5 clamps it to 5 (only -1 used);
        // the full -2 remainder is pushed onto bottom: 18 - 9 - 2 = 7.
        assert_eq!(row_rows(&core, "top"), (0, 5));
        assert_eq!(row_rows(&core, "bottom"), (5, 7));
    }

    /// A window spanning multiple columns has its height delta applied exactly
    /// ONCE, even though the column-by-column pass visits it at every column it
    /// spans. Pins invariant F (apply-once + advance-cursor at later columns).
    #[test]
    fn resize_applies_multi_column_window_delta_once() {
        // 80 wide: `wide` spans the full width above two side-by-side columns.
        let mut core = core_with_baseline(
            vec![
                text_def("wide", 0, 0, 80, 8),
                text_def("leftcol", 0, 8, 40, 16),
                text_def("rightcol", 40, 8, 40, 16),
            ],
            80,
            24,
        );
        core.resize_windows(80, 34); // +10 height

        // wide grows by 4 exactly once (not 4 per spanned column); the two
        // lower windows each grow to 22 and cascade below the resized `wide`.
        assert_eq!(row_rows(&core, "wide"), (0, 12));
        assert_eq!(row_rows(&core, "leftcol"), (12, 22));
        assert_eq!(row_rows(&core, "rightcol"), (12, 22));
    }

    /// Shrinking far past every window's minimum must terminate (all windows
    /// clamp to min) and never hang. Pins invariant G — the leftover loop's
    /// termination when the scalable set empties.
    #[test]
    fn resize_terminates_when_all_windows_hit_min() {
        let mut core = core_with_baseline(
            vec![
                text_def("a", 0, 0, 80, 6),
                text_def("b", 0, 6, 80, 6),
                text_def("c", 0, 12, 80, 12),
            ],
            80,
            24,
        );
        core.resize_windows(80, 9); // -15; text min_rows is 3, 3 windows

        // Every window bottoms out at min_rows (3) and cascades in order.
        assert_eq!(row_rows(&core, "a"), (0, 3));
        assert_eq!(row_rows(&core, "b"), (3, 3));
        assert_eq!(row_rows(&core, "c"), (6, 3));
    }

    /// The `.resize` dot-command fallback (GUI/headless frontends without a
    /// cell grid) goes through `resize_to_current_terminal`, which resizes the
    /// layout to its OWN already-stored terminal size — a zero delta — so it
    /// must leave geometry unchanged. This pins the behavior after collapsing
    /// the former second algorithm (`resize_to_terminal`) into `resize_windows`.
    #[test]
    fn resize_to_current_terminal_is_a_noop_at_stored_size() {
        let mut core = core_with_baseline(
            vec![
                text_def("top", 0, 0, 80, 10),
                text_def("bottom", 0, 10, 80, 14),
            ],
            80,
            24,
        );
        core.resize_to_current_terminal();

        // Stored size == baseline size == 80x24, so nothing moves or resizes.
        assert_eq!(row_rows(&core, "top"), (0, 10));
        assert_eq!(row_rows(&core, "bottom"), (10, 14));
    }

    /// Regression guard for the remainder-recoupling conservation bug: the
    /// loop previously visited each later window at most once, so a remainder
    /// larger than the eligible-window count dropped units and the heights no
    /// longer summed to the terminal size (this case summed to 13, not 12).
    /// After the fix the loop exhausts the remainder, so conservation holds.
    #[test]
    fn resize_min_clamp_preserves_total_height() {
        let mut top = text_def("top", 0, 0, 80, 6);
        top.base_mut().min_rows = Some(5);
        let mut core = core_with_baseline(vec![top, text_def("bottom", 0, 6, 80, 18)], 80, 24);
        core.resize_windows(80, 12); // target total height = 12

        let (_, top_rows) = row_rows(&core, "top");
        let (_, bottom_rows) = row_rows(&core, "bottom");
        assert_eq!(
            top_rows + bottom_rows,
            12,
            "window heights must sum to the requested terminal height"
        );
    }

    /// Three windows stacked in one column grow proportionally with the
    /// leftover distributed round-robin, and conservation holds across 3.
    #[test]
    fn resize_three_stacked_windows_conserve_and_cascade() {
        let mut core = core_with_baseline(
            vec![
                text_def("a", 0, 0, 80, 6),
                text_def("b", 0, 6, 80, 6),
                text_def("c", 0, 12, 80, 12),
            ],
            80,
            24,
        );
        core.resize_windows(80, 31); // +7

        assert_eq!(row_rows(&core, "a"), (0, 8));
        assert_eq!(row_rows(&core, "b"), (8, 8));
        assert_eq!(row_rows(&core, "c"), (16, 15));
        // Cascade + conservation: contiguous, summing to 31.
        assert_eq!(8 + 8 + 15, 31);
    }

    /// A column with both a min-clamped window AND a max-clamped window still
    /// conserves total height: the fully-recoupled remainder lands on the
    /// unconstrained sibling.
    #[test]
    fn resize_mixed_min_and_max_clamps_conserve() {
        let mut top = text_def("top", 0, 0, 80, 8);
        top.base_mut().min_rows = Some(7);
        let mut mid = text_def("mid", 0, 8, 80, 8);
        mid.base_mut().max_rows = Some(8);
        let mut core = core_with_baseline(vec![top, mid, text_def("bot", 0, 16, 80, 8)], 80, 24);
        core.resize_windows(80, 18); // -6

        // top stops at its min_rows (7); the remaining shrink is spread across
        // mid and bot by distribute_1d's balanced reconciliation (the old
        // three-loop code dumped it all on bot as 7/4; both conserve, but the
        // unified pass distributes more evenly). Conservation holds either way.
        assert_eq!(row_rows(&core, "top"), (0, 7));
        assert_eq!(row_rows(&core, "mid"), (7, 5));
        assert_eq!(row_rows(&core, "bot"), (12, 6));
        assert_eq!(7 + 5 + 6, 18);
    }

    /// A simultaneous width + height resize applies both passes correctly to a
    /// 2x2 grid (no interference between the axes).
    #[test]
    fn resize_asymmetric_width_and_height_together() {
        let mut core = core_with_baseline(
            vec![
                text_def("tl", 0, 0, 40, 12),
                text_def("tr", 40, 0, 40, 12),
                text_def("bl", 0, 12, 40, 12),
                text_def("br", 40, 12, 40, 12),
            ],
            80,
            24,
        );
        core.resize_windows(100, 34); // +20 width, +10 height

        // Each cell: 40 -> 50 wide, 12 -> 17 tall; bottom row cascades to 17.
        assert_eq!(col_cols(&core, "tl"), (0, 50));
        assert_eq!(col_cols(&core, "tr"), (50, 50));
        assert_eq!(row_rows(&core, "tl"), (0, 17));
        assert_eq!(row_rows(&core, "bl"), (17, 17));
        assert_eq!(col_cols(&core, "br"), (50, 50));
        assert_eq!(row_rows(&core, "br"), (17, 17));
    }

    /// A single full-screen window absorbs the entire delta on both axes.
    #[test]
    fn resize_single_window_takes_whole_delta() {
        let mut core = core_with_baseline(vec![text_def("only", 0, 0, 80, 24)], 80, 24);
        core.resize_windows(90, 30); // +10 w, +6 h

        assert_eq!(col_cols(&core, "only"), (0, 90));
        assert_eq!(row_rows(&core, "only"), (0, 30));
    }

    /// Growing only the width leaves every window's height (and row) untouched.
    #[test]
    fn resize_width_only_leaves_height_untouched() {
        let mut core = core_with_baseline(
            vec![
                text_def("l", 0, 0, 40, 24),
                text_def("r", 40, 0, 40, 24),
            ],
            80,
            24,
        );
        core.resize_windows(96, 24); // +16 w, 0 h

        assert_eq!(col_cols(&core, "l"), (0, 48));
        assert_eq!(col_cols(&core, "r"), (48, 48));
        assert_eq!(row_rows(&core, "l"), (0, 24));
        assert_eq!(row_rows(&core, "r"), (0, 24));
    }

    /// max_rows clamp on GROW (mirror of the min clamp on shrink): the capped
    /// window stops at max and the remainder flows to its sibling; conserved.
    #[test]
    fn resize_max_clamp_on_grow_recouples_to_sibling() {
        let mut cap = text_def("cap", 0, 0, 80, 10);
        cap.base_mut().max_rows = Some(12);
        let mut core = core_with_baseline(vec![cap, text_def("rest", 0, 10, 80, 14)], 80, 24);
        core.resize_windows(80, 44); // +20

        assert_eq!(row_rows(&core, "cap"), (0, 12)); // capped at max_rows
        assert_eq!(row_rows(&core, "rest"), (12, 32)); // absorbed the rest
        assert_eq!(12 + 32, 44);
    }

    // ========== distribute_1d unit tests (pure, no AppCore) ==========

    fn track(name: &str, base: u16, min: u16, max: Option<u16>, is_static: bool) -> Track {
        Track {
            name: name.to_string(),
            base_size: base,
            min,
            max,
            is_static,
        }
    }

    #[test]
    fn distribute_1d_grows_proportionally_and_conserves() {
        let tracks = vec![
            track("top", 10, 3, None, false),
            track("bottom", 14, 3, None, false),
        ];
        let out = distribute_1d(&tracks, 10);
        // Matches the cascade characterization: top 15, bottom 19, sum 34.
        assert_eq!(out, vec![15, 19]);
        assert_eq!(out.iter().map(|&s| s as i32).sum::<i32>(), 24 + 10);
    }

    #[test]
    fn distribute_1d_min_clamp_recouples_and_conserves() {
        let tracks = vec![
            track("top", 6, 5, None, false), // min 5
            track("bottom", 18, 3, None, false),
        ];
        let out = distribute_1d(&tracks, -12);
        assert_eq!(out, vec![5, 7]); // top stops at min 5, bottom absorbs the rest
        assert_eq!(out.iter().map(|&s| s as i32).sum::<i32>(), 24 - 12);
    }

    #[test]
    fn distribute_1d_conserves_the_previously_buggy_case() {
        // [5,8,4,5] shrink to 14 (delta -8) — the case the golden test caught
        // as a conservation violation in the old three-loop code.
        let tracks = vec![
            track("w0", 5, 3, None, false),
            track("w1", 8, 3, None, false),
            track("w2", 4, 3, None, false),
            track("w3", 5, 3, None, false),
        ];
        let out = distribute_1d(&tracks, -8);
        assert_eq!(out.iter().map(|&s| s as i32).sum::<i32>(), 22 - 8);
        // Every window respects its min.
        assert!(out.iter().all(|&s| s >= 3));
    }

    #[test]
    fn distribute_1d_max_clamp_on_grow_conserves() {
        let tracks = vec![
            track("cap", 10, 3, Some(12), false), // capped at 12
            track("rest", 14, 3, None, false),
        ];
        let out = distribute_1d(&tracks, 20);
        assert_eq!(out, vec![12, 32]);
        assert_eq!(out.iter().map(|&s| s as i32).sum::<i32>(), 24 + 20);
    }

    #[test]
    fn distribute_1d_static_track_never_changes() {
        let tracks = vec![
            track("ind", 1, 1, None, true), // static
            track("body", 23, 3, None, false),
        ];
        let out = distribute_1d(&tracks, 10);
        assert_eq!(out[0], 1); // static unchanged
        assert_eq!(out[1], 33); // scalable absorbs the whole delta
    }

    #[test]
    fn distribute_1d_infeasible_target_stops_at_nearest_bound() {
        // Two windows, min 3 each; ask to shrink far below Σmin. Result clamps
        // to [3,3] rather than looping forever or going negative.
        let tracks = vec![
            track("a", 10, 3, None, false),
            track("b", 10, 3, None, false),
        ];
        let out = distribute_1d(&tracks, -100);
        assert_eq!(out, vec![3, 3]);
    }

    #[test]
    fn distribute_1d_zero_delta_is_identity() {
        let tracks = vec![
            track("a", 10, 3, None, false),
            track("b", 14, 3, Some(20), false),
        ];
        assert_eq!(distribute_1d(&tracks, 0), vec![10, 14]);
    }

    /// Property/golden backstop: across many pseudo-random single-column
    /// stacks and resize targets, the resize must preserve two universal
    /// invariants — CONSERVATION (visible full-width windows sum to the
    /// terminal height) and NO GAPS/OVERLAPS (the cascade is contiguous from
    /// row 0). This guards the whole behavior space, not just fixed examples,
    /// so a transpose-extraction bug can't slip between the named cases. Uses a
    /// seeded LCG for determinism (no Math.random / wall clock).
    #[test]
    fn resize_preserves_conservation_and_contiguity_over_random_layouts() {
        // Deterministic LCG (numerical recipes constants).
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        let mut next = |bound: u16| -> u16 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as u16) % bound
        };

        for _ in 0..500 {
            let n = 2 + next(3); // 2..=4 windows stacked full-width
            let base_h: u16 = 20 + next(20); // 20..=39
            // Partition base_h into n chunks of >= 3 rows each (text min = 3).
            let mut rows_list = Vec::new();
            let mut remaining = base_h;
            for i in 0..n {
                let left = n - i;
                // Leave >= 3 rows for each remaining window.
                let max_here = remaining.saturating_sub(3 * (left - 1)).max(3);
                let r = if i == n - 1 {
                    remaining
                } else {
                    3 + next((max_here.saturating_sub(3)).max(1))
                };
                rows_list.push(r);
                remaining = remaining.saturating_sub(r);
            }
            // Build stacked defs.
            let mut defs = Vec::new();
            let mut row = 0u16;
            for (i, &r) in rows_list.iter().enumerate() {
                defs.push(text_def(
                    Box::leak(format!("w{i}").into_boxed_str()),
                    0,
                    row,
                    80,
                    r,
                ));
                row += r;
            }
            let names: Vec<String> = (0..n).map(|i| format!("w{i}")).collect();

            let mut core = core_with_baseline(defs, 80, base_h);
            // Target height: base_h +/- up to 12, floored so every window can
            // still hit its min (n * 3).
            let min_total = n * 3;
            let delta = next(25) as i32 - 12; // -12..=12
            let target = ((base_h as i32 + delta).max(min_total as i32)) as u16;
            core.resize_windows(80, target);

            // Invariant 1: contiguous cascade from row 0.
            let mut expected_row = 0u16;
            let mut total = 0u16;
            for name in &names {
                let (r, h) = row_rows(&core, name);
                assert_eq!(
                    r, expected_row,
                    "gap/overlap: {name} at row {r}, expected {expected_row} (base_h={base_h}, target={target}, rows={rows_list:?})"
                );
                expected_row += h;
                total += h;
            }
            // Invariant 2: conservation — heights sum to the target height.
            assert_eq!(
                total, target,
                "conservation broken: sum {total} != target {target} (base_h={base_h}, rows={rows_list:?})"
            );
        }
    }
}
