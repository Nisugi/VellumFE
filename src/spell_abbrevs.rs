use aho_corasick::AhoCorasick;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

const DEFAULT_SPELL_ABBREVS: &str = include_str!("../defaults/spell_abbrev.toml");

#[derive(Deserialize)]
struct SpellAbbrevFile {
    spells: HashMap<String, String>,
}

/// Spell name abbreviations for perception window
///
/// Loaded from ~/.vellum-fe/global/spell_abbrev.toml (extracted from defaults).
/// Used when `use_short_spell_names` is enabled in perception window settings.
///
/// Uses Aho-Corasick for efficient O(n) multi-pattern matching instead of
/// O(n * patterns) individual string replacements.
pub static SPELL_ABBREVIATIONS: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(load_spell_abbrevs()));

/// Pre-compiled Aho-Corasick automaton for efficient spell name matching
/// Built once at first use, then reused for all subsequent calls.
static SPELL_MATCHER: LazyLock<RwLock<SpellMatcher>> = LazyLock::new(|| {
    let abbrevs = match SPELL_ABBREVIATIONS.read() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("Spell abbreviation lock poisoned; recovering");
            poisoned.into_inner()
        }
    };
    RwLock::new(build_matcher(&abbrevs))
});

/// Compiled spell matcher using Aho-Corasick
struct SpellMatcher {
    ac: AhoCorasick,
    replacements: Vec<String>,
}

/// Apply spell abbreviations to a string
///
/// Replaces all known full spell names with their abbreviated forms.
/// Uses Aho-Corasick for O(n) matching instead of O(n * patterns) individual replacements.
pub fn abbreviate_spells(text: &str) -> String {
    let matcher = match SPELL_MATCHER.read() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("Spell matcher lock poisoned; recovering");
            poisoned.into_inner()
        }
    };

    // Use Aho-Corasick's replace_all_with for efficient multi-pattern replacement
    let mut result = String::with_capacity(text.len());
    matcher.ac.replace_all_with(text, &mut result, |mat, _, dst| {
        dst.push_str(&matcher.replacements[mat.pattern().as_usize()]);
        true
    });
    result
}

/// Reload spell abbreviations from disk and rebuild the matcher.
pub fn reload_spell_abbrevs() -> Result<(), String> {
    let map = load_spell_abbrevs();
    let matcher = build_matcher(&map);

    let mut abbrevs = SPELL_ABBREVIATIONS
        .write()
        .map_err(|_| "Spell abbreviation lock poisoned".to_string())?;
    *abbrevs = map;

    let mut matcher_lock = SPELL_MATCHER
        .write()
        .map_err(|_| "Spell matcher lock poisoned".to_string())?;
    *matcher_lock = matcher;

    Ok(())
}

fn build_matcher(abbrevs: &HashMap<String, String>) -> SpellMatcher {
    // Build patterns and replacement lists in parallel
    let mut patterns: Vec<&str> = Vec::with_capacity(abbrevs.len());
    let mut replacements: Vec<String> = Vec::with_capacity(abbrevs.len());

    for (full, abbrev) in abbrevs.iter() {
        patterns.push(full.as_str());
        replacements.push(abbrev.clone());
    }

    // Build Aho-Corasick automaton for O(n) matching
    let ac = AhoCorasick::new(&patterns).expect("valid spell patterns");

    SpellMatcher { ac, replacements }
}

fn load_spell_abbrevs() -> HashMap<String, String> {
    let default_map = parse_spell_abbrev(DEFAULT_SPELL_ABBREVS).unwrap_or_default();

    let path = match crate::config::Config::spell_abbrev_path() {
        Ok(path) => path,
        Err(err) => {
            tracing::warn!("Failed to resolve spell_abbrev.toml path: {}", err);
            return default_map;
        }
    };

    if !path.exists() {
        return default_map;
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => match parse_spell_abbrev(&contents) {
            Some(map) if !map.is_empty() => map,
            Some(_) => {
                tracing::warn!("spell_abbrev.toml is empty; using defaults");
                default_map
            }
            None => default_map,
        },
        Err(err) => {
            tracing::warn!("Failed to read spell_abbrev.toml: {}", err);
            default_map
        }
    }
}

fn parse_spell_abbrev(contents: &str) -> Option<HashMap<String, String>> {
    match toml::from_str::<SpellAbbrevFile>(contents) {
        Ok(file) => Some(file.spells),
        Err(err) => {
            tracing::warn!("Failed to parse spell_abbrev.toml: {}", err);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abbreviate_simple() {
        assert_eq!(abbreviate_spells("Bravery"), "Brvry");
        assert_eq!(abbreviate_spells("Heroism"), "Hrsm");
    }

    #[test]
    fn test_abbreviate_with_duration() {
        assert_eq!(abbreviate_spells("Bravery (94%)"), "Brvry (94%)");
        assert_eq!(abbreviate_spells("Song of Valor (OM)"), "SoV (OM)");
    }

    #[test]
    fn test_abbreviate_no_match() {
        assert_eq!(abbreviate_spells("Unknown Spell"), "Unknown Spell");
    }

    #[test]
    fn test_map_has_entries() {
        let abbrevs = SPELL_ABBREVIATIONS
            .read()
            .expect("spell abbreviation lock poisoned");
        assert!(abbrevs.len() > 200);
    }
}
