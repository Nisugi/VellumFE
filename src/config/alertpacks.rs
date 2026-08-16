//! Alert packs: shareable rule files with a content-hash trust gate.
//!
//! A pack is a self-contained `.toml` in `~/.vellum-fe/global/alertpacks/`
//! using the same rule schema as `highlights.toml`. Packs are separate files
//! rather than a section of the user's own highlights because a pack is a
//! DISTRIBUTION unit: sharing one must never ship personal settings with it.
//!
//! # The trust boundary
//!
//! Packs reuse the highlight schema, which means they inherit powers that can
//! misrepresent what the game said: `replace` rewrites text in place and
//! `redirect` reroutes whole streams. Someone else's content holding those
//! powers is a meaningfully different proposition from your own rules.
//!
//! So sensitive rules are withheld until the user approves that pack's exact
//! contents, identified by hash:
//!
//! - Alert, sound, and color rules load immediately. They cannot lie about
//!   game output, and gating them would make packs useless on arrival and
//!   train users to click approve reflexively.
//! - `replace` / `redirect` rules stay inert until approved.
//! - Approval records the file hash. Any edit — a jinx update, a hand-copied
//!   change — produces a new hash and re-arms the gate automatically. The
//!   mechanism is content-based, not install-channel-based, so there is no
//!   trusted path to smuggle a change through.
//! - The user's own `highlights.toml` is never gated. They wrote it; the gate
//!   is about authorship, not about the capability itself.

use super::*;
use std::collections::{HashMap, HashSet};

/// Where a pack's rules apply. All four selectors are optional and combine as
/// OR: a room in scope by ANY of them arms the pack. A scope with nothing set
/// means "everywhere", which is what ambiance packs want.
///
/// This is correctness before performance. A Reim encounter pack matching its
/// patterns while you stand in a Wehnimer's bank produces false positives, and
/// false positives are what make people switch an alert system off.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AlertPackScope {
    /// mapdb `location` strings, e.g. "the Settlement of Reim". The unit pack
    /// authors actually think in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub area: Vec<String>,
    /// Game-sent `realm` codes from `<roommeta>`. Works even where the mapdb
    /// has no entry, because it comes straight off the wire.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zone: Vec<u32>,
    /// mapdb curated room tags ("bank", "furrier", ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Explicit room uids, for surgical cases the other three can't express.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rooms: Vec<i64>,
}

impl AlertPackScope {
    /// A scope with no selectors states no restriction, so the pack is always
    /// armed. Ambiance packs rely on this being the default.
    pub fn is_unscoped(&self) -> bool {
        self.area.is_empty()
            && self.zone.is_empty()
            && self.tags.is_empty()
            && self.rooms.is_empty()
    }

    /// Does this scope admit the described room?
    ///
    /// Every input is optional because the client is regularly somewhere the
    /// mapdb doesn't know. Selectors whose data is missing simply don't match,
    /// rather than failing open — an unscoped pack is the way to say
    /// "everywhere", so a scoped pack matching everywhere when its data is
    /// absent would make scoping meaningless exactly where it's needed.
    pub fn admits(
        &self,
        location: Option<&str>,
        realm: Option<u32>,
        tags: &[String],
        uid: Option<i64>,
    ) -> bool {
        if self.is_unscoped() {
            return true;
        }
        if let Some(location) = location {
            if self
                .area
                .iter()
                .any(|a| a.eq_ignore_ascii_case(location))
            {
                return true;
            }
        }
        if let Some(realm) = realm {
            if self.zone.contains(&realm) {
                return true;
            }
        }
        if !self.tags.is_empty()
            && tags
                .iter()
                .any(|t| self.tags.iter().any(|want| want.eq_ignore_ascii_case(t)))
        {
            return true;
        }
        if let Some(uid) = uid {
            if self.rooms.contains(&uid) {
                return true;
            }
        }
        false
    }
}

