//! Game state management
//!
//! Tracks the current state of the game session: connection status,
//! character info, room state, inventory, etc.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};

use super::highlight_engine::SoundTrigger;

/// How often to recalculate lag estimate (in seconds of game time)
const LAG_CHECK_INTERVAL_SECS: i64 = 30;

/// Queued sounds from highlight processing
/// Pre-allocated with capacity for 5 sounds (typical is 2, but allows headroom)
#[derive(Clone, Debug, Default)]
pub struct SoundQueue {
    sounds: Vec<QueuedSound>,
}

/// A sound that has been queued for playback
#[derive(Clone, Debug)]
pub struct QueuedSound {
    pub file: String,
    pub volume: Option<f32>,
}

impl SoundQueue {
    pub fn new() -> Self {
        Self {
            sounds: Vec::with_capacity(5),
        }
    }
}

/// Game session state
#[derive(Clone, Debug)]
pub struct GameState {
    /// Connection status
    pub connected: bool,

    /// Character name
    pub character_name: Option<String>,

    /// Current room ID
    pub room_id: Option<String>,

    /// Current room name
    pub room_name: Option<String>,

    /// Available exits from current room
    pub exits: Vec<String>,

    /// Game server time from last prompt (Unix timestamp)
    /// This is the authoritative time source for roundtime/casttime comparisons
    pub game_time: i64,

    /// When `game_time` was last updated, on the LOCAL clock. Prompts only
    /// arrive with traffic, so during silence `game_time` stands still - and
    /// a roundtime measured against it never counts down, freezing travel
    /// until any line lands (a `look` used to unstick it). Extrapolating from
    /// this stamp keeps RT flowing through quiet stretches.
    pub game_time_received: Option<std::time::Instant>,

    /// Roundtime end timestamp (Unix time from game server)
    pub roundtime_end: Option<i64>,

    /// Casttime end timestamp (Unix time from game server)
    pub casttime_end: Option<i64>,

    /// Current spell being prepared
    pub spell: Option<String>,

    /// Active game streams (tags like "inv", "assess", etc.)
    pub active_streams: HashMap<String, bool>,

    /// Player status indicators
    pub status: StatusInfo,

    /// Vitals (health, mana, etc.)
    pub vitals: Vitals,

    /// Latest complete `inv` stream snapshot. Styled lines retain the game's
    /// authoritative colors and link metadata for every frontend.
    pub inventory: Vec<crate::data::widget::StyledLine>,
    /// True after at least one complete `inv` push/pop snapshot, including an
    /// authoritative empty snapshot. This distinguishes "empty" from "not
    /// received yet" for remote presentations.
    pub inventory_received: bool,

    /// Current left hand item
    pub left_hand: Option<String>,

    /// Current right hand item
    pub right_hand: Option<String>,

    /// Active effects/buffs
    pub active_effects: Vec<String>,

    /// Active effects by category ("ActiveSpells", "Buffs", "Debuffs",
    /// "Cooldowns"), stored unconditionally so remote clients (and any
    /// window added mid-session) see them even when the local layout has
    /// no effects windows. The per-window copies in ui_state remain the
    /// widgets' source of truth.
    pub effects: HashMap<String, crate::data::ActiveEffectsContent>,

    /// Quest objectives (Saga quest panel feed). Lives here rather than on a
    /// window so remote/web clients and windows added mid-session see it.
    pub objectives: crate::data::ObjectivesContent,

    /// Compass directions
    pub compass_dirs: Vec<String>,

    /// Body-part injuries: id -> level (1-3 wounds, 4-6 scars). Cleared
    /// parts are removed. Owned here (not only by the injury-doll widget)
    /// so headless/remote clients get injuries without a doll window.
    pub injuries: HashMap<String, u8>,

    /// Last prompt text (for command echoes)
    pub last_prompt: String,

    /// Target list from dDBTarget dropdown (for direct-connect users)
    pub target_list: TargetListState,

    /// Creatures currently in room (parsed from room objs component)
    /// Primary source for targets widget
    pub room_creatures: Vec<Creature>,
    /// Bumped whenever room_creatures is rewritten; sync skips unchanged rebuilds
    pub room_creatures_generation: u64,
    /// Message-derived per-creature effects (bleeding and friends), keyed
    /// by exist id. Authoritative store with expiry — the names are merged
    /// into each creature's open-vocabulary statuses by
    /// `tick_creature_effects`, so everything downstream (crtr_status
    /// conditions, badges, the web wire) sees them for free. Feed-derived
    /// crtrStatus flags never live here; only lossy messaging does, which
    /// is why every entry expires.
    pub creature_effects: std::collections::HashMap<String, Vec<ActiveCreatureEffect>>,
    /// Every effect name the store has ever applied (lowercase). The merge
    /// may remove exactly these from a creature's statuses when the effect
    /// ends — feed statuses are never touched, without consulting the
    /// effect-list table (which can be swapped out from under us).
    pub(crate) derived_status_names: std::collections::HashSet<String>,

    /// Objects (non-creatures) in room (parsed from room objs component)
    /// Primary source for items widget
    pub room_objects: Vec<RoomObject>,
    /// Bumped whenever room_objects is rewritten
    pub room_objects_generation: u64,

    /// Players currently in room (parsed from room players component)
    pub room_players: Vec<Player>,
    /// Bumped whenever room_players is rewritten
    pub room_players_generation: u64,

    /// Room description prose as styled lines (segments carry color and
    /// clickable scenery links, exactly like the room widget's buffer).
    /// Owned here so headless/remote clients get the room "look" — with its
    /// tappable scenery — without a room window.
    pub room_description: Vec<crate::data::widget::StyledLine>,
    /// Bumped whenever room_description is rewritten
    pub room_description_generation: u64,

    /// Pool image name for the game's current room picture, from the
    /// `<resource picture='N'/>` feed. `None` when the room has no picture
    /// (`picture='0'`, the near-universal case) or when the user has no art
    /// installed for that id. The wire carries only the number, so this is
    /// the id stringified — art lives at `images/inline/<id>.png`.
    ///
    /// NOT room-window state: `<resource>` arrives in the STORY stream, and
    /// the room window's art comes from the `sprite` component instead. This
    /// is parsed and tracked now so the story window can render it once that
    /// path supports inline images; nothing reads it yet.
    pub story_picture: Option<String>,

    /// Spellbook (the "Spells" stream) as styled lines: segments keep spell
    /// coloring and links. Owned here so remote clients get the full
    /// active-spell list without a Spells window; the local Spells widgets
    /// keep their own copy.
    pub spellbook: Vec<crate::data::widget::StyledLine>,
    /// Bumped whenever spellbook is rewritten
    pub spellbook_generation: u64,

    /// Room metadata codes from the `<roommeta>` tag
    pub room_meta: RoomMetaState,

    /// Latest structured inventory snapshot (`<inventoryManager>`, extended
    /// feed). None until the client has sent `_inventory manager <token>`
    /// and received an answer.
    pub managed_inventory: Option<ManagedInventoryState>,

    /// Count of `<pulse .../>` announcements received (extended feed). A
    /// pulse fires every minute ±15s: each one absorbs field exp (when any
    /// is pooled), and every other pulse is also a mana pulse. Serves as
    /// the pulse clock's generation counter.
    pub pulse_count: u64,
    /// Whether the NEXT pulse restores mana — the wire's `mana` attribute
    /// declares the alternation up front (Saga semantics), nothing inferred.
    pub next_pulse_mana: bool,
    /// Last user-requested item detail (`.viewitem` / inspector click):
    /// the parsed `<inventoryViewItem>` result sections. Generation bumps
    /// on every answer for display change detection.
    pub viewed_item: Option<ViewedItem>,

    /// Active world events (`<worldEvent>`, extended feed), pruned of
    /// expired entries whenever a new one arrives.
    pub world_events: Vec<WorldEventState>,
    /// Pantheon meter (`<PantheonStatus value>`, extended feed)
    pub pantheon_value: Option<u32>,

    /// Earliest arrival of the next pulse (server-clock epoch seconds,
    /// `now + min` from the last `<pulse>`); None before the first pulse.
    /// Drives the "pulse" countdown.
    pub pulse_next_earliest: Option<i64>,
    /// Latest arrival of the next pulse (server-clock epoch seconds,
    /// `now + max` from the last `<pulse>`).
    pub pulse_next_latest: Option<i64>,

    /// Unified game-object registry: items (containers/worn/hands/at-feet/
    /// ground), creatures, players. The single source for game objects;
    /// see `core::game_objects`.
    pub objects: crate::core::game_objects::GameObjects,

    /// Edge-triggered move-feedback events (nav arrivals, hands-full, closed
    /// door, …) awaiting the walk executor. The parser pushes on each matching
    /// game line; `tick_travel` drains this once per tick so each event fires
    /// exactly once. See `core::move_feedback` and §09/§12 of the go2 plan.
    pub move_feedback: std::collections::VecDeque<(u64, crate::core::move_feedback::MoveFeedback)>,
    /// The message processor's flushed-line count at the last prompt - the
    /// executor stamps its sends with this so stale failure lines (belonging
    /// to an already-superseded move) can be told from fresh ones.
    pub game_line_no: u64,

    /// Recent raw game lines for scripted-edge `Await` steps, newest last.
    ///
    /// `move_feedback` is a fixed enum of pre-classified recovery events; an
    /// `Await` needs the TEXT, because the pattern comes from mapdb data we
    /// can't enumerate ahead of time (a ferry's arrival line, a lever's
    /// response, a captured group interpolated into a later command).
    ///
    /// Unlike `move_feedback` this is NOT drained by the consumer — an await
    /// arms mid-tick and must see lines that arrived before it started, and
    /// several steps may match the same line. It is a bounded ring instead:
    /// pushed on every line, capped at `RAW_LINE_RING`, with each entry
    /// carrying the sequence number an await compares against so it only
    /// matches lines newer than its own arming point.
    pub recent_lines: std::collections::VecDeque<(u64, String)>,
    /// Monotonic counter stamped onto `recent_lines`; also the "now" an await
    /// records when it arms. Never reset.
    pub line_seq: u64,

    /// Character state parsed from the feed (society status/rank, profession,
    /// CHE/House, citizenship) — gates seeking, guild, and locker travel.
    /// Populated by SOCIETY/INFO/PROFILE/CITIZENSHIP output. See
    /// Spell number -> display name, remembered from every ActiveSpells/
    /// Buffs feed entry this session. Lets the missing-spells window name
    /// effects the static spell table doesn't know once they drop.
    pub spell_names_seen: std::collections::HashMap<u16, String>,
    /// `core::character_state`.
    pub character: crate::core::character_state::CharacterState,

    /// Silver on hand, parsed from `wealth`/`wealth quiet` output. `None`
    /// until first seen. Drives go2's silver-funding for paid travel.
    pub silver: Option<u64>,
    /// Flushed-line number of the last wealth reading. The funding phases
    /// only trust a reading NEWER than their `wealth quiet` probe - deciding
    /// on the cached value walked a broke character into a paid crossing
    /// (the live 'you have 2000 - funded' on a freshly emptied purse).
    pub silver_line_no: u64,
    /// Count of `<nav>` tags seen - Lich's `$room_count`. The command gate
    /// records it at each send; "a nav arrived since my send" is the
    /// movement test, independent of how fast the room id resolves.
    pub nav_count: u64,

    /// Chronomage day-pass expiry cache, learned by `look`ing at passes (Lich's
    /// `$mapdb_day_passes` + `mapdb_day_pass_monitor`). Gates day-pass travel.
    pub day_passes: crate::core::day_pass::DayPassCache,

    /// DragonRealms experience/skill component state
    pub dr_experience: DRExperienceState,

    /// GS4 experience dialog state (from expr dialog)
    pub gs4_experience: GS4ExperienceState,

