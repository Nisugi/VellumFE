//! Stable identity model for GUI tabs and windows.
//!
//! Display names are mutable and not used as persistence keys.
//! `TabKey` is the canonical identity for settings, hidden state, and restoration.

use serde::{Deserialize, Serialize};

/// Stable identifier for a tab/window type.
///
/// This enum is the canonical key for:
/// - Persistence (layout files)
/// - Settings (per-tab configuration)
/// - Hidden state (which tabs are hidden)
/// - Restoration (reopening tabs)
///
/// Never use display titles as persistence keys - they can change.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TabKey {
    /// Main text window (story/game output)
    TextMain,

    /// Custom text window with stable internal ID
    TextByName {
        /// Stable internal ID (not the display title)
        id: String,
    },

    /// Inventory window
    Inventory {
        /// Stable internal ID for multiple inventory views
        id: String,
    },

    /// Active effects (buffs/debuffs)
    ActiveEffects {
        /// Stable internal ID
        id: String,
    },

    /// Quickbar/macro bar
    Quickbar {
        /// Stable internal ID (e.g., "quickbar_1", "quickbar_2")
        id: String,
    },

    /// Vitals bars (health, mana, stamina, etc.)
    Vitals,

    /// A single progress-bar window (stance, individual health/mana bars)
    ProgressBar {
        /// Window name (e.g., "stance")
        id: String,
    },

    /// A single countdown-timer window (roundtime, casttime, stuntime).
    /// The id is the window name; the serde default keeps layouts saved
    /// before timers were per-window loading (as an empty id, which then
    /// matches no window and is dropped).
    Countdown {
        #[serde(default)]
        id: String,
    },

    /// Navigation compass
    Compass,

    /// Left hand contents
    LeftHand,

    /// Right hand contents
    RightHand,

    /// Spell hand (prepared spell)
    SpellHand,

    /// Status indicators (kneeling, hidden, etc.)
    Indicators,

    /// Target list (creatures in room)
    Targets,

    /// Players in room
    Players,

    /// Room information (name, description, exits)
    Room,

    /// Experience/training tracker
    Experience,

    /// Injury doll (body part damage)
    InjuryDoll,

    /// Character dashboard (stats grid)
    Dashboard,

    /// Encumbrance display
    Encumbrance,

    /// Perception/awareness display
    Perception,

    /// Lich WebUI panel bound to a page ("script/page")
    WebUi {
        /// Page id, e.g. "creaturebar/main"
        page: String,
    },
}

impl TabKey {
    /// Returns a default display title for this tab type.
    ///
    /// This is only used as a fallback - users can rename tabs freely.
    pub fn default_title(&self) -> String {
        match self {
            TabKey::TextMain => "Story".to_string(),
            TabKey::TextByName { id } => id.clone(),
            TabKey::Inventory { id } => format!("Inventory ({})", id),
            TabKey::ActiveEffects { id } => format!("Effects ({})", id),
            TabKey::Quickbar { id } => format!("Quickbar ({})", id),
            TabKey::Vitals => "Vitals".to_string(),
            TabKey::ProgressBar { id } => id.clone(),
            TabKey::Countdown { id } => {
                if id.is_empty() {
                    "Timers".to_string()
                } else {
                    id.clone()
                }
            }
            TabKey::Compass => "Compass".to_string(),
            TabKey::LeftHand => "Left Hand".to_string(),
            TabKey::RightHand => "Right Hand".to_string(),
            TabKey::SpellHand => "Spell".to_string(),
            TabKey::Indicators => "Status".to_string(),
            TabKey::Targets => "Targets".to_string(),
            TabKey::Players => "Players".to_string(),
            TabKey::Room => "Room".to_string(),
            TabKey::Experience => "Experience".to_string(),
            TabKey::InjuryDoll => "Injuries".to_string(),
            TabKey::Dashboard => "Dashboard".to_string(),
            TabKey::Encumbrance => "Encumbrance".to_string(),
            TabKey::Perception => "Perception".to_string(),
            TabKey::WebUi { page } => page.clone(),
        }
    }

    /// Returns a short identifier string for this tab key.
    ///
    /// Useful for logging and debugging.
    pub fn short_id(&self) -> String {
        match self {
            TabKey::TextMain => "text_main".to_string(),
            TabKey::TextByName { id } => format!("text:{}", id),
            TabKey::Inventory { id } => format!("inv:{}", id),
            TabKey::ActiveEffects { id } => format!("fx:{}", id),
            TabKey::Quickbar { id } => format!("qb:{}", id),
            TabKey::Vitals => "vitals".to_string(),
            TabKey::ProgressBar { id } => format!("bar:{}", id),
            TabKey::Countdown { id } => format!("countdown:{}", id),
            TabKey::Compass => "compass".to_string(),
            TabKey::LeftHand => "left_hand".to_string(),
            TabKey::RightHand => "right_hand".to_string(),
            TabKey::SpellHand => "spell_hand".to_string(),
            TabKey::Indicators => "indicators".to_string(),
            TabKey::Targets => "targets".to_string(),
            TabKey::Players => "players".to_string(),
            TabKey::Room => "room".to_string(),
            TabKey::Experience => "experience".to_string(),
            TabKey::InjuryDoll => "injury_doll".to_string(),
            TabKey::Dashboard => "dashboard".to_string(),
            TabKey::Encumbrance => "encumbrance".to_string(),
            TabKey::Perception => "perception".to_string(),
            TabKey::WebUi { page } => format!("webui:{}", page),
        }
    }
}

