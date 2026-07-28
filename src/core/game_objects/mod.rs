//! The GameObjects registry: one queryable model of the items, creatures,
//! and players the game feed has told us are present.
//!
//! This is Lich's `GameObj` reimplemented from the same stream Lich reads
//! (verified: `lich-5/lib/common/xmlparser.rb` builds its inv from the
//! `<inv>` push feed, exactly what VellumFE parses). It replaces a set of
//! per-feature silos that each re-derive items with their own parsing:
//! `container_cache`, `room_objects`, `room_creatures`, `room_players`,
//! and the bare-string hands/worn fields — plus a second, drifted `<a>`
//! parser in the TUI container window.
//!
//! **It is a pure data model, not a scripting engine.** It exposes queries
//! (`items_in`, `carried`, `hostile_creatures`, `classify`), never
//! behavior. The automation lease remains the only thing that acts;
//! features read from here and act through the lease.
//!
//! Migration is incremental (see the plan): this skeleton stands up the
//! model + the single anchor parser + classification queries. Consumers
//! move onto it one at a time; nothing is wired to it yet.
//!
//! Owned by `GameState` so it serializes to remote/phone clients, the way
//! `room_creatures` already flows.

// Skeleton step: the query/ingest surface is defined ahead of its
// consumers, which migrate onto it one at a time. Remove this once the
// widgets + foreach read from the registry (plan steps 3-6).
#![allow(dead_code)]

pub mod parse;

pub use parse::parse_anchor;

use std::collections::HashMap;

use crate::core::gameobj_data::GameObjData;

/// One object from the game feed: the `(id, noun, name)` triple every
/// silo already keys on. `id` is the exist id used in `#<id>` game
/// commands (works on direct connections). `name` is article-free — the
/// exact string `gameobj-data.xml` regexes and Lich's `GameObj#name` see.
///
/// `weight` is `None` on the today (Wrayth) text feeds, which don't carry
/// it; the Saga `<inventoryManager>` feed fills it. Kept optional so the
/// same struct serves both sources — see the two-feeds section of the
/// registry plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameItem {
    pub id: String,
    pub noun: String,
    pub name: String,
    /// Item weight (Saga feed only; None on the text feeds).
    pub weight: Option<u32>,
}

impl GameItem {
    pub fn new(
        id: impl Into<String>,
        noun: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        GameItem {
            id: id.into(),
            noun: noun.into(),
            name: name.into(),
            weight: None,
        }
    }
}

/// Where an item currently lives. Mirrors Lich's per-item container id,
/// including its pseudo-locations for non-container sources.
///
/// `Ground` and `RoomDesc` are deliberately distinct: Lich populates them
/// from different stream sections (`GameObj.loot` from the "you also see"
/// objs line vs `GameObj.new_room_desc` from `<a>` links inside the room
/// description prose). foreach's `floor`/`ground`/`room` targets read the
/// former; its `desc` target reads the latter. They are different sets —
/// you pick up ground loot, you reference described scenery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    /// Inside a tracked container (bag, sack, ...), by container id.
    Container(String),
    /// Worn on the body.
    Worn,
    /// At your feet ("Placed alongside you:"). Distinct from room Ground:
    /// these are YOUR items — they count toward encumbrance and a heavy
    /// one can pin you in place — they just aren't worn or held. Arrives
    /// in the `inv` stream after the worn block.
    AtFeet,
    /// On the ground in the current room ("you also see" objs line) — NOT
    /// yours; loot you could pick up. Distinct from AtFeet.
    Ground,
    /// Linked inside the room description prose (scenery: fountains,
    /// doors, signs). Referenceable but not ground loot.
    RoomDesc,
    /// In a hand.
    Hand(Hand),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

/// Mark/registration status for an item, populated only by an active
/// `INVENTORY FULL` scan (the passive feed doesn't carry it). `None`
/// fields mean "not yet scanned", distinct from a known false.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemStatus {
    pub marked: Option<bool>,
    pub registered: Option<bool>,
}

/// Open/locked state of a container. All-false is the default and the
/// only thing the text feeds can report (via prose); the Saga feed sets
/// them from `flags='closed,locked'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContainerFlags {
    pub closed: bool,
    pub locked: bool,
}