    /// Encumbrance dialog state (from encum dialog)
    pub encumbrance: EncumbranceState,

    /// Combat stance (from the stance dialog's pbarStance)
    pub stance: StanceState,

    /// Group roster, reconstructed from the game's group messaging. The
    /// feed's only structured group signal is the JOINED indicator, a bare
    /// flag with no members, so the roster comes from parsed text.
    pub group: crate::core::group::GroupState,

    /// Betrayer panel state (blood points + items) - GS4 only
    pub betrayer: BetrayerState,

    /// MiniVitals state (from minivitals dialog) - GS4 only
    pub minivitals: MiniVitalsState,

    /// Bounty state - stores raw text and parsed compact lines
    /// Buffered so bounty windows added later can immediately show data
    pub bounty: BountyState,

    /// Society state - stores society stream text for reload
    /// Buffered so society windows show data on reload
    pub society: SocietyState,

    /// Estimated lag between system time and game server time (in milliseconds)
    /// Positive = system clock ahead of game, Negative = game ahead of system
    /// Recalculated periodically (every LAG_CHECK_INTERVAL_SECS)
    pub estimated_lag_ms: Option<i64>,

    /// Game time when we last calculated lag (for throttling)
    last_lag_check_time: i64,

    /// Queued sounds from highlight processing
    pub sound_queue: SoundQueue,
}

/// Player status information.
///
/// Backed by a general id -> bool map rather than fixed fields, because the
/// game sends indicators we do not know about ahead of time (POISONED,
/// DISEASED, and whatever Simu adds next). Ids are normalized to lowercase on
/// the way in, so `"IconSTUNNED"`, `"STUNNED"` and `"stunned"` are one key.
///
/// Wire compatibility: this serializes as a flat lowercase-keyed object,
/// byte-identical to the struct it replaced for every id the phone client
/// reads (`app.js` looks up `d["stunned"]` and friends). Unknown extra keys
/// are ignored by that client, and absent keys read falsy, so adding ids is
/// backward compatible in both directions.
///
/// The typed accessors below are the supported read path -- prefer
/// `status.stunned()` over `status.get("stunned")` so a typo is a compile
/// error rather than a silent `false`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatusInfo {
    /// Lowercase indicator id -> active. Absent means "never reported",
    /// which reads as inactive.
    flags: BTreeMap<String, bool>,
}

impl StatusInfo {
    /// Normalize an id to its map key. Strips the `Icon` prefix the game uses
    /// on the wire so callers may pass either form.
    fn key(id: &str) -> String {
        let bare = id.strip_prefix("Icon").unwrap_or(id);
        bare.to_ascii_lowercase()
    }

    /// Set an indicator. Returns true if the value changed -- callers use this
    /// to avoid emitting no-op deltas.
    pub fn set(&mut self, id: &str, active: bool) -> bool {
        self.flags.insert(Self::key(id), active) != Some(active)
    }

    /// Read an indicator. Unknown ids read `false`, matching "the game never
    /// told us, so it is not happening".
    pub fn get(&self, id: &str) -> bool {
        self.flags.get(&Self::key(id)).copied().unwrap_or(false)
    }

    /// Distinguishes "reported inactive" from "never reported". Conditions do
    /// not need this, but a multi-account display does: an unreported
    /// indicator should render as unknown rather than as a confident "no".
    pub fn is_known(&self, id: &str) -> bool {
        self.flags.contains_key(&Self::key(id))
    }

    /// Every id the game has reported, with its current value.
    pub fn iter(&self) -> impl Iterator<Item = (&str, bool)> {
        self.flags.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

/// Typed accessors for the indicators the client reasons about by name.
/// Generated so the list stays in one place; `StatusInfo::set` still accepts
/// any id the game invents.
macro_rules! status_accessors {
    ($($(#[$m:meta])* $name:ident),* $(,)?) => {
        impl StatusInfo {
            $(
                $(#[$m])*
                pub fn $name(&self) -> bool {
                    self.get(stringify!($name))
                }
            )*
        }
    };
}

status_accessors! {
    standing,
    kneeling,
    sitting,
    prone,
    stunned,
    bleeding,
    hidden,
    invisible,
    webbed,
    /// True while grouped. Before the map refactor this was never written --
    /// the parser had no arm for it -- so any code reading it saw a permanent
    /// `false`. It now reflects `IconJOINED`.
    joined,
    dead,
    poisoned,
    diseased,
}

/// Player vitals (percentages only)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vitals {
    pub health: u8,
    pub mana: u8,
    pub stamina: u8,
    pub spirit: u8,
}

/// A single vital entry with full data for MiniVitals widget
#[derive(Clone, Debug, Default)]
pub struct VitalEntry {
    pub value: u32,
    pub max: u32,
    pub text: String, // e.g., "health 226/226"
}

/// MiniVitals state - stores full vital data for GS4 horizontal bar display
#[derive(Clone, Debug, Default)]
pub struct MiniVitalsState {
    pub health: VitalEntry,
    pub mana: VitalEntry,
    pub stamina: VitalEntry,
    pub spirit: VitalEntry,
    pub generation: u64,
}

impl MiniVitalsState {
    /// Update a vital entry. Returns true if changed.
    /// Note: "concentration" (DR) maps to the mana slot
    pub fn update_vital(&mut self, id: &str, value: u32, max: u32, text: String) -> bool {
        let entry = match id {
            "health" => &mut self.health,
            "mana" | "concentration" => &mut self.mana, // DR uses concentration
            "stamina" => &mut self.stamina,
            "spirit" => &mut self.spirit,
            _ => return false,
        };

        if entry.value != value || entry.max != max || entry.text != text {
            entry.value = value;
            entry.max = max;
            entry.text = text;
            self.generation += 1;
            true
        } else {
            false
        }
    }
}

/// Bounty state - stores raw bounty text and parsed compact lines
/// This allows bounty windows added later to immediately show current bounty
#[derive(Clone, Debug, Default)]
pub struct BountyState {
    /// The raw bounty text line as received from the game
    pub raw_text: String,
    /// Parsed compact bounty lines (task, creature, location, etc.)
    pub compact_lines: Vec<String>,
    /// Generation counter for change detection
    pub generation: u64,
}

impl BountyState {
    /// Update bounty state with new text. Always parses both raw and compact.
    pub fn update(&mut self, raw_text: String, compact_lines: Vec<String>) {
        self.raw_text = raw_text;
        self.compact_lines = compact_lines;
        self.generation += 1;
    }

    /// Check if there's any bounty data
    pub fn has_data(&self) -> bool {
        !self.raw_text.is_empty()
    }

    /// Clear bounty data (e.g., when bounty is completed)
    pub fn clear(&mut self) {
        self.raw_text.clear();
        self.compact_lines.clear();
        self.generation += 1;
    }
}

/// Society state - stores society stream text for reload
/// Similar to bounty caching but without parsing (just stores lines)
#[derive(Clone, Debug, Default)]
pub struct SocietyState {
    /// Lines from society stream
    pub lines: Vec<String>,
    /// Generation counter for change detection
    pub generation: u64,
}

impl SocietyState {
    /// Update society state with new lines
    pub fn update(&mut self, lines: Vec<String>) {
        self.lines = lines;
        self.generation += 1;
    }

    /// Add a single line
    pub fn add_line(&mut self, line: String) {
        self.lines.push(line);
        self.generation += 1;
    }

    /// Check if there's any society data
    pub fn has_data(&self) -> bool {
        !self.lines.is_empty()
    }

    /// Clear society data
    pub fn clear(&mut self) {
        self.lines.clear();
        self.generation += 1;
    }
}

/// Target list state from dDBTarget dropdown (for direct-connect users)
/// Creature list is now in GameState.room_creatures (parsed from room objs)
#[derive(Clone, Debug, Default)]
pub struct TargetListState {
    /// Currently selected target ID (e.g., "#146101714")
    pub current_target: String,
    /// Targetable creature IDs from dDBTarget content_value
    /// Used to filter room_creatures - only show creatures in both lists
    pub target_ids: Vec<String>,
    /// Bumped whenever the target list changes; sync skips unchanged rebuilds
    pub generation: u64,
}

/// A creature in the target list
#[derive(Clone, Debug)]
pub struct Creature {
    /// Creature display name
    pub name: String,
    /// Creature noun (short identifier, e.g., "hog" from "muddy hog")
    pub noun: Option<String>,
    /// Creature ID (e.g., "#146101714")
    pub id: String,
    /// Creature status parsed from the "(stunned)" text after the bold name.
    /// Legacy single-status fallback; `flags` is authoritative when present.
    pub status: Option<String>,
    /// Structured status snapshot from the `<crtrStatus>` XML tag.
    /// None when the feed hasn't sent one for this creature.
    pub flags: Option<CreatureFlags>,
}

impl Creature {
    /// Statuses to display, most reliable source first: the structured
    /// `<crtrStatus>` snapshot when present, else the legacy text-parsed
    /// status as a single entry.
    pub fn display_statuses(&self) -> Vec<String> {
        if let Some(flags) = &self.flags {
            let mut out = Vec::new();
            if flags.dead {
                out.push("dead".to_string());
            }
            // Extended feed extras lead: per-creature health percentage,
            // then the condition string, then the status flags.
            if let Some(pct) = flags.health_percent() {
                out.push(format!("{pct}%"));
            }
            if let Some(cond) = &flags.condition {
                out.push(cond.clone());
            }
            out.extend(flags.statuses.iter().cloned());
            out
        } else {
            self.status.clone().into_iter().collect()
        }
    }

    /// Appendage "creatures" — limbs that erupt from the ground and attack,
    /// as summoned by Grasp of the Dead (709) and similar. They are
    /// targetable but cannot be damaged, so target lists filter them out
    /// Lich-style to keep the list actionable. The kraken tentacles are real
    /// creatures despite matching the noun pattern.
    pub fn is_body_part(&self) -> bool {
        static BODY_PART_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let regex = BODY_PART_REGEX.get_or_init(|| {
            regex::Regex::new(
                r"(?i)^(?:arm|appendage|claw|limb|pincer|tentacle)s?$|^(?:palpus|palpi)$",
            )
            .unwrap()
        });
        static KRAKEN_EXCEPTION: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let kraken = KRAKEN_EXCEPTION.get_or_init(|| {
            // Real creatures whose noun looks like an appendage; Lich excepts
            // all four kraken-tentacle variants (gameobj.rb / creature.rb).
            regex::Regex::new(r"(?i)(?:amaranthine|ghostly|grizzled|ancient) kraken tentacle")
                .unwrap()
        });
        self.noun
            .as_deref()
            .is_some_and(|noun| regex.is_match(noun) && !kraken.is_match(&self.name))
    }

    /// Whether this creature should be shown as a target — Lich's
    /// `valid_target?` (dead/animated/appendage exclusions), plus the
    /// user-configured `excluded_nouns`. Hostility is a *separate* gate and
    /// is deliberately not checked here. The single source of truth for the
    /// TUI, GUI, and web targets lists; keep those callers routed through it.
    pub fn is_valid_target(&self, excluded_nouns: &[String]) -> bool {
        // Dead/gone: structured <crtrStatus> flag when present, legacy text
        // status otherwise.
        if self.is_dead() {
            return false;
        }
        // Animated decoys, except "animated slush".
        let name_lower = self.name.to_ascii_lowercase();
        if name_lower.starts_with("animated") && !name_lower.starts_with("animated slush") {
            return false;
        }
        // Severed appendages (arm, tentacle, …), except the kraken variants.
        if self.is_body_part() {
            return false;
        }
        // User-configured noun exclusions (case-insensitive).
        if let Some(noun) = self.noun.as_deref() {
            let noun_lower = noun.to_ascii_lowercase();
            if excluded_nouns
                .iter()
                .any(|e| e.eq_ignore_ascii_case(&noun_lower))
            {
                return false;
            }
        }
        true
    }

    /// Dead by the structured flag, or by the legacy text status
    /// ("dead"/"gone") when no snapshot has been seen.
    pub fn is_dead(&self) -> bool {
        if let Some(flags) = &self.flags {
            return flags.dead;
        }
        self.status.as_deref().is_some_and(|s| {
            let lower = s.to_lowercase();
            lower.contains("dead") || lower.contains("gone")
        })
    }

    /// Fingerprint for widget change detection: id plus everything that
    /// affects how the entry renders.
    pub fn cache_key(&self) -> String {
        let boss_bits = self.flags.as_ref().map_or(0u8, |f| {
            (f.ascension_boss as u8) | ((f.mini_boss as u8) << 1) | ((f.challenging as u8) << 2)
        });
        format!(
            "{}:{}:{}",
            self.id,
            self.display_statuses().join("+"),
            boss_bits
        )
    }
}

/// One live message-derived effect on one creature. `expires_at` is server
/// time (the countdown convention); a start match re-arms it.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveCreatureEffect {
    pub name: String,
    /// Rank 1-3, from the matched start message — reuses wound-rank art.
    pub severity: u8,
    pub expires_at: i64,
}

/// Structured creature status from the `<crtrStatus>` XML tag (a full
/// snapshot: absent or "0" flags mean inactive, not unknown).
///
/// Two vocabularies, mirroring lich-5's split: transient combat statuses
/// (collected into `statuses` under the same canonical names the legacy
/// text parse produces) and classification flags (dedicated bools).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreatureFlags {
    /// Active transient statuses in feed order ("stunned", "prone", ...).
    /// Open vocabulary: any unrecognized `="1"` attribute lands here too
    /// (Saga's rule), so new server effects surface without a code change.
    pub statuses: Vec<String>,
    pub hostile: bool,
    pub disengaged: bool,
    pub dead: bool,
    pub sympathetic: bool,
    pub ascended: bool,
    pub inferior: bool,
    pub ascension_boss: bool,
    pub mini_boss: bool,
    pub challenging: bool,
    pub rider: bool,
    pub mount: bool,
    /// Current hit points, when the extended feed reports them
    pub health: Option<u32>,
    /// Maximum hit points
    pub max_health: Option<u32>,
    /// True when the reporter flagged max HP as an estimate (`hpest="1"`,
    /// no bestiary template behind it) — bars render dimmed.
    pub hp_estimated: bool,
    /// Per-part wound ranks from the extended feed's `injuries` attribute
    /// (`"head:2,rightLeg:3"`). Part names are canonicalized to the doll
    /// vocabulary at parse; ranks are CritRanks R1-R3 (rank 3 on a limb is
    /// a severance — same vocabulary, no extra states).
    pub injuries: Vec<(String, u8)>,
    /// Free-text condition string, appended to the effect list by Saga
    pub condition: Option<String>,
}

/// Maps `<crtrStatus>` transient-status attribute names to the canonical
/// status names used by the text parse and the status_abbrev config
/// (e.g. the feed says "immobile", everything else says "immobilized").
pub const CRTR_STATUS_FLAGS: [(&str, &str); 12] = [
    ("immobile", "immobilized"),
    ("webbed", "webbed"),
    ("sleeping", "sleeping"),
    ("disoriented", "disoriented"),
    ("stunned", "stunned"),
    ("rooted", "rooted"),
    ("calmed", "calmed"),
    ("kneeling", "kneeling"),
    ("prone", "prone"),
    ("sitting", "sitting"),
    ("flying", "flying"),
    ("hovering", "hovering"),
];

impl CreatureFlags {
    /// Builds a snapshot from raw `<crtrStatus>` attributes (excluding
    /// `exist`). Attribute values are "1"/"0"; unknown names are ignored so
    /// new server flags degrade gracefully.
    pub fn from_xml_attrs<'a>(attrs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut flags = Self::default();
        for (name, value) in attrs {
            // Numeric/text extras ride alongside the "1" flags (extended
            // feed): per-creature hit points and a condition string.
            match name {
                "health" => {
                    flags.health = value.trim().parse().ok();
                    continue;
                }
                "maxhealth" => {
                    flags.max_health = value.trim().parse().ok();
                    continue;
                }
                "hpest" => {
                    flags.hp_estimated = value.trim() == "1";
                    continue;
                }
                "injuries" => {
                    // "head:2,rightLeg:3" — canonicalize parts (feet fold
                    // into legs, so keep the worse rank on collision),
                    // clamp ranks to the R1-R3 vocabulary, drop unknowns.
                    for entry in value.split(',') {
                        let Some((part, rank)) = entry.trim().split_once(':') else {
                            continue;
                        };
                        let Some(canonical) =
                            crate::core::creature_cards::canonical_part(part.trim())
                        else {
                            continue;
                        };
                        let Ok(rank) = rank.trim().parse::<u8>() else {
                            continue;
                        };
                        let rank = rank.min(3);
                        if rank == 0 {
                            continue;
                        }
                        match flags.injuries.iter_mut().find(|(p, _)| p == canonical) {
                            Some((_, existing)) => *existing = (*existing).max(rank),
                            None => flags.injuries.push((canonical.to_string(), rank)),
                        }
                    }
                    continue;
                }
                "condition" => {
                    let v = value.trim();
                    if !v.is_empty() {
                        flags.condition = Some(v.to_string());
                    }
                    continue;
                }
                _ => {}
            }
            let active = value == "1";
            if !active {
                continue;
            }
            if let Some((_, canonical)) = CRTR_STATUS_FLAGS.iter().find(|(xml, _)| *xml == name) {
                flags.statuses.push(canonical.to_string());
                continue;
            }
            match name {
                "hostile" => flags.hostile = true,
                "disengaged" => flags.disengaged = true,
                "dead" => flags.dead = true,
                "sympathetic" => flags.sympathetic = true,
                "ascended" => flags.ascended = true,
                "inferior" => flags.inferior = true,
                "AscensionBoss" => flags.ascension_boss = true,
                "MiniBoss" => flags.mini_boss = true,
                "challenging" => flags.challenging = true,
                "rider" => flags.rider = true,
                "mount" => flags.mount = true,
                // Open vocabulary (Saga's rule): any other ="1" attribute
                // is an effect name - new server effects surface without a
                // client release. Logged so the creature-cards flag census
                // can enumerate what the server actually sends beyond the
                // canonical table (grep logs for "crtrStatus open-vocab").
                other => {
                    tracing::debug!("crtrStatus open-vocab flag: {other}");
                    flags.statuses.push(other.to_string());
                }
            }
        }
        flags
    }

    /// Health percentage 0..=100 when both numbers are known and sane.
    pub fn health_percent(&self) -> Option<u32> {
        let (h, m) = (self.health?, self.max_health?);
        if m == 0 {
            return None;
        }
        Some(((h * 100) / m).min(100))
    }

    /// Boss-tier creature (AscensionBoss or MiniBoss).
    pub fn is_boss(&self) -> bool {
        self.ascension_boss || self.mini_boss
    }

    /// Whether a named status/classification flag is active, for
    /// `Condition::CrtrStatus`. Accepts the canonical status names
    /// ("immobilized"), the feed's raw spellings ("immobile", "MiniBoss"),
    /// and any open-vocabulary status the server sent, case-insensitively.
    /// Unknown names read false — same fail-closed shape as indicators.
    pub fn has_flag(&self, name: &str) -> bool {
        // Feed spelling -> canonical, so authors can use either.
        let name = CRTR_STATUS_FLAGS
            .iter()
            .find(|(xml, _)| xml.eq_ignore_ascii_case(name))
            .map_or(name, |(_, canonical)| canonical);
        match name.to_ascii_lowercase().as_str() {
            "hostile" => self.hostile,
            "disengaged" => self.disengaged,
            "dead" => self.dead,
            "sympathetic" => self.sympathetic,
            "ascended" => self.ascended,
            "inferior" => self.inferior,
            "ascensionboss" => self.ascension_boss,
            "miniboss" => self.mini_boss,
            "challenging" => self.challenging,
            "rider" => self.rider,
            "mount" => self.mount,
            _ => self.statuses.iter().any(|s| s.eq_ignore_ascii_case(name)),
        }
    }
}

#[cfg(test)]
mod creature_effect_tests {
    use super::*;

