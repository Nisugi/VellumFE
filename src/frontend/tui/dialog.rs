//! Dialog popup rendering and hit-testing for dynamic openDialog payloads.

use crate::data::{DialogDragOperation, DialogState};
use crate::frontend::tui::crossterm_bridge;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Hit-test area for a dialog row - mirrors DialogRow structure for accurate click detection
pub enum DialogRowArea {
    /// Display label row (no interaction)
    DisplayLabel { area: Rect },
    /// Progress bar row (no interaction)
    ProgressBar { area: Rect },
    /// Field + button on same row - separate hit areas for each
    FieldWithButton {
        field_area: Rect,
        button_area: Rect,
        field_idx: usize,
        button_idx: usize,
    },
    /// Field without paired button - editable field only
    FieldOnly {
        field_area: Rect,
        field_idx: usize,
    },
    /// Multiple links grouped on one row
    LinkGroup {
        button_areas: Vec<(Rect, usize)>, // (area, button_idx)
    },
    /// Single button centered on its own row
    SingleButton {
        area: Rect,
        button_idx: usize,
    },
}

pub struct DialogLayout {
    pub area: Rect,
    pub title_bar_area: Rect,
    pub row_areas: Vec<DialogRowArea>,
}

fn display_label(button: &crate::data::DialogButton) -> String {
    if button.is_radio {
        let marker = if button.selected { "x" } else { " " };
        format!("[{}] {}", marker, button.label)
    } else {
        button.label.clone()
    }
}

fn field_label(dialog: &DialogState, index: usize) -> String {
    // First try to get explicit label from dialog
    if let Some(label) = dialog.labels.get(index) {
        let trimmed = label.value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    // Fallback: clean up field ID (depositSB → Deposit)
    let field_id = &dialog.fields[index].id;
    let base = strip_field_suffix(&field_id.to_lowercase());
    // Capitalize first letter
    let mut chars = base.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => field_id.clone(),
    }
}

/// Format a number with commas for display (e.g., 1000 → 1,000)
fn format_number_with_commas(value: &str) -> String {
    // Only format if it's a pure number (digits only)
    if value.is_empty() || !value.chars().all(|c| c.is_ascii_digit()) {
        return value.to_string();
    }

    let mut result = String::new();
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();

    for (i, ch) in chars.iter().enumerate() {
        result.push(*ch);
        let remaining = len - i - 1;
        if remaining > 0 && remaining % 3 == 0 {
            result.push(',');
        }
    }
    result
}

/// Get formatted value for a dialog field (with commas, no cursor)
fn field_formatted_value(dialog: &DialogState, index: usize) -> String {
    let field = &dialog.fields[index];
    format_number_with_commas(&field.value)
}

/// Create a fixed-width input box display: "[     value]"
/// The box_width is the total width including brackets
const FIELD_BOX_WIDTH: usize = 22; // 20 chars + 2 brackets

fn field_input_box(value: &str) -> String {
    let content_width = FIELD_BOX_WIDTH - 2; // Space for content inside brackets
    if value.len() >= content_width {
        format!("[{}]", &value[value.len().saturating_sub(content_width)..])
    } else {
        // Right-align value in the box
        format!("[{:>width$}]", value, width = content_width)
    }
}

/// Strip common suffixes from field IDs for button matching
/// e.g., "depositSB" → "deposit", "withdrawAmount" → "withdraw"
fn strip_field_suffix(field_id: &str) -> String {
    // Common suffixes in dialog field IDs (case-insensitive, already lowercase input)
    const SUFFIXES: &[&str] = &[
        "sb",        // SpinBox
        "amount",    // Amount field
        "input",     // Input field
        "field",     // Generic field
        "value",     // Value field
        "text",      // Text field
        "edit",      // Edit field
        "box",       // Box suffix
        "entry",     // Entry field
    ];

    let mut result = field_id.to_string();
    for suffix in SUFFIXES {
        if result.len() > suffix.len() && result.ends_with(suffix) {
            result.truncate(result.len() - suffix.len());
            break; // Only strip one suffix
        }
    }
    result
}

