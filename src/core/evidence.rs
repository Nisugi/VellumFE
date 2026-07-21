//! Mapping evidence capture — the always-on local sidecar that accumulates
//! per-room observations for future mapdb submissions.
//!
//! Capture is passive and trigger-based: the parsers only fire on the fixed
//! opening phrases of `forage sense` and the ranger `sense` responses, so
//! there is no per-line cost beyond a prefix check. Observations are parsed
//! in the message pipeline (pure data, no room context), then attributed to
//! the current room uid and persisted by AppCore — the same split the sound
//! queue uses.
//!
//! Evidence NEVER mutates the in-memory mapdb. The map on screen is always
//! release data plus overrides; observations become map data only through
//! the submission pipeline's curation. This module is the raw material.
//!
//! Deliberately session-only, like ghost sketches: persisted local evidence
//! could go stale (the game changes, the room isn't revisited) and then ride
//! into a submission alongside fresh data. Dying on relog guarantees every
//! submission comes from a live walk.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One parsed capture from the game stream. Room attribution happens later,
/// in AppCore, where the current room uid is known.
#[derive(Debug, Clone, PartialEq)]
pub enum Observation {
    /// `forage sense`: forageable specimens the area supports.
    Forage(Vec<String>),
    /// Ranger `sense`: climate/terrain words, wildlife signs, the overhead
    /// creature, and visible structures (player-shop exteriors, mostly).
    Sense(SenseData),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SenseData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub climate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terrain: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wildlife: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overhead: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structures: Vec<String>,
}

const FORAGE_PREFIX: &str =
    "Glancing about, you notice the immediate area should support specimens of ";
const SENSE_PREFIX: &str =
    "You scan your surroundings, considering the various flora and fauna";

/// Split a natural-language listing ("a X, a Y and a Z") into items,
/// handling both Oxford and plain "and" joins and stripping articles.
fn split_listing(s: &str) -> Vec<String> {
    let mut items = Vec::new();
    for piece in s.split(',') {
        for part in piece.split(" and ") {
            let part = part.trim();
            let part = part.strip_prefix("and ").unwrap_or(part);
            let part = part
                .strip_prefix("an ")
                .or_else(|| part.strip_prefix("a "))
                .or_else(|| part.strip_prefix("the "))
                .unwrap_or(part)
                .trim();
            if !part.is_empty() {
                items.push(part.to_string());
            }
        }
    }
    items
}

/// Parse a `forage sense` response line into its forageables list.
pub fn parse_forage_line(line: &str) -> Option<Vec<String>> {
    let rest = line.trim().strip_prefix(FORAGE_PREFIX)?;
    let items = split_listing(rest.trim_end_matches('.'));
    (!items.is_empty()).then_some(items)
}

/// Parse a ranger `sense` response line. The opening phrase alone qualifies
/// the line; every field inside is optional (dark rooms, sparse areas).
pub fn parse_sense_line(line: &str) -> Option<SenseData> {
    let line = line.trim();
    if !line.starts_with(SENSE_PREFIX) {
        return None;
    }
    let mut data = SenseData::default();

    // "indications of the temperate climate and the sandy terrain"
    if let Some(idx) = line.find("indications of the ") {
        let tail = &line[idx + "indications of the ".len()..];
        if let Some(c_end) = tail.find(" climate") {
            data.climate = Some(tail[..c_end].trim().to_string());
        }
        if let Some(t_start) = tail.find(" and the ") {
            let t_tail = &tail[t_start + " and the ".len()..];
            if let Some(t_end) = t_tail.find(" terrain") {
                data.terrain = Some(t_tail[..t_end].trim().to_string());
            }
        }
    }

    // "signs of a X, a Y and a Z having recently been in the area"
    if let Some(idx) = line.find("signs of ") {
        let tail = &line[idx + "signs of ".len()..];
        if let Some(end) = tail.find(" having recently been in the area") {
            data.wildlife = split_listing(&tail[..end]);
        }
    }

    // "Circling overhead is a black-billed golden caracara."
    for marker in ["Circling overhead is ", "Circling overhead are "] {
        if let Some(idx) = line.find(marker) {
            let tail = &line[idx + marker.len()..];
            let end = tail.find('.').unwrap_or(tail.len());
            let items = split_listing(&tail[..end]);
            data.overhead = items.into_iter().next();
            break;
        }
    }

    // "You also see an elegant limestone store with a thick granite roof, ..."
    if let Some(idx) = line.find("You also see ") {
        let tail = &line[idx + "You also see ".len()..];
        let end = tail.find('.').unwrap_or(tail.len());
        data.structures = split_listing(&tail[..end]);
    }

    Some(data)
}

// --- Persistence -----------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForageEvidence {
    pub items: Vec<String>,
    pub observations: u32,
    /// Game-server time of the latest observation.
    pub last_seen: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SenseEvidence {
    #[serde(flatten)]
    pub data: SenseData,
    pub observations: u32,
    pub last_seen: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoomEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forage: Option<ForageEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sense: Option<SenseEvidence>,
}