    fn state_with_creature(id: &str, feed_statuses: &[&str]) -> GameState {
        let mut gs = GameState::new();
        gs.room_creatures.push(Creature {
            name: format!("a test {id}"),
            noun: Some("test".into()),
            id: id.to_string(),
            status: None,
            flags: Some(CreatureFlags {
                statuses: feed_statuses.iter().map(|s| s.to_string()).collect(),
                hostile: true,
                ..Default::default()
            }),
        });
        gs
    }

    fn statuses(gs: &GameState, id: &str) -> Vec<String> {
        gs.room_creatures
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.flags.as_ref())
            .map(|f| f.statuses.clone())
            .unwrap_or_default()
    }

    /// The full lifecycle: start merges the status (crtr_status conditions
    /// and badges see it for free), refresh re-arms and re-ranks, end
    /// removes it — feed statuses untouched throughout.
    #[test]
    fn start_refresh_end_lifecycle() {
        let mut gs = state_with_creature("607736", &["stunned"]);
        let g0 = gs.room_creatures_generation;

        gs.apply_creature_effect_event("607736", "bleeding", Some(2), 15, 1000);
        assert_eq!(statuses(&gs, "607736"), vec!["stunned", "bleeding"]);
        assert_eq!(gs.creature_effect_severity("607736", "bleeding"), Some(2));
        assert!(gs.room_creatures_generation > g0);

        // Refresh: severity climbs, timer re-arms, no stacking.
        gs.apply_creature_effect_event("607736", "bleeding", Some(3), 15, 1005);
        assert_eq!(gs.creature_effects["607736"].len(), 1);
        assert_eq!(gs.creature_effect_severity("607736", "bleeding"), Some(3));
        assert_eq!(gs.creature_effects["607736"][0].expires_at, 1020);

        // End message: gone, feed status stays.
        gs.apply_creature_effect_event("607736", "bleeding", None, 15, 1010);
        assert_eq!(statuses(&gs, "607736"), vec!["stunned"]);
        assert!(gs.creature_effects.is_empty());
        // The creature's own flag is not removable by the derived store.
        assert!(statuses(&gs, "607736").contains(&"stunned".to_string()));
    }

    /// The timeout safety net: a missed end message can never leave a
    /// stale layer.
    #[test]
    fn timeout_expires_unrefreshed_effects() {
        let mut gs = state_with_creature("1", &[]);
        gs.apply_creature_effect_event("1", "bleeding", Some(1), 15, 1000);
        assert_eq!(statuses(&gs, "1"), vec!["bleeding"]);
        gs.tick_creature_effects(1010); // not yet
        assert_eq!(statuses(&gs, "1"), vec!["bleeding"]);
        let g = gs.room_creatures_generation;
        gs.tick_creature_effects(1016); // past expiry
        assert!(statuses(&gs, "1").is_empty());
        assert!(gs.creature_effects.is_empty());
        assert!(gs.room_creatures_generation > g);
        // Settled state: further ticks change nothing.
        let g = gs.room_creatures_generation;
        gs.tick_creature_effects(1017);
        assert_eq!(gs.room_creatures_generation, g);
    }

    /// A room-objs rebuild replaces flags wholesale; the next tick repairs
    /// the derived status without needing a new message.
    #[test]
    fn derived_status_survives_roster_rebuild() {
        let mut gs = state_with_creature("1", &["stunned"]);
        gs.apply_creature_effect_event("1", "bleeding", Some(2), 60, 1000);
        // Rebuild: fresh flags from a new <crtrStatus>, derived name gone.
        gs.room_creatures[0].flags = Some(CreatureFlags {
            statuses: vec!["webbed".to_string()],
            hostile: true,
            ..Default::default()
        });
        gs.tick_creature_effects(1010);
        assert_eq!(statuses(&gs, "1"), vec!["webbed", "bleeding"]);
    }

