//! Frontend-agnostic rectangular region type.
//!
//! This module provides a simple Rect structure that represents a rectangular
//! area with position (x, y) and dimensions (width, height). It's used across
//! both TUI and GUI frontends for layout calculations and bounds checking.

/// A rectangular region with position and dimensions.
///
/// Used for layout calculations, bounds checking, and coordinate transformations
/// across different frontend implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// X coordinate (column) of the top-left corner
    pub x: u16,
    /// Y coordinate (row) of the top-left corner
    pub y: u16,
    /// Width of the rectangle
    pub width: u16,
    /// Height of the rectangle
    pub height: u16,
}

impl Rect {
    /// Create a new rectangle with the given position and dimensions
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if a point (x, y) is inside this rectangle
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x
            && x < self.x + self.width
            && y >= self.y
            && y < self.y + self.height
    }

    /// Get the right edge coordinate (exclusive)
    pub fn right(&self) -> u16 {
        self.x + self.width
    }

    /// Get the bottom edge coordinate (exclusive)
    pub fn bottom(&self) -> u16 {
        self.y + self.height
    }
}

impl Default for Rect {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // Construction tests
    // ===========================================

    #[test]
    fn test_rect_new() {
        let rect = Rect::new(10, 20, 100, 50);
        assert_eq!(rect.x, 10);
        assert_eq!(rect.y, 20);
        assert_eq!(rect.width, 100);
        assert_eq!(rect.height, 50);
    }