/// A tracked container: identity, its game-command target, and contents.
///
/// `flags`/`capacity_in`/`capacity_on` are populated only by the Saga
/// `<inventoryManager>` feed; the today (Wrayth) feeds leave them at
/// default/None. See the two-feeds section of the registry plan.
#[derive(Clone, Debug, Default)]
pub struct Container {
    pub id: String,
    pub title: String,
    pub title_lower: String,
    /// The `target` attribute (`#<exist-id>`) from the `<container>` tag —
    /// the id game commands use. Differs from `id` for `stow`. `None`
    /// until a `<container>` tag is seen.
    pub target: Option<String>,
    pub items: Vec<GameItem>,
    /// Open/locked state (Saga feed; default on text feeds).
    pub flags: ContainerFlags,
    /// Interior capacity (`in_max`) and surface capacity (`on_max`) from
    /// the Saga feed; None on text feeds.
    pub capacity_in: Option<u32>,
    pub capacity_on: Option<u32>,
    /// Bumped on every contents change (widgets diff on this).
    pub generation: u64,
}

impl Container {
    /// The id to use in game commands (`put X in #<here>`): the `target`
    /// object id when known, else `#<id>` for normal numeric containers.
    /// Strips a leading `#` since callers add their own.
    pub fn command_target(&self) -> String {
        match &self.target {
            Some(t) => t.trim_start_matches('#').to_string(),
            None => self.id.clone(),
        }
    }
}

/// The unified registry. Every collection carries a generation counter so
/// widgets keep their existing diff-on-generation sync.
#[derive(Clone, Debug, Default)]
pub struct GameObjects {
    containers: HashMap<String, Container>,
    /// Container insertion order for oldest-first eviction (cap growth).
    container_order: Vec<String>,
    /// Pickable items on the ground (the "you also see" objs line).
    ground: Vec<GameItem>,
    /// Scenery linked in the room description prose (Lich's room_desc);
    /// referenceable but distinct from ground loot.
    room_desc: Vec<GameItem>,
    worn: Vec<GameItem>,
    /// Items at your feet ("Placed alongside you:") — yours, encumbering,
    /// distinct from room ground. See `Location::AtFeet`.
    at_feet: Vec<GameItem>,
    left_hand: Option<GameItem>,
    right_hand: Option<GameItem>,
    /// exist id -> status, from the last INVENTORY FULL scan.
    statuses: HashMap<String, ItemStatus>,
    pub generation: u64,
}

/// Cap on tracked containers; the feed creates one per unique id.
const MAX_CONTAINERS: usize = 1000;

impl GameObjects {
    // ---- container ingest (fed by the parser; see migration step 2) ----

    /// Register or update a container's identity (from `<container>`).
    pub fn register_container(&mut self, id: String, title: String, target: Option<String>) {
        if let Some(c) = self.containers.get_mut(&id) {
            if c.title != title {
                c.title_lower = title.to_lowercase();
                c.title = title;
                c.generation += 1;
            }
            if target.is_some() && c.target != target {
                c.target = target;
                c.generation += 1;
            }
        } else {
            self.evict_if_full();
            self.container_order.push(id.clone());
            self.containers.insert(
                id.clone(),
                Container {
                    title_lower: title.to_lowercase(),
                    title,
                    target,
                    id,
                    items: Vec::new(),
                    generation: 0,
                    ..Default::default()
                },
            );
        }
    }

    /// Clear a container's contents (from `<clearContainer>`).
    pub fn clear_container(&mut self, id: &str) {
        if let Some(c) = self.containers.get_mut(id) {
            c.items.clear();
            c.generation += 1;
        }
    }

    /// Add a parsed item to a container (from an `<inv>` item line).
    /// Auto-creates a title-less container if items precede its
    /// `<container>` tag (the feed can do that).
    pub fn add_container_item(&mut self, container_id: &str, item: GameItem) {
        if let Some(c) = self.containers.get_mut(container_id) {
            c.items.push(item);
            c.generation += 1;
        } else {
            self.evict_if_full();
            self.container_order.push(container_id.to_string());
            self.containers.insert(
                container_id.to_string(),
                Container {
                    id: container_id.to_string(),
                    items: vec![item],
                    generation: 1,
                    ..Default::default()
                },
            );
        }
    }

    fn evict_if_full(&mut self) {
        while self.containers.len() >= MAX_CONTAINERS && !self.container_order.is_empty() {
            let oldest = self.container_order.remove(0);
            self.containers.remove(&oldest);
        }
    }

    // ---- other-source ingest ----

    pub fn set_ground(&mut self, items: Vec<GameItem>) {
        if self.ground != items {
            self.ground = items;
            self.generation += 1;
        }
    }

    /// Scenery objects linked in the room description (Lich's room_desc).
    /// Distinct from ground loot — see `Location::RoomDesc`.
    pub fn set_room_desc(&mut self, items: Vec<GameItem>) {
        if self.room_desc != items {
            self.room_desc = items;
            self.generation += 1;
        }
    }