/// A tab identifier combining stable key with mutable display title.
///
/// Rules:
/// - `key` is canonical for persistence, settings, hidden state, and restoration
/// - `title` is presentation only and can be changed by users
/// - Renaming a tab must not invalidate settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabId {
    /// The stable identity key (never changes)
    pub key: TabKey,

    /// User-visible display title (mutable)
    pub title: String,
}

impl TabId {
    /// Create a new TabId with the default title for the key.
    pub fn new(key: TabKey) -> Self {
        let title = key.default_title();
        Self { key, title }
    }

    /// Create a new TabId with a custom title.
    pub fn with_title(key: TabKey, title: impl Into<String>) -> Self {
        Self {
            key,
            title: title.into(),
        }
    }

    /// Rename this tab (title only, key is preserved).
    pub fn rename(&mut self, new_title: impl Into<String>) {
        self.title = new_title.into();
    }
}

impl std::fmt::Display for TabId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title)
    }
}

impl PartialEq for TabId {
    fn eq(&self, other: &Self) -> bool {
        // Equality is based on key only, not title
        self.key == other.key
    }
}

impl Eq for TabId {}

impl std::hash::Hash for TabId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash is based on key only, not title
        self.key.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_key_serialization() {
        let key = TabKey::TextMain;
        let json = serde_json::to_string(&key).unwrap();
        let parsed: TabKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn test_tab_key_with_id_serialization() {
        let key = TabKey::TextByName {
            id: "combat".to_string(),
        };
        let json = serde_json::to_string(&key).unwrap();
        assert!(json.contains("text_by_name"));
        assert!(json.contains("combat"));

        let parsed: TabKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn test_tab_key_webui_roundtrip() {
        let key = TabKey::WebUi {
            page: "creaturebar/main".to_string(),
        };
        let json = serde_json::to_string(&key).unwrap();
        assert!(json.contains("web_ui"));
        assert!(json.contains("creaturebar/main"));
        assert_eq!(serde_json::from_str::<TabKey>(&json).unwrap(), key);
        assert_eq!(key.short_id(), "webui:creaturebar/main");
        assert_eq!(key.default_title(), "creaturebar/main");

        // A WebUI panel and a text window whose name happens to equal the
        // page id must NOT collide on the same TabKey.
        let text = TabKey::TextByName {
            id: "creaturebar/main".to_string(),
        };
        assert_ne!(key, text);
    }

    #[test]
    fn test_tab_id_new() {
        let tab = TabId::new(TabKey::Vitals);
        assert_eq!(tab.key, TabKey::Vitals);
        assert_eq!(tab.title, "Vitals");
    }

    #[test]
    fn test_tab_id_with_title() {
        let tab = TabId::with_title(TabKey::TextMain, "Story Window");
        assert_eq!(tab.key, TabKey::TextMain);
        assert_eq!(tab.title, "Story Window");
    }

    #[test]
    fn test_tab_id_rename() {
        let mut tab = TabId::new(TabKey::TextMain);
        assert_eq!(tab.title, "Story");

        tab.rename("Game Output");
        assert_eq!(tab.title, "Game Output");
        assert_eq!(tab.key, TabKey::TextMain); // Key unchanged
    }

    #[test]
    fn test_tab_id_equality_ignores_title() {
        let tab1 = TabId::with_title(TabKey::Vitals, "Health");
        let tab2 = TabId::with_title(TabKey::Vitals, "HP Bars");

        // Same key = equal, even with different titles
        assert_eq!(tab1, tab2);
    }

    #[test]
    fn test_tab_id_hash_ignores_title() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(TabId::with_title(TabKey::Vitals, "Health"));

        // Should be found even with different title
        let lookup = TabId::with_title(TabKey::Vitals, "HP Bars");
        assert!(set.contains(&lookup));
    }

    #[test]
    fn test_tab_key_default_titles() {
        assert_eq!(TabKey::TextMain.default_title(), "Story");
        assert_eq!(TabKey::Compass.default_title(), "Compass");
        assert_eq!(
            TabKey::TextByName {
                id: "combat".to_string()
            }
            .default_title(),
            "combat"
        );
    }

    #[test]
    fn test_tab_key_short_id() {
        assert_eq!(TabKey::TextMain.short_id(), "text_main");
        assert_eq!(TabKey::Vitals.short_id(), "vitals");
        assert_eq!(
            TabKey::Quickbar {
                id: "1".to_string()
            }
            .short_id(),
            "qb:1"
        );
    }

    #[test]
    fn test_all_tab_keys_serialize() {
        // Ensure all variants serialize without panic
        let keys = vec![
            TabKey::TextMain,
            TabKey::TextByName {
                id: "test".to_string(),
            },
            TabKey::Inventory {
                id: "main".to_string(),
            },
            TabKey::ActiveEffects {
                id: "buffs".to_string(),
            },
            TabKey::Quickbar {
                id: "1".to_string(),
            },
            TabKey::Vitals,
            TabKey::Countdown {
                id: "roundtime".to_string(),
            },
            TabKey::Compass,
            TabKey::LeftHand,
            TabKey::RightHand,
            TabKey::SpellHand,
            TabKey::Indicators,
            TabKey::Targets,
            TabKey::Players,
            TabKey::Room,
            TabKey::Experience,
            TabKey::InjuryDoll,
            TabKey::Dashboard,
            TabKey::Encumbrance,
            TabKey::Perception,
        ];

        for key in keys {
            let json = serde_json::to_string(&key).unwrap();
            let parsed: TabKey = serde_json::from_str(&json).unwrap();
            assert_eq!(key, parsed);
        }
    }
}
