//! READY / STOW list state — a port of Lich's `ReadyList` / `StowList`
//! (`lib/gemstone/readylist.rb`, `stowlist.rb`) plus the stream regexes that
//! populate them (`lib/gemstone/infomon/xmlparser.rb:494-507`).
//!
//! In Lich these are populated asynchronously by the XML stream parser, not
//! by the `ready list` / `stow list` command itself — the command only sets a
//! `checked` flag. VellumFE works the same way: [`ReadyStow::parse_line`] runs
//! over each game line in `messages.rs` (like our other feed matchers) and
//! fills this state; the StashService (P2b) sends `ready list` / `stow list`
//! and waits for `checked`.
//!
//! `store_list` holds the store-mode STRING Lich stores verbatim
//! (`"put in sheath"`, `"stowed"`, …), because `stash.rb` branches on the
//! exact text.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use super::GameItem;

macro_rules! re {
    ($name:ident, $pattern:literal) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($pattern).expect("valid pattern"));
    };
}

/// A `<a exist noun>name</a>` link inside a READY/STOW row.
re!(
    LINK,
    r#"<a exist="(?P<id>-?\d+)" noun="(?P<noun>[^"]+)">(?P<name>[^<]+)</a>"#
);

// --- STOW list (game command `stow list`) ---
// "You have the following containers set as stow targets:"
re!(
    STOW_START,
    r"^You have the following containers set as stow targets:"
);
// "  a <a ...>backpack</a> (default)" — captures the (type) in parens.
re!(
    STOW_ROW,
    r"\((?P<type>box|gem|herb|skin|wand|scroll|potion|trinket|reagent|lockpick|treasure|forageable|collectible|default)\)\s*$"
);

// --- READY list (game command `ready list`) ---
// "Your current settings are:"
re!(READY_START, r"^Your current settings are:");
// A sheath / secondary sheath row carries the sheath container item.
re!(READY_SHEATH, r"^\s*(?P<type>(?:secondary )?sheath):");
// The main ready rows carry both an item and a store-mode string in
// "(<d cmd='store set'>MODE</d>)".
re!(
    READY_STORE,
    r"^\s*(?P<type>shield|(?:secondary |ranged )?weapon|ammo bundle|wand):.*<d cmd='store set'>(?P<store>worn if possible, stowed otherwise|stowed|put in (?:secondary )?sheath)</d>"
);
// Footer: "Click here to update the list." — marks the ready list complete.
re!(
    READY_DONE,
    r"Click <d cmd=.ready list.>here</d> to update the list"
);

/// A ready slot key. Mirrors Lich's `ORIGINAL_READY_LIST` symbols (minus the
/// ammo variants we don't route on).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReadyKey {
    Shield,
    Weapon,
    SecondaryWeapon,
    RangedWeapon,
    Wand,
    Sheath,
    SecondarySheath,
}

impl ReadyKey {
    /// Map the regex `(?<type>)` text to a key (Lich `normalize_name`).
    fn from_type(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "shield" => Self::Shield,
            "weapon" => Self::Weapon,
            "secondary weapon" => Self::SecondaryWeapon,
            "ranged weapon" => Self::RangedWeapon,
            "wand" => Self::Wand,
            "sheath" => Self::Sheath,
            "secondary sheath" => Self::SecondarySheath,
            _ => return None,
        })
    }
}

/// A stow slot key. Mirrors Lich's `ORIGINAL_STOW_LIST`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StowKey {
    Box,
    Gem,
    Herb,
    Skin,
    Wand,
    Scroll,
    Potion,
    Trinket,
    Reagent,
    Lockpick,
    Treasure,
    Forageable,
    Collectible,
    Default,
}

impl StowKey {
    fn from_type(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "box" => Self::Box,
            "gem" => Self::Gem,
            "herb" => Self::Herb,
            "skin" => Self::Skin,
            "wand" => Self::Wand,
            "scroll" => Self::Scroll,
            "potion" => Self::Potion,
            "trinket" => Self::Trinket,
            "reagent" => Self::Reagent,
            "lockpick" => Self::Lockpick,
            "treasure" => Self::Treasure,
            "forageable" => Self::Forageable,
            "collectible" => Self::Collectible,
            "default" => Self::Default,
            _ => return None,
        })
    }
}

/// The parsed READY/STOW state, matching `ReadyList`/`StowList`.
#[derive(Clone, Debug, Default)]
pub struct ReadyStow {
    /// Ready slot -> the item readied there (sheath containers included).
    ready: HashMap<ReadyKey, GameItem>,
    /// Ready slot -> its store-mode string (verbatim, as stash.rb branches).
    store: HashMap<ReadyKey, String>,
    /// Stow slot -> its container item.
    stow: HashMap<StowKey, GameItem>,
    /// True once a full `ready list` has been seen this session.
    ready_checked: bool,
    /// True once a full `stow list` has been seen this session.
    stow_checked: bool,
}

impl ReadyStow {
    pub fn ready_checked(&self) -> bool {
        self.ready_checked
    }
    pub fn stow_checked(&self) -> bool {
        self.stow_checked
    }

    /// The sheath container, preferring the primary sheath and falling back to
    /// the secondary (Lich `stash_hands` sheath selection).
    pub fn sheath(&self) -> Option<&GameItem> {
        self.ready
            .get(&ReadyKey::Sheath)
            .or_else(|| self.ready.get(&ReadyKey::SecondarySheath))
    }

