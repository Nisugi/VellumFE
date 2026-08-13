//! The dot-command help table — the single source `.help` renders from.
//!
//! Every dispatcher arm in `handle_dot_command` must have a row here and
//! vice versa: the tripwire test at the bottom extracts the arm literals
//! from commands.rs (between the COMMAND ARMS markers) and asserts both
//! directions, so `.help` can no longer silently drift from reality —
//! the failure mode the 2026-07 parity audit found (a dozen shipped
//! commands missing from help).

pub(crate) struct CommandHelpEntry {
    /// Every accepted spelling, without the leading dot; the first is
    /// canonical. Must match the dispatcher arm literals exactly.
    pub names: &'static [&'static str],
    /// Argument syntax rendered after the name ("" when none).
    pub args: &'static str,
    pub blurb: &'static str,
    /// Extra indented lines rendered under the entry.
    pub notes: &'static [&'static str],
}

pub(crate) struct CommandHelpSection {
    pub title: &'static str,
    pub entries: &'static [CommandHelpEntry],
}

macro_rules! entry {
    ($names:expr, $args:expr, $blurb:expr) => {
        CommandHelpEntry {
            names: $names,
            args: $args,
            blurb: $blurb,
            notes: &[],
        }
    };
    ($names:expr, $args:expr, $blurb:expr, $notes:expr) => {
        CommandHelpEntry {
            names: $names,
            args: $args,
            blurb: $blurb,
            notes: $notes,
        }
    };
}

