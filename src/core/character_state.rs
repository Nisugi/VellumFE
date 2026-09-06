//! Character state parsed from the game feed — a focused port of Lich's
//! Infomon (`lib/gemstone/infomon/parser.rb`) for the fields that gate travel:
//! society status/rank (seeking), profession (guild tags), CHE/House (locker
//! tags), and citizenship.
//!
//! Like Lich's `Infomon::Parser.parse`, this is a per-line text matcher: the
//! parser feeds each game line to [`CharacterState::parse_line`], which fills
//! the state. The state is populated in response to the SOCIETY / INFO /
//! PROFILE / CITIZENSHIP commands (and in-game society join/step/resign
//! events) — none are spontaneous, so travel features that need this state
//! ask for it first.

use std::sync::LazyLock;

use regex::Regex;

macro_rules! re {
    ($name:ident, $pattern:literal) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($pattern).expect("valid pattern"));
    };
}

// --- society (SOCIETY command) ---
// "   You are a member in the Order of Voln at step 13."
// "   You are a Master in the Council of Light."  (rank forced by handler)
re!(
    SOCIETY,
    r"^\s+You are a (?P<standing>Master|member) (?:in|of) the (?P<society>Order of Voln|Council of Light|Guardians of Sunfist)(?: at (?:rank|step) (?P<rank>[0-9]+))?\.$"
);
re!(
    NO_SOCIETY,
    r"^\s+You are not a member of any society at this time\."
);
// Join lines (rank 1, Sunfist 0).
re!(
    SOCIETY_JOIN,
    r#"^The Grandmaster says, "Welcome to the Order|^The Grandmaster says, "You are now a member of the Guardians of Sunfist"#
);
// Step-up lines (rank += 1).
re!(
    SOCIETY_STEP,
    r"^(?:Zarak|Faylanna|Draelox|Marl|Vindar|Taryn|Meaha|Oxanna|Cyndelle) traces the outline of a sigil into the air before you and says|^The High Taskmaster looks at you, consults (?:her|his) notes, and then announces in a loud voice|^The monk concludes ceremoniously,"
);
// Resign lines (status None, rank 0).
re!(
    SOCIETY_RESIGN,
    r#"^The Grandmaster says, "I'm sorry to hear that\.  You are no longer in our service\.|^The Poohbah looks at you sternly|^The Grandmaster says, "I'm sorry to hear that,"#
);

// --- profession (INFO command header) ---
// "Name: Nisugi Race: Half-Elf  Profession: Ranger (shown as: Hero)"
re!(
    CHAR_RACE_PROF,
    r"^Name:\s+(?P<name>[A-Za-z\s'-]+)\s+Race:\s+(?P<race>[A-Za-z]+|[A-Za-z]+(?: |-)[A-Za-z]+)\s+Profession:\s+(?P<profession>[-A-Za-z]+)"
);
re!(
    CHAR_PROF_LEVEL,
    r"^Profession:\s+(?P<profession>[-A-Za-z]+)\s+Level:\s*(?P<level>\d+)"
);
re!(CHAR_INFO_LEVEL, r"^Gender:.*\sLevel:\s*(?P<level>\d+)\s*$");

// --- CHE / House (PROFILE / RESIGN) ---
const HOUSE_NAMES: &str = "Argent Aspis|Rising Phoenix|Paupers|Arcane Masters|Brigatta|Twilight Hall|Silvergate Inn|Sovyn|Sylvanfair|Helden Hall|White Haven|Beacon Hall|Rone Academy|Willow Hall|Moonstone Abbey|Obsidian Tower|Cairnfang Manor";
re!(PROFILE_START, r"^PERSONAL INFORMATION$");
static PROFILE_HOUSE_CHE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^[A-Za-z\- ]+? (?:of House of the |of House of |of House |House of the |House of |House |of )(?P<house>{HOUSE_NAMES})(?: Archive)?$|^(?P<none>No House affiliation)$"
    ))
    .expect("valid pattern")
});
static RESIGN_CONFIRM_CHE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^\[You have resigned from the (?:House of the |House of |House |of House of the |of House of |of House |of )?(?P<house>{HOUSE_NAMES})(?: Archive)?\.\]$"
    ))
    .expect("valid pattern")
});

