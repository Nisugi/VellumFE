//! Geometry newtypes for window layout coordinates and extents.
//!
//! Window geometry is 2D and mixes two distinct kinds of quantity:
//! **coordinates** (a position on an axis) and **extents** (a length along an
//! axis). Plain `u16` lets you add an X-coordinate to a Y-extent, or overflow a
//! screen edge with a bare `+`. These newtypes make the axis and the
//! coordinate-vs-extent distinction part of the type:
//!
//! - `Col` / `Row`   — coordinates on the horizontal / vertical axis
//! - `Width` / `Height` — extents along the horizontal / vertical axis
//!
//! The algebra encodes what is meaningful:
//! - `Col + Width -> Col`   (move a horizontal coordinate by a horizontal extent)
//! - `Col - Col   -> Width` (the gap between two horizontal coordinates)
//! - `Width + Width -> Width`
//! - `Col + Row`, `Col + Col`, `Col + Height` — do not compile
//!
//! All arithmetic **saturates** at the `u16` bounds by construction, so the bare
//! `+` panics that plagued the resize cascade become impossible: there is no
//! plain `+` to reach for.
//!
//! Escape to a raw `u16` at rendering/interop boundaries via [`.get()`](Col::get).
//! Keeping the unwrap explicit and grep-able (`rg '\.get\(\)'`) makes every place
//! that leaves the type-safe world visible.

use serde::{Deserialize, Serialize};
use std::ops::{Add, Sub};

/// Generates a coordinate or extent newtype over `u16` with the common API:
/// `new`/`get`, `From<u16>`/`From<Self> for u16`, and a `#[serde(transparent)]`
/// derive so the on-disk representation is byte-identical to a bare `u16`
/// (required for the `#[serde(flatten)]`ed fields in `WindowBase`).
macro_rules! geometry_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default,
            Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(u16);

        impl $name {
            /// Wrap a raw `u16`.
            pub const fn new(value: u16) -> Self {
                $name(value)
            }

            /// Unwrap to the raw `u16`. Use at rendering/interop boundaries only.
            pub const fn get(self) -> u16 {
                self.0
            }

            /// Saturating conversion from a signed delta, floored at 0.
            /// Used where a pointer delta (`i32`) shifts a coordinate/extent.
            pub fn from_i32_saturating(value: i32) -> Self {
                $name(value.clamp(0, u16::MAX as i32) as u16)
            }

            /// The larger of two values.
            pub fn max(self, other: Self) -> Self {
                $name(self.0.max(other.0))
            }

            /// The smaller of two values.
            pub fn min(self, other: Self) -> Self {
                $name(self.0.min(other.0))
            }
        }

        impl From<u16> for $name {
            fn from(value: u16) -> Self {
                $name(value)
            }
        }

        impl From<$name> for u16 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

geometry_newtype! {
    /// A horizontal coordinate (column). `col`/`x` in the layout.
    Col
}
geometry_newtype! {
    /// A vertical coordinate (row). `row`/`y` in the layout.
    Row
}
geometry_newtype! {
    /// A horizontal extent (number of columns wide).
    Width
}
geometry_newtype! {
    /// A vertical extent (number of rows tall).
    Height
}

// --- Coordinate <-> extent algebra (all saturating) ---

impl Add<Width> for Col {
    type Output = Col;
    /// Shift a column coordinate right by a width, saturating at `u16::MAX`.
    fn add(self, rhs: Width) -> Col {
        Col(self.0.saturating_add(rhs.0))
    }
}

impl Sub<Width> for Col {
    type Output = Col;
    /// Shift a column coordinate left by a width, saturating at 0.
    fn sub(self, rhs: Width) -> Col {
        Col(self.0.saturating_sub(rhs.0))
    }
}

impl Sub<Col> for Col {
    type Output = Width;
    /// The horizontal gap between two columns, saturating at 0.
    fn sub(self, rhs: Col) -> Width {
        Width(self.0.saturating_sub(rhs.0))
    }
}

impl Add<Height> for Row {
    type Output = Row;
    /// Shift a row coordinate down by a height, saturating at `u16::MAX`.
    fn add(self, rhs: Height) -> Row {
        Row(self.0.saturating_add(rhs.0))
    }
}