pub(crate) const COMMAND_HELP: &[CommandHelpSection] = &[
    CommandHelpSection {
        title: "APPLICATION",
        entries: &[
            entry!(
                &["quit", "q"],
                "",
                "Disconnect but keep the window open; repeat to exit",
                &["Set ui.keep_open_on_quit off in Settings to make .quit close immediately."]
            ),
            entry!(&["exit"], "", "Exit VellumFE"),
            entry!(&["help", "h", "?"], "", "Show this help"),
            entry!(&["version", "ver"], "", "Show version info"),
            entry!(
                &["reconnect"],
                "",
                "Reconnect to the game after a dropped connection"
            ),
            entry!(
                &["launch"],
                "[character]",
                "SSH to the home PC, cold-start its headless Lich, and attach (bare = open editor)"
            ),
            entry!(
                &["launcher"],
                "",
                "Open the SSH launcher editor (host, launch command, per-character ports, key)"
            ),
            entry!(&["menu"], "", "Open main menu"),
            entry!(&["settings"], "", "Open settings editor"),
            entry!(
                &["reload"],
                "[category]",
                "Reload config from disk (highlights|keybinds|hotbars|settings|colors|layout)"
            ),
            entry!(
                &["room"],
                "",
                "Show how the current room resolved against the mapdb"
            ),
            entry!(
                &["mapdb"],
                "[download|remove|repo <r>]",
                "Manage downloaded map data (status by default)"
            ),
            entry!(
                &["data"],
                "[status|reload|update <name>]",
                "Shared game-data assets: source + age (Lich folder > local > bundled)"
            ),
            entry!(
                &["jinx"],
                "",
                "Asset manager: install skins, icons, layouts, game data (.jinx for usage)"
            ),
            entry!(
                &["tts"],
                "...",
                "Text-to-speech control (.tts for usage; GUI also has Settings > Speech)"
            ),
            entry!(
                &["portal"],
                "[n]",
                "Walk the room's non-compass exit (go door, climb stair, ...)"
            ),
            entry!(
                &["go2"],
                "<target>",
                "Travel there (room id, uid, tag, saved name, or text search)",
                &[
                    ".go2 stop|status cancels / shows the active trip",
                    ".go2 save <name> [id] saves a target (.go2 saved lists, .go2 back returns)",
                    ".go2 targets lists reachable tagged destinations, nearest first",
                ]
            ),
            entry!(
                &["sorter"],
                "[on|off|edit]",
                "Categorize 'look in container' output (edit opens the rules editor)"
            ),
            entry!(
                &["foreach"],
                "... in <bag>; cmd; cmd",
                "Batch commands over matching container items (.foreach for usage)"
            ),
            entry!(
                &["stop"],
                "",
                "Stop whatever automation is driving (go2 trip, foreach run)"
            ),
        ],
    },
    CommandHelpSection {
        title: "LAYOUTS",
        entries: &[
            entry!(
                &["savelayout"],
                "[name]",
                "Save current layout (default: 'default')"
            ),
            entry!(
                &["loadlayout"],
                "[name] [--keep-skin]",
                "Load a saved layout (--keep-skin keeps your skin/theme)"
            ),
            entry!(&["layouts"], "", "List available layouts"),
            entry!(
                &["resize"],
                "[name]",
                "Refit windows to the current size; with a layout name, adopt just its geometry (GUI)"
            ),
            entry!(
                &["anchorinfer"],
                "",
                "Anchor windows whose edges already sit flush against a pane edge or neighbor (GUI)"
            ),
        ],
    },
    CommandHelpSection {
        title: "WINDOWS",
        entries: &[
            entry!(&["windows"], "", "List all windows"),
            entry!(
                &["spellwatch"],
                "[add|rem <n>|[n,n]|all]",
                "Watch spells for the Missing Spells window; bare lists the watch list",
                &["The window shows watched spells that are NOT active in ActiveSpells/Buffs.",
                  ".spellwatch add all snapshots everything currently active."]
            ),
            entry!(
                &["emptyhands", "eh"],
                "",
                "Stow both hands (Lich empty_hands cascade), remembered for .fillhands",
                &["Worn gear wears, tattoos/bandoliers rub, weapons follow ready/stow rules",
                  "then weaponsack/lootsack/any container. Each stow confirmed by hand state."]
            ),
            entry!(
                &["fillhands", "fh"],
                "",
                "Retrieve exactly what .emptyhands stowed, in reverse order"
            ),
            entry!(
                &["viewitem", "inspect"],
                "<exist-id>",
                "Item detail over the extended feed (look/inspect/analyze/read)",
                &["Shows on the Containers window's Item tab and routes to the 'inspect'",
                  "stream (subscribe a text window to log it). Requires direct mode."]
            ),
            entry!(
                &["find"],
                "<name fragment>",
                "Find items in the .invsync snapshot and show where they live",
                &["Searches names and long descriptions; each match prints its container",
                  "path with closed containers flagged. Run .invsync first."]
            ),
            entry!(
                &["drag"],
                "<exist> left|right|drop|wear|feet | in|on|behind|underneath <dest>",
                "Verified item move (extended feed _drag verb)",
                &["Each move is confirmed against hand updates within 8s, never by prose.",
                  "Requires direct mode (WRAYTH banner)."]
            ),
            entry!(
                &["invsync"],
                "",
                "Refresh the structured inventory snapshot (extended feed)",
                &["Sends _inventory manager and follows continuation cursors until the",
                  "snapshot is complete. Requires direct mode (WRAYTH banner)."]
            ),
            entry!(
                &["addwindow"],
                "",
                "Open widget type picker",
                &[".addwindow <name> <type> <x> <y> <w> [h] adds a window manually"]
            ),
            entry!(
                &["deletewindow", "delwindow"],
                "<name>",
                "Remove a window from the layout (restorable)",
                &[".hidewindow keeps it in the layout; delete stashes it for restore"]
            ),
            entry!(
                &["hidewindow", "hidewin"],
                "[name]",
                "Hide window (or open picker)"
            ),
            entry!(
                &["header", "footer", "leftbar", "rightbar"],
                "[on|off|toggle]",
                "Show/hide a GUI shell zone (default: toggle)"
            ),
            entry!(
                &["editwindow", "editwin"],
                "[name]",
                "Edit window (or open picker)"
            ),
            entry!(&["rename"], "<win> <title>", "Rename window title"),
            entry!(
                &["border"],
                "<win> <style> [color]",
                "Set window border",
                &["Styles: all, none, top, bottom, left, right"]
            ),
            entry!(
                &["streams"],
                "",
                "Stream routing: every known stream and where it goes"
            ),
            entry!(
                &["hidecontainers"],
                "[title]",
                "Close popped-up container windows (all, or by title)"
            ),
        ],
    },
    CommandHelpSection {
        title: "TESTING",
        entries: &[entry!(
            &["testline"],
            "<text>",
            "Test highlights/squelch with fake game line"
        )],
    },
    CommandHelpSection {
        title: "HIGHLIGHTS",
        entries: &[
            entry!(&["highlights", "hl"], "", "Open highlights browser"),
            entry!(&["addhighlight", "addhl"], "", "Create new highlight"),
            entry!(
                &["edithighlight", "edithl"],
                "[name]",
                "Edit existing highlight (or open the browser)"
            ),
            entry!(
                &["savehighlights", "savehl"],
                "[name]",
                "Save highlights as profile (default: 'default')"
            ),
            entry!(
                &["loadhighlights", "loadhl"],
                "[name]",
                "Load highlights from profile"
            ),
            entry!(
                &["highlightprofiles", "hlprofiles"],
                "",
                "List saved highlight profiles"
            ),
        ],
    },
    CommandHelpSection {
        title: "KEYBINDS",
        entries: &[
            entry!(&["keybinds", "kb"], "", "Open keybinds browser"),
            entry!(
                &["menukeybinds", "menukb"],
                "",
                "Edit the menu navigation/action keys"
            ),
            entry!(
                &["controller"],
                "",
                "Open the controller bindings editor (GUI)"
            ),
            entry!(&["addkeybind", "addkey"], "", "Create new keybind"),
            entry!(
                &["savekeybinds", "savekb"],
                "[name]",
                "Save keybinds as profile (default: 'default')"
            ),
            entry!(
                &["loadkeybinds", "loadkb"],
                "<name>",
                "Load keybinds from profile"
            ),
            entry!(
                &["keybindprofiles", "kbprofiles"],
                "",
                "List saved keybind profiles"
            ),
        ],
    },
    CommandHelpSection {
        title: "HOTBARS",
        entries: &[entry!(
            &["hotbars", "hotbar"],
            "",
            "Open hotbar editor (bars of command buttons)",
            &["Add a bar to a layout with a 'hotkeybar' window (.addwindow)"]
        )],
    },
    CommandHelpSection {
        title: "INDICATORS",
        entries: &[entry!(
            &["indicators", "indicator"],
            "",
            "Open the indicator builder (create/edit all status indicators, conditions, and icons)"
        )],
    },
    CommandHelpSection {
        title: "COLORS",
        entries: &[
            entry!(
                &["colors", "colorpalette"],
                "",
                "Open color palette browser"
            ),
            entry!(
                &["addcolor", "createcolor"],
                "",
                "Create new palette color"
            ),
            entry!(&["uicolors"], "", "Open UI colors browser"),
            entry!(&["spellcolors"], "", "Open spell colors browser"),
            entry!(
                &["addspellcolor", "newspellcolor"],
                "",
                "Create new spell color"
            ),
            entry!(
                &["setpalette"],
                "",
                "Load palette colors into terminal (TUI)"
            ),
            entry!(
                &["resetpalette"],
                "",
                "Reset terminal palette to defaults (TUI)"
            ),
            entry!(
                &["harmony"],
                "[scheme|schemes|skin <name>]",
                "Generate preset colors from the active theme; 'skin' writes matching panel/frame images"
            ),
        ],
    },
    CommandHelpSection {
        title: "THEMES & SKINS",
        entries: &[
            entry!(&["themes"], "", "Open themes browser"),
            entry!(&["settheme", "theme"], "<name>", "Switch to a theme"),
            entry!(&["edittheme"], "", "Edit current theme"),
            entry!(&["skins"], "", "List installed GUI skins"),
            entry!(
                &["setskin", "skin"],
                "<name>",
                "Activate a GUI skin (.setskin none to disable)"
            ),
            entry!(&["makeskin"], "<name>", "Create a starter skin to edit"),
            entry!(
                &["roomimages", "roomimg"],
                "[on|off|set <image>|clear|list|edit]",
                "Room art by room id (set/clear act on the room you are in)"
            ),
            entry!(
                &["saveskin"],
                "<name>",
                "Bake the current GUI appearance into a skin (GUI)"
            ),
            entry!(&["reloadskin"], "", "Reload the active skin's images"),
        ],
    },
    CommandHelpSection {
        title: "SHARING",
        entries: &[
            entry!(
                &["packs", "packeditor"],
                "",
                "Open the pack editor: guided export/import of shareable UI packs"
            ),
            entry!(
                &["uiexport"],
                "<name> [parts]",
                "Export layout/highlights/keybinds/hotbars/colors/macros/skin/theme/sounds/quickbars/settings as a shareable pack"
            ),
            entry!(
                &["uiimport"],
                "<name|file>",
                "Preview a shared UI pack; add 'apply [parts...]' to install (with backups)"
            ),
        ],
    },
    CommandHelpSection {
        title: "TAB NAVIGATION",
        entries: &[
            entry!(&["nexttab"], "", "Switch to next tab"),
            entry!(&["prevtab"], "", "Switch to previous tab"),
            entry!(
                &["gonew", "nextunread"],
                "",
                "Jump to next tab with unread messages"
            ),
        ],
    },
    CommandHelpSection {
        title: "TOGGLES",
        entries: &[
            entry!(
                &["transparent"],
                "",
                "Toggle transparent window backgrounds"
            ),
            entry!(
                &["snapdebug"],
                "",
                "Toggle the snap-engine trace in vellum-fe.log (GUI)"
            ),
            entry!(
                &["performance", "perf"],
                "[dump]",
                "Toggle the performance monitor; 'dump' writes a diagnostic report file"
            ),
        ],
    },
    CommandHelpSection {
        title: "WINDOW LOCKING",
        entries: &[
            entry!(
                &["lockwindows", "lockall", "unlockwindows", "unlockall"],
                "[on|off]",
                "Lock/unlock all windows (prevent move/resize)"
            ),
            entry!(
                &["lockwindow", "unlockwindow"],
                "<window> [on|off]",
                "Lock/unlock one window (.unlockwindow forces off)"
            ),
        ],
    },
    CommandHelpSection {
        title: "WEB & PHONES",
        entries: &[
            entry!(&["webinfo"], "", "Show the phone pairing URL + QR code"),
            entry!(
                &["reloadmacros"],
                "",
                "Reload macros.toml and push to connected phones"
            ),
            entry!(
                &["webui"],
                "[page|off]",
                "Lich WebUI pages as native panels (GUI)"
            ),
        ],
    },
];