    /// Replace worn AND at-feet items from the buffered `inv` block. The
    /// game sends both under one push: worn items after "Your worn items
    /// are:", then optionally items after "Placed alongside you:" (at your
    /// feet — yours, encumbering). We split on that header, so the two land
    /// in separate collections.
    ///
    /// Each item line contributes its first link-bearing segment (the
    /// item's `<a>`), reusing the main parser's already-resolved link_data
    /// rather than re-parsing XML. Header/blank lines carry no link.
    /// Verified against real feeds 2026-01-02 and 2026-07-28.
    pub fn set_worn_from_lines(&mut self, lines: &[Vec<crate::data::widget::TextSegment>]) {
        fn line_text(segments: &[crate::data::widget::TextSegment]) -> String {
            segments.iter().map(|s| s.text.as_str()).collect()
        }
        fn item_of(segments: &[crate::data::widget::TextSegment]) -> Option<GameItem> {
            let seg = segments.iter().find(|s| s.link_data.is_some())?;
            let link = seg.link_data.as_ref()?;
            Some(GameItem {
                id: link.exist_id.clone(),
                noun: link.noun.clone(),
                name: seg.text.trim().to_string(),
                weight: None,
            })
        }

        let mut worn = Vec::new();
        let mut at_feet = Vec::new();
        // Everything after the "Placed alongside you:" header is at-feet.
        let mut in_feet = false;
        for segments in lines {
            if line_text(segments)
                .trim_start()
                .starts_with("Placed alongside you")
            {
                in_feet = true;
                continue;
            }
            if let Some(item) = item_of(segments) {
                if in_feet {
                    at_feet.push(item);
                } else {
                    worn.push(item);
                }
            }
        }
        self.set_worn(worn);
        self.set_at_feet(at_feet);
    }

    pub fn set_worn(&mut self, items: Vec<GameItem>) {
        if self.worn != items {
            self.worn = items;
            self.generation += 1;
        }
    }

    /// Items at your feet ("Placed alongside you:") — yours + encumbering,
    /// distinct from room ground.
    pub fn set_at_feet(&mut self, items: Vec<GameItem>) {
        if self.at_feet != items {
            self.at_feet = items;
            self.generation += 1;
        }
    }

    pub fn set_hands(&mut self, left: Option<GameItem>, right: Option<GameItem>) {
        if self.left_hand != left || self.right_hand != right {
            self.left_hand = left;
            self.right_hand = right;
            self.generation += 1;
        }
    }

    /// Record mark/registration status for an item (from an INV FULL scan).
    pub fn set_status(&mut self, item_id: String, status: ItemStatus) {
        self.statuses.insert(item_id, status);
    }

    // ---- queries (the consumer surface) ----

    pub fn container(&self, id: &str) -> Option<&Container> {
        self.containers.get(id)
    }

    pub fn containers(&self) -> impl Iterator<Item = &Container> {
        self.containers.values()
    }

    /// Items inside a tracked container.
    pub fn items_in(&self, container_id: &str) -> &[GameItem] {
        self.containers
            .get(container_id)
            .map(|c| c.items.as_slice())
            .unwrap_or(&[])
    }

    pub fn ground(&self) -> &[GameItem] {
        &self.ground
    }

    /// Scenery objects from the room description (the `desc` target).
    pub fn room_desc(&self) -> &[GameItem] {
        &self.room_desc
    }

    pub fn worn(&self) -> &[GameItem] {
        &self.worn
    }

    /// Items at your feet ("Placed alongside you:") — yours, encumbering.
    pub fn at_feet(&self) -> &[GameItem] {
        &self.at_feet
    }

    pub fn hand(&self, hand: Hand) -> Option<&GameItem> {
        match hand {
            Hand::Left => self.left_hand.as_ref(),
            Hand::Right => self.right_hand.as_ref(),
        }
    }

    /// Everything in your possession: worn + both hands + at-feet. At-feet
    /// items count toward encumbrance and are yours, so they belong here
    /// (they're in the `inv` enumeration, unlike room ground loot). Does
    /// not include container contents.
    pub fn carried(&self) -> Vec<&GameItem> {
        self.worn
            .iter()
            .chain(self.left_hand.iter())
            .chain(self.right_hand.iter())
            .chain(self.at_feet.iter())
            .collect()
    }

    pub fn status_of(&self, item_id: &str) -> Option<&ItemStatus> {
        self.statuses.get(item_id)
    }

