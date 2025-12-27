//! Bounty text parser for compact display mode.
//!
//! Transforms verbose bounty task text into 1-4 line compact format.
//! Based on minibounty.lic by Demandred.

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Compact bounty representation with 1-4 lines.
#[derive(Debug, Clone)]
pub struct CompactBounty {
    pub lines: Vec<String>,
}

impl CompactBounty {
    fn new(lines: Vec<&str>) -> Self {
        Self {
            lines: lines.into_iter().filter(|s| !s.is_empty()).map(String::from).collect(),
        }
    }

    fn from_strings(lines: Vec<String>) -> Self {
        Self {
            lines: lines.into_iter().filter(|s| !s.is_empty()).collect(),
        }
    }
}

// Long creature names that should be shortened for display
static LONGNAME_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("athletic dark-eyed incubus", "dark-eyed incubus");
    m.insert("behemothic gorefrost golem", "gorefrost golem");
    m.insert("blackened decaying tumbleweed", "decaying tumbleweed");
    m.insert("brawny gigas shield-maiden", "gigas shield-maiden");
    m.insert("cinereous chthonian sybil", "chthonian sybil");
    m.insert("colossal boreal undansormr", "boreal undansormr");
    m.insert("darkly inked fetish master", "inked fetish master");
    m.insert("decaying Citadel guardsman", "Citadel guardsman");
    m.insert("ethereal triton psionicist", "triton psionicist");
    m.insert("heavily armored battle mastodon", "battle mastodon");
    m.insert("immense gold-bristled hinterboar", "immense hinterboar");
    m.insert("patchwork flesh monstrosity", "flesh monstrosity");
    m.insert("phantasmal bestial swordsman", "bestial swordsman");
    m.insert("roiling crimson angargeist", "crimson angargeist");
    m.insert("rotting Citadel arbalester", "Citadel arbalester");
    m.insert("savage fork-tongued wendigo", "savage wendigo");
    m.insert("seething pestilent vision", "pestilent vision");
    m.insert("spectral triton protector", "triton protector");
    m.insert("squamous reptilian mutant", "reptilian mutant");
    m.insert("stunted halfling bloodspeaker", "stunted bloodspeaker");
    m.insert("withered shadow-cloaked draugr", "withered draugr");
    m.insert("writhing frost-glazed vine", "frost-glazed vine");
    m.insert("frost-glazed vine", "frost-glazed vine");
    m
});

/// Shorten long creature names for compact display.
fn shorten_creature_name(name: &str) -> String {
    for (long, short) in LONGNAME_MAP.iter() {
        if name.contains(long) {
            return name.replace(long, short);
        }
    }
    name.to_string()
}

// Compiled regex patterns for each bounty type
static RE_RESCUE_CHILD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"A local divinist has had visions of the child fleeing from (?:a|an) (?P<target>.*?) (?:in|on|near|between|under) (?:the )?(?P<area>.*?)(?:\s(?:near|between|under)|\.)").unwrap()
});

static RE_SKINNING: LazyLock<Regex> = LazyLock::new(|| {
    // Allow 1-2 spaces after period, and flexible ending
    Regex::new(r"You have been tasked to retrieve (?P<count>\d+) (?P<skin>.*?) of at least (?P<quality>.*?) quality for .*?\.\s+You can SKIN them off the corpse of (?:a|an) (?P<target>.*?) or").unwrap()
});

static RE_GEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"received orders from multiple customers requesting (?:a|an|some) (?P<gem>.*?)\..*?You have been tasked to retrieve (?P<count>\d+) (?:more )?of them\..*?You can SELL").unwrap()
});

static RE_FORAGING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"concoction that requires (?:a |an )?(?P<herb>.*?) found (?:in|on|near) (?:the )?(?P<area>.*?)(?:\s?(?:near|between|under).*\.|\.).*These samples must be in pristine condition\.\s+You have been tasked to retrieve (?P<count>\d+) (?:more )?samples?\.").unwrap()
});

static RE_BANDIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You have been tasked to (?:help )?(?P<name>[a-zA-Z]+)? ?suppress bandit activity (?:in|on|near) (?:the )?(?P<area>.*?)(?:\s?(?:near|between|under)|\.).*You need to kill (?P<count>\d+) (?:more )?of them to complete your task\.").unwrap()
});

static RE_CULLING: LazyLock<Regex> = LazyLock::new(|| {
    // Note: We don't need negative lookahead for "bandit" since we match bandit tasks first
    Regex::new(r"You have been tasked to.*?(?: help (?P<name>[a-zA-Z]+))?.*?suppress(?:ing)? (?P<target>.+?) activity (?:in|on|near) (?:the )?(?P<area>.*?)(?:\s?(?:near|between|under).*\.|\.).*You need to kill (?P<count>\d+) (?:more )?of them to complete your task\.").unwrap()
});

