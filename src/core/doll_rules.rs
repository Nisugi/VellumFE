//! Doll condition rules for remote clients: which `[[injury_doll.variants]]`
//! set is active and which parts its `hidden_when` conditions suppress.
//!
//! The GUI evaluates these straight off its loaded skin every frame; the
//! phone client can't (and must not — one evaluator, in Rust) so the host
//! resolves the ACTIVE VARIANT NAME and HIDDEN PART LIST here and pushes
//! them on the state stream. The client just switches sets by name.
//!
//! Rules come from the active skin's manifest. Loading a manifest per
//! message batch would hammer the filesystem, so the cache re-stats
//! skin.toml at most every couple of seconds and reloads only when the
//! skin selection or the file's mtime changed — the same freshness the
//! GUI's hot-reload poll gives.

use std::time::{Duration, Instant, SystemTime};

use crate::config::skins::{self, DOLL_PARTS};
use crate::config::Condition;
use crate::core::gameobj_data::GameObjData;
use crate::core::state::GameState;

/// Extracted condition rules for one skin's doll section.
#[derive(Debug, Clone, Default)]
pub struct DollRules {
    /// (name, activation condition) in declaration order.
    pub variants: Vec<(String, Condition)>,
    /// Default set: (canonical part key, suppression condition).
    pub default_hidden: Vec<(String, Condition)>,
    /// Per-variant suppression rules, aligned with `variants`.
    pub variant_hidden: Vec<Vec<(String, Condition)>>,
}

impl DollRules {
    fn is_empty(&self) -> bool {
        self.variants.is_empty() && self.default_hidden.is_empty()
    }

    fn from_manifest(doll: &skins::InjuryDollSkin) -> Self {
        // Manifest part keys canonicalize onto the protocol names so the
        // pushed hidden list matches the client's injuries-map keys;
        // unknown parts are dropped, same as everywhere else.
        let canonical = |part: &str| -> Option<String> {
            DOLL_PARTS
                .iter()
                .find(|(key, _, _)| key.eq_ignore_ascii_case(part))
                .map(|(key, _, _)| (*key).to_string())
        };
        let hidden_of = |parts: &std::collections::HashMap<String, skins::DollPartSpec>| {
            let mut hidden: Vec<(String, Condition)> = parts
                .iter()
                .filter_map(|(part, spec)| Some((canonical(part)?, spec.hidden_when.clone()?)))
                .collect();
            hidden.sort_by(|a, b| a.0.cmp(&b.0));
            hidden
        };
        Self {
            variants: doll
                .variants
                .iter()
                .map(|variant| (variant.name.clone(), variant.when.clone()))
                .collect(),
            default_hidden: hidden_of(&doll.parts),
            variant_hidden: doll
                .variants
                .iter()
                .map(|variant| hidden_of(&variant.skin.parts))
                .collect(),
        }
    }
}

/// Throttled loader: rules for the currently configured skin, refreshed
/// when the selection or skin.toml changes (checked at most every 2s).
#[derive(Debug, Default)]
pub struct DollRulesCache {
    cached: Option<Cached>,
}

#[derive(Debug)]
struct Cached {
    active_skin: Option<String>,
    doll_image: Option<String>,
    mtime: Option<SystemTime>,
    checked: Instant,
    rules: DollRules,
}

const RECHECK: Duration = Duration::from_secs(2);

impl DollRulesCache {
    /// The active variant name plus the active set's suppressed parts,
    /// for the character's own doll. Empty/None whenever no skin doll is
    /// configured, a pool doll override is active (they carry no
    /// variants), or nothing matches.
    pub fn resolve(
        &mut self,
        active_skin: Option<&str>,
        doll_image: Option<&str>,
        gs: &GameState,
        now_server: i64,
        gameobj: Option<&GameObjData>,
    ) -> (Option<String>, Vec<String>) {
        let rules = self.rules(active_skin, doll_image);
        if rules.is_empty() {
            return (None, Vec::new());
        }
        let active = rules.variants.iter().position(|(_, when)| {
            crate::core::conditions::eval_condition(when, gs, now_server, gameobj)
        });
        let hidden_rules = match active {
            Some(index) => &rules.variant_hidden[index],
            None => &rules.default_hidden,
        };
        let hidden: Vec<String> = hidden_rules
            .iter()
            .filter(|(_, condition)| {
                crate::core::conditions::eval_condition(condition, gs, now_server, gameobj)
            })
            .map(|(part, _)| part.clone())
            .collect();
        (active.map(|index| rules.variants[index].0.clone()), hidden)
    }