    /// Derived effects can arrive before any <crtrStatus> snapshot; the
    /// badge shows on a default snapshot until the feed profiles it.
    #[test]
    fn effect_on_flagless_creature_creates_snapshot() {
        let mut gs = state_with_creature("1", &[]);
        gs.room_creatures[0].flags = None;
        gs.apply_creature_effect_event("1", "bleeding", Some(1), 15, 1000);
        assert_eq!(statuses(&gs, "1"), vec!["bleeding"]);
        // And the crtr_status condition path sees it.
        assert!(gs.room_creatures[0]
            .flags
            .as_ref()
            .unwrap()
            .has_flag("bleeding"));
    }
}

/// A player in the room (from room players component)
#[derive(Clone, Debug)]
pub struct Player {
    /// Player display name
    pub name: String,
    /// Player ID from exist attribute (e.g., "-10154507")
    pub id: String,
    /// Primary status (prepended, e.g., "stunned" from "a stunned Player")
    pub primary_status: Option<String>,
    /// Secondary status (appended, e.g., "prone" from "Player (prone)" or
    /// "prone" from the verbose clause "Player who is lying down")
    pub secondary_status: Option<String>,
    /// True when the room roster shows the player as a corpse, i.e. the
    /// segment is prefixed with "the body of " before the link. Drives the
    /// `[ded]` status tag and the dim `dead_color` styling in both frontends.
    pub dead: bool,
}

/// A room object (non-creature item) from room objs component
/// These are items on the ground that can be picked up, examined, etc.
#[derive(Clone, Debug)]
pub struct RoomObject {
    /// Object display name (e.g., "a silver ring")
    pub name: String,
    /// Object noun (e.g., "ring")
    pub noun: Option<String>,
    /// Object ID from exist attribute (e.g., "123456789")
    pub id: String,
}

impl TargetListState {
    /// Clear the current target
    pub fn clear(&mut self) {
        self.current_target.clear();
    }
}

/// DragonRealms experience/skill component tracking state
/// Stores values from `<component id='exp XXX'>` tags, ordered by `<compDef>` at login
#[derive(Clone, Debug, Default)]
pub struct DRExperienceState {
    /// Ordered list of field names (from compDef tags at login)
    /// e.g., ["Stealth", "Locksmithing", "Brawling", "tdp", ...]
    pub field_order: Vec<String>,

    /// Current values for each field (field_name -> value string)
    /// Values are stored as-is from the XML, frontend handles parsing/display
    pub values: HashMap<String, String>,

    /// Generation counter for change detection by frontend
    pub generation: u64,
}

impl DRExperienceState {
    /// Register a field from compDef (establishes order)
    pub fn register_field(&mut self, field_name: String) {
        if !self.field_order.contains(&field_name) {
            self.field_order.push(field_name);
        }
    }

    /// Update a field value, returns true if value changed
    pub fn update_field(&mut self, field_name: &str, value: String) -> bool {
        // Only update if value actually changed
        if let Some(existing) = self.values.get(field_name) {
            if existing == &value {
                return false;
            }
        }
        self.values.insert(field_name.to_string(), value);
        self.generation += 1;
        true
    }

    /// Get fields with values in order (for display)
    pub fn fields_with_values(&self) -> Vec<(&str, &str)> {
        self.field_order
            .iter()
            .filter_map(|name| {
                self.values
                    .get(name)
                    .filter(|v| !v.is_empty())
                    .map(|v| (name.as_str(), v.as_str()))
            })
            .collect()
    }

    /// Clear all values (on disconnect/login)
    pub fn clear(&mut self) {
        self.values.clear();
        self.generation += 1;
    }
}

/// Room metadata from the self-closing `<roommeta .../>` tag - numeric
/// codes describing the current room. The game only sends the attributes
/// it knows for a room, so fields update independently and `None` means
/// "never received", not zero (mirrors lich-5's xmlparser).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomMetaState {
    pub climate: Option<u32>,
    pub terrain: Option<u32>,
    pub weather: Option<u32>,
    pub bonfire: Option<u32>,
    pub inside: Option<u32>,
    pub water: Option<u32>,
    pub sanctuary: Option<u32>,
    pub realm: Option<u32>,
    /// Generation counter for change detection
    pub generation: u64,
}

impl RoomMetaState {
    /// Applies raw `<roommeta>` attributes. Only fields present in the tag
    /// are updated; unknown names are ignored so new server fields degrade
    /// gracefully. Returns true if anything changed.
    pub fn update_from_attrs<'a>(
        &mut self,
        attrs: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> bool {
        let mut changed = false;
        for (name, value) in attrs {
            let Ok(code) = value.parse::<u32>() else {
                continue;
            };
            let field = match name {
                "climate" => &mut self.climate,
                "terrain" => &mut self.terrain,
                "weather" => &mut self.weather,
                "bonfire" => &mut self.bonfire,
                "inside" => &mut self.inside,
                "water" => &mut self.water,
                "sanctuary" => &mut self.sanctuary,
                "realm" => &mut self.realm,
                _ => {
                    tracing::debug!("Unknown roommeta field: {}", name);
                    continue;
                }
            };
            if *field != Some(code) {
                *field = Some(code);
                changed = true;
            }
        }
        if changed {
            self.generation += 1;
        }
        changed
    }
}

/// One item from an `<inventoryManager>` snapshot, mapped from the raw
/// `<i .../>` attributes. Lenient where Saga's own validator is strict:
/// a malformed field degrades to a default instead of dropping the item.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ManagedInventoryItem {
    /// Exist id
    pub id: String,
    /// worn/righthand/lefthand/atfeet/reserved (parent = player),
    /// in/on/behind/underneath (parent = a container's exist id), or "room"
    pub relation: String,
    /// "player", "room", or the parent container's exist id
    pub parent: String,
    /// Full display name (article + adjective + noun)
    pub name: String,
    pub article: String,
    pub adjective: String,
    pub noun: String,
    /// Long description when it differs from `name` (`$_..$_` markers stripped)
    pub long: Option<String>,
    /// Item weight in pounds; -1 = unknown (the wire's sentinel, e.g. on
    /// room furniture)
    pub weight: i32,
    /// Encumbrance override; -1 (or weight -1 when absent) = the item
    /// cannot be picked up (fixed furniture, room fixtures).
    pub encum: Option<i32>,
    /// Packed container capacity (contents): `v/10` = weight capacity in
    /// pounds, `v % 10` = max item count (0 = unlimited count). Nonzero =
    /// the item is a container. Decode with [`Self::in_capacity`].
    pub in_max: Option<u32>,
    /// Packed surface capacity (on top), same encoding as `in_max`.
    pub on_max: Option<u32>,
    /// Current contained encumbrance (pounds), when the server reports it
    pub in_encum: Option<u32>,
    /// Noun phrase to use in commands instead of `#id` paths (lockers and
    /// similar containers the game addresses by selector)
    pub in_selector: Option<String>,
    /// `locker="1"`: this container is a locker
    pub locker: bool,
    /// `familyvault="1"`: this container is a family vault
    pub familyvault: bool,
    /// Raw flags from the comma-separated `flags` attribute (e.g. "closed",
    /// "locked")
    pub flags: Vec<String>,
}

/// A user-requested item detail from `<inventoryViewItem>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewedItem {
    /// Exist id of the viewed item
    pub exist: String,
    /// Display name resolved from the managed snapshot at answer time
    pub name: String,
    /// (command, flattened text) sections in feed order ("look", "read", ...)
    pub results: Vec<(String, String)>,
    /// Bumps per answer, for display change detection
    pub generation: u64,
}

/// One active `<worldEvent>` announcement.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldEventState {
    pub realm: Option<String>,
    pub text: String,
    /// Epoch seconds when the event lapses (wire `expires` is in minutes);
    /// None = no stated expiry.
    pub expires_at: Option<i64>,
}

/// Decoded packed capacity from `in_max`/`on_max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerCapacity {
    /// Weight capacity in pounds
    pub pounds: u32,
    /// Maximum item count; None = unlimited
    pub max_items: Option<u32>,
}

