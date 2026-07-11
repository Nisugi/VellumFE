//! Hand-curated edge overrides (go2 plan §2 tier 2): a shipped-plus-user
//! TOML table making specific edges walkable when their mapdb script is
//! beyond the transpiler — major gates, ferries, event travel.
//!
//! Loaded once per process: `defaults/globals/travel_overrides.toml`
//! (embedded, ships empty with a documented example) merged under
//! `~/.vellum-fe/travel_overrides.toml` (user entries win per edge).

use std::collections::HashMap;
use std::sync::LazyLock;

use super::edge::WalkAction;

const DEFAULT_OVERRIDES: &str = include_str!("../../../defaults/globals/travel_overrides.toml");

#[derive(Debug, Clone)]
pub struct TravelEdgeOverride {
    pub actions: Vec<WalkAction>,
    /// Pathfinding cost in seconds when the mapdb timeto can't resolve.
    pub cost: Option<f64>,
}

#[derive(serde::Deserialize)]
struct FileFormat {
    #[serde(default)]
    edge: Vec<EdgeEntry>,
}

#[derive(serde::Deserialize)]
struct EdgeEntry {
    from: u32,
    to: u32,
    actions: Vec<String>,
    #[serde(default)]
    cost: Option<f64>,
}

/// `"move:go gate"` → `WalkAction::Move("go gate")`, etc. `None` on
/// anything unrecognized — a typo must not half-apply an override.
fn parse_action(spec: &str) -> Option<WalkAction> {
    let spec = spec.trim();
    if spec == "noop" {
        return Some(WalkAction::Noop);
    }
    if spec == "waitrt" {
        return Some(WalkAction::WaitRt);
    }
    if let Some(cmd) = spec.strip_prefix("move:") {
        return Some(WalkAction::Move(cmd.trim().to_owned()));
    }
    if let Some(cmd) = spec.strip_prefix("put:") {
        return Some(WalkAction::Put(cmd.trim().to_owned()));
    }
    if let Some(seconds) = spec.strip_prefix("sleep:") {
        return Some(WalkAction::Sleep(seconds.trim().parse().ok()?));
    }
    None
}

fn parse_table(source: &str, origin: &str, table: &mut HashMap<(u32, u32), TravelEdgeOverride>) {
    let parsed: FileFormat = match toml::from_str(source) {
        Ok(parsed) => parsed,
        Err(e) => {
            tracing::warn!("{origin}: travel overrides ignored (parse error: {e})");
            return;
        }
    };
    for entry in parsed.edge {
        let actions: Option<Vec<WalkAction>> =
            entry.actions.iter().map(|s| parse_action(s)).collect();
        match actions {
            Some(actions) if !actions.is_empty() => {
                table.insert(
                    (entry.from, entry.to),
                    TravelEdgeOverride {
                        actions,
                        cost: entry.cost,
                    },
                );
            }
            _ => {
                tracing::warn!(
                    "{origin}: override {} → {} skipped (bad or empty actions)",
                    entry.from,
                    entry.to
                );
            }
        }
    }
}

static TABLE: LazyLock<HashMap<(u32, u32), TravelEdgeOverride>> = LazyLock::new(|| {
    let mut table = HashMap::new();
    parse_table(DEFAULT_OVERRIDES, "defaults", &mut table);
    // Unit tests use synthetic room ids that must not collide with a
    // developer's real user overrides.
    #[cfg(not(test))]
    if let Ok(base) = crate::config::Config::base_dir() {
        let user_path = base.join("travel_overrides.toml");
        if let Ok(source) = std::fs::read_to_string(&user_path) {
            parse_table(&source, "travel_overrides.toml", &mut table);
        }
    }
    table
});

/// The curated way across `from → to`, if one is shipped or user-defined.
pub fn edge_override(from: u32, to: u32) -> Option<&'static TravelEdgeOverride> {
    TABLE.get(&(from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_defaults_parse_clean_and_actions_round_trip() {
        // The embedded file must never fail to parse (it ships commented).
        let mut table = HashMap::new();
        parse_table(DEFAULT_OVERRIDES, "defaults", &mut table);
        assert!(table.is_empty(), "shipped file has no active entries");

        let mut table = HashMap::new();
        parse_table(
            r#"
            [[edge]]
            from = 1
            to = 2
            cost = 12.0
            actions = ["put:pull lever", "waitrt", "sleep:0.5", "move:go gate", "noop"]

            [[edge]]
            from = 3
            to = 4
            actions = ["frobnicate:zzz"]
            "#,
            "test",
            &mut table,
        );
        let ov = table.get(&(1, 2)).expect("valid entry loads");
        assert_eq!(ov.cost, Some(12.0));
        assert_eq!(
            ov.actions,
            vec![
                WalkAction::Put("pull lever".into()),
                WalkAction::WaitRt,
                WalkAction::Sleep(0.5),
                WalkAction::Move("go gate".into()),
                WalkAction::Noop,
            ]
        );
        assert!(
            !table.contains_key(&(3, 4)),
            "unrecognized action skips the whole entry"
        );
    }
}