    fn rules(&mut self, active_skin: Option<&str>, doll_image: Option<&str>) -> &DollRules {
        let fresh_enough = self.cached.as_ref().is_some_and(|cached| {
            cached.active_skin.as_deref() == active_skin
                && cached.doll_image.as_deref() == doll_image
                && cached.checked.elapsed() < RECHECK
        });
        if !fresh_enough {
            let mtime = active_skin
                .filter(|_| doll_image.is_none())
                .and_then(|name| {
                    let path = crate::config::Config::skins_dir()
                        .ok()?
                        .join(name)
                        .join("skin.toml");
                    std::fs::metadata(path).ok()?.modified().ok()
                });
            let selection_unchanged = self.cached.as_ref().is_some_and(|cached| {
                cached.active_skin.as_deref() == active_skin
                    && cached.doll_image.as_deref() == doll_image
                    && cached.mtime == mtime
            });
            if selection_unchanged {
                // Same file, same mtime: just refresh the throttle clock.
                if let Some(cached) = self.cached.as_mut() {
                    cached.checked = Instant::now();
                }
            } else {
                // A pool doll override replaces the skin doll wholesale and
                // carries no variants or hidden_when — empty rules.
                let rules = match (active_skin, doll_image) {
                    (Some(name), None) => skins::load_manifest(name)
                        .map(|(manifest, _)| DollRules::from_manifest(&manifest.injury_doll))
                        .unwrap_or_default(),
                    _ => DollRules::default(),
                };
                self.cached = Some(Cached {
                    active_skin: active_skin.map(str::to_owned),
                    doll_image: doll_image.map(str::to_owned),
                    mtime,
                    checked: Instant::now(),
                    rules,
                });
            }
        }
        &self.cached.as_ref().expect("cache populated above").rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules_from(toml_src: &str) -> DollRules {
        let manifest: skins::SkinManifest = toml::from_str(toml_src).unwrap();
        DollRules::from_manifest(&manifest.injury_doll)
    }

    #[test]
    fn rules_extract_variants_and_hidden_with_canonical_keys() {
        let rules = rules_from(
            r#"
            [injury_doll]
            base = "doll/standing.png"

            [injury_doll.lefthand]
            hidden_when = { type = "injury", area = "leftArm", cmp = ">=", level = 3 }
            healthy = "doll/hand_ok.png"

            [[injury_doll.variants]]
            name = "downed"
            when = { type = "indicator", id = "prone", active = true }
            [injury_doll.variants.skin]
            base = "doll/downed.png"
            "#,
        );
        assert_eq!(rules.variants.len(), 1);
        assert_eq!(rules.variants[0].0, "downed");
        // Lowercase manifest key canonicalized onto the protocol name.
        assert_eq!(rules.default_hidden.len(), 1);
        assert_eq!(rules.default_hidden[0].0, "leftHand");
        // The variant declares no hidden_when of its own.
        assert_eq!(rules.variant_hidden.len(), 1);
        assert!(rules.variant_hidden[0].is_empty());
    }

    #[test]
    fn resolve_reports_variant_name_and_active_set_hidden() {
        let rules = rules_from(
            r#"
            [injury_doll]
            base = "doll/standing.png"

            [injury_doll.leftHand]
            hidden_when = { type = "injury", area = "leftArm", cmp = ">=", level = 3 }
            healthy = "doll/hand_ok.png"

            [[injury_doll.variants]]
            name = "downed"
            [injury_doll.variants.when]
            type = "all"
            conditions = [
              { type = "injury", area = "leftLeg",  cmp = ">=", level = 3 },
              { type = "injury", area = "rightLeg", cmp = ">=", level = 3 },
            ]
            [injury_doll.variants.skin]
            base = "doll/downed.png"
            "#,
        );
        let mut cache = DollRulesCache::default();
        cache.cached = Some(Cached {
            active_skin: Some("test".into()),
            doll_image: None,
            mtime: None,
            checked: Instant::now(),
            rules,
        });

        let mut gs = GameState::new();
        // Healthy: default set, nothing hidden.
        let (variant, hidden) = cache.resolve(Some("test"), None, &gs, 0, None);
        assert_eq!(variant, None);
        assert!(hidden.is_empty());

        // Severed arm: default set's hand suppression fires.
        gs.injuries.insert("leftArm".to_string(), 3);
        let (variant, hidden) = cache.resolve(Some("test"), None, &gs, 0, None);
        assert_eq!(variant, None);
        assert_eq!(hidden, vec!["leftHand"]);

        // Both legs severed: the downed variant activates, and ITS (empty)
        // hidden rules apply — full replace extends to suppression too.
        gs.injuries.insert("leftLeg".to_string(), 3);
        gs.injuries.insert("rightLeg".to_string(), 3);
        let (variant, hidden) = cache.resolve(Some("test"), None, &gs, 0, None);
        assert_eq!(variant.as_deref(), Some("downed"));
        assert!(hidden.is_empty());
    }

    #[test]
    fn pool_doll_override_yields_empty_rules() {
        let mut cache = DollRulesCache::default();
        let gs = GameState::new();
        let (variant, hidden) = cache.resolve(Some("test"), Some("dolls/gnome.png"), &gs, 0, None);
        assert_eq!(variant, None);
        assert!(hidden.is_empty());
    }
}