impl Sub<Height> for Row {
    type Output = Row;
    /// Shift a row coordinate up by a height, saturating at 0.
    fn sub(self, rhs: Height) -> Row {
        Row(self.0.saturating_sub(rhs.0))
    }
}

impl Sub<Row> for Row {
    type Output = Height;
    /// The vertical gap between two rows, saturating at 0.
    fn sub(self, rhs: Row) -> Height {
        Height(self.0.saturating_sub(rhs.0))
    }
}

impl Add<Width> for Width {
    type Output = Width;
    fn add(self, rhs: Width) -> Width {
        Width(self.0.saturating_add(rhs.0))
    }
}

impl Sub<Width> for Width {
    type Output = Width;
    fn sub(self, rhs: Width) -> Width {
        Width(self.0.saturating_sub(rhs.0))
    }
}

impl Add<Height> for Height {
    type Output = Height;
    fn add(self, rhs: Height) -> Height {
        Height(self.0.saturating_add(rhs.0))
    }
}

impl Sub<Height> for Height {
    type Output = Height;
    fn sub(self, rhs: Height) -> Height {
        Height(self.0.saturating_sub(rhs.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_round_trips_new() {
        assert_eq!(Col::new(7).get(), 7);
        assert_eq!(Row::new(0).get(), 0);
        assert_eq!(Width::new(u16::MAX).get(), u16::MAX);
    }

    #[test]
    fn from_u16_and_back() {
        let c: Col = 12u16.into();
        assert_eq!(c, Col::new(12));
        assert_eq!(u16::from(c), 12);
    }

    #[test]
    fn col_plus_width_shifts_and_saturates() {
        assert_eq!(Col::new(10) + Width::new(5), Col::new(15));
        // saturates instead of panicking on overflow
        assert_eq!(Col::new(u16::MAX) + Width::new(1), Col::new(u16::MAX));
    }

    #[test]
    fn col_minus_width_saturates_at_zero() {
        assert_eq!(Col::new(10) - Width::new(4), Col::new(6));
        assert_eq!(Col::new(3) - Width::new(10), Col::new(0));
    }

    #[test]
    fn col_minus_col_is_a_width_gap() {
        let gap: Width = Col::new(30) - Col::new(10);
        assert_eq!(gap, Width::new(20));
        // saturates at 0 when the coordinates are reversed
        assert_eq!(Col::new(10) - Col::new(30), Width::new(0));
    }

    #[test]
    fn row_algebra_mirrors_col() {
        assert_eq!(Row::new(2) + Height::new(3), Row::new(5));
        assert_eq!(Row::new(2) - Height::new(9), Row::new(0));
        let gap: Height = Row::new(9) - Row::new(4);
        assert_eq!(gap, Height::new(5));
    }

    #[test]
    fn extent_addition_saturates() {
        assert_eq!(Width::new(4) + Width::new(6), Width::new(10));
        assert_eq!(
            Height::new(u16::MAX) + Height::new(1),
            Height::new(u16::MAX)
        );
        assert_eq!(Width::new(3) - Width::new(8), Width::new(0));
    }

    #[test]
    fn from_i32_saturating_floors_and_caps() {
        assert_eq!(Col::from_i32_saturating(-5), Col::new(0));
        assert_eq!(Width::from_i32_saturating(42), Width::new(42));
        assert_eq!(Row::from_i32_saturating(i32::MAX), Row::new(u16::MAX));
    }

    #[test]
    fn min_max_helpers() {
        assert_eq!(Col::new(3).max(Col::new(7)), Col::new(7));
        assert_eq!(Height::new(3).min(Height::new(7)), Height::new(3));
    }

    #[test]
    fn serde_is_transparent_to_bare_u16() {
        // A newtype must serialize identically to the u16 it wraps, so the
        // #[serde(flatten)]ed WindowBase fields keep the on-disk TOML format.
        let c = Col::new(13);
        let as_json = serde_json::to_string(&c).unwrap();
        assert_eq!(as_json, "13");
        let back: Col = serde_json::from_str("13").unwrap();
        assert_eq!(back, c);
    }
}