/// Where the player currently is, as far as pack scoping is concerned.
///
/// A plain snapshot rather than a live borrow: it is cheap to build, trivial
/// to compare (which is how re-arming is gated to actual scope changes), and
/// keeps `config` free of any dependency on the map service.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoomScope {
    /// mapdb location of the current room, when it has one.
    pub location: Option<String>,
    /// Game-sent realm code from `<roommeta>`.
    pub realm: Option<u32>,
    /// mapdb tags of the current room.
    pub tags: Vec<String>,
    /// Current room uid.
    pub uid: Option<i64>,
}

/// The reserved rule name a pack uses to declare its scope. Packs are plain
/// rule maps, so the scope rides in a table that is not itself a rule.
pub const SCOPE_KEY: &str = "__scope__";

/// One installed pack: its rules plus the identity used to gate them.
#[derive(Debug, Clone)]
pub struct AlertPack {
    /// File stem, e.g. `reim-encounters`. The pack's identity for enabling.
    pub name: String,
    /// SHA-1 of the file's bytes. Changes whenever the content changes.
    pub hash: String,
    /// Every rule in the pack, keyed by rule name.
    pub rules: HashMap<String, HighlightPattern>,
    /// Where these rules apply. Unscoped packs are always armed.
    pub scope: AlertPackScope,
}

impl AlertPack {
    /// Rules that can misrepresent game output, as `(rule name, what it does)`
    /// pairs for the approval digest. Sorted so the digest is stable across
    /// runs — a digest that reshuffles itself is one users stop reading.
    pub fn sensitive_rules(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for (name, rule) in &self.rules {
            if let Some(replacement) = rule.replace.as_deref() {
                out.push((
                    name.clone(),
                    format!(
                        "rewrites matching text as {:?} (pattern: {:?})",
                        replacement, rule.pattern
                    ),
                ));
            }
            if let Some(target) = rule.redirect_to.as_deref() {
                out.push((
                    name.clone(),
                    format!(
                        "reroutes matching lines to the {:?} window (pattern: {:?})",
                        target, rule.pattern
                    ),
                ));
            }
        }
        out.sort();
        out
    }

    /// Does this pack contain anything requiring approval? A pack of pure
    /// alerts and colors never prompts.
    pub fn needs_approval(&self) -> bool {
        !self.sensitive_rules().is_empty()
    }

    /// The pack's rules as they should actually run, given whether the user
    /// has approved this exact content.
    ///
    /// Unapproved packs keep their alert/sound/color behavior; only the
    /// sensitive powers are stripped. Stripping rather than dropping the rule
    /// matters: a rule that both colors text and rewrites it still colors.
    pub fn effective_rules(&self, approved: bool) -> HashMap<String, HighlightPattern> {
        if approved {
            return self.rules.clone();
        }
        self.rules
            .iter()
            .map(|(name, rule)| {
                let mut rule = rule.clone();
                rule.replace = None;
                rule.redirect_to = None;
                (name.clone(), rule)
            })
            .collect()
    }
}

/// Which packs are enabled, and which exact contents the user approved.
///
/// Persisted next to the config rather than inside any pack — see the module
/// docs for why an approval must never travel with the thing it approves.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlertPackApprovals {
    /// Pack names the user turned on. Absent = off, so a freshly installed
    /// pack does nothing until the user opts in.
    #[serde(default)]
    pub enabled: HashSet<String>,
    /// Approved content hash per pack name. A pack whose current hash differs
    /// from this needs re-review.
    #[serde(default)]
    pub approved_hashes: HashMap<String, String>,
}

impl AlertPackApprovals {
    pub fn is_enabled(&self, name: &str) -> bool {
        self.enabled.contains(name)
    }

    /// Has the user approved this pack's CURRENT contents? Compares hashes,
    /// so an edited or updated pack reports false until re-approved.
    pub fn is_approved(&self, name: &str, hash: &str) -> bool {
        self.approved_hashes.get(name).is_some_and(|h| h == hash)
    }

