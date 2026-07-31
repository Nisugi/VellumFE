//! Shared game-state condition vocabulary. Hotbar button states and hand
//! widget icon states both declare conditions from this enum; the evaluator
//! lives in `core::conditions`. Editors build these from dropdowns and limit
//! group nesting to one level; the evaluator is recursive so deeper
//! hand-authored files still evaluate.

use serde::{Deserialize, Serialize};

/// Structured condition tree, `type`-tagged in TOML.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    All {
        conditions: Vec<Condition>,
    },
    Any {
        conditions: Vec<Condition>,
    },
    EffectActive {
        category: EffectCategory,
        name: String,
        #[serde(default)]
        name_match: NameMatch,
    },
    EffectInactive {
        category: EffectCategory,
        name: String,
        #[serde(default)]
        name_match: NameMatch,
    },
    /// Remaining seconds of an effect compared against a threshold.
    /// False when the effect is absent or has no parseable expiry.
    EffectTime {
        category: EffectCategory,
        name: String,
        #[serde(default)]
        name_match: NameMatch,
        cmp: Cmp,
        seconds: i64,
    },
    RtActive,
    CtActive,
    /// Status indicator by id: standing, kneeling, sitting, prone, stunned,
    /// bleeding, hidden, invisible, webbed, joined, dead.
    Indicator {
        id: String,
        #[serde(default = "default_true")]
        active: bool,
    },
    Vital {
        vital: VitalKind,
        cmp: Cmp,
        value: u32,
        #[serde(default)]
        unit: VitalUnit,
    },
    /// A body-part injury (from the injury XML that drives the doll) compared
    /// against a level threshold. Levels: 1-3 = wounds, 4-6 = scars, absent
    /// (healthy) = 0. `area` is the body-part id the injury feed uses (e.g.
    /// "neck", "head", "leftArm"). False when the part is healthy/unknown —
    /// e.g. `cmp: >=, level: 2` matches a rank-2+ wound on that part.
    Injury {
        area: String,
        cmp: Cmp,
        level: u8,
    },
    /// The bundled spell table (effect-list.xml) lists static costs for
    /// this spell number and current absolute vitals cover them. Fails
    /// closed: unknown numbers, formula costs, and missing vitals data
    /// all evaluate false. No Lich required.
    SpellAffordable { number: u16 },
    /// The hand holds nothing (spell hand: nothing prepared).
    HandEmpty {
        #[serde(default)]
        hand: HandSlot,
    },
    /// The hand holds an item matching the given tests, ANDed when both
    /// set: `item_type` is a gameobj-data type tag ("weapon", "armor",
    /// "gem", ...) matched via GameObjData; `name` matches against the
    /// held item's display name (stock gameobj-data has no `shield` type
    /// — use type "armor" plus a name/noun match). Neither set = any item.
    HandHolds {
        #[serde(default)]
        hand: HandSlot,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default)]
        name_match: NameMatch,
    },
    /// A spell is prepared (spell hand). `name: None` = any spell.
    SpellPrepared {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default)]
        name_match: NameMatch,
    },
}

fn default_true() -> bool {
    true
}

/// Body-part ids the injury feed uses (same keys as the injury doll). The
/// canonical list the parser emits on a full clear; editors offer these in a
/// dropdown for `Condition::Injury.area`.
pub const INJURY_AREAS: &[&str] = &[
    "head", "neck", "chest", "abdomen", "back", "leftArm", "rightArm", "leftHand", "rightHand",
    "leftLeg", "rightLeg", "leftEye", "rightEye", "nsys",
];

/// Which hand a hand condition inspects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandSlot {
    #[default]
    Right,
    Left,
    /// Left or right (never the spell hand).
    Either,
    /// The prepared-spell hand.
    Spell,
}

impl HandSlot {
    pub const ALL: [HandSlot; 4] = [Self::Right, Self::Left, Self::Either, Self::Spell];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Right => "Right hand",
            Self::Left => "Left hand",
            Self::Either => "Either hand",
            Self::Spell => "Spell hand",
        }
    }
}

/// Effect dialog categories, matching the GameState.effects HashMap keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectCategory {
    Buffs,
    Debuffs,
    Cooldowns,
    ActiveSpells,
}

impl EffectCategory {
    /// Key into `GameState.effects`.
    pub fn state_key(&self) -> &'static str {
        match self {
            Self::Buffs => "Buffs",
            Self::Debuffs => "Debuffs",
            Self::Cooldowns => "Cooldowns",
            Self::ActiveSpells => "ActiveSpells",
        }
    }

    pub const ALL: [EffectCategory; 4] = [
        Self::Buffs,
        Self::Debuffs,
        Self::Cooldowns,
        Self::ActiveSpells,
    ];
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameMatch {
    #[default]
    Exact,
    Contains,
}

/// Numeric comparison, serialized as its operator symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cmp {
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
}

impl Cmp {
    pub fn eval(&self, lhs: i64, rhs: i64) -> bool {
        match self {
            Self::Lt => lhs < rhs,
            Self::Le => lhs <= rhs,
            Self::Gt => lhs > rhs,
            Self::Ge => lhs >= rhs,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VitalKind {
    Health,
    Mana,
    Stamina,
    Spirit,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VitalUnit {
    #[default]
    Percent,
    Absolute,
}
