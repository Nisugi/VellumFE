//! Missing-spells watchlist: which watched spell numbers are NOT currently
//! active in the ActiveSpells or Buffs effect feeds.
//!
//! The watch list lives in `CharacterState.watched_spells` (persisted
//! per-character in the session cache) and is edited by `.spellwatch`.
//! The live feeds are authoritative for presence: the game clears and
//! re-sends a category on every change, so absence from
//! `game_state.effects` means the spell is down. Debuffs and Cooldowns
//! are deliberately ignored — a debuff being "missing" is good news, not
//! a gap in your defenses.

use crate::core::GameState;

/// Effect categories a watched spell counts as "active" in.
pub const WATCHED_CATEGORIES: [&str; 2] = ["ActiveSpells", "Buffs"];

/// One watched-but-inactive spell, ready for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingSpell {
    pub number: u16,
    pub name: String,
}

/// Display name for a spell number: the effect-list.xml table first, then
/// the name last seen on the live feed this session, then the bare number.
pub fn spell_display_name(game_state: &GameState, number: u16) -> String {
    if let Some(info) = crate::core::spell_table::spell(number) {
        return info.name;
    }
    if let Some(seen) = game_state.spell_names_seen.get(&number) {
        return seen.clone();
    }
    number.to_string()
}

/// Is `number` currently active in ActiveSpells or Buffs?
pub fn is_active(game_state: &GameState, number: u16) -> bool {
    let id = number.to_string();
    WATCHED_CATEGORIES.iter().any(|category| {
        game_state
            .effects
            .get(*category)
            .is_some_and(|store| store.effects.iter().any(|effect| effect.id == id))
    })
}

/// The watched spells that are NOT currently active, in watch-list order
/// (the order they were added).
pub fn missing(game_state: &GameState) -> Vec<MissingSpell> {
    game_state
        .character
        .watched_spells
        .iter()
        .filter(|number| !is_active(game_state, **number))
        .map(|&number| MissingSpell {
            number,
            name: spell_display_name(game_state, number),
        })
        .collect()
}

/// All spell numbers currently active in ActiveSpells/Buffs (for
/// `.spellwatch add all`), deduped, ascending.
pub fn active_numbers(game_state: &GameState) -> Vec<u16> {
    let mut numbers: Vec<u16> = WATCHED_CATEGORIES
        .iter()
        .filter_map(|category| game_state.effects.get(*category))
        .flat_map(|store| store.effects.iter())
        .filter_map(|effect| effect.id.parse::<u16>().ok())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    numbers
}

/// Parse a `.spellwatch` spell-list argument: a bare number ("606") or a
/// bracketed array ("[101,103, 107]"). Returns None on any unparseable
/// element so a typo never half-applies.
pub fn parse_spell_list(raw: &str) -> Option<Vec<u16>> {
    let raw = raw.trim();
    let inner = raw
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(raw);
    let mut numbers = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        numbers.push(part.parse::<u16>().ok()?);
    }
    (!numbers.is_empty()).then_some(numbers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_and_array_forms() {
        assert_eq!(parse_spell_list("606"), Some(vec![606]));
        assert_eq!(
            parse_spell_list("[101,103, 107]"),
            Some(vec![101, 103, 107])
        );
        assert_eq!(parse_spell_list("[ 120 ]"), Some(vec![120]));
        assert_eq!(parse_spell_list("101,103"), Some(vec![101, 103]));
    }

    #[test]
    fn parse_rejects_garbage_wholesale() {
        assert_eq!(parse_spell_list("abc"), None);
        assert_eq!(parse_spell_list("[101,abc]"), None);
        assert_eq!(parse_spell_list(""), None);
        assert_eq!(parse_spell_list("[]"), None);
    }

    fn state_with_effects(pairs: &[(&str, u16, &str)]) -> GameState {
        let mut gs = GameState::default();
        for (category, number, name) in pairs {
            let store = gs
                .effects
                .entry(category.to_string())
                .or_insert_with(|| crate::data::ActiveEffectsContent {
                    category: category.to_string(),
                    effects: Vec::new(),
                    generation: 0,
                });
            store.effects.push(crate::data::ActiveEffect {
                id: number.to_string(),
                text: name.to_string(),
                value: 100,
                time: "10:00".to_string(),
                expires_at: None,
                bar_color: None,
                text_color: None,
            });
        }
        gs
    }

    #[test]
    fn missing_reports_watched_inactive_spells_only() {
        let mut gs = state_with_effects(&[("ActiveSpells", 101, "Spirit Warding I")]);
        gs.character.watched_spells = vec![101, 606];
        let missing = missing(&gs);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].number, 606);
    }

    #[test]
    fn buffs_count_as_active_too() {
        let mut gs = state_with_effects(&[("Buffs", 9505, "Surge of Strength")]);
        gs.character.watched_spells = vec![9505];
        assert!(missing(&gs).is_empty());
    }

    #[test]
    fn debuffs_do_not_count_as_active() {
        let mut gs = state_with_effects(&[("Debuffs", 101, "Curse")]);
        gs.character.watched_spells = vec![101];
        assert_eq!(missing(&gs).len(), 1);
    }

    #[test]
    fn display_name_prefers_table_then_seen_then_number() {
        let mut gs = GameState::default();
        // 101 is in the bundled effect-list.xml.
        assert_eq!(spell_display_name(&gs, 101), "Spirit Warding I");
        // Unknown number with a feed-seen name.
        gs.spell_names_seen.insert(64000, "Custom Effect".to_string());
        assert_eq!(spell_display_name(&gs, 64000), "Custom Effect");
        // Unknown number never seen.
        assert_eq!(spell_display_name(&gs, 64001), "64001");
    }

    #[test]
    fn active_numbers_unions_and_dedupes() {
        let gs = state_with_effects(&[
            ("ActiveSpells", 101, "Spirit Warding I"),
            ("ActiveSpells", 107, "Spirit Warding II"),
            ("Buffs", 101, "Spirit Warding I"),
            ("Buffs", 9505, "Surge"),
        ]);
        assert_eq!(active_numbers(&gs), vec![101, 107, 9505]);
    }
}
