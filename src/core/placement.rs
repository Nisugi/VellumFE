//! Ephemeral-window placement policy — the seed of the placement engine.
//!
//! Window-system redesign Phase 3e (spec:
//! `.beads/artifacts/window-system-redesign/spec.md`): the two
//! previously-hardcoded ephemeral placements (containers 40×15 centered,
//! dialog panels 26×20 right edge) route through ONE helper that honors
//! the game's own declaration hints — the `WindowHints` attributes the
//! parser now captures from `openDialog`/`streamWindow`/`container`.
//!
//! Precedence (per the design): saved per-id position (callers check
//! first) → game hint (location + size, clamped — even the game sends
//! viewport-busting sizes like espMasterDialog height='2100') → kind
//! default. Full free-rect packing against existing windows is the
//! follow-up hook for the GUI placement engine.

/// Where a window lands when neither a saved position nor a location
/// hint says otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementAnchor {
    Center,
    RightEdge,
}

/// Wrayth hint sizes are in PIXELS (combat declares ~190×288 for a
/// ~24×18-cell panel); the cell grid is ~8×16 px.
const PX_PER_COL: u16 = 8;
const PX_PER_ROW: u16 = 16;

fn hint<'a>(hints: Option<&'a [(String, String)]>, name: &str) -> Option<&'a str> {
    hints?
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// Resolve an ephemeral window's seed rect `(x, y, w, h)` in cells.
pub fn ephemeral_placement(
    hints: Option<&[(String, String)]>,
    default_size: (u16, u16),
    default_anchor: PlacementAnchor,
    terminal: (u16, u16),
) -> (u16, u16, u16, u16) {
    let (term_w, term_h) = terminal;

    // Size: hint pixels → cells, clamped into the terminal (minus a
    // 1-cell margin) and floored at a usable minimum.
    let hinted_dim = |name: &str, px_per_cell: u16, term: u16, default: u16| -> u16 {
        let hinted = hint(hints, name)
            .and_then(|value| value.parse::<u32>().ok())
            .map(|px| (px / px_per_cell as u32).max(1) as u16);
        hinted
            .unwrap_or(default)
            .clamp(4, term.saturating_sub(2).max(4))
    };
    let w = hinted_dim("width", PX_PER_COL, term_w, default_size.0);
    let h = hinted_dim("height", PX_PER_ROW, term_h, default_size.1);

    // Anchor: the wire's location vocabulary, else the kind default.
    let anchor = match hint(hints, "location") {
        Some("center") | Some("force-center") => PlacementAnchor::Center,
        Some("right") => PlacementAnchor::RightEdge,
        _ => default_anchor,
    };
    let (x, y) = match anchor {
        PlacementAnchor::Center => (
            term_w.saturating_sub(w) / 2,
            term_h.saturating_sub(h) / 2,
        ),
        PlacementAnchor::RightEdge => (term_w.saturating_sub(w + 1), 1),
    };
    (x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERM: (u16, u16) = (120, 40);

    fn hints(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn no_hints_reproduces_the_hardcoded_defaults() {
        // Containers: 40x15 centered.
        assert_eq!(
            ephemeral_placement(None, (40, 15), PlacementAnchor::Center, TERM),
            (40, 12, 40, 15)
        );
        // Panels: 26x20 right edge.
        assert_eq!(
            ephemeral_placement(None, (26, 20), PlacementAnchor::RightEdge, TERM),
            (93, 1, 26, 20)
        );
    }

    #[test]
    fn location_hint_overrides_the_kind_default() {
        let h = hints(&[("location", "force-center")]);
        let (x, y, ..) =
            ephemeral_placement(Some(&h), (26, 20), PlacementAnchor::RightEdge, TERM);
        assert_eq!((x, y), (47, 10), "force-center recenters a panel");

        let h = hints(&[("location", "right")]);
        let (x, y, ..) =
            ephemeral_placement(Some(&h), (40, 15), PlacementAnchor::Center, TERM);
        assert_eq!((x, y), (79, 1), "right docks a container");
    }

    #[test]
    fn pixel_size_hints_convert_to_cells_and_clamp() {
        // combat declares ~190x288 px → ~23x18 cells.
        let h = hints(&[("width", "190"), ("height", "288")]);
        let (.., w, hgt) =
            ephemeral_placement(Some(&h), (26, 20), PlacementAnchor::RightEdge, TERM);
        assert_eq!((w, hgt), (23, 18));

        // The game itself sends viewport-busting sizes (espMasterDialog
        // height='2100' → 131 rows): clamped into the terminal.
        let h = hints(&[("height", "2100")]);
        let (.., _, hgt) =
            ephemeral_placement(Some(&h), (26, 20), PlacementAnchor::RightEdge, TERM);
        assert_eq!(hgt, 38, "clamped to terminal height - 2");

        // Garbage sizes fall back to the default.
        let h = hints(&[("width", "25%")]);
        let (.., w, _) =
            ephemeral_placement(Some(&h), (26, 20), PlacementAnchor::RightEdge, TERM);
        assert_eq!(w, 26);
    }
}