impl ManagedInventoryItem {
    /// Map one `<i .../>`'s raw attributes; None only when the id or loc is
    /// missing/unusable (an item we could never anchor in the tree).
    pub fn from_attrs(attrs: &[(String, String)]) -> Option<Self> {
        let get = |name: &str| {
            attrs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        let id = get("id")?.to_string();
        let loc = get("loc")?;
        let (relation, parent) = if loc == "room" {
            ("room".to_string(), "room".to_string())
        } else {
            let (rel, parent) = loc.split_once(',')?;
            (rel.trim().to_string(), parent.trim().to_string())
        };
        // name is "article,adjective,noun"; either of the first two may be
        // empty. Anything that doesn't split into three keeps the whole
        // string as the noun rather than losing the item.
        let raw_name = get("name").unwrap_or_default();
        let (article, adjective, noun) = match raw_name.splitn(3, ',').collect::<Vec<_>>()[..] {
            [a, adj, n] => (
                a.trim().to_string(),
                adj.trim().to_string(),
                n.trim().to_string(),
            ),
            _ => (String::new(), String::new(), raw_name.trim().to_string()),
        };
        let name = [article.as_str(), adjective.as_str(), noun.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        let long = get("long")
            .map(|l| l.replace("$_", "").trim().to_string())
            .filter(|l| !l.is_empty());
        Some(Self {
            id,
            relation,
            parent,
            name,
            article,
            adjective,
            noun,
            long,
            weight: get("weight")
                .and_then(|w| w.trim().parse().ok())
                .unwrap_or(0),
            encum: get("encum").and_then(|v| v.trim().parse().ok()),
            in_max: get("in_max").and_then(|v| v.trim().parse().ok()),
            on_max: get("on_max").and_then(|v| v.trim().parse().ok()),
            in_encum: get("in_encum").and_then(|v| v.trim().parse().ok()),
            in_selector: get("in_selector")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            locker: get("locker") == Some("1"),
            familyvault: get("familyvault") == Some("1"),
            flags: get("flags")
                .map(|f| {
                    f.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// Decode a packed capacity value (`floor(v/10)` pounds, `v % 10` max
    /// item count with 0 = unlimited).
    fn decode_capacity(packed: u32) -> ContainerCapacity {
        ContainerCapacity {
            pounds: packed / 10,
            max_items: match packed % 10 {
                0 => None,
                n => Some(n),
            },
        }
    }

    /// Contents capacity when the item is a container (nonzero `in_max`).
    pub fn in_capacity(&self) -> Option<ContainerCapacity> {
        self.in_max.filter(|v| *v > 0).map(Self::decode_capacity)
    }

    /// Surface capacity when things can rest on the item (nonzero `on_max`).
    pub fn on_capacity(&self) -> Option<ContainerCapacity> {
        self.on_max.filter(|v| *v > 0).map(Self::decode_capacity)
    }

    /// True when the item is a container in either orientation.
    pub fn is_container(&self) -> bool {
        self.in_capacity().is_some() || self.on_capacity().is_some()
    }

    /// False for fixed items: `encum == -1`, or `weight == -1` with no
    /// encum override (Saga's Tc rule).
    pub fn can_pick_up(&self) -> bool {
        match self.encum {
            Some(e) => e != -1,
            None => self.weight != -1,
        }
    }

    /// The wire flags the item as closed (containers only carry this when
    /// the server knows).
    pub fn is_closed(&self) -> bool {
        self.flags.iter().any(|f| f == "closed")
    }

    /// The wire flags the item as locked.
    pub fn is_locked(&self) -> bool {
        self.flags.iter().any(|f| f == "locked")
    }
}

/// Latest `<inventoryManager>` snapshot (the structured inventory tree the
/// extended feed serves in response to `_inventory manager <token>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ManagedInventoryState {
    /// Correlation token from the request
    pub token: String,
    /// Room uid the snapshot was taken in
    pub room: String,
    pub items: Vec<ManagedInventoryItem>,
    /// False when the response carried continuation cursors (paginated
    /// inventory); continuation-following isn't implemented yet, so an
    /// incomplete snapshot stays incomplete.
    pub complete: bool,
    /// Bumped on every snapshot for change detection
    pub generation: u64,
}

/// Recursive weight of one item: its own weight plus everything inside,
/// per Saga's accounting. None = unknown (a -1 weight somewhere below).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeightBreakdown {
    /// The item's own weight; None when the wire said -1 (unknown/fixed)
    pub own: Option<f32>,
    /// Sum of contents' totals; None when any is unknown
    pub contents: Option<f32>,
    /// own + contents; None when contents are unknown
    pub total: Option<f32>,
}

impl ManagedInventoryState {
    /// Recursive weight breakdowns for every item, keyed by exist id.
    /// GS rules ported from Saga's manager: a 0-weight item counts as
    /// 0.1 lb, and `in` contents are skipped when the container reports
    /// `in_encum == 0` (deep/weightless containers don't pass their
    /// contents' weight to the carrier). Cycle-safe.
    pub fn weight_breakdowns(&self) -> std::collections::HashMap<String, WeightBreakdown> {
        let mut children: std::collections::HashMap<&str, Vec<&ManagedInventoryItem>> =
            std::collections::HashMap::new();
        for item in &self.items {
            if item.parent != "player" && item.parent != "room" {
                children.entry(item.parent.as_str()).or_default().push(item);
            }
        }
        let by_id: std::collections::HashMap<&str, &ManagedInventoryItem> =
            self.items.iter().map(|i| (i.id.as_str(), i)).collect();

        fn resolve<'a>(
            item: &'a ManagedInventoryItem,
            children: &std::collections::HashMap<&'a str, Vec<&'a ManagedInventoryItem>>,
            memo: &mut std::collections::HashMap<String, WeightBreakdown>,
            visiting: &mut std::collections::HashSet<String>,
        ) -> WeightBreakdown {
            if let Some(done) = memo.get(&item.id) {
                return *done;
            }
            if !visiting.insert(item.id.clone()) {
                // Parent cycle: report unknown rather than recurse forever.
                return WeightBreakdown::default();
            }
            let own = match item.weight {
                w if w < 0 => None,
                0 => Some(0.1),
                w => Some(w as f32),
            };
            let mut contents = Some(0.0f32);
            for kid in children.get(item.id.as_str()).into_iter().flatten() {
                // Weightless/deep containers: `in` contents don't weigh on
                // the carrier when the container reports in_encum == 0.
                if kid.relation == "in" && item.in_encum == Some(0) {
                    continue;
                }
                match (contents, resolve(kid, children, memo, visiting).total) {
                    (Some(sum), Some(t)) => contents = Some(((sum + t) * 10.0).round() / 10.0),
                    _ => {
                        contents = None;
                        break;
                    }
                }
            }
            visiting.remove(&item.id);
            let total = match (own, contents) {
                (o, Some(c)) => Some(((o.unwrap_or(0.0) + c) * 10.0).round() / 10.0),
                _ => None,
            };
            let out = WeightBreakdown {
                own,
                contents,
                total,
            };
            memo.insert(item.id.clone(), out);
            out
        }

        let mut memo = std::collections::HashMap::new();
        let mut visiting = std::collections::HashSet::new();
        for item in &self.items {
            resolve(item, &children, &mut memo, &mut visiting);
        }
        let _ = by_id;
        memo
    }

    /// The `via` selector for addressing an item: the nearest strict
    /// ancestor with an `in_selector` (lockers speak noun phrases, not
    /// `#id` paths). None for normally-addressed items.
    pub fn via_selector_for(&self, exist: &str) -> Option<String> {
        let by_id: std::collections::HashMap<&str, &ManagedInventoryItem> =
            self.items.iter().map(|i| (i.id.as_str(), i)).collect();
        let mut parent = by_id.get(exist)?.parent.as_str();
        let mut seen = std::collections::HashSet::new();
        while let Some(p) = by_id.get(parent) {
            if !seen.insert(p.id.as_str()) {
                return None; // cycle
            }
            if let Some(sel) = p.in_selector.as_deref() {
                return Some(sel.to_string());
            }
            parent = p.parent.as_str();
        }
        None
    }

    /// Descendant item counts per container (everything nested below, not
    /// just direct children), keyed by exist id. Non-containers are absent.
    pub fn descendant_counts(&self) -> std::collections::HashMap<String, usize> {
        let by_id: std::collections::HashMap<&str, &ManagedInventoryItem> =
            self.items.iter().map(|i| (i.id.as_str(), i)).collect();
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for item in &self.items {
            let mut seen = std::collections::HashSet::new();
            let mut parent = item.parent.as_str();
            while parent != "player" && parent != "room" && seen.insert(parent) {
                *counts.entry(parent.to_string()).or_insert(0) += 1;
                match by_id.get(parent) {
                    Some(p) => parent = p.parent.as_str(),
                    None => break,
                }
            }
        }
        counts
    }

    /// Human-readable location of an item: the container chain walked up
    /// to its root ("worn", a hand, at feet, or the room floor), innermost
    /// last, closed containers flagged. E.g.
    /// "in your quilled iron boar hide bandolier > coal black purse (closed)".
    pub fn location_of(&self, item: &ManagedInventoryItem) -> String {
        // Article-free form ("leather bandolier", not "a leather bandolier")
        // so the possessive path reads naturally after "your".
        let display = |i: &ManagedInventoryItem| -> String {
            let mut name = [i.adjective.as_str(), i.noun.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            if name.is_empty() {
                name = i.name.clone();
            }
            if i.is_closed() {
                name.push_str(" (closed)");
            }
            name
        };
        // Root relations need no walking.
        let root_label = |relation: &str| -> Option<&'static str> {
            match relation {
                "worn" => Some("worn"),
                "righthand" => Some("in your right hand"),
                "lefthand" => Some("in your left hand"),
                "atfeet" => Some("at your feet"),
                "reserved" => Some("reserved"),
                "room" => Some("on the floor"),
                _ => None,
            }
        };
        if item.parent == "player" || item.parent == "room" {
            return root_label(&item.relation).unwrap_or("carried").to_string();
        }
        // Walk the container chain outward, then print outermost-first.
        let by_id: std::collections::HashMap<&str, &ManagedInventoryItem> =
            self.items.iter().map(|i| (i.id.as_str(), i)).collect();
        let mut chain: Vec<&ManagedInventoryItem> = Vec::new();
        let mut cursor = item;
        let mut root = "carried";
        // Bounded by item count to survive a (malformed) parent cycle.
        for _ in 0..=self.items.len() {
            let Some(parent) = by_id.get(cursor.parent.as_str()) else {
                break;
            };
            chain.push(parent);
            if parent.parent == "player" || parent.parent == "room" {
                root = match root_label(&parent.relation) {
                    Some("worn") | None => "your",
                    Some("on the floor") => "the floor's",
                    Some(other) => {
                        // Hand/feet-held containers read naturally enough
                        // with the plain chain; keep "your".
                        let _ = other;
                        "your"
                    }
                };
                break;
            }
            cursor = parent;
        }
        if chain.is_empty() {
            return "carried".to_string();
        }
        chain.reverse();
        let path: Vec<String> = chain.iter().map(|c| display(c)).collect();
        format!("in {} {}", root, path.join(" > "))
    }
}

/// GS4 Experience dialog state (from `<openDialog id='expr'>`)
/// Composite of: yourLvl label + mindState progress + nextLvlPB progress
#[derive(Clone, Debug, Default)]
pub struct GS4ExperienceState {
    /// Current level text (e.g., "Level 100")
    pub level_text: String,
    /// Mind state percentage (0-100)
    pub mind_state_value: u32,
    /// Mind state display text (e.g., "clear as a bell")
    pub mind_state_text: String,
    /// Experience to next level percentage (0-100)
    pub next_level_value: u32,
    /// Experience to next level text (e.g., "43904921 experience")
    pub next_level_text: String,
    /// Exact field (unabsorbed) experience, from mindState bar attributes.
    /// All the exact numbers below are None until the game first sends them.
    pub field_exp: Option<u64>,
    /// Field experience capacity
    pub max_field_exp: Option<u64>,
    /// Total absorbed experience
    pub exp: Option<u64>,
    /// Total ascension experience
    pub ascension_exp: Option<u64>,
    /// Experience remaining until next level
    pub until_next: Option<u64>,
    /// Fash'lonae orb: 1 = redeemed (inactive), 2 = active; None = no orb
    pub fashlonae: Option<u8>,
    /// Lumnis bonus; only present while active
    pub lumnis: Option<u8>,
    /// RPA bonus multiplier (can be fractional); only present while active
    pub rpa: Option<f32>,
    /// Physical training points (raw label text, e.g. "23")
    pub ptps: Option<String>,
    /// Mental training points
    pub mtps: Option<String>,
    /// Ascension training points
    pub atps: Option<String>,
    /// Physical-to-mental conversion rate label
    pub p2m: Option<String>,
    /// Mental-to-physical conversion rate label
    pub m2p: Option<String>,
    /// Generation counter for change detection
    pub generation: u64,
}

impl GS4ExperienceState {
    /// Update level text, returns true if changed
    pub fn update_level(&mut self, text: String) -> bool {
        if self.level_text != text {
            self.level_text = text;
            self.generation += 1;
            true
        } else {
            false
        }
    }

    /// Update mind state, returns true if changed
    pub fn update_mind_state(&mut self, value: u32, text: String) -> bool {
        if self.mind_state_value != value || self.mind_state_text != text {
            self.mind_state_value = value;
            self.mind_state_text = text;
            self.generation += 1;
            true
        } else {
            false
        }
    }

    /// Update experience to next level, returns true if changed
    pub fn update_next_level(&mut self, value: u32, text: String) -> bool {
        if self.next_level_value != value || self.next_level_text != text {
            self.next_level_value = value;
            self.next_level_text = text;
            self.generation += 1;
            true
        } else {
            false
        }
    }

    /// Applies the exact-experience attributes carried on a mindState
    /// progress bar. The exp numbers are sticky (absent = unchanged); the
    /// event-bonus flags are a snapshot (absent = bonus over, clear).
    /// Returns true if anything changed.
    #[allow(clippy::too_many_arguments)]
    pub fn update_exp_attrs(
        &mut self,
        field_exp: Option<u64>,
        max_field_exp: Option<u64>,
        exp: Option<u64>,
        ascension_exp: Option<u64>,
        until_next: Option<u64>,
        fashlonae: Option<u8>,
        lumnis: Option<u8>,
        rpa: Option<f32>,
    ) -> bool {
        let mut changed = false;
        for (field, incoming) in [
            (&mut self.field_exp, field_exp),
            (&mut self.max_field_exp, max_field_exp),
            (&mut self.exp, exp),
            (&mut self.ascension_exp, ascension_exp),
            (&mut self.until_next, until_next),
        ] {
            if incoming.is_some() && *field != incoming {
                *field = incoming;
                changed = true;
            }
        }
        if self.fashlonae != fashlonae {
            self.fashlonae = fashlonae;
            changed = true;
        }
        if self.lumnis != lumnis {
            self.lumnis = lumnis;
            changed = true;
        }
        if self.rpa != rpa {
            self.rpa = rpa;
            changed = true;
        }
        if changed {
            self.generation += 1;
        }
        changed
    }

    /// Update a training-point/conversion label from the expr dialog
    /// (PTPs/MTPs/ATPs/p2m/m2p). Returns true if changed.
    pub fn update_tp_label(&mut self, id: &str, value: &str) -> bool {
        let field = match id {
            "PTPs" => &mut self.ptps,
            "MTPs" => &mut self.mtps,
            "ATPs" => &mut self.atps,
            "p2m" => &mut self.p2m,
            "m2p" => &mut self.m2p,
            _ => return false,
        };
        if field.as_deref() != Some(value) {
            *field = Some(value.to_string());
            self.generation += 1;
            true
        } else {
            false
        }
    }

    /// Clear all values (on disconnect/login)
    pub fn clear(&mut self) {
        self.level_text.clear();
        self.mind_state_value = 0;
        self.mind_state_text.clear();
        self.next_level_value = 0;
        self.next_level_text.clear();
        self.field_exp = None;
        self.max_field_exp = None;
        self.exp = None;
        self.ascension_exp = None;
        self.until_next = None;
        self.fashlonae = None;
        self.lumnis = None;
        self.rpa = None;
        self.ptps = None;
        self.mtps = None;
        self.atps = None;
        self.p2m = None;
        self.m2p = None;
        self.generation += 1;
    }
}

/// Encumbrance state (from `<openDialog id='encum'>`)
/// Composite of: encumlevel progress bar + encumblurb label
#[derive(Clone, Debug, Default)]
pub struct EncumbranceState {
    /// Encumbrance percentage (0-100)
    pub value: u32,
    /// Encumbrance level text (e.g., "None", "Light", "Moderate")
    pub text: String,
    /// Descriptive blurb (e.g., "You are not encumbered enough to notice.")
    pub blurb: String,
    /// Generation counter for change detection
    pub generation: u64,
}

impl EncumbranceState {
    /// Update from progress bar data, returns true if changed
    pub fn update_level(&mut self, value: u32, text: String) -> bool {
        if self.value != value || self.text != text {
            self.value = value;
            self.text = text;
            self.generation += 1;
            true
        } else {
            false
        }
    }

    /// Update blurb text, returns true if changed
    pub fn update_blurb(&mut self, blurb: String) -> bool {
        if self.blurb != blurb {
            self.blurb = blurb;
            self.generation += 1;
            true
        } else {
            false
        }
    }

    /// Clear all values (on disconnect/login)
    pub fn clear(&mut self) {
        self.value = 0;
        self.text.clear();
        self.blurb.clear();
        self.generation += 1;
    }
}

/// Combat stance, from `<progressBar id='pbarStance' value='100'
/// text='defensive (100%)'/>` inside the stance dialog.
///
/// Before this existed the stance bar rendered straight into a window widget
/// and never reached game state, so headless and remote clients -- anything
/// without a stance window -- had no stance at all. It is stored here for the
/// same reason injuries and vitals are: the data belongs to the session, not
/// to whichever window happens to be on screen.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StanceState {
    /// Percent of stance contributing to defense (0-100). 100 is fully
    /// defensive, 0 fully offensive.
    pub value: u32,
    /// Stance name parsed out of the bar text ("defensive", "offensive", ...).
    /// Empty until the first stance bar arrives.
    pub text: String,
    /// Generation counter for change detection
    pub generation: u64,
}

impl StanceState {
    /// Update from progress bar data; returns true if changed.
    ///
    /// The feed's text is `"defensive (100%)"` -- the percent is already in
    /// `value`, so the parenthetical is stripped and only the name kept.
    /// Callers pass the raw text; parsing lives here so every entry point
    /// (dialog path and bare progressBar path) normalizes identically.
    pub fn update(&mut self, value: u32, text: &str) -> bool {
        let name = text
            .split('(')
            .next()
            .unwrap_or(text)
            .trim()
            .to_ascii_lowercase();
        if self.value != value || self.text != name {
            self.value = value;
            self.text = name;
            self.generation += 1;
            true
        } else {
            false
        }
    }

    /// Clear on disconnect/login.
    pub fn clear(&mut self) {
        self.value = 0;
        self.text.clear();
        self.generation += 1;
    }
}

/// Betrayer panel state (from `<dialogData id='BetrayerPanel'>`)
/// Displays blood points as progress bar + list of contributing items
#[derive(Clone, Debug, Default)]
pub struct BetrayerState {
    /// Blood points value (0-100)
    pub value: u32,
    /// Display text (e.g., "Blood Points: 100")
    pub text: String,
    /// List of items contributing to blood pool
    pub items: Vec<String>,
    /// Generation counter for change detection
    pub generation: u64,
}

impl BetrayerState {
    /// Update blood points from lblBPs label value
    /// Parses "Blood Points: XXX" → value=XXX
    pub fn update_blood_points(&mut self, value_text: &str) -> bool {
        let value = value_text
            .strip_prefix("Blood Points: ")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);

        if self.value != value || self.text != value_text {
            self.value = value;
            self.text = value_text.to_string();
            self.generation += 1;
            return true;
        }
        false
    }

    /// Update items list from lblitemX labels
    pub fn update_items(&mut self, items: Vec<String>) -> bool {
        if self.items != items {
            self.items = items;
            self.generation += 1;
            return true;
        }
        false
    }

    /// Clear all values (on disconnect/login or clear='t')
    pub fn clear(&mut self) {
        self.value = 0;
        self.text.clear();
        self.items.clear();
        self.generation += 1;
    }
}
/// How many recent game lines stay available to `Await` steps. An await
/// polls once per tick, so this only has to cover the burst a single tick can
/// miss; 64 is far more than any observed edge needs and costs nothing.
pub const RAW_LINE_RING: usize = 64;

impl GameState {
    /// Apply one message-derived creature-effect event: a start re-arms the
    /// timer and takes the newest severity (refresh, never stack); an end
    /// removes the effect. The status name is merged into / removed from
    /// the creature's flags immediately, with a generation bump so widgets
    /// and the web wire react.
    pub fn apply_creature_effect_event(
        &mut self,
        exist: &str,
        name: &str,
        severity: Option<u8>, // Some = start (rank), None = end
        timeout_s: u32,
        now_server: i64,
    ) {
        self.derived_status_names.insert(name.to_ascii_lowercase());
        let effects = self.creature_effects.entry(exist.to_string()).or_default();
        match severity {
            Some(severity) => {
                let expires_at = now_server + timeout_s as i64;
                match effects.iter_mut().find(|e| e.name == name) {
                    Some(e) => {
                        e.severity = severity;
                        e.expires_at = expires_at;
                    }
                    None => effects.push(ActiveCreatureEffect {
                        name: name.to_string(),
                        severity,
                        expires_at,
                    }),
                }
            }
            None => effects.retain(|e| e.name != name),
        }
        if effects.is_empty() {
            self.creature_effects.remove(exist);
        }
        self.merge_creature_effect_statuses();
    }

    /// Expire timed-out effects and (re)merge the survivors' names into
    /// their creatures' open-vocabulary statuses. Called once per frame —
    /// the merge also repairs flags after a room-objs rebuild replaced
    /// them, so derived statuses survive roster refreshes. Generation bumps
    /// only on real change.
    pub fn tick_creature_effects(&mut self, now_server: i64) {
        if self.creature_effects.is_empty() {
            return;
        }
        for effects in self.creature_effects.values_mut() {
            effects.retain(|e| e.expires_at > now_server);
        }
        self.creature_effects
            .retain(|_, effects| !effects.is_empty());
        self.merge_creature_effect_statuses();
    }

    /// Live severity (1-3) of a named derived effect on a creature — the
    /// `{severity}` in ranked overlay art paths.
    pub fn creature_effect_severity(&self, exist: &str, name: &str) -> Option<u8> {
        self.creature_effects
            .get(exist)?
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.severity)
    }

    /// Reconcile every room creature's statuses with the derived-effect
    /// store: add missing names, drop stale ones. Only names the store has
    /// ever produced are dropped — feed statuses are never touched.
    fn merge_creature_effect_statuses(&mut self) {
        // Disjoint field borrows: the closure reads derived names while the
        // loop mutates creatures.
        let GameState {
            room_creatures,
            creature_effects,
            derived_status_names,
            room_creatures_generation,
            ..
        } = self;
        let mut changed = false;
        for creature in room_creatures.iter_mut() {
            let wanted: Vec<&str> = creature_effects
                .get(&creature.id)
                .map(|effects| effects.iter().map(|e| e.name.as_str()).collect())
                .unwrap_or_default();
            let Some(flags) = creature.flags.as_mut() else {
                // Derived effects can attach before any <crtrStatus> was
                // seen; a default snapshot carries the badge until then.
                if !wanted.is_empty() {
                    let mut flags = CreatureFlags::default();
                    flags.statuses.extend(wanted.iter().map(|s| s.to_string()));
                    creature.flags = Some(flags);
                    changed = true;
                }
                continue;
            };
            // Drop derived names no longer active — only names this store
            // has itself applied are removable; feed statuses never are.
            let before = flags.statuses.len();
            flags.statuses.retain(|s| {
                wanted.iter().any(|w| w.eq_ignore_ascii_case(s))
                    || !derived_status_names.contains(&s.to_ascii_lowercase())
            });
            changed |= flags.statuses.len() != before;
            for name in wanted {
                if !flags.statuses.iter().any(|s| s.eq_ignore_ascii_case(name)) {
                    flags.statuses.push(name.to_string());
                    changed = true;
                }
            }
        }
        if changed {
            *room_creatures_generation += 1;
        }
    }

    /// Record a game line for scripted-edge awaits, evicting the oldest past
    /// [`RAW_LINE_RING`]. Returns nothing; awaits read `recent_lines`.
    pub fn push_recent_line(&mut self, line: &str) {
        // Blank lines can't match a meaningful pattern and would evict real
        // content from a small ring.
        if line.trim().is_empty() {
            return;
        }
        self.line_seq += 1;
        self.recent_lines
            .push_back((self.line_seq, line.to_string()));
        while self.recent_lines.len() > RAW_LINE_RING {
            self.recent_lines.pop_front();
        }
    }

    /// Whether a named debuff is on the Debuffs board right now. Lich's
    /// `Status.bound?`/`Status.sleeping?` gate on the Bind/Sleep debuff
    /// entries; the feed removes expired entries, so presence is the signal.
    /// Matches the display text exactly or as a leading word ("Bind" also
    /// matches "Bind (214)").
    pub fn debuff_active(&self, name: &str) -> bool {
        self.effects
            .get("Debuffs")
            .map(|c| {
                c.effects.iter().any(|e| {
                    let t = e.text.trim();
                    t == name || t.starts_with(&format!("{name} "))
                })
            })
            .unwrap_or(false)
    }

    pub fn new() -> Self {
        Self {
            connected: false,
            character_name: None,
            room_id: None,
            room_name: None,
            exits: Vec::new(),
            game_time: 0,
            game_time_received: None,
            roundtime_end: None,
            casttime_end: None,
            spell: None,
            active_streams: HashMap::new(),
            status: StatusInfo::default(),
            vitals: Vitals::default(),
            inventory: Vec::new(),
            left_hand: None,
            right_hand: None,
            active_effects: Vec::new(),
            effects: HashMap::new(),
            objectives: crate::data::ObjectivesContent::default(),
            compass_dirs: Vec::new(),
            injuries: HashMap::new(),
            last_prompt: String::from(">"), // Default prompt
            target_list: TargetListState::default(),
            room_creatures: Vec::new(),
            room_creatures_generation: 0,
            creature_effects: std::collections::HashMap::new(),
            derived_status_names: std::collections::HashSet::new(),
            room_objects: Vec::new(),
            room_objects_generation: 0,
            room_players: Vec::new(),
            room_players_generation: 0,
            room_description: Vec::new(),
            room_description_generation: 0,
            story_picture: None,
            spellbook: Vec::new(),
            spellbook_generation: 0,
            inventory_received: false,
            room_meta: RoomMetaState::default(),
            managed_inventory: None,
            pulse_count: 0,
            viewed_item: None,
            world_events: Vec::new(),
            pantheon_value: None,
            next_pulse_mana: false,
            pulse_next_earliest: None,
            pulse_next_latest: None,
            objects: crate::core::game_objects::GameObjects::default(),
            move_feedback: std::collections::VecDeque::new(),
            game_line_no: 0,
            silver_line_no: 0,
            nav_count: 0,
            recent_lines: std::collections::VecDeque::new(),
            line_seq: 0,
            spell_names_seen: std::collections::HashMap::new(),
            character: crate::core::character_state::CharacterState::default(),
            silver: None,
            day_passes: crate::core::day_pass::DayPassCache::default(),
            dr_experience: DRExperienceState::default(),
            gs4_experience: GS4ExperienceState::default(),
            encumbrance: EncumbranceState::default(),
            stance: StanceState::default(),
            group: crate::core::group::GroupState::default(),
            minivitals: MiniVitalsState::default(),
            betrayer: BetrayerState::default(),
            bounty: BountyState::default(),
            society: SocietyState::default(),
            estimated_lag_ms: None,
            last_lag_check_time: 0,
            sound_queue: SoundQueue::new(),
        }
    }

    /// Update game time from prompt timestamp.
    /// Also periodically recalculates estimated lag (every 30 seconds of game time).
    pub fn update_game_time(&mut self, prompt_time: i64) {
        self.game_time = prompt_time;
        self.game_time_received = Some(std::time::Instant::now());

        // Periodically calculate lag (every LAG_CHECK_INTERVAL_SECS)
        if prompt_time - self.last_lag_check_time >= LAG_CHECK_INTERVAL_SECS {
            let system_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;

            // Convert game time to milliseconds for comparison
            let game_time_ms = prompt_time * 1000;

            // Positive lag = system ahead, Negative = game ahead
            self.estimated_lag_ms = Some(system_time - game_time_ms);
            self.last_lag_check_time = prompt_time;
        }
    }

    /// Server "now", extrapolated: the last prompt's timestamp plus how long
    /// ago it arrived on the local clock. Timers keep flowing between lines.
    pub fn game_time_now(&self) -> i64 {
        self.game_time
            + self
                .game_time_received
                .map(|at| at.elapsed().as_secs() as i64)
                .unwrap_or(0)
    }

    /// Check if currently in roundtime.
    /// Compares against extrapolated game server time, not system time.
    pub fn in_roundtime(&self) -> bool {
        if let Some(end_time) = self.roundtime_end {
            self.game_time_now() < end_time
        } else {
            false
        }
    }

    /// Check if currently in casttime.
    /// Compares against extrapolated game server time, not system time.
    pub fn in_casttime(&self) -> bool {
        if let Some(end_time) = self.casttime_end {
            self.game_time_now() < end_time
        } else {
            false
        }
    }

    /// Get remaining roundtime in seconds (0 if not in roundtime)
    pub fn roundtime_remaining(&self) -> i64 {
        if let Some(end_time) = self.roundtime_end {
            (end_time - self.game_time_now()).max(0)
        } else {
            0
        }
    }

    /// Get remaining casttime in seconds (0 if not in casttime)
    pub fn casttime_remaining(&self) -> i64 {
        if let Some(end_time) = self.casttime_end {
            (end_time - self.game_time_now()).max(0)
        } else {
            0
        }
    }

    /// Get estimated lag in milliseconds, if available
    pub fn lag_ms(&self) -> Option<i64> {
        self.estimated_lag_ms
    }

    /// Queue a sound trigger from highlight processing
    pub fn queue_sound(&mut self, trigger: SoundTrigger) {
        self.sound_queue.sounds.push(QueuedSound {
            file: trigger.file,
            volume: trigger.volume,
        });
    }

    /// Drain all queued sounds for playback
    /// Returns the queued sounds and replaces the queue with a fresh pre-allocated vector
    pub fn drain_sound_queue(&mut self) -> Vec<QueuedSound> {
        std::mem::replace(&mut self.sound_queue.sounds, Vec::with_capacity(5))
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            health: 100,
            mana: 100,
            stamina: 100,
            spirit: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Creature::is_body_part tests ==========

    fn body_part_creature(name: &str, noun: Option<&str>) -> Creature {
        Creature {
            name: name.to_string(),
            noun: noun.map(str::to_string),
            id: "#1".to_string(),
            status: None,
            flags: None,
        }
    }

    #[test]
    fn test_is_body_part_matches_appendage_nouns() {
        for noun in [
            "arm", "arms", "tentacle", "claws", "limb", "pincer", "palpi",
        ] {
            assert!(
                body_part_creature("a severed thing", Some(noun)).is_body_part(),
                "noun '{}' should be a body part",
                noun
            );
        }
    }

    #[test]
    fn test_is_body_part_kraken_tentacle_is_a_creature() {
        // All four variants Lich excepts (gameobj.rb / creature.rb), not just
        // amaranthine — these are real creatures despite the "tentacle" noun.
        for variant in ["amaranthine", "ghostly", "grizzled", "ancient"] {
            let name = format!("a {variant} kraken tentacle");
            assert!(
                !body_part_creature(&name, Some("tentacle")).is_body_part(),
                "'{name}' should be a creature, not an appendage"
            );
        }
        // A plain tentacle with no kraken qualifier is still an appendage.
        assert!(body_part_creature("a severed tentacle", Some("tentacle")).is_body_part());
    }

    #[test]
    fn test_is_body_part_normal_creature_and_missing_noun() {
        assert!(!body_part_creature("a muddy hog", Some("hog")).is_body_part());
        assert!(!body_part_creature("a severed arm", None).is_body_part());
    }

    // ========== Creature::is_valid_target tests ==========

    #[test]
    fn test_is_valid_target_lich_valid_target_rules() {
        let excluded = vec!["coal".to_string()];

        // Creature.name holds the bold link text WITHOUT the article
        // (the feed keeps "a"/"an" outside the <a> tag), so the animated
        // check is anchored at "animated", matching Lich's /^animated\b/.

        // Live hostile-eligible creature passes.
        assert!(body_part_creature("muddy hog", Some("hog")).is_valid_target(&excluded));

        // Animated decoy fails, but "animated slush" passes.
        assert!(!body_part_creature("animated statue", Some("statue")).is_valid_target(&excluded));
        assert!(body_part_creature("animated slush", Some("slush")).is_valid_target(&excluded));

        // Appendage fails; kraken tentacle passes.
        assert!(!body_part_creature("severed arm", Some("arm")).is_valid_target(&excluded));
        assert!(
            body_part_creature("ancient kraken tentacle", Some("tentacle"))
                .is_valid_target(&excluded)
        );

        // Configured excluded noun fails (case-insensitive).
        assert!(!body_part_creature("lump of coal", Some("Coal")).is_valid_target(&excluded));

        // Dead fails.
        let dead = Creature {
            flags: Some(CreatureFlags {
                dead: true,
                ..Default::default()
            }),
            ..body_part_creature("slain orc", Some("orc"))
        };
        assert!(!dead.is_valid_target(&excluded));
    }

    // ========== GameState tests ==========

    #[test]
    fn test_game_state_new() {
        let state = GameState::new();
        assert!(!state.connected);
        assert!(state.character_name.is_none());
        assert!(state.room_id.is_none());
        assert!(state.room_name.is_none());
        assert!(state.exits.is_empty());
        assert_eq!(state.game_time, 0);
        assert!(state.roundtime_end.is_none());
        assert!(state.casttime_end.is_none());
        assert!(state.spell.is_none());
        assert!(state.active_streams.is_empty());
        assert!(state.inventory.is_empty());
        assert!(state.left_hand.is_none());
        assert!(state.right_hand.is_none());
        assert!(state.active_effects.is_empty());
        assert!(state.compass_dirs.is_empty());
        assert_eq!(state.last_prompt, ">");
        assert!(state.estimated_lag_ms.is_none());
    }

    #[test]
    fn test_game_state_default() {
        let state = GameState::default();
        assert!(!state.connected);
        assert_eq!(state.last_prompt, ">");
        assert_eq!(state.game_time, 0);
    }

    #[test]
    fn test_game_state_vitals_default() {
        let state = GameState::new();
        assert_eq!(state.vitals.health, 100);
        assert_eq!(state.vitals.mana, 100);
        assert_eq!(state.vitals.stamina, 100);
        assert_eq!(state.vitals.spirit, 100);
    }

    #[test]
    fn test_game_state_status_default() {
        let state = GameState::new();
        assert!(!state.status.standing());
        assert!(!state.status.kneeling());
        assert!(!state.status.sitting());
        assert!(!state.status.prone());
        assert!(!state.status.stunned());
        assert!(!state.status.bleeding());
        assert!(!state.status.hidden());
        assert!(!state.status.invisible());
        assert!(!state.status.webbed());
        assert!(!state.status.joined());
        assert!(!state.status.dead());
    }

    // ========== Game Time tests ==========

    #[test]
    fn test_update_game_time() {
        let mut state = GameState::new();
        let game_time = 1764905000;

        state.update_game_time(game_time);

        assert_eq!(state.game_time, game_time);
    }

    #[test]
    fn test_update_game_time_calculates_lag_on_first_call() {
        let mut state = GameState::new();
        let game_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        state.update_game_time(game_time);

        // Should calculate lag on first call (since last_lag_check_time is 0)
        assert!(state.estimated_lag_ms.is_some());
        assert_eq!(state.last_lag_check_time, game_time);
    }

    #[test]
    fn test_update_game_time_throttles_lag_calculation() {
        let mut state = GameState::new();
        let base_time = 1764905000i64;

        // First update - should calculate lag
        state.update_game_time(base_time);
        let first_lag = state.estimated_lag_ms;
        assert!(first_lag.is_some());

        // Update 10 seconds later - should NOT recalculate (< 30 sec threshold)
        state.update_game_time(base_time + 10);
        assert_eq!(state.estimated_lag_ms, first_lag);
        assert_eq!(state.last_lag_check_time, base_time); // Still the original check time

        // Update 35 seconds later - SHOULD recalculate (> 30 sec threshold)
        state.update_game_time(base_time + 35);
        assert_eq!(state.last_lag_check_time, base_time + 35);
    }

    // ========== Roundtime tests (using game time) ==========

    #[test]
    fn test_game_state_in_roundtime_none() {
        let state = GameState::new();
        assert!(!state.in_roundtime());
    }

    #[test]
    fn test_game_state_in_roundtime_future() {
        let mut state = GameState::new();
        let game_time = 1764905000;

        // Simulate: game time is 1764905000, roundtime ends at 1764905005 (5 sec RT)
        state.game_time = game_time;
        state.roundtime_end = Some(game_time + 5);

        assert!(state.in_roundtime());
    }

    #[test]
    fn test_game_state_in_roundtime_past() {
        let mut state = GameState::new();
        let game_time = 1764905010;

        // Simulate: game time is 1764905010, roundtime ended at 1764905005
        state.game_time = game_time;
        state.roundtime_end = Some(1764905005);

        assert!(!state.in_roundtime());
    }

    #[test]
    fn test_roundtime_remaining() {
        let mut state = GameState::new();
        state.game_time = 1764905000;
        state.roundtime_end = Some(1764905005);

        assert_eq!(state.roundtime_remaining(), 5);
    }

    #[test]
    fn test_roundtime_remaining_expired() {
        let mut state = GameState::new();
        state.game_time = 1764905010;
        state.roundtime_end = Some(1764905005);

        assert_eq!(state.roundtime_remaining(), 0); // Clamped to 0
    }

    #[test]
    fn test_roundtime_remaining_none() {
        let state = GameState::new();
        assert_eq!(state.roundtime_remaining(), 0);
    }

    // ========== Casttime tests (using game time) ==========

    #[test]
    fn test_game_state_in_casttime_none() {
        let state = GameState::new();
        assert!(!state.in_casttime());
    }

    #[test]
    fn test_game_state_in_casttime_future() {
        let mut state = GameState::new();
        let game_time = 1764905000;

        // Simulate: game time is 1764905000, casttime ends at 1764905003 (3 sec cast)
        state.game_time = game_time;
        state.casttime_end = Some(game_time + 3);

        assert!(state.in_casttime());
    }

    #[test]
    fn test_game_state_in_casttime_past() {
        let mut state = GameState::new();
        let game_time = 1764905010;

        // Simulate: game time is 1764905010, casttime ended at 1764905003
        state.game_time = game_time;
        state.casttime_end = Some(1764905003);

        assert!(!state.in_casttime());
    }

    #[test]
    fn test_casttime_remaining() {
        let mut state = GameState::new();
        state.game_time = 1764905000;
        state.casttime_end = Some(1764905003);

        assert_eq!(state.casttime_remaining(), 3);
    }

    #[test]
    fn test_casttime_remaining_expired() {
        let mut state = GameState::new();
        state.game_time = 1764905010;
        state.casttime_end = Some(1764905003);

        assert_eq!(state.casttime_remaining(), 0);
    }

    // ========== Lag tests ==========

    #[test]
    fn test_lag_ms_initially_none() {
        let state = GameState::new();
        assert!(state.lag_ms().is_none());
    }

    #[test]
    fn test_lag_ms_after_update() {
        let mut state = GameState::new();
        let game_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        state.update_game_time(game_time);

        // Lag should be calculated and be relatively small (within a few hundred ms)
        let lag = state.lag_ms().expect("lag should be calculated");
        // Allow for some system timing variance (within 5 seconds = 5000ms)
        assert!(lag.abs() < 5000, "lag {} ms is unexpectedly large", lag);
    }

    // ========== Clone and other tests ==========

    #[test]
    fn test_game_state_clone() {
        let mut state = GameState::new();
        state.connected = true;
        state.character_name = Some("TestChar".to_string());
        state.exits.push("north".to_string());
        state.vitals.health = 75;
        state.game_time = 1764905000;

        let cloned = state.clone();
        assert!(cloned.connected);
        assert_eq!(cloned.character_name, Some("TestChar".to_string()));
        assert_eq!(cloned.exits.len(), 1);
        assert_eq!(cloned.vitals.health, 75);
        assert_eq!(cloned.game_time, 1764905000);
    }

    #[test]
    fn test_game_state_active_streams() {
        let mut state = GameState::new();
        state.active_streams.insert("inv".to_string(), true);
        state.active_streams.insert("assess".to_string(), false);

        assert_eq!(state.active_streams.get("inv"), Some(&true));
        assert_eq!(state.active_streams.get("assess"), Some(&false));
        assert_eq!(state.active_streams.get("unknown"), None);
    }

    // ========== StatusInfo tests ==========

    #[test]
    fn test_status_info_default() {
        let status = StatusInfo::default();
        assert!(!status.standing());
        assert!(!status.kneeling());
        assert!(!status.sitting());
        assert!(!status.prone());
        assert!(!status.stunned());
        assert!(!status.bleeding());
        assert!(!status.hidden());
        assert!(!status.invisible());
        assert!(!status.webbed());
        assert!(!status.joined());
        assert!(!status.dead());
    }

    #[test]
    fn test_status_info_clone() {
        let mut status = StatusInfo::default();
        status.set("standing", true);
        status.set("hidden", true);

        let cloned = status.clone();
        assert!(cloned.standing());
        assert!(cloned.hidden());
        assert!(!cloned.dead());
    }

    #[test]
    fn stance_parses_name_out_of_bar_text() {
        let mut stance = StanceState::default();
        // The feed's text is "defensive (100%)" -- the percent is already in
        // `value`, so only the name is kept.
        assert!(stance.update(100, "defensive (100%)"));
        assert_eq!(stance.value, 100);
        assert_eq!(stance.text, "defensive");

        assert!(stance.update(0, "offensive (0%)"));
        assert_eq!(stance.value, 0);
        assert_eq!(stance.text, "offensive");
    }

    #[test]
    fn stance_handles_text_without_percent() {
        let mut stance = StanceState::default();
        // Defensive against a feed that omits the parenthetical.
        stance.update(50, "guarded");
        assert_eq!(stance.text, "guarded");

        // ...and against casing drift.
        stance.update(50, "Advance (50%)");
        assert_eq!(stance.text, "advance");
    }

    #[test]
    fn stance_reports_changes_for_delta_suppression() {
        let mut stance = StanceState::default();
        assert!(stance.update(100, "defensive (100%)"));
        // Same value and name: no change, so no delta is emitted.
        assert!(!stance.update(100, "defensive (100%)"));
        // A percent change alone counts.
        assert!(stance.update(75, "defensive (75%)"));
        let gen = stance.generation;
        assert!(!stance.update(75, "defensive (75%)"));
        assert_eq!(stance.generation, gen, "no-op must not bump generation");
    }

    #[test]
    fn stance_clear_resets() {
        let mut stance = StanceState::default();
        stance.update(100, "defensive (100%)");
        stance.clear();
        assert_eq!(stance.value, 0);
        assert!(stance.text.is_empty());
    }

    #[test]
    fn status_info_normalizes_case_and_icon_prefix() {
        let mut status = StatusInfo::default();
        // The game sends "IconSTUNNED"; config stores "STUNNED"; the old code
        // matched "stunned". All three must be one key.
        status.set("IconSTUNNED", true);
        assert!(status.stunned());
        assert!(status.get("STUNNED"));
        assert!(status.get("stunned"));

        // ...and clearing through a different casing clears the same key.
        status.set("Stunned", false);
        assert!(!status.stunned());
    }

    #[test]
    fn status_info_stores_arbitrary_ids() {
        let mut status = StatusInfo::default();
        // Ids with no typed accessor still round-trip -- the whole point of
        // the map. POISONED/DISEASED previously had nowhere to live.
        status.set("POISONED", true);
        status.set("SOME_FUTURE_ICON", true);
        assert!(status.poisoned());
        assert!(status.get("some_future_icon"));
    }

    #[test]
    fn status_info_distinguishes_unreported_from_inactive() {
        let mut status = StatusInfo::default();
        assert!(!status.is_known("stunned"), "never reported");
        assert!(!status.get("stunned"), "and reads false");

        status.set("stunned", false);
        assert!(status.is_known("stunned"), "explicitly reported inactive");
        assert!(!status.get("stunned"));
    }

    #[test]
    fn status_info_set_reports_changes() {
        let mut status = StatusInfo::default();
        // First report is a change even when the value is the default...
        assert!(status.set("stunned", false));
        // ...but a repeat of the same value is not, so no delta is emitted.
        assert!(!status.set("stunned", false));
        assert!(status.set("stunned", true));
        assert!(!status.set("stunned", true));
    }

    /// The phone client reads flat lowercase keys (`d["stunned"]` in app.js),
    /// so the map MUST serialize transparently -- no wrapper object, no
    /// uppercase. This pins the wire shape against accidental restructuring.
    #[test]
    fn status_info_serializes_as_flat_lowercase_object() {
        let mut status = StatusInfo::default();
        status.set("IconSTUNNED", true);
        status.set("BLEEDING", false);

        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, r#"{"bleeding":false,"stunned":true}"#);

        let back: StatusInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, status);
    }

    // ========== Vitals tests ==========

    #[test]
    fn test_vitals_default() {
        let vitals = Vitals::default();
        assert_eq!(vitals.health, 100);
        assert_eq!(vitals.mana, 100);
        assert_eq!(vitals.stamina, 100);
        assert_eq!(vitals.spirit, 100);
    }

    #[test]
    fn test_vitals_clone() {
        let mut vitals = Vitals::default();
        vitals.health = 50;
        vitals.mana = 75;

        let cloned = vitals.clone();
        assert_eq!(cloned.health, 50);
        assert_eq!(cloned.mana, 75);
        assert_eq!(cloned.stamina, 100);
        assert_eq!(cloned.spirit, 100);
    }

    #[test]
    fn test_vitals_boundary_values() {
        let mut vitals = Vitals::default();
        vitals.health = 0;
        vitals.mana = 255; // u8 max

        assert_eq!(vitals.health, 0);
        assert_eq!(vitals.mana, 255);
    }

    // ========== Debug trait tests ==========

    #[test]
    fn test_game_state_debug() {
        let state = GameState::new();
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("GameState"));
        assert!(debug_str.contains("connected"));
        assert!(debug_str.contains("game_time"));
    }

