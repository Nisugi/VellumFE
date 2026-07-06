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
    /// Load macros: profile macros.toml if present, else global, else empty.
    pub fn load(character: Option<&str>) -> Result<Self> {
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