    /// Resolve a user's container reference tolerantly (`my bando`,
    /// `the sack`, `boar hide`) — the logic that lets `.foreach` and
    /// `move to` name containers the way players do. Exact/substring
    /// matches win; then article-stripped substring; then per-word prefix
    /// subsequence.
    pub fn find_container(&self, query: &str) -> Option<&Container> {
        let q = query.to_lowercase();
        // exact
        if let Some(c) = self.containers.values().find(|c| c.title_lower == q) {
            return Some(c);
        }
        // substring on the raw query
        if let Some(c) = self.containers.values().find(|c| c.title_lower.contains(&q)) {
            return Some(c);
        }
        let words = parse::strip_articles(&q);
        if words.is_empty() {
            return None;
        }
        let stripped = words.join(" ");
        if let Some(c) = self
            .containers
            .values()
            .find(|c| c.title_lower.contains(&stripped))
        {
            return Some(c);
        }
        self.containers
            .values()
            .find(|c| parse::title_matches_words(&c.title_lower, &words))
    }

    // ---- classification (delegates to the data-pack classifier) ----

    /// All type tags for an item ("gem", "valuable", ...), via
    /// `gameobj-data.xml`. Comma-joined string available as `classify`.
    pub fn types_of<'a>(&self, item: &GameItem, data: &'a GameObjData) -> Vec<&'a str> {
        data.types_of(&item.name, &item.noun)
    }

    /// Comma-joined type string, Lich `GameObj#type` shape, or None.
    pub fn classify(&self, item: &GameItem, data: &GameObjData) -> Option<String> {
        data.classify(&item.name, &item.noun)
    }

    pub fn sellable(&self, item: &GameItem, data: &GameObjData) -> Option<String> {
        data.sellable(&item.name, &item.noun)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameObjData {
        GameObjData::parse(
            r#"<data>
                 <type name="gem"><name>^(blue sapphire|quartz crystal)$</name></type>
               </data>"#,
        )
    }

    #[test]
    fn container_ingest_and_command_target() {
        let mut reg = GameObjects::default();
        // Normal container: target == #id.
        reg.register_container(
            "77".into(),
            "Bandolier".into(),
            Some("#77".into()),
        );
        reg.add_container_item("77", GameItem::new("101", "crystal", "quartz crystal"));
        assert_eq!(reg.items_in("77").len(), 1);
        assert_eq!(reg.container("77").unwrap().command_target(), "77");

        // stow: id is the string, target is the object id.
        reg.register_container("stow".into(), "My Shroud".into(), Some("#691".into()));
        assert_eq!(reg.container("stow").unwrap().command_target(), "691");
    }

    #[test]
    fn find_container_is_article_and_abbreviation_tolerant() {
        let mut reg = GameObjects::default();
        reg.register_container("77".into(), "iron boar hide bandolier".into(), None);
        reg.register_container("88".into(), "coal black purse".into(), None);
        assert_eq!(reg.find_container("my bando").map(|c| &c.id), Some(&"77".into()));
        assert_eq!(reg.find_container("boar hide").map(|c| &c.id), Some(&"77".into()));
        assert_eq!(reg.find_container("my purse").map(|c| &c.id), Some(&"88".into()));
        assert!(reg.find_container("my locker").is_none());
    }

    #[test]
    fn set_worn_from_lines_skips_header_and_takes_links() {
        use crate::data::widget::{LinkData, TextSegment};
        let link = |id: &str, noun: &str, name: &str| TextSegment {
            text: name.to_string(),
            link_data: Some(LinkData {
                exist_id: id.to_string(),
                noun: noun.to_string(),
                text: name.to_string(),
                coord: None,
            }),
            ..Default::default()
        };
        // Mirrors the real 2026-01-02 feed: header line (no link), then
        // item lines where the article sits OUTSIDE the anchor as its own
        // plain segment.
        let lines = vec![
            vec![TextSegment::plain("Your worn items are:")],
            vec![
                TextSegment::plain("  a "),
                link("417774141", "ring", "smooth bone ring"),
                TextSegment::plain(" pierced by tiny crystal shards"),
            ],
            vec![
                TextSegment::plain("  some viridian "),
                link("417774188", "pants", "elesine pants"),
                TextSegment::plain(" patterned with vivid metallic-hued briars"),
            ],
            // Blank line — no link, skipped.
            vec![TextSegment::plain("")],
        ];
        let mut reg = GameObjects::default();
        reg.set_worn_from_lines(&lines);
        let worn = reg.worn();
        assert_eq!(worn.len(), 2, "header + blank skipped, 2 items kept");
        assert_eq!(worn[0].id, "417774141");
        assert_eq!(worn[0].name, "smooth bone ring");
        assert_eq!(worn[1].id, "417774188");
        assert_eq!(worn[1].noun, "pants");
        assert_eq!(worn[1].name, "elesine pants");
        assert!(reg.at_feet().is_empty(), "no Placed-alongside block here");
    }

    #[test]
    fn set_worn_from_lines_splits_at_feet() {
        use crate::data::widget::{LinkData, TextSegment};
        let link = |id: &str, noun: &str, name: &str| TextSegment {
            text: name.to_string(),
            link_data: Some(LinkData {
                exist_id: id.to_string(),
                noun: noun.to_string(),
                text: name.to_string(),
                coord: None,
            }),
            ..Default::default()
        };
        // Real 2026-07-28 feed shape: worn block, then "Placed alongside
        // you:" header, then feet items. The jewel must NOT be worn.
        let lines = vec![
            vec![TextSegment::plain("Your worn items are:")],
            vec![
                TextSegment::plain("  some "),
                link("511640642", "gloves", "darkened triton hide gloves"),
                TextSegment::plain(" with faenor-plated knuckles"),
            ],
            vec![TextSegment::plain("")],
            vec![TextSegment::plain("Placed alongside you:")],
            vec![
                TextSegment::plain("  a "),
                link("511775582", "jewel", "trilliant-cut leaden grey jewel"),
                TextSegment::plain(" mottled with fractal forms"),
            ],
        ];
        let mut reg = GameObjects::default();
        reg.set_worn_from_lines(&lines);
        assert_eq!(reg.worn().len(), 1);
        assert_eq!(reg.worn()[0].id, "511640642");
        assert_eq!(reg.at_feet().len(), 1, "the jewel is at feet, not worn");
        assert_eq!(reg.at_feet()[0].id, "511775582");
        assert_eq!(reg.at_feet()[0].noun, "jewel");
        // At-feet counts as carried (encumbrance), room ground would not.
        assert_eq!(reg.carried().len(), 2);
    }

    #[test]
    fn carried_is_worn_plus_hands() {
        let mut reg = GameObjects::default();
        reg.set_worn(vec![GameItem::new("1", "cloak", "wool cloak")]);
        reg.set_hands(
            Some(GameItem::new("2", "sword", "short sword")),
            None,
        );
        let carried: Vec<&str> = reg.carried().iter().map(|i| i.id.as_str()).collect();
        assert_eq!(carried, vec!["1", "2"]);
    }

    #[test]
    fn ground_and_room_desc_are_separate_sets() {
        // Lich populates these from different stream sections; foreach's
        // floor/ground/room vs desc targets read them separately.
        let mut reg = GameObjects::default();
        reg.set_ground(vec![GameItem::new("1", "ring", "silver ring")]);
        reg.set_room_desc(vec![GameItem::new("2", "fountain", "marble fountain")]);
        assert_eq!(reg.ground().len(), 1);
        assert_eq!(reg.room_desc().len(), 1);
        assert_eq!(reg.ground()[0].id, "1");
        assert_eq!(reg.room_desc()[0].id, "2");
        // Setting one must not touch the other.
        reg.set_ground(vec![]);
        assert!(reg.ground().is_empty());
        assert_eq!(reg.room_desc().len(), 1, "room_desc untouched");
    }

    #[test]
    fn saga_metadata_fields_default_on_text_feeds() {
        // The today (Wrayth) ingest leaves weight/flags/capacity at their
        // defaults; the Saga feed will fill them. This pins that contract
        // so the fields exist and default cleanly for both sources.
        let mut reg = GameObjects::default();
        reg.register_container("77".into(), "Bandolier".into(), Some("#77".into()));
        reg.add_container_item("77", GameItem::new("101", "crystal", "quartz crystal"));
        let c = reg.container("77").unwrap();
        assert_eq!(c.flags, ContainerFlags::default());
        assert!(c.capacity_in.is_none() && c.capacity_on.is_none());
        assert!(c.items[0].weight.is_none());
    }

    #[test]
    fn classification_delegates_to_data_pack() {
        let reg = GameObjects::default();
        let data = data();
        let gem = GameItem::new("1", "crystal", "quartz crystal");
        let junk = GameItem::new("2", "rock", "smooth rock");
        assert_eq!(reg.classify(&gem, &data).as_deref(), Some("gem"));
        assert!(reg.classify(&junk, &data).is_none());
    }

    #[test]
    fn set_ground_bumps_generation_only_on_change() {
        let mut reg = GameObjects::default();
        let g0 = reg.generation;
        reg.set_ground(vec![GameItem::new("1", "ring", "silver ring")]);
        assert!(reg.generation > g0);
        let g1 = reg.generation;
        reg.set_ground(vec![GameItem::new("1", "ring", "silver ring")]);
        assert_eq!(reg.generation, g1, "no-op set must not bump");
    }
}