/// Count the actual number of rows that will be rendered
fn count_dialog_rows(dialog: &DialogState) -> usize {
    let mut count = 0;

    // Display labels (one row each)
    count += dialog.display_labels.len();

    // Progress bars (one row each)
    count += dialog.progress_bars.len();

    // Fields with paired buttons - mirrors build_dialog_rows() logic
    let mut used_buttons = std::collections::HashSet::new();
    for field in &dialog.fields {
        // Always count one row per field (FieldWithButton or FieldOnly)
        count += 1;

        // Try explicit enter_button first
        if let Some(ref enter_button_id) = field.enter_button {
            if let Some(idx) = dialog.buttons.iter().position(|b| &b.id == enter_button_id) {
                used_buttons.insert(idx);
                continue;
            }
        }

        // Fallback: ID-based heuristic pairing (mirrors build_dialog_rows)
        let field_id_lower = field.id.to_lowercase();
        let field_id_base = strip_field_suffix(&field_id_lower);
        for (button_idx, button) in dialog.buttons.iter().enumerate() {
            if used_buttons.contains(&button_idx) {
                continue;
            }
            if button.is_close || button.command.is_empty() {
                continue;
            }
            let button_id_lower = button.id.to_lowercase();
            if field_id_base == button_id_lower
                || field_id_lower.starts_with(&button_id_lower)
                || button_id_lower.starts_with(&field_id_base)
            {
                used_buttons.insert(button_idx);
                break;
            }
        }
    }

    // Remaining buttons: group consecutive links, individual non-links
    let mut in_link_group = false;
    for (idx, button) in dialog.buttons.iter().enumerate() {
        if used_buttons.contains(&idx) {
            continue;
        }

        let is_link = !button.is_close && !button.command.is_empty() && !button.is_radio;

        if is_link {
            if !in_link_group {
                count += 1; // Start new link group row
                in_link_group = true;
            }
            // Additional links in same group don't add rows
        } else {
            in_link_group = false;
            count += 1; // Non-link button gets its own row
        }
    }

    count.max(1)
}