// --- citizenship (CITIZENSHIP command) ---
re!(
    CITIZENSHIP,
    r"^You currently have .*? citizenship in (?P<town>.*)\.$"
);
re!(NO_CITIZENSHIP, r"^You don't seem to have citizenship\.");

// --- urchin access (`urchin status` command) ---
// "You will have access to the urchin guides until 8/28/2026 14:30:00 ET."
re!(
    URCHIN_EXPIRES,
    r"^You will have access to the urchin guides until (?P<m>\d+)/(?P<d>\d+)/(?P<y>\d+) (?P<h>\d+):(?P<min>\d+):(?P<s>\d+)"
);
re!(URCHIN_PERMANENT, r"permanent access to the urchin guides");
re!(
    URCHIN_NONE,
    r"You currently have no access to the urchin guides"
);
// The PROFILE block states citizenship differently:
// "Full citizen of Kraken's Fall" / "Partial citizen of X" / "No citizenship".
re!(
    PROFILE_CITIZENSHIP,
    r"^(?:Full|Partial) citizen of (?P<town>.+)$"
);
re!(PROFILE_NO_CITIZENSHIP, r"^No citizenship$");

/// Parsed character state driving travel gates. `None`/empty fields mean
/// "not yet known" (the relevant command hasn't been run this session).
///
/// Persisted per-character in the session cache (these attributes change
/// rarely and the game announces changes, which the parser catches). The
/// runtime-only `in_profile`/`generation` fields are not serialized.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CharacterState {
    /// Society name (Lich `society.status`), e.g. "Order of Voln", or "None".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub society: Option<String>,
    /// Society rank/step (Lich `society.rank`). Master → 26 (Voln) / 20.
    #[serde(default)]
    pub society_rank: u32,
    /// Profession (Lich `stat.profession`), e.g. "Ranger".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profession: Option<String>,
    /// Display-ready character level from the character sheet. The live
    /// experience dialog remains authoritative while it is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// CHE / House affiliation, normalized (Lich `che`), e.g. "twilight_hall"
    /// or "none".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub che: Option<String>,
    /// Home-town citizenship (Lich `citizenship`), or "None".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citizenship: Option<String>,
    /// Urchin-guide access expiry as a Unix epoch (Lich
    /// `UserVars.mapdb_urchins_expire`). `0` = no/unknown access; a far-future
    /// value = permanent. Parsed from `urchin status`. Gates urchin travel.
    #[serde(default)]
    pub urchins_expire: i64,
    /// Spell numbers watched by the missing-spells window (`.spellwatch`).
    /// Kept in add order; persisted per-character with the rest of this
    /// struct through the session cache.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub watched_spells: Vec<u16>,
    /// Base character name observed from the live `INFO` character-sheet
    /// header. Runtime-only: a cached name must never authenticate a later
    /// detachable-client connection.
    #[serde(skip)]
    observed_name: Option<String>,
    /// True while inside a PROFILE block (between PERSONAL INFORMATION and the
    /// house line) — the house line is only trusted then, matching Lich's
    /// State::Profile gate. Runtime-only.
    #[serde(skip)]
    in_profile: bool,
    /// Bumped on any change; callers diff on it. Runtime-only.
    #[serde(skip)]
    pub generation: u64,
}

impl CharacterState {
    /// Add spell numbers to the watch list (dedup, keep add order).
    /// Returns how many were newly added; bumps `generation` on change so
    /// the session-cache persistence picks it up.
    pub fn watch_spells(&mut self, numbers: &[u16]) -> usize {
        let mut added = 0;
        for &number in numbers {
            if !self.watched_spells.contains(&number) {
                self.watched_spells.push(number);
                added += 1;
            }
        }
        if added > 0 {
            self.generation += 1;
        }
        added
    }

    /// Remove spell numbers from the watch list. Returns how many were
    /// actually removed; bumps `generation` on change.
    pub fn unwatch_spells(&mut self, numbers: &[u16]) -> usize {
        let before = self.watched_spells.len();
        self.watched_spells
            .retain(|number| !numbers.contains(number));
        let removed = before - self.watched_spells.len();
        if removed > 0 {
            self.generation += 1;
        }
        removed
    }

    /// Clear the whole watch list. Returns how many entries were dropped.
    pub fn unwatch_all_spells(&mut self) -> usize {
        let removed = self.watched_spells.len();
        if removed > 0 {
            self.watched_spells.clear();
            self.generation += 1;
        }
        removed
    }