    #[test]
    fn test_status_info_debug() {
        // A default StatusInfo is now an EMPTY map -- nothing reported yet --
        // so Debug names the type but lists no ids. Reported ids appear.
        let mut status = StatusInfo::default();
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("StatusInfo"));
        assert!(!debug_str.contains("standing"), "nothing reported yet");

        status.set("STANDING", true);
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("standing"), "reported ids are listed");
    }

    #[test]
    fn test_vitals_debug() {
        let vitals = Vitals::default();
        let debug_str = format!("{:?}", vitals);
        assert!(debug_str.contains("Vitals"));
        assert!(debug_str.contains("health"));
    }

    // ========== RoomMetaState tests ==========

    #[test]
    fn test_roommeta_updates_are_sticky_per_field() {
        let mut meta = RoomMetaState::default();

        assert!(meta.update_from_attrs([("climate", "3"), ("terrain", "7")]));
        assert_eq!(meta.climate, Some(3));
        assert_eq!(meta.terrain, Some(7));
        let gen_after_first = meta.generation;

        // A later tag carrying only weather must not disturb earlier fields
        assert!(meta.update_from_attrs([("weather", "2")]));
        assert_eq!(meta.climate, Some(3));
        assert_eq!(meta.terrain, Some(7));
        assert_eq!(meta.weather, Some(2));
        assert!(meta.generation > gen_after_first);

        // Re-sending identical values is not a change
        let gen_before_repeat = meta.generation;
        assert!(!meta.update_from_attrs([("weather", "2")]));
        assert_eq!(meta.generation, gen_before_repeat);
    }

    #[test]
    fn test_roommeta_ignores_unknown_and_non_numeric() {
        let mut meta = RoomMetaState::default();
        assert!(!meta.update_from_attrs([("newfangled", "1"), ("climate", "temperate")]));
        assert_eq!(meta, RoomMetaState::default());
    }

    // ========== GS4ExperienceState exp-attrs tests ==========

    #[test]
    fn test_mindstate_exp_sticky_numbers_snapshot_bonuses() {
        let mut exp = GS4ExperienceState::default();

        assert!(exp.update_exp_attrs(
            Some(340),
            Some(1000),
            Some(1_234_567),
            Some(150_000),
            Some(4321),
            Some(2),
            Some(1),
            Some(1.5),
        ));
        assert_eq!(exp.field_exp, Some(340));
        assert_eq!(exp.rpa, Some(1.5));

        // A bare mindState bar (all attrs absent): the exp numbers are
        // sticky and survive, the event bonuses are snapshot and clear
        assert!(exp.update_exp_attrs(None, None, None, None, None, None, None, None));
        assert_eq!(exp.field_exp, Some(340));
        assert_eq!(exp.max_field_exp, Some(1000));
        assert_eq!(exp.exp, Some(1_234_567));
        assert_eq!(exp.ascension_exp, Some(150_000));
        assert_eq!(exp.until_next, Some(4321));
        assert_eq!(exp.fashlonae, None);
        assert_eq!(exp.lumnis, None);
        assert_eq!(exp.rpa, None);

        // Nothing left to clear: identical update is not a change
        assert!(!exp.update_exp_attrs(None, None, None, None, None, None, None, None));

        // clear() resets the exact numbers too
        exp.clear();
        assert_eq!(exp.field_exp, None);
        assert_eq!(exp.exp, None);
    }
}
