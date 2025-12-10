use ratatui::layout::Rect;

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