    /// Feed one game line. Returns true if it changed state.
    pub fn parse_line(&mut self, line: &str) -> bool {
        let before = self.generation;

        if let Some(c) = SOCIETY.captures(line) {
            let society = c["society"].to_string();
            let mut rank = c
                .name("rank")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            if &c["standing"] == "Master" {
                rank = if society.contains("Voln") { 26 } else { 20 };
            }
            self.set_society(Some(society), rank);
        } else if NO_SOCIETY.is_match(line) {
            self.set_society(Some("None".into()), 0);
        } else if SOCIETY_JOIN.is_match(line) {
            // Sunfist join sets rank 0; the others rank 1. Match Lich's
            // substring branch: only Voln/Council welcome lines get rank 1.
            if line.contains("Guardians of Sunfist") {
                self.set_society(Some("Guardians of Sunfist".into()), 0);
            } else if line.contains("Order") {
                self.set_society(Some("Order of Voln".into()), 1);
            }
        } else if SOCIETY_STEP.is_match(line) {
            self.set_society(self.society.clone(), self.society_rank + 1);
        } else if SOCIETY_RESIGN.is_match(line) {
            self.set_society(Some("None".into()), 0);
        } else if let Some(c) = CHAR_RACE_PROF.captures(line.trim_start()) {
            // Lich buffers this until StatEnd; we can commit directly — the
            // header line alone carries the profession.
            self.observed_name = c["name"].split_whitespace().next().map(str::to_string);
            self.set_profession(Some(c["profession"].to_string()));
        } else if let Some(c) = CHAR_PROF_LEVEL.captures(line) {
            self.set_profession(Some(c["profession"].to_string()));
            self.set_level(Some(c["level"].to_string()));
        } else if let Some(c) = CHAR_INFO_LEVEL.captures(line) {
            self.set_level(Some(c["level"].to_string()));
        } else if PROFILE_START.is_match(line) {
            self.in_profile = true;
        } else if self.in_profile && PROFILE_HOUSE_CHE.is_match(line) {
            // Only consume the line when it actually IS the house line — a
            // non-matching profile line (citizenship, etc.) falls through to
            // the arms below instead of being swallowed by the in_profile gate.
            let c = PROFILE_HOUSE_CHE.captures(line).expect("matched above");
            let che = if c.name("none").is_some() {
                "none".to_string()
            } else {
                normalize_name(&c["house"])
            };
            self.set_che(Some(che));
            self.in_profile = false;
        } else if let Some(c) = RESIGN_CONFIRM_CHE.captures(line) {
            let _ = &c; // resigning always clears CHE
            self.set_che(Some("none".into()));
        } else if let Some(c) = CITIZENSHIP.captures(line) {
            self.set_citizenship(Some(c["town"].to_string()));
        } else if NO_CITIZENSHIP.is_match(line) {
            self.set_citizenship(Some("None".into()));
        } else if let Some(c) = PROFILE_CITIZENSHIP.captures(line) {
            // "Full citizen of Kraken's Fall" (PROFILE block).
            self.set_citizenship(Some(c["town"].to_string()));
        } else if PROFILE_NO_CITIZENSHIP.is_match(line) {
            self.set_citizenship(Some("None".into()));
        } else if let Some(c) = URCHIN_EXPIRES.captures(line) {
            // Parse the M/D/Y H:M:S into a Unix epoch (the game prints local
            // ET; we parse the wall-clock naively — good enough for the
            // "expired?" comparison, which only needs day-ish granularity).
            if let Some(epoch) = urchin_epoch(&c) {
                self.set_urchins_expire(epoch);
            }
        } else if URCHIN_PERMANENT.is_match(line) {
            self.set_urchins_expire(permanent_epoch());
        } else if URCHIN_NONE.is_match(line) {
            self.set_urchins_expire(0);
        }

        self.generation != before
    }

    // --- travel-facing queries ---

    /// True when the character is a Voln Master (rank 26) — the seeking gate.
    pub fn can_seek(&self) -> bool {
        self.society.as_deref() == Some("Order of Voln") && self.society_rank == 26
    }

