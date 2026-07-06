//! Macro button definitions for the web (phone) frontend.
//!
//! Loaded from `macros.toml` — profile file wins over the global file
//! wholesale (no merging; a profile that wants tweaks copies the file).
//! Definitions live in core config so remote clients only ever reference
//! buttons by id; the server resolves ids back to commands
//! (docs/mobile-web-frontend-plan.md, Phase 5).
//!
//! Two button shapes:
//! - action button: `command` fires immediately on tap
//! - menu button: `option` entries open a bottom-sheet picker
//!
//! Buttons live either in switchable `[[group]]`s (rendered as a rail) or
//! in `[[floating]]` (overlay buttons, always visible; the client owns
//! their on-screen positions).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

/// One tappable choice inside a menu button's bottom sheet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MacroOption {
    pub label: String,
    pub command: String,
    /// Ask before sending (two-button confirm sheet on the client).
    #[serde(default)]
    pub confirm: bool,
}

/// A macro button: either an immediate action (`command`) or a menu
/// (`option` list). If both are present the options win.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MacroButton {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Hex color for the button face (e.g. "#d9b44f").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default, rename = "option", skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<MacroOption>,
    /// Default position for floating buttons, as fractions of the text
    /// pane (0.0-1.0). The client persists user-dragged positions locally
    /// per device; this is only the starting point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    /// True when this button came from macros-local.toml (phone-created)
    /// and may be edited/deleted remotely. Set during merge, never stored.
    #[serde(skip)]
    pub editable: bool,
}

/// A named, switchable set of rail buttons.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MacroGroup {
    pub name: String,
    #[serde(default, rename = "button")]
    pub buttons: Vec<MacroButton>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MacrosConfig {
    #[serde(default, rename = "group")]
    pub groups: Vec<MacroGroup>,
    #[serde(default, rename = "floating")]
    pub floating: Vec<MacroButton>,
}

impl MacrosConfig {
    /// Load the merged macro set: the hand-written base file plus the
    /// phone-edited local overlay (see [`MacrosConfig::merge`]).
    pub fn load(character: Option<&str>) -> Result<Self> {
        let base = Self::load_base(character)?;
        let local = Self::load_local(character).unwrap_or_default();
        Ok(Self::merge(base, local))
    }