/// Per-uid observation records for this session (serializable so the
/// eventual submission export is a serde call away).
#[derive(Debug, Default)]
pub struct EvidenceStore {
    records: BTreeMap<i64, RoomEvidence>,
}

impl EvidenceStore {
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn get(&self, uid: i64) -> Option<&RoomEvidence> {
        self.records.get(&uid)
    }

    /// Record an observation for a room. Latest observation wins per field,
    /// but a sparse later capture never erases richer earlier data (a dark
    /// room shouldn't blank out yesterday's structure list).
    pub fn record(&mut self, uid: i64, title: Option<String>, obs: Observation, now: i64) {
        let entry = self.records.entry(uid).or_default();
        if title.is_some() {
            entry.title = title;
        }
        match obs {
            Observation::Forage(items) => {
                let forage = entry.forage.get_or_insert_with(Default::default);
                if !items.is_empty() {
                    forage.items = items;
                }
                forage.observations += 1;
                forage.last_seen = now;
            }
            Observation::Sense(data) => {
                let sense = entry.sense.get_or_insert_with(Default::default);
                if data.climate.is_some() {
                    sense.data.climate = data.climate;
                }
                if data.terrain.is_some() {
                    sense.data.terrain = data.terrain;
                }
                if !data.wildlife.is_empty() {
                    sense.data.wildlife = data.wildlife;
                }
                if data.overhead.is_some() {
                    sense.data.overhead = data.overhead;
                }
                if !data.structures.is_empty() {
                    sense.data.structures = data.structures;
                }
                sense.observations += 1;
                sense.last_seen = now;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORAGE_LINE: &str = "Glancing about, you notice the immediate area should support specimens of acantha leaf, murdroot, wingstem root, bur-clover root, small coconut, pink peppercorn, black peppercorn, cardamom, vanilla bean, nutmeg, cumin seeds, and star anise.";
    const SENSE_LINE: &str = "You scan your surroundings, considering the various flora and fauna found here (and noting those which are absent).  The indications of the temperate climate and the sandy terrain are clearly evident to your seasoned eye.  You quickly note the signs of a lop-eared pale golden yowler, a sharp-nosed dune curhound and a crop-tailed coastal muzzlerat having recently been in the area.  Circling overhead is a black-billed golden caracara.  You also see an elegant limestone store with a thick granite roof, a tall whitewashed deckhouse with an ornate seashelled roof, a warped wooden hut with a thatched grass roof, a dark speckled marble storefront and a grottoed seaglass beach shack with a buccaneer-garbed cypress bear mounted to the roof.";

    #[test]
    fn forage_line_parses_all_items() {
        let items = parse_forage_line(FORAGE_LINE).expect("forage line");
        assert_eq!(items.len(), 12);
        assert_eq!(items[0], "acantha leaf");
        assert_eq!(items[3], "bur-clover root");
        assert_eq!(items[11], "star anise");
        assert!(parse_forage_line("You forage around but find nothing.").is_none());
    }

    #[test]
    fn sense_line_parses_every_section() {
        let data = parse_sense_line(SENSE_LINE).expect("sense line");
        assert_eq!(data.climate.as_deref(), Some("temperate"));
        assert_eq!(data.terrain.as_deref(), Some("sandy"));
        assert_eq!(
            data.wildlife,
            vec![
                "lop-eared pale golden yowler",
                "sharp-nosed dune curhound",
                "crop-tailed coastal muzzlerat"
            ]
        );
        assert_eq!(data.overhead.as_deref(), Some("black-billed golden caracara"));
        assert_eq!(data.structures.len(), 5);
        assert_eq!(
            data.structures[0],
            "elegant limestone store with a thick granite roof"
        );
        assert!(parse_sense_line("You scan the horizon.").is_none());
    }

    #[test]
    fn store_merges_without_erasing_richer_data() {
        let mut store = EvidenceStore::default();

        store.record(
            731009,
            Some("[Sandy Beach]".into()),
            Observation::Sense(parse_sense_line(SENSE_LINE).unwrap()),
            1000,
        );
        // A later, sparser sense (dark room) must not blank earlier fields.
        store.record(
            731009,
            None,
            Observation::Sense(SenseData::default()),
            2000,
        );
        store.record(
            731009,
            None,
            Observation::Forage(parse_forage_line(FORAGE_LINE).unwrap()),
            3000,
        );
        let rec = store.get(731009).unwrap();
        assert_eq!(rec.title.as_deref(), Some("[Sandy Beach]"));
        let sense = rec.sense.as_ref().unwrap();
        assert_eq!(sense.observations, 2);
        assert_eq!(sense.last_seen, 2000);
        assert_eq!(sense.data.climate.as_deref(), Some("temperate"));
        assert_eq!(sense.data.structures.len(), 5);
        assert_eq!(rec.forage.as_ref().unwrap().items.len(), 12);
        assert_eq!(store.len(), 1);
    }
}
