//! Local-catalog façade — ONE enumeration seam for "which windows can
//! the user create/toggle here".
//!
//! Window-system redesign Phase 3c (spec:
//! `.beads/artifacts/window-system-redesign/spec.md`). Both frontends'
//! Add/Hide menus and the Windows-list phantom pass enumerate through
//! these functions instead of calling the template catalog directly.
//! Today every function is a PURE DELEGATION — output byte-identical,
//! pinned by the Phase 0 golden fixture and the menu-content tests.
//! Phase 6 swaps the delegation for the real `CatalogEntry` table and
//! deletes the 55-arm template match behind this unchanged seam.

use crate::config::{Config, GameType, Layout, WidgetCategory, WindowDef};
use std::collections::HashMap;

/// Everything creatable for this game, in catalog order.
pub fn creatable_for_game(game: Option<GameType>) -> Vec<String> {
    Config::list_window_templates_for_game(game)
}

/// Creatable entries not already visible in the layout, by category —
/// the Add menus' source.
pub fn addable_by_category(
    layout: &Layout,
    game: Option<GameType>,
) -> HashMap<WidgetCategory, Vec<String>> {
    Config::get_addable_templates_by_category(layout, game)
}

/// Layout windows by category filtered on shown state — the Hide/Edit
/// menus' source.
pub fn visible_by_category(
    layout: &Layout,
    shown: bool,
) -> HashMap<WidgetCategory, Vec<String>> {
    Config::get_visible_templates_by_category(layout, shown)
}

/// Seed a fresh `WindowDef` for a catalog key.
pub fn seed(key: &str) -> Option<WindowDef> {
    Config::get_window_template(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The façade is a pure delegation until Phase 6: byte-identical to
    /// the catalog for every game type. When the CatalogEntry table
    /// replaces the delegation, THIS test (plus the Phase 0 golden)
    /// proves the swap changed nothing.
    #[test]
    fn facade_is_equivalent_to_the_template_catalog()  {
        let empty: Layout = toml::from_str("windows = []").unwrap();
        for game in [None, Some(GameType::GS4), Some(GameType::DR)] {
            assert_eq!(
                creatable_for_game(game),
                Config::list_window_templates_for_game(game)
            );
            assert_eq!(
                addable_by_category(&empty, game),
                Config::get_addable_templates_by_category(&empty, game)
            );
        }
        for key in Config::list_window_templates() {
            assert_eq!(seed(&key), Config::get_window_template(&key));
        }
    }
}
