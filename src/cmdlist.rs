use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// A single command list entry from cmdlist1.xml
#[derive(Debug, Clone)]
pub struct CmdListEntry {
    pub coord: String,           // e.g., "2524,2061"
    pub menu: String,            // Display text: e.g., "look @"
    pub command: String,         // Command to send: e.g., "look #"
    pub menu_cat: String,        // Category: e.g., "1" or "5_roleplay"
}

/// Parser and lookup for cmdlist1.xml
pub struct CmdList {
    entries: HashMap<String, CmdListEntry>,  // coord -> entry
}

impl CmdList {
    /// Load cmdlist1.xml from ~/.vellum-fe/cmdlist1.xml
    pub fn load() -> Result<Self> {
        let path = Self::get_cmdlist_path()?;

        if !path.exists() {
            return Err(anyhow::anyhow!(
                "cmdlist1.xml not found at {}. Please copy it to this location.",
                path.display()
            ));
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read cmdlist1.xml from {}", path.display()))?;

        Self::parse(&content)
    }

    /// Get the path to cmdlist1.xml (~/.vellum-fe/cmdlist1.xml)
    fn get_cmdlist_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;

        Ok(home.join(".vellum-fe").join("cmdlist1.xml"))
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
                            entries.insert(coord.clone(), CmdListEntry {
                                coord,
                                menu,
                                command,
                                menu_cat,
                            });
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow::anyhow!("XML parse error at position {}: {}", reader.buffer_position(), e)),
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
    pub fn substitute_command(command: &str, noun: &str, exist_id: &str, secondary: Option<&str>) -> String {
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

    #[test]
    fn test_substitute_command() {
        assert_eq!(
            CmdList::substitute_command("look @", "pendant", "12345", None),
            "look pendant"
        );

        assert_eq!(
            CmdList::substitute_command("look #", "pendant", "12345", None),
            "look #12345"
        );

        assert_eq!(
            CmdList::substitute_command("transfer # %", "pendant", "12345", Some("right arm")),
            "transfer #12345 right arm"
        );
    }

    #[test]
    fn test_substitute_menu() {
        assert_eq!(
            CmdList::substitute_menu("look @", "pendant"),
            "look pendant"
        );
    }
}