    /// Record approval of one exact content hash.
    pub fn approve(&mut self, name: &str, hash: &str) {
        self.approved_hashes
            .insert(name.to_string(), hash.to_string());
    }

    /// Withdraw approval, re-arming the gate for this pack.
    pub fn revoke(&mut self, name: &str) {
        self.approved_hashes.remove(name);
    }

    pub fn set_enabled(&mut self, name: &str, on: bool) {
        if on {
            self.enabled.insert(name.to_string());
        } else {
            self.enabled.remove(name);
        }
    }
}

/// SHA-1 of a pack file's raw bytes. Hashing BYTES, not the parsed rules,
/// is deliberate: it means nothing in the file can change without the gate
/// noticing, including additions this version of the parser ignores.
pub fn pack_hash(bytes: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

impl Config {
    /// Load every installed pack. A malformed pack is skipped with a warning
    /// rather than failing the whole load — one bad file from a shared repo
    /// must not take the client's highlights down with it.
    pub fn load_alert_packs() -> Vec<AlertPack> {
        let Ok(dir) = Self::alertpacks_dir() else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(&dir) else {
            // Directory absent simply means no packs installed.
            return Vec::new();
        };

        let mut packs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(err) => {
                    tracing::warn!("alert pack {name}: unreadable ({err})");
                    continue;
                }
            };
            let text = match std::str::from_utf8(&bytes) {
                Ok(text) => text,
                Err(err) => {
                    tracing::warn!("alert pack {name}: not valid UTF-8 ({err})");
                    continue;
                }
            };
            // The scope table shares the file with the rules, so pull it out
            // as a raw value first — it is not a HighlightPattern and would
            // fail to deserialize as one.
            let mut raw: toml::Table = match toml::from_str(text) {
                Ok(raw) => raw,
                Err(err) => {
                    tracing::warn!("alert pack {name}: parse failed ({err})");
                    continue;
                }
            };
            let scope = match raw.remove(SCOPE_KEY) {
                None => AlertPackScope::default(),
                Some(value) => match value.try_into::<AlertPackScope>() {
                    Ok(scope) => scope,
                    Err(err) => {
                        // A malformed scope must not silently become
                        // "everywhere" — that is the permissive direction.
                        tracing::warn!(
                            "alert pack {name}: bad {SCOPE_KEY} ({err}); pack skipped"
                        );
                        continue;
                    }
                },
            };
            let rules: HashMap<String, HighlightPattern> =
                match toml::Value::Table(raw).try_into() {
                    Ok(rules) => rules,
                    Err(err) => {
                        tracing::warn!("alert pack {name}: parse failed ({err})");
                        continue;
                    }
                };
            packs.push(AlertPack {
                name: name.to_string(),
                hash: pack_hash(&bytes),
                rules,
                scope,
            });
        }
        packs.sort_by(|a, b| a.name.cmp(&b.name));
        packs
    }

    /// Load the local enable/approval record.
    pub fn load_alertpack_approvals() -> AlertPackApprovals {
        let Ok(path) = Self::alertpack_approvals_path() else {
            return AlertPackApprovals::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return AlertPackApprovals::default();
        };
        toml::from_str(&text).unwrap_or_else(|err| {
            tracing::warn!("alertpack approvals: parse failed ({err}); starting empty");
            AlertPackApprovals::default()
        })
    }

    /// Persist the enable/approval record.
    pub fn save_alertpack_approvals(approvals: &AlertPackApprovals) -> Result<()> {
        let path = Self::alertpack_approvals_path()?;
        let text = toml::to_string_pretty(approvals)
            .context("Failed to serialize alert pack approvals")?;
        write_atomic(&path, text).context("Failed to write alertpack-approvals.toml")?;
        Ok(())
    }

    /// Merge enabled packs into an existing rule set.
    ///
    /// Rule names are namespaced as `pack:<pack>/<rule>` so two packs (or a
    /// pack and the user's own rules) can use the same rule name without one
    /// silently clobbering the other — the exact collision a shared-content
    /// ecosystem produces constantly.
    ///
    /// The user's own rules are never overwritten by a pack, because the pack
    /// keys cannot collide with unprefixed personal ones.
    pub fn merge_alert_packs(
        highlights: &mut HashMap<String, HighlightPattern>,
        packs: &[AlertPack],
        approvals: &AlertPackApprovals,
        room: &RoomScope,
    ) {
        for pack in packs {
            if !approvals.is_enabled(&pack.name) {
                continue;
            }
            // Out-of-area packs are dropped from the rule set entirely rather
            // than flagged, because the matcher has no per-pack partitioning:
            // membership of this Vec IS the arming mechanism.
            if !pack.scope.admits(
                room.location.as_deref(),
                room.realm,
                &room.tags,
                room.uid,
            ) {
                continue;
            }
            let approved = approvals.is_approved(&pack.name, &pack.hash);
            for (rule_name, rule) in pack.effective_rules(approved) {
                highlights.insert(format!("pack:{}/{}", pack.name, rule_name), rule);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str) -> HighlightPattern {
        HighlightPattern {
            pattern: pattern.to_string(),
            fg: None,
            bg: None,
            bold: false,
            color_entire_line: false,
            fast_parse: false,
            case_insensitive: false,
            sound: None,
            sound_volume: None,
            rumble: None,
            category: None,
            squelch: false,
            silent_prompt: false,
            redirect_to: None,
            redirect_mode: RedirectMode::default(),
            replace: None,
            stream: None,
            window: None,
            set_status: None,
            status_duration: None,
            clear_status: None,
            alert: None,
            compiled_regex: None,
        }
    }

    fn pack_with(rules: Vec<(&str, HighlightPattern)>) -> AlertPack {
        AlertPack {
            name: "testpack".to_string(),
            hash: "abc123".to_string(),
            rules: rules
                .into_iter()
                .map(|(n, r)| (n.to_string(), r))
                .collect(),
            scope: AlertPackScope::default(),
        }
    }

    #[test]
    fn hash_tracks_content_exactly() {
        assert_eq!(pack_hash(b"same"), pack_hash(b"same"));
        assert_ne!(pack_hash(b"one"), pack_hash(b"two"));
        // A single byte's difference must re-arm the gate.
        assert_ne!(pack_hash(b"pattern = 'a'"), pack_hash(b"pattern = 'b'"));
    }

    #[test]
    fn a_pack_of_alerts_and_colors_needs_no_approval() {
        let mut colored = rule("goblin");
        colored.fg = Some("#ff0000".to_string());
        let mut alerting = rule("stunned");
        alerting.alert = Some(AlertSpec {
            banner: Some("STUNNED".to_string()),
            ..Default::default()
        });
        let pack = pack_with(vec![("color", colored), ("alert", alerting)]);

        assert!(!pack.needs_approval(), "these powers cannot misrepresent output");
        assert!(pack.sensitive_rules().is_empty());
    }

    #[test]
    fn replace_and_redirect_are_both_flagged() {
        let mut rewriter = rule("kobold");
        rewriter.replace = Some("KOBOLD".to_string());
        let mut router = rule("whisper");
        router.redirect_to = Some("thoughts".to_string());
        let pack = pack_with(vec![("rewrite", rewriter), ("route", router)]);

        assert!(pack.needs_approval());
        let digest = pack.sensitive_rules();
        assert_eq!(digest.len(), 2, "both powers listed");
        // Sorted, so the digest reads the same every time.
        assert_eq!(digest[0].0, "rewrite");
        assert_eq!(digest[1].0, "route");
        assert!(digest[0].1.contains("rewrites"));
        assert!(digest[1].1.contains("reroutes"));
    }

    #[test]
    fn unapproved_pack_keeps_alerts_but_loses_sensitive_powers() {
        let mut mixed = rule("kobold");
        mixed.fg = Some("#ff0000".to_string());
        mixed.replace = Some("KOBOLD".to_string());
        mixed.redirect_to = Some("combat".to_string());
        mixed.alert = Some(AlertSpec {
            banner: Some("KOBOLD".to_string()),
            ..Default::default()
        });
        let pack = pack_with(vec![("mixed", mixed)]);

        let effective = pack.effective_rules(false);
        let got = effective.get("mixed").expect("rule survives");
        // Stripped, not dropped: the safe half of the rule still works.
        assert!(got.replace.is_none(), "rewrite withheld");
        assert!(got.redirect_to.is_none(), "reroute withheld");
        assert_eq!(got.fg.as_deref(), Some("#ff0000"), "coloring still applies");
        assert!(got.alert.is_some(), "alert still fires");
    }

    #[test]
    fn approved_pack_gets_its_full_powers() {
        let mut rewriter = rule("kobold");
        rewriter.replace = Some("KOBOLD".to_string());
        let pack = pack_with(vec![("rewrite", rewriter)]);

        let effective = pack.effective_rules(true);
        assert_eq!(
            effective.get("rewrite").expect("rule").replace.as_deref(),
            Some("KOBOLD")
        );
    }

    #[test]
    fn approval_is_bound_to_one_exact_hash() {
        let mut approvals = AlertPackApprovals::default();
        approvals.approve("testpack", "abc123");
        assert!(approvals.is_approved("testpack", "abc123"));
        // A jinx update or hand edit changes the hash -> gate re-arms itself.
        assert!(
            !approvals.is_approved("testpack", "def456"),
            "changed content must be re-reviewed"
        );
    }

    #[test]
    fn revoking_re_arms_the_gate() {
        let mut approvals = AlertPackApprovals::default();
        approvals.approve("p", "h");
        approvals.revoke("p");
        assert!(!approvals.is_approved("p", "h"));
    }

    #[test]
    fn disabled_packs_contribute_nothing() {
        let pack = pack_with(vec![("r", rule("goblin"))]);
        let approvals = AlertPackApprovals::default(); // nothing enabled
        let mut highlights = HashMap::new();
        Config::merge_alert_packs(&mut highlights, &[pack], &approvals, &RoomScope::default());
        assert!(highlights.is_empty(), "installed is not the same as enabled");
    }

    #[test]
    fn enabled_pack_rules_are_namespaced_and_never_clobber_personal_rules() {
        let pack = pack_with(vec![("stun", rule("packstun"))]);
        let mut approvals = AlertPackApprovals::default();
        approvals.set_enabled("testpack", true);

        // The user happens to have their own rule with the same bare name.
        let mut highlights = HashMap::new();
        highlights.insert("stun".to_string(), rule("mystun"));

        Config::merge_alert_packs(&mut highlights, &[pack], &approvals, &RoomScope::default());

        assert_eq!(
            highlights.get("stun").expect("personal rule").pattern,
            "mystun",
            "a pack must never overwrite the user's own rule"
        );
        assert_eq!(
            highlights
                .get("pack:testpack/stun")
                .expect("pack rule")
                .pattern,
            "packstun"
        );
    }

    #[test]
    fn two_packs_sharing_a_rule_name_coexist() {
        let mut a = pack_with(vec![("boss", rule("from-a"))]);
        a.name = "packa".to_string();
        let mut b = pack_with(vec![("boss", rule("from-b"))]);
        b.name = "packb".to_string();

        let mut approvals = AlertPackApprovals::default();
        approvals.set_enabled("packa", true);
        approvals.set_enabled("packb", true);

        let mut highlights = HashMap::new();
        Config::merge_alert_packs(&mut highlights, &[a, b], &approvals, &RoomScope::default());
        assert_eq!(highlights.len(), 2, "namespacing keeps both");
    }

    // ---- Area scoping -----------------------------------------------

    fn reim() -> RoomScope {
        RoomScope {
            location: Some("the Settlement of Reim".to_string()),
            realm: Some(7),
            tags: vec!["bank".to_string()],
            uid: Some(12345),
        }
    }

    fn scoped(pack_name: &str, scope: AlertPackScope) -> AlertPack {
        let mut pack = pack_with(vec![("r", rule("goblin"))]);
        pack.name = pack_name.to_string();
        pack.scope = scope;
        pack
    }

    #[test]
    fn an_unscoped_pack_is_armed_everywhere() {
        let scope = AlertPackScope::default();
        assert!(scope.is_unscoped());
        assert!(scope.admits(None, None, &[], None), "even with no room data");
        assert!(scope.admits(Some("anywhere"), Some(1), &["x".into()], Some(9)));
    }

    #[test]
    fn area_scope_matches_the_location_case_insensitively() {
        let scope = AlertPackScope {
            area: vec!["The Settlement Of Reim".to_string()],
            ..Default::default()
        };
        assert!(scope.admits(Some("the Settlement of Reim"), None, &[], None));
        assert!(!scope.admits(Some("Wehnimer's Landing"), None, &[], None));
    }

    #[test]
    fn zone_scope_works_without_any_mapdb_data() {
        // The whole point of zone: it comes off the wire, so it still scopes
        // correctly in places the mapdb has never heard of.
        let scope = AlertPackScope { zone: vec![7], ..Default::default() };
        assert!(scope.admits(None, Some(7), &[], None));
        assert!(!scope.admits(None, Some(8), &[], None));
    }

    #[test]
    fn tag_scope_matches_any_shared_tag() {
        let scope = AlertPackScope {
            tags: vec!["bank".to_string()],
            ..Default::default()
        };
        assert!(scope.admits(None, None, &["shop".into(), "bank".into()], None));
        assert!(!scope.admits(None, None, &["shop".into()], None));
    }

    #[test]
    fn room_scope_matches_an_exact_uid() {
        let scope = AlertPackScope { rooms: vec![42], ..Default::default() };
        assert!(scope.admits(None, None, &[], Some(42)));
        assert!(!scope.admits(None, None, &[], Some(43)));
    }

    #[test]
    fn selectors_combine_as_or_so_any_one_match_arms_the_pack() {
        let scope = AlertPackScope {
            area: vec!["Nowhere".to_string()],
            rooms: vec![42],
            ..Default::default()
        };
        // Area misses but the uid hits: still in scope.
        assert!(scope.admits(Some("Elsewhere"), None, &[], Some(42)));
        assert!(!scope.admits(Some("Elsewhere"), None, &[], Some(43)));
    }

    #[test]
    fn a_scoped_pack_stays_closed_when_its_data_is_missing() {
        // Failing OPEN here would make scoping meaningless exactly where it
        // matters — unmapped rooms. "Everywhere" is spelled by not scoping.
        let scope = AlertPackScope {
            area: vec!["the Settlement of Reim".to_string()],
            ..Default::default()
        };
        assert!(!scope.admits(None, None, &[], None));
    }

    #[test]
    fn out_of_area_packs_contribute_no_rules_at_all() {
        let pack = scoped(
            "reim",
            AlertPackScope {
                area: vec!["the Settlement of Reim".to_string()],
                ..Default::default()
            },
        );
        let mut approvals = AlertPackApprovals::default();
        approvals.set_enabled("reim", true);

        // Standing in Reim: armed.
        let mut inside = HashMap::new();
        Config::merge_alert_packs(&mut inside, &[pack.clone()], &approvals, &reim());
        assert_eq!(inside.len(), 1, "armed inside its area");

        // Standing in a bank in Wehnimer's: contributes nothing, because
        // membership of the rule set IS the arming mechanism.
        let elsewhere = RoomScope {
            location: Some("Wehnimer's Landing".to_string()),
            ..Default::default()
        };
        let mut outside = HashMap::new();
        Config::merge_alert_packs(&mut outside, &[pack], &approvals, &elsewhere);
        assert!(outside.is_empty(), "disarmed outside its area");
    }

    #[test]
    fn scoping_and_the_trust_gate_are_independent() {
        // An in-scope but unapproved pack still loses its sensitive powers.
        let mut rewriter = rule("kobold");
        rewriter.replace = Some("KOBOLD".to_string());
        let mut pack = pack_with(vec![("rewrite", rewriter)]);
        pack.scope = AlertPackScope { zone: vec![7], ..Default::default() };
        let mut approvals = AlertPackApprovals::default();
        approvals.set_enabled("testpack", true);

        let mut highlights = HashMap::new();
        Config::merge_alert_packs(&mut highlights, &[pack], &approvals, &reim());
        assert!(
            highlights
                .get("pack:testpack/rewrite")
                .expect("in scope, so present")
                .replace
                .is_none(),
            "in scope does not imply approved"
        );
    }

    #[test]
    fn an_unscoped_pack_survives_a_move_that_disarms_a_scoped_one() {
        let ambiance = scoped("ambiance", AlertPackScope::default());
        let encounter = scoped(
            "reim",
            AlertPackScope { zone: vec![7], ..Default::default() },
        );
        let mut approvals = AlertPackApprovals::default();
        approvals.set_enabled("ambiance", true);
        approvals.set_enabled("reim", true);

        let packs = [ambiance, encounter];
        let mut here = HashMap::new();
        Config::merge_alert_packs(&mut here, &packs, &approvals, &reim());
        assert_eq!(here.len(), 2, "both armed in zone 7");

        let far = RoomScope { realm: Some(99), ..Default::default() };
        let mut there = HashMap::new();
        Config::merge_alert_packs(&mut there, &packs, &approvals, &far);
        assert_eq!(there.len(), 1, "ambiance stays, encounter drops");
        assert!(there.contains_key("pack:ambiance/r"));
    }

    #[test]
    fn a_pack_file_can_declare_its_scope() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        let packs_dir = Config::alertpacks_dir().unwrap();
        fs::create_dir_all(&packs_dir).unwrap();
        fs::write(
            packs_dir.join("reim.toml"),
            "[__scope__]\narea = [\"the Settlement of Reim\"]\nzone = [7]\n\n\
             [stun]\npattern = \"You are stunned\"\n",
        )
        .unwrap();

        let packs = Config::load_alert_packs();
        assert_eq!(packs.len(), 1);
        // The scope table must not be mistaken for a rule.
        assert_eq!(packs[0].rules.len(), 1, "only the real rule counts");
        assert!(packs[0].rules.contains_key("stun"));
        assert_eq!(packs[0].scope.area, vec!["the Settlement of Reim"]);
        assert_eq!(packs[0].scope.zone, vec![7]);

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn a_pack_with_a_malformed_scope_is_skipped_not_armed_everywhere() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        let packs_dir = Config::alertpacks_dir().unwrap();
        fs::create_dir_all(&packs_dir).unwrap();
        fs::write(
            packs_dir.join("bad.toml"),
            "[__scope__]\nzone = \"not a list of numbers\"\n\n[r]\npattern = \"x\"\n",
        )
        .unwrap();

        // Silently treating a broken scope as "everywhere" is the permissive
        // direction, and the wrong one.
        assert!(Config::load_alert_packs().is_empty());

        std::env::remove_var("VELLUM_FE_DIR");
    }

    // ---- Disk round-trips -------------------------------------------

    #[test]
    fn packs_load_from_disk_and_hash_their_contents() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        let packs_dir = Config::alertpacks_dir().unwrap();
        fs::create_dir_all(&packs_dir).unwrap();
        fs::write(
            packs_dir.join("reim.toml"),
            "[stun]\npattern = \"You are stunned\"\n",
        )
        .unwrap();
        // Not a .toml: must be ignored rather than tripping the parser.
        fs::write(packs_dir.join("README.md"), "notes").unwrap();
        // Malformed pack: must be skipped WITHOUT killing the good one, or a
        // single bad file from a shared repo takes down the whole client.
        fs::write(packs_dir.join("broken.toml"), "this is not = valid = toml").unwrap();

        let packs = Config::load_alert_packs();
        assert_eq!(packs.len(), 1, "only the valid .toml loads");
        assert_eq!(packs[0].name, "reim");
        assert_eq!(packs[0].rules.len(), 1);
        assert!(!packs[0].hash.is_empty());

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn editing_a_pack_on_disk_changes_its_hash_and_re_arms_the_gate() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        let packs_dir = Config::alertpacks_dir().unwrap();
        fs::create_dir_all(&packs_dir).unwrap();
        let path = packs_dir.join("p.toml");
        fs::write(&path, "[r]\npattern = \"a\"\nreplace = \"A\"\n").unwrap();

        let before = Config::load_alert_packs();
        let mut approvals = AlertPackApprovals::default();
        approvals.set_enabled("p", true);
        approvals.approve("p", &before[0].hash);
        assert!(approvals.is_approved("p", &before[0].hash));

        // Simulate a jinx update / hand edit changing the pack's content.
        fs::write(&path, "[r]\npattern = \"a\"\nreplace = \"SOMETHING ELSE\"\n").unwrap();
        let after = Config::load_alert_packs();
        assert_ne!(before[0].hash, after[0].hash, "content change -> new hash");
        assert!(
            !approvals.is_approved("p", &after[0].hash),
            "an updated pack must be re-reviewed, whatever channel delivered it"
        );

        // And the updated-but-unapproved pack runs with its powers stripped.
        let mut highlights = HashMap::new();
        Config::merge_alert_packs(&mut highlights, &after, &approvals, &RoomScope::default());
        assert!(highlights
            .get("pack:p/r")
            .expect("rule loaded")
            .replace
            .is_none());

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn approvals_survive_a_save_load_round_trip() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        let mut approvals = AlertPackApprovals::default();
        approvals.set_enabled("reim", true);
        approvals.approve("reim", "deadbeef");
        Config::save_alertpack_approvals(&approvals).unwrap();

        let loaded = Config::load_alertpack_approvals();
        assert!(loaded.is_enabled("reim"));
        assert!(loaded.is_approved("reim", "deadbeef"));
        assert!(!loaded.is_approved("reim", "other"));

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn missing_pack_directory_is_not_an_error() {
        let _guard = crate::config::VELLUM_FE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("VELLUM_FE_DIR", dir.path());

        // Nothing installed is the normal state, not a failure.
        assert!(Config::load_alert_packs().is_empty());
        let approvals = Config::load_alertpack_approvals();
        assert!(approvals.enabled.is_empty());

        std::env::remove_var("VELLUM_FE_DIR");
    }

    #[test]
    fn merging_an_enabled_but_unapproved_pack_strips_sensitive_rules() {
        let mut rewriter = rule("kobold");
        rewriter.replace = Some("KOBOLD".to_string());
        let pack = pack_with(vec![("rewrite", rewriter)]);
        let mut approvals = AlertPackApprovals::default();
        approvals.set_enabled("testpack", true); // enabled, NOT approved

        let mut highlights = HashMap::new();
        Config::merge_alert_packs(&mut highlights, &[pack], &approvals, &RoomScope::default());

        assert!(
            highlights
                .get("pack:testpack/rewrite")
                .expect("rule present")
                .replace
                .is_none(),
            "enabling alone must not grant sensitive powers"
        );
    }
}