    /// Load only the hand-written base file: profile macros.toml if
    /// present, else global, else empty. Never written by the app.
    pub fn load_base(character: Option<&str>) -> Result<Self> {
        let profile_path = super::Config::profile_dir(character)?.join("macros.toml");
        let global_path = super::Config::global_dir()?.join("macros.toml");
        let path = if profile_path.exists() {
            profile_path
        } else if global_path.exists() {
            global_path
        } else {
            return Ok(Self::default());
        };
        let text = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("Failed to parse {}", path.display()))
    }

    /// Load the phone-edited overlay (profile macros-local.toml).
    /// Kept separate from the base file so remote edits never rewrite the
    /// user's hand-authored macros.toml (which would lose its comments).
    pub fn load_local(character: Option<&str>) -> Result<Self> {
        let path = super::Config::profile_dir(character)?.join("macros-local.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("Failed to parse {}", path.display()))
    }

    /// Persist this config as the phone-edited overlay.
    pub fn save_local(&self, character: Option<&str>) -> Result<()> {
        let dir = super::Config::profile_dir(character)?;
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create {}", dir.display()))?;
        let path = dir.join("macros-local.toml");
        let text = toml::to_string_pretty(self).context("Failed to serialize macros-local")?;
        fs::write(&path, text).with_context(|| format!("Failed to write {}", path.display()))
    }

    /// Merge the local overlay onto the base: same-named groups gain the
    /// local buttons (appended), unknown local groups are added, floating
    /// buttons append. Everything from `local` is marked editable.
    pub fn merge(base: Self, mut local: Self) -> Self {
        let mut merged = base;
        for group in &mut local.groups {
            for button in &mut group.buttons {
                button.editable = true;
            }
        }
        for button in &mut local.floating {
            button.editable = true;
        }
        for local_group in local.groups {
            match merged
                .groups
                .iter_mut()
                .find(|g| g.name == local_group.name)
            {
                Some(existing) => existing.buttons.extend(local_group.buttons),
                None => merged.groups.push(local_group),
            }
        }
        merged.floating.extend(local.floating);
        merged
    }

    /// Insert or replace a button in this (local-overlay) config.
    /// `group`: Some(name) targets a rail group (created if missing),
    /// None targets the floating set. `original` identifies an existing
    /// local button being edited: (group name or None-for-floating, label).
    pub fn upsert_button(
        &mut self,
        group: Option<&str>,
        button: MacroButton,
        original: Option<(Option<&str>, &str)>,
    ) {
        if let Some(original) = original {
            self.delete_button(original.0, original.1);
        }
        match group {
            Some(name) => {
                match self.groups.iter_mut().find(|g| g.name == name) {
                    Some(existing) => existing.buttons.push(button),
                    None => self.groups.push(MacroGroup {
                        name: name.to_string(),
                        buttons: vec![button],
                    }),
                }
            }
            None => self.floating.push(button),
        }
    }

    /// Remove a button by (group-or-floating, label). Returns true if
    /// something was removed. Empty groups are dropped.
    pub fn delete_button(&mut self, group: Option<&str>, label: &str) -> bool {
        let removed = match group {
            Some(name) => match self.groups.iter_mut().find(|g| g.name == name) {
                Some(existing) => {
                    let before = existing.buttons.len();
                    existing.buttons.retain(|b| b.label != label);
                    existing.buttons.len() != before
                }
                None => false,
            },
            None => {
                let before = self.floating.len();
                self.floating.retain(|b| b.label != label);
                self.floating.len() != before
            }
        };
        self.groups.retain(|g| !g.buttons.is_empty());
        removed
    }

    /// Look up the command behind a client-supplied macro id and whether it
    /// is confirm-gated. Ids are index paths into this config snapshot:
    /// `g:<group>:b:<button>` (+ `:o:<option>`) or `f:<index>` (+ option).
    /// Returns None for stale/malformed ids (e.g. a client that missed a
    /// `.reloadmacros`).
    pub fn resolve(&self, id: &str) -> Option<&str> {
        let parts: Vec<&str> = id.split(':').collect();
        let (button, rest): (&MacroButton, &[&str]) = match parts.as_slice() {
            ["g", group, "b", button, rest @ ..] => {
                let group: usize = group.parse().ok()?;
                let button: usize = button.parse().ok()?;
                (self.groups.get(group)?.buttons.get(button)?, rest)
            }
            ["f", index, rest @ ..] => {
                let index: usize = index.parse().ok()?;
                (self.floating.get(index)?, rest)
            }
            _ => return None,
        };
        match rest {
            [] => button.command.as_deref(),
            ["o", option] => {
                let option: usize = option.parse().ok()?;
                Some(button.options.get(option)?.command.as_str())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MacrosConfig {
        toml::from_str(
            r##"
            [[group]]
            name = "Town"

            [[group.button]]
            label = "Look"
            command = "look"

            [[group.button]]
            label = "Travel"
            color = "#d9b44f"

            [[group.button.option]]
            label = "Bank"
            command = ";go2 bank"

            [[group.button.option]]
            label = "Gate"
            command = ";go2 gate"
            confirm = true

            [[floating]]
            label = "Atk"
            command = ";bigshot"
            x = 0.85
            y = 0.6
            "##,
        )
        .expect("sample macros parse")
    }

    #[test]
    fn parses_groups_options_and_floating() {
        let macros = sample();
        assert_eq!(macros.groups.len(), 1);
        assert_eq!(macros.groups[0].name, "Town");
        assert_eq!(macros.groups[0].buttons.len(), 2);
        assert_eq!(macros.groups[0].buttons[1].options.len(), 2);
        assert!(macros.groups[0].buttons[1].options[1].confirm);
        assert_eq!(macros.floating.len(), 1);
        assert_eq!(macros.floating[0].x, Some(0.85));
    }

    #[test]
    fn merge_appends_local_and_marks_editable() {
        let base = sample();
        let local: MacrosConfig = toml::from_str(
            r#"
            [[group]]
            name = "Town"
            [[group.button]]
            label = "Sell gems"
            command = ";sellgems"

            [[group]]
            name = "Couch"
            [[group.button]]
            label = "Nap"
            command = "sleep"

            [[floating]]
            label = "Heal"
            command = ";heal"
            "#,
        )
        .unwrap();
        let merged = MacrosConfig::merge(base, local);
        // Town gains the local button after the base ones.
        let town = &merged.groups[0];
        assert_eq!(town.buttons.len(), 3);
        assert!(!town.buttons[0].editable, "base buttons stay read-only");
        assert!(town.buttons[2].editable);
        assert_eq!(town.buttons[2].label, "Sell gems");
        // New local group appended after base groups.
        assert_eq!(merged.groups[1].name, "Couch");
        assert!(merged.groups[1].buttons[0].editable);
        // Floating appends and is editable.
        assert_eq!(merged.floating.len(), 2);
        assert!(merged.floating[1].editable);
    }

    #[test]
    fn upsert_and_delete_buttons() {
        let mut local = MacrosConfig::default();
        let button = |label: &str, command: &str| MacroButton {
            label: label.to_string(),
            command: Some(command.to_string()),
            ..Default::default()
        };

        // Create into a new group, then floating.
        local.upsert_button(Some("Couch"), button("Nap", "sleep"), None);
        local.upsert_button(None, button("Heal", ";heal"), None);
        assert_eq!(local.groups[0].buttons[0].label, "Nap");
        assert_eq!(local.floating[0].label, "Heal");

        // Edit: rename + move group in one upsert.
        local.upsert_button(
            Some("Town"),
            button("Long nap", "sleep"),
            Some((Some("Couch"), "Nap")),
        );
        assert!(local.groups.iter().all(|g| g.name != "Couch"), "empty group dropped");
        assert_eq!(local.groups[0].name, "Town");
        assert_eq!(local.groups[0].buttons[0].label, "Long nap");

        // Delete.
        assert!(local.delete_button(Some("Town"), "Long nap"));
        assert!(!local.delete_button(Some("Town"), "Long nap"));
        assert!(local.delete_button(None, "Heal"));
        assert!(local.groups.is_empty());
        assert!(local.floating.is_empty());
    }

    #[test]
    fn resolve_by_index_path() {
        let macros = sample();
        assert_eq!(macros.resolve("g:0:b:0"), Some("look"));
        assert_eq!(macros.resolve("g:0:b:1:o:1"), Some(";go2 gate"));
        assert_eq!(macros.resolve("f:0"), Some(";bigshot"));
        // Menu button without option index has no direct command.
        assert_eq!(macros.resolve("g:0:b:1"), None);
        // Stale/malformed ids resolve to nothing.
        assert_eq!(macros.resolve("g:9:b:0"), None);
        assert_eq!(macros.resolve("bogus"), None);
        assert_eq!(macros.resolve("g:0:b:0:o:5"), None);
    }
}