    #[test]
    fn test_rect_new_at_origin() {
        let rect = Rect::new(0, 0, 100, 50);
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, 0);
    }

    #[test]
    fn test_rect_new_unit_size() {
        let rect = Rect::new(5, 5, 1, 1);
        assert_eq!(rect.width, 1);
        assert_eq!(rect.height, 1);
    }

    #[test]
    fn test_rect_new_max_values() {
        let rect = Rect::new(u16::MAX, u16::MAX, u16::MAX, u16::MAX);
        assert_eq!(rect.x, u16::MAX);
        assert_eq!(rect.y, u16::MAX);
        assert_eq!(rect.width, u16::MAX);
        assert_eq!(rect.height, u16::MAX);
    }

    // ===========================================
    // Default trait tests
    // ===========================================

    #[test]
    fn test_rect_default() {
        let rect = Rect::default();
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, 0);
        assert_eq!(rect.width, 0);
        assert_eq!(rect.height, 0);
    }

    #[test]
    fn test_rect_default_is_zero_size() {
        let rect = Rect::default();
        assert_eq!(rect.right(), 0);
        assert_eq!(rect.bottom(), 0);
    }

    // ===========================================
    // Contains tests - interior points
    // ===========================================

    #[test]
    fn test_contains_interior_point() {
        let rect = Rect::new(10, 10, 20, 20);
        // Point in the middle
        assert!(rect.contains(20, 20));
    }

    #[test]
    fn test_contains_top_left_corner() {
        let rect = Rect::new(10, 10, 20, 20);
        // Top-left corner is INSIDE (inclusive)
        assert!(rect.contains(10, 10));
    }

    #[test]
    fn test_contains_just_inside_edges() {
        let rect = Rect::new(10, 10, 20, 20);
        // Just inside right edge (x = 29, not 30)
        assert!(rect.contains(29, 15));
        // Just inside bottom edge (y = 29, not 30)
        assert!(rect.contains(15, 29));
    }

    // ===========================================
    // Contains tests - boundary exclusion
    // ===========================================

    #[test]
    fn test_contains_right_edge_excluded() {
        let rect = Rect::new(10, 10, 20, 20);
        // Right edge is at x=30, which is OUTSIDE (exclusive)
        assert!(!rect.contains(30, 15));
    }

    #[test]
    fn test_contains_bottom_edge_excluded() {
        let rect = Rect::new(10, 10, 20, 20);
        // Bottom edge is at y=30, which is OUTSIDE (exclusive)
        assert!(!rect.contains(15, 30));
    }

    #[test]
    fn test_contains_bottom_right_corner_excluded() {
        let rect = Rect::new(10, 10, 20, 20);
        // Bottom-right corner (30, 30) is OUTSIDE
        assert!(!rect.contains(30, 30));
    }

    // ===========================================
    // Contains tests - outside points
    // ===========================================

    #[test]
    fn test_contains_point_left_of_rect() {
        let rect = Rect::new(10, 10, 20, 20);
        assert!(!rect.contains(5, 15));
    }

    #[test]
    fn test_contains_point_above_rect() {
        let rect = Rect::new(10, 10, 20, 20);
        assert!(!rect.contains(15, 5));
    }

    #[test]
    fn test_contains_point_right_of_rect() {
        let rect = Rect::new(10, 10, 20, 20);
        assert!(!rect.contains(35, 15));
    }

    #[test]
    fn test_contains_point_below_rect() {
        let rect = Rect::new(10, 10, 20, 20);
        assert!(!rect.contains(15, 35));
    }

    #[test]
    fn test_contains_point_diagonal_outside() {
        let rect = Rect::new(10, 10, 20, 20);
        // Top-left outside
        assert!(!rect.contains(5, 5));
        // Bottom-right outside
        assert!(!rect.contains(35, 35));
    }

    // ===========================================
    // Contains tests - zero-size rectangles
    // ===========================================

    #[test]
    fn test_contains_zero_width() {
        let rect = Rect::new(10, 10, 0, 20);
        // Zero width means nothing can be inside
        assert!(!rect.contains(10, 15));
    }

    #[test]
    fn test_contains_zero_height() {
        let rect = Rect::new(10, 10, 20, 0);
        // Zero height means nothing can be inside
        assert!(!rect.contains(15, 10));
    }

    #[test]
    fn test_contains_zero_size() {
        let rect = Rect::new(10, 10, 0, 0);
        // Zero size means nothing can be inside
        assert!(!rect.contains(10, 10));
    }

    // ===========================================
    // Contains tests - unit rectangles
    // ===========================================

    #[test]
    fn test_contains_unit_rect_inside() {
        let rect = Rect::new(5, 5, 1, 1);
        // Only (5, 5) should be inside
        assert!(rect.contains(5, 5));
    }

    #[test]
    fn test_contains_unit_rect_edges() {
        let rect = Rect::new(5, 5, 1, 1);
        // All edges should be outside
        assert!(!rect.contains(4, 5)); // left
        assert!(!rect.contains(6, 5)); // right
        assert!(!rect.contains(5, 4)); // top
        assert!(!rect.contains(5, 6)); // bottom
    }

    // ===========================================
    // Contains tests - origin rectangle
    // ===========================================

    #[test]
    fn test_contains_at_origin() {
        let rect = Rect::new(0, 0, 10, 10);
        assert!(rect.contains(0, 0));
        assert!(rect.contains(5, 5));
        assert!(!rect.contains(10, 10)); // exclusive
    }

    // ===========================================
    // Right and bottom edge tests
    // ===========================================

    #[test]
    fn test_right_calculation() {
        let rect = Rect::new(10, 20, 100, 50);
        assert_eq!(rect.right(), 110);
    }

    #[test]
    fn test_bottom_calculation() {
        let rect = Rect::new(10, 20, 100, 50);
        assert_eq!(rect.bottom(), 70);
    }

    #[test]
    fn test_right_at_origin() {
        let rect = Rect::new(0, 0, 50, 30);
        assert_eq!(rect.right(), 50);
    }

    #[test]
    fn test_bottom_at_origin() {
        let rect = Rect::new(0, 0, 50, 30);
        assert_eq!(rect.bottom(), 30);
    }

    #[test]
    fn test_right_zero_width() {
        let rect = Rect::new(10, 10, 0, 20);
        assert_eq!(rect.right(), 10);
    }

    #[test]
    fn test_bottom_zero_height() {
        let rect = Rect::new(10, 10, 20, 0);
        assert_eq!(rect.bottom(), 10);
    }

    // ===========================================
    // Trait implementation tests
    // ===========================================

    #[test]
    fn test_rect_clone() {
        let rect = Rect::new(10, 20, 30, 40);
        let cloned = rect.clone();
        assert_eq!(rect, cloned);
    }

    #[test]
    fn test_rect_copy() {
        let rect = Rect::new(10, 20, 30, 40);
        let copied = rect; // Copy happens here
        assert_eq!(rect, copied); // Original still accessible
    }

    #[test]
    fn test_rect_equality() {
        let rect1 = Rect::new(10, 20, 30, 40);
        let rect2 = Rect::new(10, 20, 30, 40);
        let rect3 = Rect::new(10, 20, 30, 50);

        assert_eq!(rect1, rect2);
        assert_ne!(rect1, rect3);
    }

    #[test]
    fn test_rect_debug() {
        let rect = Rect::new(10, 20, 30, 40);
        let debug_str = format!("{:?}", rect);
        assert!(debug_str.contains("10"));
        assert!(debug_str.contains("20"));
        assert!(debug_str.contains("30"));
        assert!(debug_str.contains("40"));
    }

    // ===========================================
    // Edge case tests
    // ===========================================

    #[test]
    fn test_large_coordinates() {
        let rect = Rect::new(1000, 2000, 500, 300);
        assert!(rect.contains(1250, 2150));
        assert_eq!(rect.right(), 1500);
        assert_eq!(rect.bottom(), 2300);
    }

    #[test]
    fn test_contains_boundary_sweep() {
        // Test all four corners and edges of a small rectangle
        let rect = Rect::new(5, 5, 3, 3);

        // Inside points
        assert!(rect.contains(5, 5)); // top-left (inclusive)
        assert!(rect.contains(6, 6)); // center
        assert!(rect.contains(7, 7)); // bottom-right - 1 (last valid)

        // Exclusive edges
        assert!(!rect.contains(8, 6)); // right edge
        assert!(!rect.contains(6, 8)); // bottom edge
        assert!(!rect.contains(8, 8)); // bottom-right corner
    }
}
