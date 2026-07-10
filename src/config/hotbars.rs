//! Hotbar (hotkey bar) definitions loaded from hotbars.toml.
//!
//! Bars are named collections of buttons; a `hotkeybar` widget in layout.toml
//! references a bar by name. Buttons send a game command on click or hotkey,
//! and can change appearance via ordered condition-driven states plus show a
//! countdown overlay (effect remaining time, roundtime, or casttime).
//!
//! Files: ~/.vellum-fe/global/hotbars.toml plus an optional per-character
//! ~/.vellum-fe/profiles/{character}/hotbars.toml. A character bar with the
//! same name replaces the global bar wholesale; character-only bars append.

use crate::config::Config;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

/// Root of hotbars.toml: a list of named bars.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HotbarsConfig {
    #[serde(default)]
    pub bars: Vec<HotbarDef>,
}

impl HotbarsConfig {
    pub fn find_bar(&self, name: &str) -> Option<&HotbarDef> {
        self.bars.iter().find(|b| b.name == name)
    }
}

/// One named bar of buttons.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HotbarDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub buttons: Vec<HotbarButton>,
}

/// A single button on a bar.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HotbarButton {
    /// Stable id, unique within the bar (editor bookkeeping / reordering).
    pub id: String,
    pub label: String,
    /// Game command sent on click or hotkey press.
    pub command: String,
    /// Optional hotkey in keybinds.toml key syntax (e.g. "alt+h", "f5").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    /// GUI hover text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    /// Organizational grouping used by the editors; no runtime effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Optional countdown overlay rendered on the button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub countdown: Option<HotbarCountdownSource>,
    /// Ordered states; the first whose condition matches wins.
    #[serde(default)]
    pub states: Vec<HotbarButtonState>,
    /// Appearance when no state matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_style: Option<HotbarStyle>,
}

/// Where a button's countdown overlay gets its remaining seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum HotbarCountdownSource {
    Effect {
        category: EffectCategory,
        name: String,
        #[serde(default)]
        name_match: NameMatch,
    },
    Roundtime,
    Casttime,
}

/// A condition → appearance rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotbarButtonState {
    pub when: HotbarCondition,
    #[serde(default)]
    pub style: HotbarStyle,
}

/// Appearance overrides; unset fields fall through to the button's
/// default_style, then to widget/theme defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HotbarStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default)]
    pub dim: bool,
}

/// Structured condition vocabulary. Editors build these from dropdowns and
/// limit group nesting to one level; the evaluator is recursive so deeper
/// hand-authored files still evaluate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HotbarCondition {
    All {
        conditions: Vec<HotbarCondition>,
    },
    Any {
        conditions: Vec<HotbarCondition>,
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
        cmp: HotbarCmp,
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
        cmp: HotbarCmp,
        value: u32,
        #[serde(default)]
        unit: VitalUnit,
    },
}