pub(crate) const COMMAND_HELP_FOOTER: &str =
    "Commands marked (GUI) need the desktop GUI; .setpalette/.resetpalette and .resize are TUI-only.";

/// Render the table into `.help`'s line format.
pub(crate) fn render_help_lines() -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("=== VellumFE Dot Commands ===".to_string());
    lines.push(String::new());
    for section in COMMAND_HELP {
        lines.push(format!("{}:", section.title));
        for entry in section.entries {
            let names = entry
                .names
                .iter()
                .map(|name| format!(".{name}"))
                .collect::<Vec<_>>()
                .join(" / ");
            let left = if entry.args.is_empty() {
                format!("  {names}")
            } else {
                format!("  {names} {}", entry.args)
            };
            lines.push(format!("{left:<26}- {}", entry.blurb));
            for note in entry.notes {
                lines.push(format!("    {note}"));
            }
        }
        lines.push(String::new());
    }
    lines.push(COMMAND_HELP_FOOTER.to_string());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every command spelling the dispatcher accepts, extracted from the
    /// source between the COMMAND ARMS markers in commands.rs. Top-level
    /// match arms sit at exactly 12 spaces of indent and start with a
    /// string literal; nested matches are deeper, so this only sees the
    /// real command names.
    fn dispatcher_command_names() -> BTreeSet<String> {
        let src = include_str!("commands.rs");
        let begin = src
            .find("=== COMMAND ARMS BEGIN")
            .expect("commands.rs must keep the COMMAND ARMS BEGIN marker");
        let end = src
            .find("=== COMMAND ARMS END")
            .expect("commands.rs must keep the COMMAND ARMS END marker");
        let mut names = BTreeSet::new();
        for line in src[begin..end].lines() {
            let Some(rest) = line.strip_prefix("            \"") else {
                continue;
            };
            // The pattern part of the arm, before its body/guard.
            let pattern = rest.split("=>").next().unwrap_or(rest);
            for piece in format!("\"{pattern}").split('|') {
                let piece = piece.trim();
                if let Some(name) = piece
                    .strip_prefix('"')
                    .and_then(|p| p.strip_suffix('"'))
                {
                    names.insert(name.to_string());
                }
            }
        }
        assert!(
            names.len() > 50,
            "extractor found only {} arms — markers or indent drifted",
            names.len()
        );
        names
    }

    fn help_command_names() -> BTreeSet<String> {
        COMMAND_HELP
            .iter()
            .flat_map(|section| section.entries)
            .flat_map(|entry| entry.names.iter().map(|name| name.to_string()))
            .collect()
    }

    /// The tripwire: `.help` and the dispatcher can no longer drift.
    #[test]
    fn help_table_matches_dispatcher_arms_exactly() {
        let dispatch = dispatcher_command_names();
        let help = help_command_names();
        let undocumented: Vec<_> = dispatch.difference(&help).collect();
        let phantom: Vec<_> = help.difference(&dispatch).collect();
        assert!(
            undocumented.is_empty(),
            "dispatcher commands missing from the help table: {undocumented:?}"
        );
        assert!(
            phantom.is_empty(),
            "help table lists commands the dispatcher doesn't have: {phantom:?}"
        );
    }

    #[test]
    fn help_renders_every_entry() {
        let lines = render_help_lines();
        let text = lines.join("\n");
        for section in COMMAND_HELP {
            assert!(text.contains(&format!("{}:", section.title)));
            for entry in section.entries {
                assert!(
                    text.contains(&format!(".{}", entry.names[0])),
                    "help output lost .{}",
                    entry.names[0]
                );
            }
        }
    }
}
