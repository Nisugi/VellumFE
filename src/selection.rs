//! Helpers for tracking mouse-driven selections inside text windows.
//!
//! The `selection` module tracks the active selection range in window space and
//! offers utilities for translating between screen coordinates and window
//! rectangles.

use crate::frontend::common::Rect;

/// Represents a position in the text (window, line, column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPosition {
    pub window_index: usize,
    pub line: usize,
    pub col: usize,
}

/// Tracks the current text selection state
#[derive(Debug, Clone)]
pub struct SelectionState {
    /// The starting position of the selection
    pub start: TextPosition,
    /// The current end position of the selection (updated as mouse moves)
    pub end: TextPosition,
    /// Whether the selection is currently active
    pub active: bool,
}

impl SelectionState {
    /// Create a new selection anchored at the provided window/line/column.
    pub fn new(window_index: usize, line: usize, col: usize) -> Self {
        let pos = TextPosition {
            window_index,
            line,
            col,
        };
        Self {
            start: pos,
            end: pos,
            active: true,
        }
    }

    /// Update the end position of the selection
    pub fn update_end(&mut self, window_index: usize, line: usize, col: usize) {
        // Only update if we're in the same window (respect window boundaries)
        if window_index == self.start.window_index {
            self.end = TextPosition {
                window_index,
                line,
                col,
            };
        }
    }

    /// Check if a given line/col is within the selection range
    pub fn contains(&self, window_index: usize, line: usize, col: usize) -> bool {
        if !self.active || window_index != self.start.window_index {
            return false;
        }

        let (start, end) = self.normalized_range();

        // Same line selection
        if start.line == end.line {
            return line == start.line && col >= start.col && col < end.col;
        }

        // Multi-line selection
        if line < start.line || line > end.line {
            return false;
        }

        if line == start.line {
            col >= start.col
        } else if line == end.line {
            col < end.col
        } else {
            true // Middle lines are fully selected
        }
    }