fn default_true() -> bool {
    true
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotbarCmp {
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
}

impl HotbarCmp {
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

impl Config {
    /// Load common (global) hotbars from ~/.vellum-fe/global/hotbars.toml.
    pub fn load_common_hotbars() -> Result<HotbarsConfig> {
        let path = Self::common_hotbars_path()?;
        if !path.exists() {
            return Ok(HotbarsConfig::default());
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read common hotbars: {:?}", path))?;
        toml::from_str(&contents).context("Failed to parse common hotbars.toml")
    }

    /// Load only character-specific hotbars (not merged with global).
    pub fn load_character_hotbars_only(character: Option<&str>) -> Result<HotbarsConfig> {
        let path = Self::hotbars_path(character)?;
        if !path.exists() {
            return Ok(HotbarsConfig::default());
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read character hotbars: {:?}", path))?;
        toml::from_str(&contents).context("Failed to parse character hotbars.toml")
    }

    /// Load hotbars for a character: global bars first, then character bars
    /// (same name replaces the global bar wholesale; new names append).
    /// Falls back to the embedded defaults when neither file has bars.
    pub fn load_hotbars(character: Option<&str>) -> Result<HotbarsConfig> {
        let mut config = Self::load_common_hotbars()?;
        let character_config = Self::load_character_hotbars_only(character)?;
        merge_hotbars(&mut config, character_config);

        if config.bars.is_empty() {
            config = toml::from_str(crate::config::DEFAULT_HOTBARS).unwrap_or_default();
        }
        Ok(config)
    }

    /// Save (insert or replace by name) a single bar in the chosen scope file.
    pub fn save_hotbar(bar: &HotbarDef, is_global: bool, character: Option<&str>) -> Result<()> {
        let path = if is_global {
            Self::common_hotbars_path()?
        } else {
            Self::hotbars_path(character)?
        };

        let mut config: HotbarsConfig = if path.exists() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read hotbars file: {:?}", path))?;
            toml::from_str(&contents).unwrap_or_default()
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {:?}", parent))?;
            }
            HotbarsConfig::default()
        };

        if let Some(existing) = config.bars.iter_mut().find(|b| b.name == bar.name) {
            *existing = bar.clone();
        } else {
            config.bars.push(bar.clone());
        }

        let contents = toml::to_string_pretty(&config).context("Failed to serialize hotbars")?;
        fs::write(&path, contents)
            .with_context(|| format!("Failed to write hotbars file: {:?}", path))?;

        tracing::info!(
            "Saved hotbar '{}' to {} hotbars file: {:?}",
            bar.name,
            if is_global { "global" } else { "character" },
            path
        );
        Ok(())
    }

    /// Delete a bar by name from the chosen scope file.
    pub fn delete_hotbar(name: &str, is_global: bool, character: Option<&str>) -> Result<()> {
        let path = if is_global {
            Self::common_hotbars_path()?
        } else {
            Self::hotbars_path(character)?
        };
        if !path.exists() {
            return Ok(());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read hotbars file: {:?}", path))?;
        let mut config: HotbarsConfig = toml::from_str(&contents).unwrap_or_default();
        let before = config.bars.len();
        config.bars.retain(|b| b.name != name);

        if config.bars.len() != before {
            let contents =
                toml::to_string_pretty(&config).context("Failed to serialize hotbars")?;
            fs::write(&path, contents)
                .with_context(|| format!("Failed to write hotbars file: {:?}", path))?;
            tracing::info!("Deleted hotbar '{}' from {:?}", name, path);
        }
        Ok(())
    }

    /// Report which scope files define a bar: (in_global, in_character).
    /// Used by editors to show [G]/[C] badges.
    pub fn hotbar_scope(name: &str, character: Option<&str>) -> (bool, bool) {
        let in_global = Self::load_common_hotbars()
            .map(|c| c.find_bar(name).is_some())
            .unwrap_or(false);
        let in_character = Self::load_character_hotbars_only(character)
            .map(|c| c.find_bar(name).is_some())
            .unwrap_or(false);
        (in_global, in_character)
    }
}

/// Character bars replace same-named global bars wholesale; new names append.
pub(crate) fn merge_hotbars(base: &mut HotbarsConfig, overrides: HotbarsConfig) {
    for bar in overrides.bars {
        if let Some(existing) = base.bars.iter_mut().find(|b| b.name == bar.name) {
            *existing = bar;
        } else {
            base.bars.push(bar);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_EXAMPLE: &str = r##"
[[bars]]
name = "combat"
title = "Combat"

[[bars.buttons]]
id = "hide"
label = "Hide"
command = "hide"
hotkey = "alt+h"
tooltip = "Attempt to hide"
category = "Stealth"

[bars.buttons.countdown]
source = "effect"
category = "Cooldowns"
name = "Shadow Mastery"
name_match = "contains"

[[bars.buttons.states]]
[bars.buttons.states.when]
type = "all"
conditions = [
  { type = "indicator", id = "hidden", active = true },
]
[bars.buttons.states.style]
label = "Hidden"
fg = "#80ff80"
bg = "#204020"
dim = false

[[bars.buttons.states]]
[bars.buttons.states.when]
type = "any"
conditions = [
  { type = "rt_active" },
  { type = "effect_active", category = "Cooldowns", name = "Shadow Mastery", name_match = "contains" },
  { type = "all", conditions = [
      { type = "vital", vital = "stamina", cmp = "<", value = 20, unit = "percent" },
  ] },
]
[bars.buttons.states.style]
dim = true

[bars.buttons.default_style]
fg = "#d0d0d0"

[[bars.buttons]]
id = "spell909"
label = "909"
command = "incant 909"

[bars.buttons.countdown]
source = "casttime"

[[bars.buttons.states]]
[bars.buttons.states.when]
type = "all"
conditions = [ { type = "vital", vital = "mana", cmp = "<", value = 9, unit = "absolute" } ]
[bars.buttons.states.style]
dim = true
"##;

    #[test]
    fn parse_full_example() {
        let config: HotbarsConfig = toml::from_str(FULL_EXAMPLE).expect("parse");
        assert_eq!(config.bars.len(), 1);
        let bar = &config.bars[0];
        assert_eq!(bar.name, "combat");
        assert_eq!(bar.buttons.len(), 2);

        let hide = &bar.buttons[0];
        assert_eq!(hide.hotkey.as_deref(), Some("alt+h"));
        assert!(matches!(
            hide.countdown,
            Some(HotbarCountdownSource::Effect {
                category: EffectCategory::Cooldowns,
                ref name,
                name_match: NameMatch::Contains,
            }) if name == "Shadow Mastery"
        ));
        assert_eq!(hide.states.len(), 2);
        assert!(matches!(
            hide.states[0].when,
            HotbarCondition::All { ref conditions } if conditions.len() == 1
        ));
        assert_eq!(hide.states[0].style.label.as_deref(), Some("Hidden"));
        // Nested group inside "any"
        if let HotbarCondition::Any { conditions } = &hide.states[1].when {
            assert_eq!(conditions.len(), 3);
            assert!(matches!(conditions[0], HotbarCondition::RtActive));
            assert!(matches!(conditions[2], HotbarCondition::All { .. }));
        } else {
            panic!("expected any group");
        }
        assert_eq!(hide.default_style.as_ref().unwrap().fg.as_deref(), Some("#d0d0d0"));

        let spell = &bar.buttons[1];
        assert!(matches!(spell.countdown, Some(HotbarCountdownSource::Roundtime) | Some(HotbarCountdownSource::Casttime)));
        if let HotbarCondition::All { conditions } = &spell.states[0].when {
            assert!(matches!(
                conditions[0],
                HotbarCondition::Vital {
                    vital: VitalKind::Mana,
                    cmp: HotbarCmp::Lt,
                    value: 9,
                    unit: VitalUnit::Absolute,
                }
            ));
        } else {
            panic!("expected all group");
        }
    }

    #[test]
    fn roundtrip_serialization() {
        let config: HotbarsConfig = toml::from_str(FULL_EXAMPLE).expect("parse");
        let serialized = toml::to_string_pretty(&config).expect("serialize");
        let reparsed: HotbarsConfig = toml::from_str(&serialized).expect("reparse");
        assert_eq!(config, reparsed);
    }

    #[test]
    fn merge_replaces_by_name_and_appends() {
        let mut base: HotbarsConfig = toml::from_str(
            r#"
            [[bars]]
            name = "combat"
            [[bars.buttons]]
            id = "a"
            label = "A"
            command = "attack"
            "#,
        )
        .unwrap();
        let overrides: HotbarsConfig = toml::from_str(
            r#"
            [[bars]]
            name = "combat"
            [[bars.buttons]]
            id = "b"
            label = "B"
            command = "backstab"
            [[bars.buttons]]
            id = "c"
            label = "C"
            command = "cast"

            [[bars]]
            name = "utility"
            "#,
        )
        .unwrap();

        merge_hotbars(&mut base, overrides);
        assert_eq!(base.bars.len(), 2);
        let combat = base.find_bar("combat").unwrap();
        // Replaced wholesale, not merged button-by-button
        assert_eq!(combat.buttons.len(), 2);
        assert_eq!(combat.buttons[0].id, "b");
        assert!(base.find_bar("utility").is_some());
    }

    #[test]
    fn unknown_condition_type_fails() {
        let result: std::result::Result<HotbarsConfig, _> = toml::from_str(
            r#"
            [[bars]]
            name = "bad"
            [[bars.buttons]]
            id = "x"
            label = "X"
            command = "x"
            [[bars.buttons.states]]
            [bars.buttons.states.when]
            type = "ruby_eval"
            code = "Effects::Buffs.active?"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn defaults_apply() {
        let config: HotbarsConfig = toml::from_str(
            r#"
            [[bars]]
            name = "minimal"
            [[bars.buttons]]
            id = "look"
            label = "Look"
            command = "look"
            [[bars.buttons.states]]
            [bars.buttons.states.when]
            type = "effect_active"
            category = "Buffs"
            name = "Song of Luck"
            "#,
        )
        .unwrap();
        let button = &config.bars[0].buttons[0];
        assert!(button.hotkey.is_none());
        assert!(button.countdown.is_none());
        assert!(button.default_style.is_none());
        // name_match defaults to Exact; style defaults empty/dim=false
        assert!(matches!(
            button.states[0].when,
            HotbarCondition::EffectActive { name_match: NameMatch::Exact, .. }
        ));
        assert_eq!(button.states[0].style, HotbarStyle::default());
    }

    #[test]
    fn cmp_eval() {
        assert!(HotbarCmp::Lt.eval(1, 2));
        assert!(!HotbarCmp::Lt.eval(2, 2));
        assert!(HotbarCmp::Le.eval(2, 2));
        assert!(HotbarCmp::Gt.eval(3, 2));
        assert!(!HotbarCmp::Gt.eval(2, 2));
        assert!(HotbarCmp::Ge.eval(2, 2));
    }
}