pub fn compute_dialog_layout(screen: Rect, dialog: &DialogState) -> DialogLayout {
    let title = dialog.title.as_deref().unwrap_or("Dialog");

    // Calculate content widths - need wider for field+button rows
    let max_button_len = dialog
        .buttons
        .iter()
        .map(|button| display_label(button).len())
        .max()
        .unwrap_or(4);

    // For field+button pairs, we need field + spacing + button
    // Use same pairing logic as build_dialog_rows() (explicit enter_button or ID-based fallback)
    let max_field_button_len = {
        let mut max_len = 0;
        let mut used_buttons: std::collections::HashSet<usize> = std::collections::HashSet::new();

        for (idx, field) in dialog.fields.iter().enumerate() {
            // Try explicit enter_button first
            let mut paired_button: Option<&crate::data::DialogButton> = None;
            if let Some(ref enter_id) = field.enter_button {
                if let Some(button) = dialog.buttons.iter().find(|b| &b.id == enter_id) {
                    paired_button = Some(button);
                    if let Some(btn_idx) = dialog.buttons.iter().position(|b| &b.id == enter_id) {
                        used_buttons.insert(btn_idx);
                    }
                }
            }

            // Fallback: ID-based heuristic pairing (mirrors build_dialog_rows)
            if paired_button.is_none() {
                let field_id_lower = field.id.to_lowercase();
                let field_id_base = strip_field_suffix(&field_id_lower);
                for (button_idx, button) in dialog.buttons.iter().enumerate() {
                    if used_buttons.contains(&button_idx) {
                        continue;
                    }
                    if button.is_close || button.command.is_empty() {
                        continue;
                    }
                    let button_id_lower = button.id.to_lowercase();
                    if field_id_base == button_id_lower
                        || field_id_lower.starts_with(&button_id_lower)
                        || button_id_lower.starts_with(&field_id_base)
                    {
                        paired_button = Some(button);
                        used_buttons.insert(button_idx);
                        break;
                    }
                }
            }

            if let Some(button) = paired_button {
                // Input box is fixed 22 chars from field_input_box(), no label
                let input_box_len = 22;
                let button_len = display_label(button).len();
                max_len = max_len.max(input_box_len + 2 + button_len); // input_box + spacing + button
            }
        }
        max_len
    };

    let max_progress_bar_len = dialog
        .progress_bars
        .iter()
        .map(|pb| pb.text.len())
        .max()
        .unwrap_or(0);
    let max_display_label_len = dialog
        .display_labels
        .iter()
        .map(|label| label.value.len())
        .max()
        .unwrap_or(0);

    // For link groups, sum all link widths + spacing
    let link_group_width: usize = dialog
        .buttons
        .iter()
        .filter(|b| !b.is_close && !b.command.is_empty() && !b.is_radio)
        .map(|b| display_label(b).len() + 2)
        .sum();

    let content_width = max_button_len
        .max(max_field_button_len)
        .max(max_progress_bar_len)
        .max(max_display_label_len)
        .max(link_group_width)
        .max(title.len())
        .max(20); // Minimum width for progress bars to look good

    // Calculate size (use overrides if present)
    let (width, height) = if let Some((w, h)) = dialog.size {
        (w, h)
    } else {
        let mut w = (content_width + 4) as u16;
        if screen.width > 0 {
            w = w.min(screen.width);
        }

        let total_lines = count_dialog_rows(dialog);
        let mut h = (total_lines + 2) as u16;
        if screen.height > 0 {
            h = h.min(screen.height);
        }
        (w, h)
    };

    // Calculate position (use overrides if present)
    let (x, y) = if let Some((px, py)) = dialog.position {
        (px, py)
    } else {
        let x = screen
            .x
            .saturating_add(screen.width.saturating_sub(width) / 2);
        let y = screen
            .y
            .saturating_add(screen.height.saturating_sub(height) / 2);
        (x, y)
    };

    let area = Rect { x, y, width, height };

    // Title bar area (top border line)
    let title_bar_area = Rect {
        x,
        y,
        width,
        height: 1,
    };

    // Build row areas using the same logic as build_dialog_rows()
    // This ensures hit-testing matches actual rendered positions
    let mut row_areas = Vec::new();
    let mut current_y = y.saturating_add(1); // Start after title bar
    let inner_width = width.saturating_sub(2);
    let inner_x = x.saturating_add(1);

    // Track which buttons are used by fields (same as build_dialog_rows)
    let mut used_buttons: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Pre-compute max button width for FieldWithButton rows (for alignment)
    let max_field_button_width_layout: usize = {
        let mut temp_used: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut max_width = 0usize;
        for field in &dialog.fields {
            // Same pairing logic as build_dialog_rows
            let mut paired_button_idx: Option<usize> = None;
            if let Some(ref enter_button_id) = field.enter_button {
                if let Some(idx) = dialog.buttons.iter().position(|b| &b.id == enter_button_id) {
                    paired_button_idx = Some(idx);
                    temp_used.insert(idx);
                }
            }
            if paired_button_idx.is_none() {
                let field_id_lower = field.id.to_lowercase();
                let field_id_base = strip_field_suffix(&field_id_lower);
                for (idx, button) in dialog.buttons.iter().enumerate() {
                    if temp_used.contains(&idx) || button.is_close || button.command.is_empty() {
                        continue;
                    }
                    let button_id_lower = button.id.to_lowercase();
                    if field_id_base == button_id_lower
                        || field_id_lower.starts_with(&button_id_lower)
                        || button_id_lower.starts_with(&field_id_base)
                    {
                        paired_button_idx = Some(idx);
                        temp_used.insert(idx);
                        break;
                    }
                }
            }
            if let Some(idx) = paired_button_idx {
                max_width = max_width.max(display_label(&dialog.buttons[idx]).len());
            }
        }
        max_width
    };

    // 1. Display labels first
    for _ in &dialog.display_labels {
        if current_y < y.saturating_add(height.saturating_sub(1)) {
            row_areas.push(DialogRowArea::DisplayLabel {
                area: Rect {
                    x: inner_x,
                    y: current_y,
                    width: inner_width,
                    height: 1,
                },
            });
            current_y += 1;
        }
    }

    // 2. Progress bars
    for _ in &dialog.progress_bars {
        if current_y < y.saturating_add(height.saturating_sub(1)) {
            row_areas.push(DialogRowArea::ProgressBar {
                area: Rect {
                    x: inner_x,
                    y: current_y,
                    width: inner_width,
                    height: 1,
                },
            });
            current_y += 1;
        }
    }

    // 3. Field+button pairs (or field-only rows)
    // Mirrors build_dialog_rows() logic for field pairing
    for (field_idx, field) in dialog.fields.iter().enumerate() {
        if current_y >= y.saturating_add(height.saturating_sub(1)) {
            continue;
        }

        // Try explicit enter_button first
        let mut paired_button: Option<usize> = None;
        if let Some(ref enter_button_id) = field.enter_button {
            if let Some(button_idx) = dialog.buttons.iter().position(|b| &b.id == enter_button_id) {
                paired_button = Some(button_idx);
            }
        }

        // Fallback: ID-based heuristic pairing (mirrors build_dialog_rows)
        if paired_button.is_none() {
            let field_id_lower = field.id.to_lowercase();
            let field_id_base = strip_field_suffix(&field_id_lower);
            for (button_idx, button) in dialog.buttons.iter().enumerate() {
                if used_buttons.contains(&button_idx) {
                    continue;
                }
                if button.is_close || button.command.is_empty() {
                    continue;
                }
                let button_id_lower = button.id.to_lowercase();
                if field_id_base == button_id_lower
                    || field_id_lower.starts_with(&button_id_lower)
                    || button_id_lower.starts_with(&field_id_base)
                {
                    paired_button = Some(button_idx);
                    break;
                }
            }
        }

        if let Some(button_idx) = paired_button {
            // Calculate field and button widths for hit areas
            // Layout: "  [     value]  Button  " with fixed padding of 2
            let value = field_formatted_value(dialog, field_idx);
            let input_box = field_input_box(&value);
            let button_text = display_label(&dialog.buttons[button_idx]);

            let box_len = input_box.len() as u16;
            let padded_button_len = max_field_button_width_layout as u16;

            // Fixed padding of 2 on left side
            let field_start_x = inner_x + 2;
            let field_area = Rect {
                x: field_start_x,
                y: current_y,
                width: box_len,
                height: 1,
            };

            // Button area (after input box + 2 spaces)
            let button_x = field_start_x + box_len + 2;
            let button_area = Rect {
                x: button_x,
                y: current_y,
                width: button_text.len() as u16, // Use actual button width for click area
                height: 1,
            };

            row_areas.push(DialogRowArea::FieldWithButton {
                field_area,
                button_area,
                field_idx,
                button_idx,
            });
            used_buttons.insert(button_idx);
        } else {
            // Field without a paired button (fixed padding of 2)
            let value = field_formatted_value(dialog, field_idx);
            let input_box = field_input_box(&value);

            let box_len = input_box.len() as u16;

            let field_start_x = inner_x + 2;
            let field_area = Rect {
                x: field_start_x,
                y: current_y,
                width: box_len,
                height: 1,
            };

            row_areas.push(DialogRowArea::FieldOnly {
                field_area,
                field_idx,
            });
        }
        current_y += 1;
    }

    // 4. Group remaining buttons (links and single buttons)
    let mut link_group: Vec<usize> = Vec::new();

    for (idx, button) in dialog.buttons.iter().enumerate() {
        if used_buttons.contains(&idx) {
            continue;
        }

        let is_link = !button.is_close && !button.command.is_empty() && !button.is_radio;

        if is_link {
            link_group.push(idx);
        } else {
            // Flush any pending link group
            if !link_group.is_empty() && current_y < y.saturating_add(height.saturating_sub(1)) {
                let button_areas = compute_link_group_areas(
                    dialog,
                    &link_group,
                    inner_x,
                    current_y,
                    inner_width,
                );
                row_areas.push(DialogRowArea::LinkGroup { button_areas });
                link_group.clear();
                current_y += 1;
            }

            // Add single button
            if current_y < y.saturating_add(height.saturating_sub(1)) {
                let button_text = display_label(button);
                let button_len = button_text.len() as u16;
                let padding = inner_width.saturating_sub(button_len) / 2;

                row_areas.push(DialogRowArea::SingleButton {
                    area: Rect {
                        x: inner_x + padding,
                        y: current_y,
                        width: button_len,
                        height: 1,
                    },
                    button_idx: idx,
                });
                current_y += 1;
            }
        }
    }

    // Flush remaining link group
    if !link_group.is_empty() && current_y < y.saturating_add(height.saturating_sub(1)) {
        let button_areas = compute_link_group_areas(
            dialog,
            &link_group,
            inner_x,
            current_y,
            inner_width,
        );
        row_areas.push(DialogRowArea::LinkGroup { button_areas });
    }

    DialogLayout {
        area,
        title_bar_area,
        row_areas,
    }
}