static RE_DANGEROUS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You have been tasked to hunt down and kill a particularly dangerous (?P<target>.*?) that has established a territory (?:in|on|near) (?:the )?(?P<area>.*?)(?:\s(?:in|on|near|between|under).*\.|\.).*You can").unwrap()
});

static RE_DANGEROUS_PROVOKED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You have been tasked to hunt down and kill a particularly dangerous (?P<target>.*?) that has established a territory (?:in|on|near) (?:the )?(?P<area>.*?)(?:\s(?:in|on|near|between|under).*\.|\.)\s+ You have provoked").unwrap()
});

static RE_HEIRLOOM_SEARCH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You have been tasked to recover (?P<heirloom>.*?) that an unfortunate citizen lost after being attacked by (?:a|an) (?P<target>.*?) (?:in|on|near) (?:the )?(?P<area>.*?)(?:\s(?:in|on|near|between|under)|\.).*The heirloom can be identified by the initials (?P<initials>.*?) engraved upon it\..*?SEARCH the area until you find it\.").unwrap()
});

static RE_HEIRLOOM_LOOT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You have been tasked to recover (?P<heirloom>.*?) that an unfortunate citizen lost after being attacked by (?:a|an) (?P<target>.*?) (?:in|on|near) (?:the )?(?P<area>.*?)(?:\s(?:in|on|near|between|under).*\.|\.).*?The heirloom can be identified by the initials (?P<initials>.*?) engraved upon it\..*?Hunt down the creature and LOOT the item from its corpse\.").unwrap()
});

static RE_ESCORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"A certain client has hired us to provide a protective escort on .* upcoming journey\..*?Go to the .* and WAIT for .* to meet you there\..*?You must guarantee .* safety to (?P<place>.*?) as soon as you can").unwrap()
});

// Status patterns
static RE_NO_TASK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You are not currently assigned a task\.").unwrap()
});

static RE_TURN_IN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You have succeeded in your task and can return to the Adventurer's Guild to receive your reward\.").unwrap()
});

static RE_VISIT_GUARD_BANDIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"It appears they have a bandit problem they'd like you to solve").unwrap()
});

static RE_VISIT_GUARD_HELP_BANDIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"in order to help (?P<person>.+?) take care of a bandit").unwrap()
});

static RE_VISIT_GUARD_CREATURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"It appears they have a creature problem they'd like you to solve").unwrap()
});

static RE_VISIT_GUARD_RESCUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"urgently needs our help in some matter").unwrap()
});

static RE_VISIT_GUARD_HEIRLOOM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"It appears they need your help in tracking down some kind of lost heirloom").unwrap()
});

static RE_VISIT_FURRIER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"The local furrier .* has an order to fill and wants our help\..*?Head over there and see what you can do\.").unwrap()
});

static RE_VISIT_FORAGER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"The local (?:halfling )?(?P<person>healer|herbalist|alchemist)(?:.s ass.*?)?, .*?, has asked for our aid\..*?Head over there and see what you can do\.").unwrap()
});

static RE_VISIT_GEMSHOP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"The local gem dealer, .*, has an order to fill and wants our help\..*?Head over there and see what you can do\.").unwrap()
});

static RE_REPORT_SUCCESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You succeeded in your task and should report back to").unwrap()
});

static RE_RETURN_HEIRLOOM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You have located (?:a|an|the|some)? .* and should bring it back to").unwrap()
});

static RE_RETURN_CHILD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"You have made contact with the child").unwrap()
});

static RE_FAILED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"The child you were tasked to rescue is gone and your task is failed\.|You have failed in your task\.  Return to the Adventurer's Guild for further instructions\.").unwrap()
});