    /// Character name observed from live character-sheet output during this
    /// process lifetime. This is deliberately excluded from persistence.
    pub fn observed_name(&self) -> Option<&str> {
        self.observed_name.as_deref()
    }

    /// Forget parser observations that cannot cross a transport generation.
    pub fn clear_connection_observations(&mut self) {
        self.observed_name = None;
        self.in_profile = false;
    }

    /// The guild tag for `.go2 guild` (Lich composes `<prof> guild`), lower
    /// case, e.g. "wizard guild". None until profession is known.
    pub fn guild_tag(&self, suffix: &str) -> Option<String> {
        let prof = self.profession.as_deref()?.to_lowercase();
        let suffix = suffix.trim().to_lowercase();
        Some(if suffix.is_empty() {
            format!("{prof} guild")
        } else {
            format!("{prof} {suffix}")
        })
    }

    /// Whether urchin travel is currently valid: access hasn't expired at
    /// `now_epoch` and the character isn't hidden/invisible (Lich's urchin
    /// timeto gate, minus the `use_urchins` toggle which the caller applies).
    pub fn urchins_valid(&self, now_epoch: i64, hidden: bool, invisible: bool) -> bool {
        self.urchins_expire != 0 && now_epoch < self.urchins_expire && !hidden && !invisible
    }

    /// The locker tags for `.go2 locker` (Lich builds from CHE membership).
    /// Non-members (`che == none`/unknown) get the public-locker tag.
    pub fn locker_tags(&self) -> Vec<String> {
        match self.che.as_deref() {
            Some(che) if che != "none" => vec![
                format!("meta:che:{che}:locker"),
                format!("meta:che:{che}:entrance_locker"),
                format!("meta:che:{che}:entrance_annex"),
                format!("meta:che:{che}:entrance_che"),
            ],
            _ => vec!["public locker".to_string()],
        }
    }

    // --- setters (bump generation on real change) ---

    fn set_society(&mut self, society: Option<String>, rank: u32) {
        if self.society != society || self.society_rank != rank {
            self.society = society;
            self.society_rank = rank;
            self.generation += 1;
        }
    }
    fn set_profession(&mut self, p: Option<String>) {
        if self.profession != p {
            self.profession = p;
            self.generation += 1;
        }
    }
    fn set_level(&mut self, level: Option<String>) {
        if self.level != level {
            self.level = level;
            self.generation += 1;
        }
    }

    /// Mirror the live experience dialog's level into the durable identity
    /// cache. The caller still owns the richer experience state.
    pub fn observe_level(&mut self, text: &str) -> bool {
        let before = self.generation;
        self.set_level(normalized_level_text(text));
        self.generation != before
    }
    fn set_che(&mut self, c: Option<String>) {
        if self.che != c {
            self.che = c;
            self.generation += 1;
        }
    }
    fn set_citizenship(&mut self, c: Option<String>) {
        if self.citizenship != c {
            self.citizenship = c;
            self.generation += 1;
        }
    }
    fn set_urchins_expire(&mut self, epoch: i64) {
        if self.urchins_expire != epoch {
            self.urchins_expire = epoch;
            self.generation += 1;
        }
    }
}

/// Parse the `urchin status` M/D/Y H:M:S capture into a Unix epoch. The game
/// prints local wall-clock (ET); we parse it naively as UTC — the "expired?"
/// check only needs coarse accuracy, and a few hours of TZ skew never matters
/// for a multi-day/week guide window.
fn urchin_epoch(c: &regex::Captures) -> Option<i64> {
    use chrono::{NaiveDate, NaiveTime};
    let g = |name: &str| c.name(name)?.as_str().parse::<u32>().ok();
    let (m, d, y, h, min, s) = (g("m")?, g("d")?, g("y")?, g("h")?, g("min")?, g("s")?);
    let date = NaiveDate::from_ymd_opt(y as i32, m, d)?;
    let time = NaiveTime::from_hms_opt(h, min, s)?;
    Some(date.and_time(time).and_utc().timestamp())
}

/// A far-future sentinel epoch for permanent urchin access (Lich uses Jan 1 of
/// next year; we use a fixed far-future date so it needs no clock).
fn permanent_epoch() -> i64 {
    use chrono::NaiveDate;
    NaiveDate::from_ymd_opt(9999, 1, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc().timestamp())
        .unwrap_or(i64::MAX)
}