    /// Get the normalized range (start before end)
    pub fn normalized_range(&self) -> (TextPosition, TextPosition) {
        if self.start.line < self.end.line
            || (self.start.line == self.end.line && self.start.col <= self.end.col)
        {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// Clear the selection
    pub fn clear(&mut self) {
        self.active = false;
    }

    /// Check if the selection is empty (start == end)
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Convert screen coordinates (x, y) to window-relative coordinates
/// Returns None if the click is outside any window
pub fn screen_to_window_coords(
    screen_x: u16,
    screen_y: u16,
    window_rect: Rect,
) -> Option<(u16, u16)> {
    // Check if click is within window bounds
    if screen_x < window_rect.x
        || screen_x >= window_rect.x + window_rect.width
        || screen_y < window_rect.y
        || screen_y >= window_rect.y + window_rect.height
    {
        return None;
    }

    // Convert to window-relative coordinates
    let rel_x = screen_x - window_rect.x;
    let rel_y = screen_y - window_rect.y;

    Some((rel_x, rel_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== TextPosition Tests ====================

    #[test]
    fn test_text_position_equality() {
        let pos1 = TextPosition {
            window_index: 0,
            line: 5,
            col: 10,
        };
        let pos2 = TextPosition {
            window_index: 0,
            line: 5,
            col: 10,
        };
        let pos3 = TextPosition {
            window_index: 0,
            line: 5,
            col: 11,
        };

        assert_eq!(pos1, pos2);
        assert_ne!(pos1, pos3);
    }

    // ==================== SelectionState Creation ====================

    #[test]
    fn test_selection_new() {
        let selection = SelectionState::new(0, 5, 10);

        assert!(selection.active);
        assert_eq!(selection.start.window_index, 0);
        assert_eq!(selection.start.line, 5);
        assert_eq!(selection.start.col, 10);
        // Start and end should be the same initially
        assert_eq!(selection.start, selection.end);
    }

    #[test]
    fn test_selection_is_empty() {
        let selection = SelectionState::new(0, 5, 10);
        assert!(selection.is_empty());

        let mut selection2 = SelectionState::new(0, 5, 10);
        selection2.update_end(0, 5, 15);
        assert!(!selection2.is_empty());
    }

    // ==================== Selection Update ====================

    #[test]
    fn test_update_end_same_window() {
        let mut selection = SelectionState::new(0, 5, 10);
        selection.update_end(0, 7, 20);

        assert_eq!(selection.end.line, 7);
        assert_eq!(selection.end.col, 20);
        // Start should remain unchanged
        assert_eq!(selection.start.line, 5);
        assert_eq!(selection.start.col, 10);
    }

    #[test]
    fn test_update_end_different_window_ignored() {
        let mut selection = SelectionState::new(0, 5, 10);
        selection.update_end(1, 7, 20); // Different window

        // End should NOT be updated (window boundary respected)
        assert_eq!(selection.end.line, 5);
        assert_eq!(selection.end.col, 10);
    }

    // ==================== Normalized Range ====================

    #[test]
    fn test_normalized_range_forward_selection() {
        let mut selection = SelectionState::new(0, 5, 10);
        selection.update_end(0, 8, 15);

        let (start, end) = selection.normalized_range();
        assert_eq!(start.line, 5);
        assert_eq!(start.col, 10);
        assert_eq!(end.line, 8);
        assert_eq!(end.col, 15);
    }

    #[test]
    fn test_normalized_range_backward_selection() {
        let mut selection = SelectionState::new(0, 8, 15);
        selection.update_end(0, 5, 10);

        let (start, end) = selection.normalized_range();
        // Should be normalized (start before end)
        assert_eq!(start.line, 5);
        assert_eq!(start.col, 10);
        assert_eq!(end.line, 8);
        assert_eq!(end.col, 15);
    }

    #[test]
    fn test_normalized_range_same_line_backward() {
        let mut selection = SelectionState::new(0, 5, 20);
        selection.update_end(0, 5, 10);

        let (start, end) = selection.normalized_range();
        assert_eq!(start.col, 10);
        assert_eq!(end.col, 20);
    }

    // ==================== Contains Tests ====================

    #[test]
    fn test_contains_single_line_selection() {
        let mut selection = SelectionState::new(0, 5, 10);
        selection.update_end(0, 5, 20);

        // Within range
        assert!(selection.contains(0, 5, 10));
        assert!(selection.contains(0, 5, 15));
        assert!(selection.contains(0, 5, 19));

        // End is exclusive
        assert!(!selection.contains(0, 5, 20));

        // Before range
        assert!(!selection.contains(0, 5, 9));

        // Different line
        assert!(!selection.contains(0, 4, 15));
        assert!(!selection.contains(0, 6, 15));

        // Different window
        assert!(!selection.contains(1, 5, 15));
    }

    #[test]
    fn test_contains_multi_line_selection() {
        let mut selection = SelectionState::new(0, 5, 10);
        selection.update_end(0, 8, 15);

        // Start line - from col onwards
        assert!(selection.contains(0, 5, 10));
        assert!(selection.contains(0, 5, 50));
        assert!(!selection.contains(0, 5, 9));

        // Middle lines - fully selected
        assert!(selection.contains(0, 6, 0));
        assert!(selection.contains(0, 6, 100));
        assert!(selection.contains(0, 7, 0));
        assert!(selection.contains(0, 7, 100));

        // End line - up to col (exclusive)
        assert!(selection.contains(0, 8, 0));
        assert!(selection.contains(0, 8, 14));
        assert!(!selection.contains(0, 8, 15));
        assert!(!selection.contains(0, 8, 20));

        // Outside range
        assert!(!selection.contains(0, 4, 15));
        assert!(!selection.contains(0, 9, 5));
    }

    #[test]
    fn test_contains_inactive_selection() {
        let mut selection = SelectionState::new(0, 5, 10);
        selection.update_end(0, 5, 20);
        selection.clear();

        // Should not contain anything when inactive
        assert!(!selection.contains(0, 5, 15));
    }

    #[test]
    fn test_contains_backward_selection() {
        // Select from (8, 15) back to (5, 10)
        let mut selection = SelectionState::new(0, 8, 15);
        selection.update_end(0, 5, 10);

        // Should still work correctly (normalized internally)
        assert!(selection.contains(0, 5, 10));
        assert!(selection.contains(0, 6, 5));
        assert!(selection.contains(0, 8, 14));
        assert!(!selection.contains(0, 8, 15));
    }

    // ==================== Clear Tests ====================

    #[test]
    fn test_clear_selection() {
        let mut selection = SelectionState::new(0, 5, 10);
        selection.update_end(0, 8, 15);

        assert!(selection.active);
        selection.clear();
        assert!(!selection.active);
    }

    // ==================== Screen to Window Coords ====================

    #[test]
    fn test_screen_to_window_coords_inside() {
        let rect = Rect {
            x: 10,
            y: 5,
            width: 80,
            height: 24,
        };

        // Click inside window
        let result = screen_to_window_coords(15, 10, rect);
        assert_eq!(result, Some((5, 5)));

        // Click at top-left corner
        let result = screen_to_window_coords(10, 5, rect);
        assert_eq!(result, Some((0, 0)));

        // Click at bottom-right (just inside)
        let result = screen_to_window_coords(89, 28, rect);
        assert_eq!(result, Some((79, 23)));
    }

    #[test]
    fn test_screen_to_window_coords_outside() {
        let rect = Rect {
            x: 10,
            y: 5,
            width: 80,
            height: 24,
        };

        // Click to the left
        assert_eq!(screen_to_window_coords(9, 10, rect), None);

        // Click above
        assert_eq!(screen_to_window_coords(15, 4, rect), None);

        // Click to the right (at boundary)
        assert_eq!(screen_to_window_coords(90, 10, rect), None);

        // Click below (at boundary)
        assert_eq!(screen_to_window_coords(15, 29, rect), None);
    }

    #[test]
    fn test_screen_to_window_coords_at_origin() {
        let rect = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };

        let result = screen_to_window_coords(0, 0, rect);
        assert_eq!(result, Some((0, 0)));

        let result = screen_to_window_coords(50, 25, rect);
        assert_eq!(result, Some((50, 25)));
    }
}
