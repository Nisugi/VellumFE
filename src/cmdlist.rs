//! Parser/lookup helpers for the `cmdlist1.xml` dataset distributed with Lich.
//!
//! Cmdlist entries power the radial context menu system by mapping coordinates
//! to displayable text/commands and providing placeholder substitution.

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::fs;

/// A single command list entry from cmdlist1.xml
#[derive(Debug, Clone)]
pub struct CmdListEntry {
    pub coord: String,    // e.g., "2524,2061"
    pub menu: String,     // Display text: e.g., "look @"
    pub command: String,  // Command to send: e.g., "look #"
    pub menu_cat: String, // Category: e.g., "1" or "5_roleplay"
}

/// Parser and lookup for cmdlist1.xml
#[derive(Clone)]
pub struct CmdList {
    entries: HashMap<String, CmdListEntry>, // coord -> entry
}

impl CmdList {
    /// Load cmdlist1.xml from ~/.vellum-fe/cmdlist1.xml (single source of truth)
    pub fn load() -> Result<Self> {
        let path = crate::config::Config::cmdlist_path()?;

        if !path.exists() {
            return Err(anyhow::anyhow!(
                "cmdlist1.xml not found at {}. This should have been extracted on first run!",
                path.display()
            ));
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read cmdlist1.xml from {}", path.display()))?;

        Self::parse(&content)
    }

    /// Parse cmdlist1.xml content
    fn parse(content: &str) -> Result<Self> {
        let mut reader = Reader::from_str(content);
        reader.config_mut().trim_text(true);

        let mut entries = HashMap::new();
        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    if e.name().as_ref() == b"cli" {
                        // Parse <cli coord="..." menu="..." command="..." menu_cat="..."/>
                        let mut coord = None;
                        let mut menu = None;
                        let mut command = None;
                        let mut menu_cat = None;

                        for attr in e.attributes() {
                            let attr = attr?;
                            let key = attr.key.as_ref();
                            let value = String::from_utf8_lossy(&attr.value).to_string();

                            match key {
                                b"coord" => coord = Some(value),
                                b"menu" => menu = Some(value),
                                b"command" => command = Some(value),
                                b"menu_cat" => menu_cat = Some(value),
                                _ => {}
                            }
                        }

                        // Store entry if we have all required fields
                        if let (Some(coord), Some(menu), Some(command), Some(menu_cat)) =
                            (coord, menu, command, menu_cat)
                        {
                            entries.insert(
                                coord.clone(),
                                CmdListEntry {
                                    coord,
                                    menu,
                                    command,
                                    menu_cat,
                                },
                            );
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "XML parse error at position {}: {}",
                        reader.buffer_position(),
                        e
                    ))
                }
                _ => {}
            }
            buf.clear();
        }

        tracing::info!("Loaded {} command list entries", entries.len());
        Ok(Self { entries })
    }

    /// Look up a command entry by coord
    pub fn get(&self, coord: &str) -> Option<&CmdListEntry> {
        self.entries.get(coord)
    }

    /// Substitute placeholders in a command string
    /// @ = noun (display text)
    /// # = "#exist_id" (with # prefix)
    /// % = secondary item placeholder (for commands like "transfer @ %")
    pub fn substitute_command(
        command: &str,
        noun: &str,
        exist_id: &str,
        secondary: Option<&str>,
    ) -> String {
        let mut result = command.to_string();

        // Replace @ with noun
        result = result.replace('@', noun);

        // Replace # with #exist_id
        result = result.replace('#', &format!("#{}", exist_id));

        // Replace % with secondary item if provided
        if let Some(sec) = secondary {
            result = result.replace('%', sec);
        }

        result
    }

    /// Substitute placeholders in menu text
    /// @ = noun (display text)
    pub fn substitute_menu(menu: &str, noun: &str) -> String {
        menu.replace('@', noun)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== substitute_command Tests ====================

    #[test]
    fn test_substitute_command_at_symbol() {
        assert_eq!(
            CmdList::substitute_command("look @", "pendant", "12345", None),
            "look pendant"
        );
    }

    #[test]
    fn test_substitute_command_hash_symbol() {
        assert_eq!(
            CmdList::substitute_command("look #", "pendant", "12345", None),
            "look #12345"
        );
    }

    #[test]
    fn test_substitute_command_percent_symbol() {
        assert_eq!(
            CmdList::substitute_command("transfer # %", "pendant", "12345", Some("right arm")),
            "transfer #12345 right arm"
        );
    }

    #[test]
    fn test_substitute_command_all_placeholders() {
        assert_eq!(
            CmdList::substitute_command("give @ to # in %", "gold", "99999", Some("bag")),
            "give gold to #99999 in bag"
        );
    }

    #[test]
    fn test_substitute_command_no_placeholders() {
        assert_eq!(
            CmdList::substitute_command("inventory", "pendant", "12345", None),
            "inventory"
        );
    }

    #[test]
    fn test_substitute_command_multiple_at_symbols() {
        assert_eq!(
            CmdList::substitute_command("@ attacks @", "orc", "12345", None),
            "orc attacks orc"
        );
    }

    #[test]
    fn test_substitute_command_multiple_hash_symbols() {
        assert_eq!(
            CmdList::substitute_command("compare # with #", "sword", "555", None),
            "compare #555 with #555"
        );
    }

    #[test]
    fn test_substitute_command_empty_noun() {
        assert_eq!(
            CmdList::substitute_command("look @", "", "12345", None),
            "look "
        );
    }

    #[test]
    fn test_substitute_command_empty_exist_id() {
        assert_eq!(
            CmdList::substitute_command("look #", "pendant", "", None),
            "look #"
        );
    }

    #[test]
    fn test_substitute_command_percent_without_secondary() {
        // If no secondary is provided, % stays in the string
        assert_eq!(
            CmdList::substitute_command("transfer % to me", "pendant", "12345", None),
            "transfer % to me"
        );
    }

    #[test]
    fn test_substitute_command_special_chars_in_noun() {
        assert_eq!(
            CmdList::substitute_command("look @", "rusty<iron>sword", "12345", None),
            "look rusty<iron>sword"
        );
    }

    #[test]
    fn test_substitute_command_unicode_noun() {
        assert_eq!(
            CmdList::substitute_command("look @", "魔剣", "12345", None),
            "look 魔剣"
        );
    }

    // ==================== substitute_menu Tests ====================

    #[test]
    fn test_substitute_menu_basic() {
        assert_eq!(
            CmdList::substitute_menu("look @", "pendant"),
            "look pendant"
        );
    }

    #[test]
    fn test_substitute_menu_no_placeholder() {
        assert_eq!(
            CmdList::substitute_menu("inventory", "pendant"),
            "inventory"
        );
    }

    #[test]
    fn test_substitute_menu_multiple_placeholders() {
        assert_eq!(
            CmdList::substitute_menu("@ attacks @", "goblin"),
            "goblin attacks goblin"
        );
    }

    #[test]
    fn test_substitute_menu_empty_noun() {
        assert_eq!(CmdList::substitute_menu("look @", ""), "look ");
    }

    #[test]
    fn test_substitute_menu_long_noun() {
        let long_noun = "a".repeat(1000);
        let result = CmdList::substitute_menu("look @", &long_noun);
        assert!(result.starts_with("look "));
        assert_eq!(result.len(), 5 + 1000);
    }

    // ==================== parse Tests ====================

    #[test]
    fn test_parse_empty_content() {
        let content = "";
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        let cmdlist = result.unwrap();
        assert!(cmdlist.entries.is_empty());
    }

    #[test]
    fn test_parse_empty_cmdlist() {
        let content = r#"<cmdlist></cmdlist>"#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        assert!(result.unwrap().entries.is_empty());
    }

    #[test]
    fn test_parse_single_entry() {
        let content = r#"
            <cmdlist>
                <cli coord="100,200" menu="look @" command="look #" menu_cat="1"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        let cmdlist = result.unwrap();
        assert_eq!(cmdlist.entries.len(), 1);

        let entry = cmdlist.get("100,200").unwrap();
        assert_eq!(entry.coord, "100,200");
        assert_eq!(entry.menu, "look @");
        assert_eq!(entry.command, "look #");
        assert_eq!(entry.menu_cat, "1");
    }

    #[test]
    fn test_parse_multiple_entries() {
        let content = r#"
            <cmdlist>
                <cli coord="100,200" menu="look @" command="look #" menu_cat="1"/>
                <cli coord="300,400" menu="get @" command="get #" menu_cat="2"/>
                <cli coord="500,600" menu="drop @" command="drop #" menu_cat="3"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        let cmdlist = result.unwrap();
        assert_eq!(cmdlist.entries.len(), 3);

        assert!(cmdlist.get("100,200").is_some());
        assert!(cmdlist.get("300,400").is_some());
        assert!(cmdlist.get("500,600").is_some());
    }

    #[test]
    fn test_parse_entry_missing_coord() {
        let content = r#"
            <cmdlist>
                <cli menu="look @" command="look #" menu_cat="1"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        // Entry should be skipped since coord is required
        assert!(result.unwrap().entries.is_empty());
    }

    #[test]
    fn test_parse_entry_missing_menu() {
        let content = r#"
            <cmdlist>
                <cli coord="100,200" command="look #" menu_cat="1"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        // Entry should be skipped since menu is required
        assert!(result.unwrap().entries.is_empty());
    }

    #[test]
    fn test_parse_entry_missing_command() {
        let content = r#"
            <cmdlist>
                <cli coord="100,200" menu="look @" menu_cat="1"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        // Entry should be skipped since command is required
        assert!(result.unwrap().entries.is_empty());
    }

    #[test]
    fn test_parse_entry_missing_menu_cat() {
        let content = r#"
            <cmdlist>
                <cli coord="100,200" menu="look @" command="look #"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        // Entry should be skipped since menu_cat is required
        assert!(result.unwrap().entries.is_empty());
    }

    #[test]
    fn test_parse_mixed_valid_invalid_entries() {
        let content = r#"
            <cmdlist>
                <cli coord="100,200" menu="look @" command="look #" menu_cat="1"/>
                <cli menu="invalid" command="x" menu_cat="2"/>
                <cli coord="300,400" menu="get @" command="get #" menu_cat="3"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        let cmdlist = result.unwrap();
        // Only 2 valid entries (with coord)
        assert_eq!(cmdlist.entries.len(), 2);
    }

    #[test]
    fn test_parse_self_closing_tags() {
        let content = r#"<cli coord="100,200" menu="look @" command="look #" menu_cat="1"/>"#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().entries.len(), 1);
    }

    #[test]
    fn test_parse_with_extra_attributes() {
        let content = r#"
            <cmdlist>
                <cli coord="100,200" menu="look @" command="look #" menu_cat="1" extra="ignored"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        let cmdlist = result.unwrap();
        assert_eq!(cmdlist.entries.len(), 1);
    }

    #[test]
    fn test_parse_duplicate_coords() {
        let content = r#"
            <cmdlist>
                <cli coord="100,200" menu="first @" command="first #" menu_cat="1"/>
                <cli coord="100,200" menu="second @" command="second #" menu_cat="2"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        let cmdlist = result.unwrap();
        // HashMap replaces duplicates, so only one entry
        assert_eq!(cmdlist.entries.len(), 1);
        // The second entry should overwrite the first
        assert_eq!(cmdlist.get("100,200").unwrap().menu, "second @");
    }

    #[test]
    fn test_parse_malformed_xml() {
        let content = r#"<cmdlist><cli coord="100"</cmdlist>"#;
        let result = CmdList::parse(content);
        // Should return an error for malformed XML
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_special_chars_in_values() {
        let content = r#"
            <cmdlist>
                <cli coord="100,200" menu="look &lt;item&gt;" command="look #" menu_cat="5_roleplay"/>
            </cmdlist>
        "#;
        let result = CmdList::parse(content);
        assert!(result.is_ok());
        let cmdlist = result.unwrap();
        let entry = cmdlist.get("100,200").unwrap();
        // quick_xml uses String::from_utf8_lossy which doesn't decode XML entities
        // The raw attribute value is preserved
        assert_eq!(entry.menu, "look &lt;item&gt;");
    }

    // ==================== get Tests ====================

    #[test]
    fn test_get_existing_coord() {
        let content = r#"<cli coord="100,200" menu="look @" command="look #" menu_cat="1"/>"#;
        let cmdlist = CmdList::parse(content).unwrap();
        let entry = cmdlist.get("100,200");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().menu, "look @");
    }

    #[test]
    fn test_get_nonexistent_coord() {
        let content = r#"<cli coord="100,200" menu="look @" command="look #" menu_cat="1"/>"#;
        let cmdlist = CmdList::parse(content).unwrap();
        let entry = cmdlist.get("999,999");
        assert!(entry.is_none());
    }

    #[test]
    fn test_get_empty_cmdlist() {
        let content = "";
        let cmdlist = CmdList::parse(content).unwrap();
        assert!(cmdlist.get("100,200").is_none());
    }

    #[test]
    fn test_get_with_spaces_in_coord() {
        // Coords don't have spaces, but test exact matching
        let content = r#"<cli coord="100, 200" menu="look @" command="look #" menu_cat="1"/>"#;
        let cmdlist = CmdList::parse(content).unwrap();
        assert!(cmdlist.get("100, 200").is_some());
        assert!(cmdlist.get("100,200").is_none()); // Different key
    }

    // ==================== CmdListEntry Tests ====================

    #[test]
    fn test_cmdlist_entry_clone() {
        let entry = CmdListEntry {
            coord: "100,200".to_string(),
            menu: "look @".to_string(),
            command: "look #".to_string(),
            menu_cat: "1".to_string(),
        };
        let cloned = entry.clone();
        assert_eq!(cloned.coord, entry.coord);
        assert_eq!(cloned.menu, entry.menu);
        assert_eq!(cloned.command, entry.command);
        assert_eq!(cloned.menu_cat, entry.menu_cat);
    }

    #[test]
    fn test_cmdlist_entry_debug() {
        let entry = CmdListEntry {
            coord: "100,200".to_string(),
            menu: "look @".to_string(),
            command: "look #".to_string(),
            menu_cat: "1".to_string(),
        };
        let debug_str = format!("{:?}", entry);
        assert!(debug_str.contains("CmdListEntry"));
        assert!(debug_str.contains("100,200"));
    }

    // ==================== CmdList Clone Tests ====================

    #[test]
    fn test_cmdlist_clone() {
        let content = r#"<cli coord="100,200" menu="look @" command="look #" menu_cat="1"/>"#;
        let cmdlist = CmdList::parse(content).unwrap();
        let cloned = cmdlist.clone();
        assert_eq!(cloned.entries.len(), cmdlist.entries.len());
        assert!(cloned.get("100,200").is_some());
    }
}