/// Lich's `Util.normalize_name`: lowercase, spaces/hyphens → underscores.
fn normalize_name(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .replace([' ', '-'], "_")
        .replace(['\'', ':'], "")
}

// "You have 1,234,567 silver with you." / "no silver" / "but one silver".
re!(WEALTH, r"^You have (no|[\d,]+|but one) silver with you");

/// Parse silver-on-hand from a `wealth`/`wealth quiet` line (go2's
/// `wealth_quiet`). `no` → 0, `but one` → 1, commas stripped. None if the
/// line isn't a wealth line.
pub fn parse_wealth_line(line: &str) -> Option<u64> {
    let c = WEALTH.captures(line.trim())?;
    Some(match &c[1] {
        "no" => 0,
        "but one" => 1,
        n => n.replace(',', "").parse().ok()?,
    })
}

/// Cheap gate: could this line carry character state? Lets the feed skip the
/// full regex battery on ordinary lines.
pub fn line_is_character_state(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("You are a ")
        || t.starts_with("You are not a member")
        || t.starts_with("Name:")
        || t.starts_with("Profession:")
        || t.starts_with("Gender:")
        || t.starts_with("PERSONAL INFORMATION")
        || t.starts_with("You currently have ")
        || t.starts_with("You don't seem to have citizenship")
        || t.starts_with("Full citizen of ")
        || t.starts_with("Partial citizen of ")
        || t == "No citizenship"
        || t.contains("access to the urchin guides")
        || t.starts_with("The Grandmaster says")
        || t.starts_with("The Poohbah")
        || t.starts_with("[You have resigned")
        || line.contains("traces the outline of a sigil")
        || line.contains("High Taskmaster")
        || line.contains("monk concludes ceremoniously")
        // A PROFILE house line (only trusted while in_profile); cheap check.
        || t.contains(" of House ")
        || t == "No House affiliation"
}