/// Parse bounty text and return compact representation if it matches a known pattern.
///
/// Returns `None` if the text doesn't match any bounty pattern.
pub fn parse_bounty(text: &str) -> Option<CompactBounty> {
    // Normalize whitespace for matching (bounty text may span multiple lines)
    let normalized = text.replace('\n', " ").replace("  ", " ");
    let text = normalized.trim();

    // Try each pattern in order (more specific patterns first)

    // Rescue Child Task
    if let Some(caps) = RE_RESCUE_CHILD.captures(text) {
        let target = shorten_creature_name(&caps["target"]);
        let area = caps["area"].to_string();
        return Some(CompactBounty::new(vec!["Rescue Child Task", &target, &area]));
    }

    // Skinning Task
    if let Some(caps) = RE_SKINNING.captures(text) {
        let count = &caps["count"];
        let skin = &caps["skin"];
        let quality = &caps["quality"];
        let quality_short = if quality.len() > 4 { &quality[..4] } else { quality };
        let target = shorten_creature_name(&caps["target"]);
        let skin_line = format!("{} {} ({})", count, skin, quality_short);
        return Some(CompactBounty::from_strings(vec![
            "Skinning Task".to_string(),
            skin_line,
            target,
        ]));
    }

    // Gem Task
    if let Some(caps) = RE_GEM.captures(text) {
        let count = &caps["count"];
        let gem = &caps["gem"];
        let gem_line = format!("{} {}", count, gem);
        return Some(CompactBounty::from_strings(vec![
            "Gem Task".to_string(),
            gem_line,
        ]));
    }

    // Foraging Task
    if let Some(caps) = RE_FORAGING.captures(text) {
        let herb = &caps["herb"];
        let count = &caps["count"];
        let area = &caps["area"];
        let herb_line = format!("{} ({})", herb, count);
        return Some(CompactBounty::from_strings(vec![
            "Foraging Task".to_string(),
            herb_line,
            area.to_string(),
        ]));
    }

    // Bandit Task (must come before Culling to avoid false matches)
    if let Some(caps) = RE_BANDIT.captures(text) {
        let count = &caps["count"];
        let area = &caps["area"];
        let name = caps.name("name").map(|m| m.as_str());
        let title = match name {
            Some(n) => format!("Help {}", n),
            None => "Bandit Task".to_string(),
        };
        return Some(CompactBounty::from_strings(vec![
            title,
            format!("{} Bandits", count),
            area.to_string(),
        ]));
    }

    // Culling Task
    if let Some(caps) = RE_CULLING.captures(text) {
        let target = shorten_creature_name(&caps["target"]);
        let count = &caps["count"];
        let area = &caps["area"];
        let name = caps.name("name").map(|m| m.as_str());
        let title = match name {
            Some(n) => format!("Help {}", n),
            None => "Culling Task".to_string(),
        };
        return Some(CompactBounty::from_strings(vec![
            title,
            format!("{} {}", count, target),
            area.to_string(),
        ]));
    }

    // Dangerous Creature (provoked) - must come before non-provoked
    if RE_DANGEROUS_PROVOKED.is_match(text) {
        if let Some(caps) = RE_DANGEROUS_PROVOKED.captures(text) {
            let target = shorten_creature_name(&caps["target"]);
            let area = caps["area"].to_string();
            return Some(CompactBounty::from_strings(vec![
                "Found Dangerous".to_string(),
                target,
                area,
            ]));
        }
    }

    // Dangerous Creature Task
    if let Some(caps) = RE_DANGEROUS.captures(text) {
        let target = shorten_creature_name(&caps["target"]);
        let area = caps["area"].to_string();
        return Some(CompactBounty::from_strings(vec![
            "Dangerous Creature Task".to_string(),
            target,
            area,
        ]));
    }

    // Heirloom SEARCH
    if let Some(caps) = RE_HEIRLOOM_SEARCH.captures(text) {
        let target = shorten_creature_name(&caps["target"]);
        let heirloom = caps["heirloom"].split_whitespace().last().unwrap_or("item");
        let initials = &caps["initials"];
        let area = caps["area"].to_string();
        return Some(CompactBounty::from_strings(vec![
            "SEARCH Heirloom".to_string(),
            target,
            format!("{} ({})", heirloom, initials),
            area,
        ]));
    }

    // Heirloom LOOT
    if let Some(caps) = RE_HEIRLOOM_LOOT.captures(text) {
        let target = shorten_creature_name(&caps["target"]);
        let heirloom = caps["heirloom"].split_whitespace().last().unwrap_or("item");
        let initials = &caps["initials"];
        let area = caps["area"].to_string();
        return Some(CompactBounty::from_strings(vec![
            "LOOT Heirloom".to_string(),
            target,
            format!("{} ({})", heirloom, initials),
            area,
        ]));
    }

    // Escort Task
    if let Some(caps) = RE_ESCORT.captures(text) {
        let place = &caps["place"];
        return Some(CompactBounty::from_strings(vec![
            "Escort Task".to_string(),
            format!("Escort to {}", place),
        ]));
    }

    // Status messages
    if RE_NO_TASK.is_match(text) {
        return Some(CompactBounty::new(vec!["No task currently"]));
    }

    if RE_TURN_IN.is_match(text) {
        return Some(CompactBounty::new(vec!["Turn in task at AG"]));
    }

    if RE_FAILED.is_match(text) {
        return Some(CompactBounty::new(vec!["FAILED BOUNTY!!"]));
    }

    if RE_RETURN_CHILD.is_match(text) {
        return Some(CompactBounty::new(vec!["Return Child to Guard"]));
    }

    // Visit NPC patterns
    if let Some(caps) = RE_VISIT_GUARD_HELP_BANDIT.captures(text) {
        let person = &caps["person"];
        return Some(CompactBounty::from_strings(vec![
            "Visit Guard".to_string(),
            format!("Help {}", person),
            "Bandits".to_string(),
        ]));
    }

    if RE_VISIT_GUARD_BANDIT.is_match(text) {
        return Some(CompactBounty::new(vec!["Visit Guard", "Bandits"]));
    }

    if RE_VISIT_GUARD_CREATURE.is_match(text) {
        return Some(CompactBounty::new(vec!["Visit Guard", "Creatures"]));
    }

    if RE_VISIT_GUARD_RESCUE.is_match(text) {
        return Some(CompactBounty::new(vec!["Visit Guard", "Rescue Kid"]));
    }

    if RE_VISIT_GUARD_HEIRLOOM.is_match(text) {
        return Some(CompactBounty::new(vec!["Visit Guard", "Heirloom Task"]));
    }

    if RE_VISIT_FURRIER.is_match(text) {
        return Some(CompactBounty::new(vec!["Visit Furrier", "Skinning Bounty"]));
    }

    if let Some(caps) = RE_VISIT_FORAGER.captures(text) {
        let person = caps["person"].to_string();
        let title = format!("Visit {}", capitalize_first(&person));
        return Some(CompactBounty::from_strings(vec![title, "Foraging Bounty".to_string()]));
    }

    if RE_VISIT_GEMSHOP.is_match(text) {
        return Some(CompactBounty::new(vec!["Visit Gemshop", "Gem Bounty"]));
    }

    if RE_REPORT_SUCCESS.is_match(text) {
        return Some(CompactBounty::new(vec!["Visit Guard", "Report Success"]));
    }

    if RE_RETURN_HEIRLOOM.is_match(text) {
        return Some(CompactBounty::new(vec!["Visit Guard", "Return Heirloom"]));
    }

    // No pattern matched
    None
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_culling_task() {
        let text = "You have been tasked to suppress triton fanatic activity in the Atoll near Kraken's Fall. You need to kill 15 of them to complete your task.";
        let result = parse_bounty(text).unwrap();
        assert_eq!(result.lines[0], "Culling Task");
        assert_eq!(result.lines[1], "15 triton fanatic");
        assert!(result.lines[2].contains("Atoll"));
    }

    #[test]
    fn test_no_task() {
        let text = "You are not currently assigned a task.";
        let result = parse_bounty(text).unwrap();
        assert_eq!(result.lines[0], "No task currently");
    }

    #[test]
    fn test_turn_in() {
        let text = "You have succeeded in your task and can return to the Adventurer's Guild to receive your reward.";
        let result = parse_bounty(text).unwrap();
        assert_eq!(result.lines[0], "Turn in task at AG");
    }

    #[test]
    fn test_skinning_task() {
        let text = "You have been tasked to retrieve 3 troll skins of at least exceptional quality for the furrier.  You can SKIN them off the corpse of a forest troll or buy them.";
        let result = parse_bounty(text).unwrap();
        assert_eq!(result.lines[0], "Skinning Task");
        assert!(result.lines[1].contains("3"));
        assert!(result.lines[1].contains("troll skins"));
    }

    #[test]
    fn test_dangerous_creature() {
        let text = "You have been tasked to hunt down and kill a particularly dangerous fire giant that has established a territory in the Red Forest. You can track the beast down using the TRACK command.";
        let result = parse_bounty(text).unwrap();
        assert_eq!(result.lines[0], "Dangerous Creature Task");
        assert!(result.lines[1].contains("fire giant"));
    }

    #[test]
    fn test_longname_shortening() {
        let text = "You have been tasked to suppress athletic dark-eyed incubus activity in the Rift. You need to kill 10 of them to complete your task.";
        let result = parse_bounty(text).unwrap();
        assert!(result.lines[1].contains("dark-eyed incubus"));
        assert!(!result.lines[1].contains("athletic"));
    }

    #[test]
    fn test_unknown_bounty_returns_none() {
        let text = "This is just some random text that doesn't match any bounty pattern.";
        assert!(parse_bounty(text).is_none());
    }

    #[test]
    fn test_visit_furrier() {
        let text = "The local furrier Jarvis has an order to fill and wants our help.  Head over there and see what you can do.";
        let result = parse_bounty(text).unwrap();
        assert_eq!(result.lines[0], "Visit Furrier");
        assert_eq!(result.lines[1], "Skinning Bounty");
    }
}