/// Compute hit-test areas for a group of link buttons on the same row
fn compute_link_group_areas(
    dialog: &DialogState,
    button_indices: &[usize],
    inner_x: u16,
    row_y: u16,
    inner_width: u16,
) -> Vec<(Rect, usize)> {
    let mut areas = Vec::new();
    let num_buttons = button_indices.len();
    if num_buttons == 0 {
        return areas;
    }

    let total_button_len: u16 = button_indices
        .iter()
        .map(|idx| display_label(&dialog.buttons[*idx]).len() as u16)
        .sum();

    let spacing = if num_buttons > 1 {
        inner_width.saturating_sub(total_button_len) / (num_buttons as u16 + 1)
    } else {
        inner_width.saturating_sub(total_button_len) / 2
    };

    let mut current_x = inner_x + spacing;

    for &btn_idx in button_indices {
        let button_text = display_label(&dialog.buttons[btn_idx]);
        let button_len = button_text.len() as u16;

        areas.push((
            Rect {
                x: current_x,
                y: row_y,
                width: button_len,
                height: 1,
            },
            btn_idx,
        ));

        current_x += button_len + spacing.max(2);
    }

    areas
}

pub fn hit_test_button(layout: &DialogLayout, x: u16, y: u16) -> Option<usize> {
    for row_area in &layout.row_areas {
        match row_area {
            DialogRowArea::FieldWithButton { button_area, button_idx, .. } => {
                if x >= button_area.x
                    && x < button_area.x.saturating_add(button_area.width)
                    && y >= button_area.y
                    && y < button_area.y.saturating_add(button_area.height)
                {
                    return Some(*button_idx);
                }
            }
            DialogRowArea::LinkGroup { button_areas } => {
                for (area, btn_idx) in button_areas {
                    if x >= area.x
                        && x < area.x.saturating_add(area.width)
                        && y >= area.y
                        && y < area.y.saturating_add(area.height)
                    {
                        return Some(*btn_idx);
                    }
                }
            }
            DialogRowArea::SingleButton { area, button_idx } => {
                if x >= area.x
                    && x < area.x.saturating_add(area.width)
                    && y >= area.y
                    && y < area.y.saturating_add(area.height)
                {
                    return Some(*button_idx);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn hit_test_field(layout: &DialogLayout, x: u16, y: u16) -> Option<usize> {
    for row_area in &layout.row_areas {
        match row_area {
            DialogRowArea::FieldWithButton { field_area, field_idx, .. } => {
                if x >= field_area.x
                    && x < field_area.x.saturating_add(field_area.width)
                    && y >= field_area.y
                    && y < field_area.y.saturating_add(field_area.height)
                {
                    return Some(*field_idx);
                }
            }
            DialogRowArea::FieldOnly { field_area, field_idx } => {
                if x >= field_area.x
                    && x < field_area.x.saturating_add(field_area.width)
                    && y >= field_area.y
                    && y < field_area.y.saturating_add(field_area.height)
                {
                    return Some(*field_idx);
                }
            }
            _ => {}
        }
    }
    None
}

/// Test if a mouse position is on the dialog title bar (for dragging)
pub fn hit_test_title_bar(layout: &DialogLayout, x: u16, y: u16) -> bool {
    let area = &layout.title_bar_area;
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y == area.y
}

/// Test if a mouse position is on a resize handle.
/// Returns the resize operation type if on a handle, None otherwise.
/// Resize handles are the edges and corners of the dialog.
pub fn hit_test_resize_handle(layout: &DialogLayout, x: u16, y: u16) -> Option<DialogDragOperation> {
    let area = &layout.area;
    let left = area.x;
    let right = area.x.saturating_add(area.width.saturating_sub(1));
    let top = area.y;
    let bottom = area.y.saturating_add(area.height.saturating_sub(1));

    // Corner detection (2 cells from corner)
    let corner_size = 2u16;
    let is_near_left = x <= left.saturating_add(corner_size);
    let is_near_right = x >= right.saturating_sub(corner_size);
    let is_on_left = x == left;
    let is_on_right = x == right;
    let is_on_top = y == top;
    let is_on_bottom = y == bottom;

    // Corners take priority
    if is_on_top && is_near_left && is_on_left {
        return Some(DialogDragOperation::ResizeTopLeft);
    }
    if is_on_top && is_near_right && is_on_right {
        return Some(DialogDragOperation::ResizeTopRight);
    }
    if is_on_bottom && is_near_left && is_on_left {
        return Some(DialogDragOperation::ResizeBottomLeft);
    }
    if is_on_bottom && is_near_right && is_on_right {
        return Some(DialogDragOperation::ResizeBottomRight);
    }

    // Edges
    if is_on_left && y > top && y < bottom {
        return Some(DialogDragOperation::ResizeLeft);
    }
    if is_on_right && y > top && y < bottom {
        return Some(DialogDragOperation::ResizeRight);
    }
    if is_on_bottom && x > left && x < right {
        return Some(DialogDragOperation::ResizeBottom);
    }
    // Note: top edge is title bar for move, not resize

    None
}

/// Represents a row in the dialog layout
enum DialogRow<'a> {
    /// Centered display label (e.g., "Balance: 144573897")
    DisplayLabel(&'a crate::data::DialogLabel),
    /// Progress bar
    ProgressBar(&'a crate::data::DialogProgressBar),
    /// Field with its paired button on the same row
    FieldWithButton {
        field_idx: usize,
        button_idx: usize,
    },
    /// Field without a paired button (rendered alone)
    FieldOnly(usize),
    /// Multiple links grouped on the same row
    LinkGroup(Vec<usize>), // button indices
    /// Single button (e.g., Close) - centered
    SingleButton(usize),
}

/// Build dialog rows from dialog state
fn build_dialog_rows(dialog: &DialogState) -> Vec<DialogRow<'_>> {
    let mut rows = Vec::new();

    // 1. Display labels first (centered)
    for label in &dialog.display_labels {
        rows.push(DialogRow::DisplayLabel(label));
    }

    // 2. Progress bars
    for pb in &dialog.progress_bars {
        rows.push(DialogRow::ProgressBar(pb));
    }

    // 3. Build field+button pairs and remaining buttons
    // Track which buttons are already used by fields
    let mut used_buttons: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // For each field, find its paired button
    for (field_idx, field) in dialog.fields.iter().enumerate() {
        // Try explicit enter_button first
        if let Some(ref enter_button_id) = field.enter_button {
            if let Some(button_idx) = dialog.buttons.iter().position(|b| &b.id == enter_button_id) {
                rows.push(DialogRow::FieldWithButton { field_idx, button_idx });
                used_buttons.insert(button_idx);
                continue;
            }
        }

        // FALLBACK: ID-based heuristic pairing
        // Strip common suffixes from field ID (SB=SpinBox, Amount, Input, Field, Value)
        // e.g., field "depositSB" → "deposit" matches button "deposit"
        let field_id_lower = field.id.to_lowercase();
        let field_id_base = strip_field_suffix(&field_id_lower);
        let mut paired = false;
        for (button_idx, button) in dialog.buttons.iter().enumerate() {
            if used_buttons.contains(&button_idx) {
                continue;
            }
            // Skip close buttons and buttons without commands
            if button.is_close || button.command.is_empty() {
                continue;
            }
            let button_id_lower = button.id.to_lowercase();
            // Try multiple matching strategies:
            // 1. field base matches button ID exactly (depositSB→deposit == deposit)
            // 2. field ID starts with button ID (depositAmount starts with deposit)
            // 3. button ID starts with field base (deposit starts with deposit)
            if field_id_base == button_id_lower
                || field_id_lower.starts_with(&button_id_lower)
                || button_id_lower.starts_with(&field_id_base)
            {
                rows.push(DialogRow::FieldWithButton { field_idx, button_idx });
                used_buttons.insert(button_idx);
                paired = true;
                break;
            }
        }

        // Last resort: render field alone
        if !paired {
            rows.push(DialogRow::FieldOnly(field_idx));
        }
    }

    // 4. Group remaining buttons
    // - Links (non-close buttons with commands) get grouped together
    // - Close buttons are rendered individually
    let mut link_group: Vec<usize> = Vec::new();

    for (idx, button) in dialog.buttons.iter().enumerate() {
        if used_buttons.contains(&idx) {
            continue;
        }

        // Check if this is a "link" type button (has command, not a close button)
        let is_link = !button.is_close && !button.command.is_empty() && !button.is_radio;

        if is_link {
            link_group.push(idx);
        } else {
            // Flush any pending link group
            if !link_group.is_empty() {
                rows.push(DialogRow::LinkGroup(std::mem::take(&mut link_group)));
            }
            // Add this button as a single button
            rows.push(DialogRow::SingleButton(idx));
        }
    }

    // Flush remaining link group
    if !link_group.is_empty() {
        rows.push(DialogRow::LinkGroup(link_group));
    }

    rows
}

pub fn render_dialog(
    dialog: &DialogState,
    screen: Rect,
    buf: &mut Buffer,
    theme: &crate::theme::AppTheme,
) {
    let layout = compute_dialog_layout(screen, dialog);
    let title = dialog.title.as_deref().unwrap_or("Dialog");

    Clear.render(layout.area, buf);

    // Build the rows from dialog elements
    let rows = build_dialog_rows(dialog);

    // Compute max button label width among FieldWithButton rows for alignment
    let max_field_button_width: usize = rows
        .iter()
        .filter_map(|row| {
            if let DialogRow::FieldWithButton { button_idx, .. } = row {
                Some(display_label(&dialog.buttons[*button_idx]).len())
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0);

    let mut lines: Vec<Line> = Vec::new();
    let inner_width = layout.area.width.saturating_sub(4) as usize; // Account for borders + padding

    let normal_style = Style::default()
        .fg(crossterm_bridge::to_ratatui_color(theme.menu_item_normal))
        .bg(crossterm_bridge::to_ratatui_color(theme.menu_background));
    let selected_style = Style::default()
        .fg(crossterm_bridge::to_ratatui_color(theme.menu_item_selected))
        .bg(crossterm_bridge::to_ratatui_color(theme.menu_item_focused));

    for row in &rows {
        match row {
            DialogRow::DisplayLabel(label) => {
                // Centered label
                let text = &label.value;
                let padding = inner_width.saturating_sub(text.len()) / 2;
                let padded = format!("{:>width$}{}", "", text, width = padding);
                let line = Line::from(vec![
                    Span::raw(" "),
                    Span::styled(padded, normal_style),
                    Span::raw(" "),
                ]);
                lines.push(line);
            }

            DialogRow::ProgressBar(pb) => {
                let line = render_progress_bar_line(pb, inner_width, theme);
                lines.push(line);
            }

            DialogRow::FieldWithButton { field_idx, button_idx } => {
                // Field + button on same row: "  [    1,000]  Deposit  " with fixed padding
                let is_focused = dialog.focused_field == Some(*field_idx);
                let field_style = if is_focused { selected_style } else { normal_style };

                let value = field_formatted_value(dialog, *field_idx);
                let input_box = field_input_box(&value);

                let button = &dialog.buttons[*button_idx];
                let button_selected = *button_idx == dialog.selected;
                let button_style = if button_selected { selected_style } else { normal_style };
                let button_text = display_label(button);

                // Pad button text to max width so all FieldWithButton rows align
                let padded_button = format!("{:<width$}", button_text, width = max_field_button_width);

                // Fixed padding of 2 on left side
                let line = Line::from(vec![
                    Span::raw("  "), // Fixed left padding of 2
                    Span::styled(input_box, field_style),
                    Span::raw("  "),
                    Span::styled(padded_button, button_style),
                ]);
                lines.push(line);
            }

            DialogRow::LinkGroup(button_indices) => {
                // Multiple links on same row, evenly spaced
                let mut spans = vec![Span::raw(" ")];

                let total_button_len: usize = button_indices
                    .iter()
                    .map(|idx| display_label(&dialog.buttons[*idx]).len())
                    .sum();
                let num_buttons = button_indices.len();
                let spacing = if num_buttons > 1 {
                    inner_width.saturating_sub(total_button_len) / (num_buttons + 1)
                } else {
                    inner_width.saturating_sub(total_button_len) / 2
                };

                for (i, &btn_idx) in button_indices.iter().enumerate() {
                    if i == 0 || num_buttons == 1 {
                        spans.push(Span::raw(" ".repeat(spacing)));
                    }

                    let button = &dialog.buttons[btn_idx];
                    let button_selected = btn_idx == dialog.selected;
                    let button_style = if button_selected { selected_style } else { normal_style };
                    spans.push(Span::styled(display_label(button), button_style));

                    if i < button_indices.len() - 1 {
                        spans.push(Span::raw(" ".repeat(spacing.max(2))));
                    }
                }
                spans.push(Span::raw(" "));

                lines.push(Line::from(spans));
            }

            DialogRow::SingleButton(button_idx) => {
                // Centered single button (typically Close)
                let button = &dialog.buttons[*button_idx];
                let button_selected = *button_idx == dialog.selected;
                let button_style = if button_selected { selected_style } else { normal_style };
                let button_text = display_label(button);

                let padding = inner_width.saturating_sub(button_text.len()) / 2;
                let line = Line::from(vec![
                    Span::raw(" "),
                    Span::raw(" ".repeat(padding)),
                    Span::styled(button_text, button_style),
                    Span::raw(" "),
                ]);
                lines.push(line);
            }

            DialogRow::FieldOnly(field_idx) => {
                // Field without a paired button - render field alone with fixed padding
                let is_focused = dialog.focused_field == Some(*field_idx);
                let field_style = if is_focused { selected_style } else { normal_style };

                let value = field_formatted_value(dialog, *field_idx);
                let input_box = field_input_box(&value);

                // Fixed padding of 2 on left side
                let line = Line::from(vec![
                    Span::raw("  "), // Fixed left padding of 2
                    Span::styled(input_box, field_style),
                ]);
                lines.push(line);
            }
        }
    }

    // If completely empty, add a placeholder
    if lines.is_empty() {
        lines.push(Line::from(Span::raw(" ")));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(crossterm_bridge::to_ratatui_color(theme.menu_border)))
        .style(Style::default().bg(crossterm_bridge::to_ratatui_color(theme.menu_background)));

    let paragraph = Paragraph::new(lines).block(block);
    paragraph.render(layout.area, buf);
}

/// Render a progress bar as a styled line for dialog display
fn render_progress_bar_line(
    pb: &crate::data::DialogProgressBar,
    width: usize,
    theme: &crate::theme::AppTheme,
) -> Line<'static> {
    let value = pb.value.min(100) as usize;
    let text = &pb.text;

    // Calculate filled vs unfilled portions
    let bar_width = width.saturating_sub(2); // Leave space for borders
    let filled_width = (bar_width * value) / 100;

    // Create the progress bar characters
    let filled_char = '█';
    let unfilled_char = '░';

    // Use theme colors
    let filled_style = Style::default()
        .fg(crossterm_bridge::to_ratatui_color(theme.background_selected));
    let unfilled_style = Style::default()
        .fg(crossterm_bridge::to_ratatui_color(theme.background_secondary));

    // Build the line with text overlaid on the bar
    // Center the text over the bar
    let text_start = if bar_width > text.len() {
        (bar_width - text.len()) / 2
    } else {
        0
    };
    let text_end = text_start + text.len().min(bar_width);

    let mut spans = Vec::new();
    spans.push(Span::raw(" ")); // Left padding

    // Build the bar with text overlay
    let mut pos = 0;
    while pos < bar_width {
        if pos >= text_start && pos < text_end {
            // Render text character with appropriate background
            let char_idx = pos - text_start;
            let ch = text.chars().nth(char_idx).unwrap_or(' ');
            let is_filled = pos < filled_width;
            let style = if is_filled {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                    .bg(crossterm_bridge::to_ratatui_color(theme.background_selected))
            } else {
                Style::default()
                    .fg(crossterm_bridge::to_ratatui_color(theme.text_primary))
                    .bg(crossterm_bridge::to_ratatui_color(theme.menu_background))
            };
            spans.push(Span::styled(ch.to_string(), style));
            pos += 1;
        } else {
            // Render bar character
            let is_filled = pos < filled_width;
            if is_filled {
                // Collect consecutive filled chars
                let count = (text_start.saturating_sub(pos)).min(filled_width.saturating_sub(pos));
                if count > 0 {
                    let s: String = std::iter::repeat(filled_char).take(count).collect();
                    spans.push(Span::styled(s, filled_style));
                    pos += count;
                } else {
                    spans.push(Span::styled(filled_char.to_string(), filled_style));
                    pos += 1;
                }
            } else {
                // Collect consecutive unfilled chars
                let count = text_start.saturating_sub(pos).max(1).min(bar_width.saturating_sub(pos));
                if count > 0 && pos < text_start {
                    let s: String = std::iter::repeat(unfilled_char).take(count).collect();
                    spans.push(Span::styled(s, unfilled_style));
                    pos += count;
                } else {
                    spans.push(Span::styled(unfilled_char.to_string(), unfilled_style));
                    pos += 1;
                }
            }
        }
    }

    spans.push(Span::raw(" ")); // Right padding

    Line::from(spans)
}