/// Normalize either the experience label (`Level: 90`) or the character
/// sheet's numeric capture (`90`) into the value clients display.
pub(crate) fn normalized_level_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_label = trimmed
        .get(..5)
        .filter(|prefix| prefix.eq_ignore_ascii_case("level"))
        .map_or(trimmed, |_| {
            trimmed[5..].trim_start_matches(|c: char| c == ':' || c.is_whitespace())
        });
    (!without_label.is_empty()).then(|| without_label.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_society_status_and_rank() {
        let mut s = CharacterState::default();
        s.parse_line("   You are a member in the Order of Voln at step 13.");
        assert_eq!(s.society.as_deref(), Some("Order of Voln"));
        assert_eq!(s.society_rank, 13);
        assert!(!s.can_seek());

        // Voln Master → rank forced to 26 → can seek.
        s.parse_line("   You are a Master in the Order of Voln.");
        assert_eq!(s.society_rank, 26);
        assert!(s.can_seek());

        // CoL Master → 20.
        s.parse_line("   You are a Master in the Council of Light.");
        assert_eq!(s.society.as_deref(), Some("Council of Light"));
        assert_eq!(s.society_rank, 20);
        assert!(!s.can_seek());
    }

    #[test]
    fn society_step_and_resign() {
        let mut s = CharacterState::default();
        s.parse_line("   You are a member in the Order of Voln at step 13.");
        s.parse_line("Zarak traces the outline of a sigil into the air before you and says, \"Congratulations.\"");
        assert_eq!(s.society_rank, 14);
        s.parse_line(
            "The Grandmaster says, \"I'm sorry to hear that.  You are no longer in our service.\"",
        );
        assert_eq!(s.society.as_deref(), Some("None"));
        assert_eq!(s.society_rank, 0);
    }

    #[test]
    fn parses_profession_and_guild_tag() {
        let mut s = CharacterState::default();
        s.parse_line("Name: Nisugi Race: Half-Elf  Profession: Ranger (shown as: Hero)");
        assert_eq!(s.profession.as_deref(), Some("Ranger"));
        assert_eq!(s.guild_tag("guild").as_deref(), Some("ranger guild"));
        assert_eq!(s.guild_tag("").as_deref(), Some("ranger guild"));
    }

    #[test]
    fn parses_character_sheet_profession_and_level_for_persistence() {
        let mut s = CharacterState::default();

        let identity =
            "Name: Calvix Shenron Race: Dark Elf  Profession: Sorcerer (shown as: Warlock)";
        let details = "Gender: Male    Age: 472    Expr: 6,527,263    Level:  90";
        assert!(line_is_character_state(identity));
        assert!(line_is_character_state(details));
        assert!(s.parse_line(identity));
        assert!(s.parse_line(details));
        assert_eq!(s.observed_name(), Some("Calvix"));
        assert_eq!(s.profession.as_deref(), Some("Sorcerer"));
        assert_eq!(s.level.as_deref(), Some("90"));

        let toml = toml::to_string(&s).unwrap();
        let restored: CharacterState = toml::from_str(&toml).unwrap();
        assert_eq!(restored.observed_name(), None);
        assert_eq!(restored.profession.as_deref(), Some("Sorcerer"));
        assert_eq!(restored.level.as_deref(), Some("90"));
    }

    #[test]
    fn parses_indented_character_sheet_identity() {
        let mut state = CharacterState::default();
        assert!(line_is_character_state(
            "   Name: Calvix Shenron Race: Dark Elf  Profession: Sorcerer"
        ));
        state.parse_line("   Name: Calvix Shenron Race: Dark Elf  Profession: Sorcerer");
        assert_eq!(state.observed_name(), Some("Calvix"));
        assert_eq!(state.profession.as_deref(), Some("Sorcerer"));
    }

    #[test]
    fn parses_che_from_profile_block() {
        let mut s = CharacterState::default();
        // House line ignored until the PROFILE header opens the block.
        s.parse_line("Lord Someone of House Twilight Hall");
        assert_eq!(s.che, None);
        s.parse_line("PERSONAL INFORMATION");
        s.parse_line("Lord Someone of House Twilight Hall");
        assert_eq!(s.che.as_deref(), Some("twilight_hall"));
        assert_eq!(
            s.locker_tags(),
            vec![
                "meta:che:twilight_hall:locker",
                "meta:che:twilight_hall:entrance_locker",
                "meta:che:twilight_hall:entrance_annex",
                "meta:che:twilight_hall:entrance_che",
            ]
        );
        // Non-member → public locker.
        s.parse_line("[You have resigned from the House of Twilight Hall.]");
        assert_eq!(s.che.as_deref(), Some("none"));
        assert_eq!(s.locker_tags(), vec!["public locker"]);
    }

    #[test]
    fn parses_citizenship() {
        let mut s = CharacterState::default();
        s.parse_line("You currently have full citizenship in Wehnimer's Landing.");
        assert_eq!(s.citizenship.as_deref(), Some("Wehnimer's Landing"));
        s.parse_line("You don't seem to have citizenship.");
        assert_eq!(s.citizenship.as_deref(), Some("None"));
    }

    #[test]
    fn no_house_affiliation() {
        let mut s = CharacterState::default();
        s.parse_line("PERSONAL INFORMATION");
        s.parse_line("No House affiliation");
        assert_eq!(s.che.as_deref(), Some("none"));
    }

    #[test]
    fn parses_urchin_expiry() {
        let mut s = CharacterState::default();
        // Timed access → an epoch; valid before it, expired after.
        s.parse_line("You will have access to the urchin guides until 8/28/2026 14:30:00 ET.");
        assert!(s.urchins_expire > 0);
        let expire = s.urchins_expire;
        assert!(
            s.urchins_valid(expire - 1, false, false),
            "valid before expiry"
        );
        assert!(
            !s.urchins_valid(expire + 1, false, false),
            "invalid after expiry"
        );
        // Hidden/invisible blocks urchins even when not expired.
        assert!(!s.urchins_valid(expire - 1, true, false), "hidden blocks");
        assert!(
            !s.urchins_valid(expire - 1, false, true),
            "invisible blocks"
        );

        // Permanent → far future.
        s.parse_line("You have permanent access to the urchin guides.");
        assert!(s.urchins_valid(4_000_000_000, false, false), "permanent");

        // No access → expired (0).
        s.parse_line("You currently have no access to the urchin guides.");
        assert_eq!(s.urchins_expire, 0);
        assert!(!s.urchins_valid(0, false, false), "no access");
    }

    #[test]
    fn parses_wealth_lines() {
        assert_eq!(
            parse_wealth_line("You have 1,234,567 silver with you."),
            Some(1_234_567)
        );
        assert_eq!(parse_wealth_line("You have no silver with you."), Some(0));
        assert_eq!(
            parse_wealth_line("You have but one silver with you."),
            Some(1)
        );
        assert_eq!(parse_wealth_line("A cat wanders by."), None);
    }

    #[test]
    fn round_trips_through_toml_for_persistence() {
        let mut s = CharacterState::default();
        s.parse_line("   You are a Master of the Guardians of Sunfist.");
        s.parse_line("Name: Nisugi Race: Half-Elf  Profession: Ranger (shown as: Hero)");
        s.parse_line("PERSONAL INFORMATION");
        s.parse_line("Member of House of Paupers");
        s.parse_line("You currently have full citizenship in Kraken's Fall.");

        let toml = toml::to_string(&s).unwrap();
        let back: CharacterState = toml::from_str(&toml).unwrap();
        // The persisted attributes survive; the runtime-only fields reset.
        assert_eq!(back.society.as_deref(), Some("Guardians of Sunfist"));
        assert_eq!(back.society_rank, 20);
        assert_eq!(back.profession.as_deref(), Some("Ranger"));
        assert_eq!(back.che.as_deref(), Some("paupers"));
        assert_eq!(back.citizenship.as_deref(), Some("Kraken's Fall"));
        // Reloaded state still drives the gates.
        assert_eq!(back.guild_tag("guild").as_deref(), Some("ranger guild"));
        assert_eq!(back.locker_tags()[0], "meta:che:paupers:locker");
    }

    #[test]
    fn parses_real_society_info_and_profile_output() {
        // Nisugi's actual SOCIETY, INFO, and PROFILE FULL output.
        let mut s = CharacterState::default();
        for line in [
            "Current society status:",
            "   You are a Master of the Guardians of Sunfist.",
            "Name: Nisugi Race: Half-Elf  Profession: Ranger (shown as: Hero)",
            "PERSONAL INFORMATION",
            "Name: Nisugi",
            "Profession: Ranger   Level: 100",
            "AFFILIATIONS",
            "Full citizen of Kraken's Fall",
            "Member of The Kraken Collective",
            "Master of the Guardians of Sunfist",
            "No Guild affiliation",
            "Follower of Zelia",
            "Attuned to the Element of Earth",
            "Member of House of Paupers",
        ] {
            s.parse_line(line);
        }
        // Sunfist Master → rank 20, not a seeker (seeking is Voln-only).
        assert_eq!(s.society.as_deref(), Some("Guardians of Sunfist"));
        assert_eq!(s.society_rank, 20);
        assert!(!s.can_seek());
        // Profession from the INFO header.
        assert_eq!(s.profession.as_deref(), Some("Ranger"));
        assert_eq!(s.guild_tag("guild").as_deref(), Some("ranger guild"));
        // Citizenship from the PROFILE "Full citizen of" line.
        assert_eq!(s.citizenship.as_deref(), Some("Kraken's Fall"));
        // House of Paupers → paupers.
        assert_eq!(s.che.as_deref(), Some("paupers"));
        assert_eq!(s.locker_tags()[0], "meta:che:paupers:locker");
    }

    #[test]
    fn parses_real_urchin_status_line() {
        // Nisugi's live `urchin status` output (CST timezone, far-future date).
        let mut s = CharacterState::default();
        s.parse_line("You will have access to the urchin guides until 1/18/2038 21:14:07 CST.");
        assert!(s.urchins_expire > 0, "expiry parsed from the live line");
        // Far future → valid now, when not hidden/invisible.
        let now = chrono::Utc::now().timestamp();
        assert!(s.urchins_valid(now, false, false));
        // Hidden or invisible closes the gate (Lich's combined condition).
        assert!(!s.urchins_valid(now, true, false));
        assert!(!s.urchins_valid(now, false, true));
    }

    #[test]
    fn urchin_status_survives_session_cache_round_trip() {
        let mut s = CharacterState::default();
        s.parse_line("You will have access to the urchin guides until 1/18/2038 21:14:07 CST.");
        let expire = s.urchins_expire;
        // Serialize → deserialize (the session_cache path).
        let json = serde_json::to_string(&s).unwrap();
        let restored: CharacterState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.urchins_expire, expire, "expiry persists");
    }
}