    /// The secondary sheath, falling back to the primary (mirrors Lich's
    /// `second_sheath` selection when only one is set).
    pub fn second_sheath(&self) -> Option<&GameItem> {
        self.ready
            .get(&ReadyKey::SecondarySheath)
            .or_else(|| self.ready.get(&ReadyKey::Sheath))
    }

    /// The default stow container (`StowList.default`).
    pub fn default_stow(&self) -> Option<&GameItem> {
        self.stow.get(&StowKey::Default)
    }

    /// The store-mode string for the ready slot matching `item`, if any.
    /// Lich looks the item up in `ready_list` by id and reads `store_list`.
    pub fn store_mode_for(&self, item: &GameItem) -> Option<&str> {
        let (&key, _) = self.ready.iter().find(|(_, v)| v.id == item.id)?;
        self.store.get(&key).map(String::as_str)
    }

    /// Feed one game line as a raw XML string (still carrying `<a exist>`
    /// links). Convenience for tests and the raw-line path. Returns true if
    /// it touched ready/stow state.
    pub fn parse_line(&mut self, line: &str) -> bool {
        self.parse(line, parse_link(line))
    }

    /// Feed one game line: `text` is the flat line (for the type/store-mode
    /// regexes) and `link` is the first `<a exist noun>name</a>` on it, if
    /// any (the readied/stow item). Ported from `xmlparser.rb` READY/STOW
    /// dispatch. Returns true if it touched ready/stow state.
    pub fn parse(&mut self, text: &str, link: Option<GameItem>) -> bool {
        if READY_START.is_match(text) {
            self.ready.clear();
            self.store.clear();
            self.ready_checked = false;
            return true;
        }
        if STOW_START.is_match(text) {
            self.stow.clear();
            self.stow_checked = false;
            return true;
        }
        if READY_DONE.is_match(text) {
            self.ready_checked = true;
            return true;
        }
        // Sheath rows fill ready.sheath / ready.secondary_sheath.
        if let Some(c) = READY_SHEATH.captures(text) {
            if let (Some(key), Some(item)) = (ReadyKey::from_type(&c["type"]), link.clone()) {
                self.ready.insert(key, item);
                return true;
            }
        }
        // Main ready rows: item (optional) + store-mode string.
        if let Some(c) = READY_STORE.captures(text) {
            if let Some(key) = ReadyKey::from_type(&c["type"]) {
                if let Some(item) = link.clone() {
                    self.ready.insert(key, item);
                }
                self.store.insert(key, c["store"].to_string());
                return true;
            }
        }
        // Stow rows: "  a backpack (gem)".
        if let Some(c) = STOW_ROW.captures(text) {
            if let (Some(key), Some(item)) = (StowKey::from_type(&c["type"]), link) {
                self.stow.insert(key, item);
                self.stow_checked = true;
                return true;
            }
        }
        false
    }
}

/// Extract the first `<a exist noun>name</a>` link on a raw line as a
/// GameItem. Only used by the raw-line convenience path / tests; the live
/// feed passes the item via `parse`.
fn parse_link(line: &str) -> Option<GameItem> {
    let c = LINK.captures(line)?;
    Some(GameItem::new(&c["id"], &c["noun"], &c["name"]))
}

/// Cheap gate: could this line be part of a READY/STOW list? Lets the feed
/// skip the full regex battery on ordinary lines. Matches the two headers,
/// the footer, and the row shapes (`sheath:`/`weapon:`/… or a `(gem)`-style
/// stow-type suffix).
pub fn line_is_ready_stow(text: &str) -> bool {
    READY_START.is_match(text)
        || STOW_START.is_match(text)
        || READY_DONE.is_match(text)
        || READY_SHEATH.is_match(text)
        || READY_STORE.is_match(text)
        || STOW_ROW.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_stow_list() {
        let mut rs = ReadyStow::default();
        assert!(!rs.stow_checked());
        rs.parse_line("You have the following containers set as stow targets:");
        rs.parse_line(r#"  a <a exist="111" noun="backpack">leather backpack</a> (default)"#);
        rs.parse_line(r#"  a <a exist="222" noun="pouch">gem pouch</a> (gem)"#);
        assert!(rs.stow_checked());
        assert_eq!(rs.default_stow().map(|i| i.id.as_str()), Some("111"));
        assert_eq!(
            rs.stow.get(&StowKey::Gem).map(|i| i.id.as_str()),
            Some("222")
        );
    }

    #[test]
    fn parses_ready_sheath_and_store_mode() {
        let mut rs = ReadyStow::default();
        rs.parse_line("Your current settings are:");
        rs.parse_line(
            r#"  sheath: <d cmd="store SHEATH clear">a <a exist="333" noun="scabbard">leather scabbard</a></d>"#,
        );
        rs.parse_line(
            r#"  weapon: (<d cmd='store WEAPON clear'>a <a exist="444" noun="sword">broadsword</a></d>) (<d cmd='store set'>put in sheath</d>)"#,
        );
        assert_eq!(rs.sheath().map(|i| i.id.as_str()), Some("333"));
        // The weapon's store-mode routes it to the sheath.
        let sword = GameItem::new("444", "sword", "broadsword");
        assert_eq!(rs.store_mode_for(&sword), Some("put in sheath"));
    }

    #[test]
    fn ready_start_resets_and_done_marks_checked() {
        let mut rs = ReadyStow::default();
        rs.parse_line("Your current settings are:");
        assert!(!rs.ready_checked());
        rs.parse_line(r#"Click <d cmd="ready list">here</d> to update the list."#);
        assert!(rs.ready_checked());
        // A fresh ready list clears prior state.
        rs.parse_line("Your current settings are:");
        assert!(!rs.ready_checked());
    }
}
