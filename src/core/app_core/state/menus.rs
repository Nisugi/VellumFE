//! Popup-menu building (main menu, submenus, add/hide/edit window and
//! indicator menus) plus the game _menu request/response protocol and
//! link activation.

use super::*;

impl AppCore {
    /// Build main menu for .menu command
    pub(in crate::core::app_core) fn build_main_menu(
        &self,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Colors >".to_string(),
                command: "__SUBMENU__colors".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Highlights >".to_string(),
                command: "__SUBMENU__highlights".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Keybinds >".to_string(),
                command: "__SUBMENU__keybinds".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Layouts >".to_string(),
                command: "__SUBMENU__layouts".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Settings".to_string(),
                command: ".settings".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Streams".to_string(),
                command: ".streams".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Windows >".to_string(),
                command: "__SUBMENU__windows".to_string(),
                disabled: false,
            },
            // First-class entry to the indicator template builder — reachable
            // even when every indicator is already placed (the Add/Edit
            // submenus' "Editor" leaf disappears once none are left to add).
            crate::data::ui_state::PopupMenuItem {
                text: "Indicators".to_string(),
                command: ".indicators".to_string(),
                disabled: false,
            },
            // Standalone editor for the target_list.status_abbrev map (shown
            // in the targets & players windows). Uses the action: command form
            // rather than a dot-command. In the GUI this opens the Window
            // Editor's Targets section where the same map is edited.
            crate::data::ui_state::PopupMenuItem {
                text: "Status Abbrevs".to_string(),
                command: "action:editstatusabbrev".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build colors submenu
    pub(super) fn build_colors_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Add".to_string(),
                command: ".addcolor".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Browse".to_string(),
                command: ".colors".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Spells".to_string(),
                command: ".spellcolors".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Themes".to_string(),
                command: ".themes".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build highlights submenu
    pub(in crate::core::app_core) fn build_highlights_submenu(
        &self,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Add".to_string(),
                command: ".addhighlight".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Browse".to_string(),
                command: ".highlights".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build keybinds submenu
    pub(super) fn build_keybinds_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Add".to_string(),
                command: ".addkeybind".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Browse".to_string(),
                command: ".keybinds".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build themes submenu
    pub(super) fn build_themes_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Browse themes".to_string(),
                command: ".themes".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Edit theme".to_string(),
                command: ".edittheme".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build windows submenu. U6: "Show/Hide windows" is the primary
    /// manager (every known window, toggle each); Add creates new ones;
    /// Edit tweaks geometry/settings. ("Hide window" is subsumed by the
    /// Show/Hide list — you untick a row there.)
    pub fn build_windows_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        vec![
            crate::data::ui_state::PopupMenuItem {
                text: "Show/Hide windows >".to_string(),
                command: "menu:knownwindows".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Add window >".to_string(),
                command: "menu:addwindow".to_string(),
                disabled: false,
            },
            crate::data::ui_state::PopupMenuItem {
                text: "Edit window >".to_string(),
                command: "menu:editwindow".to_string(),
                disabled: false,
            },
        ]
    }

    /// Build layouts submenu
    pub fn build_layouts_submenu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let mut items = Vec::new();

        // Get list of saved layouts
        match Config::list_layouts() {
            Ok(mut layouts) => {
                // Sort alphabetically for predictability
                layouts.sort();
                let page_size = 10;
                let mut page = 0;
                let mut count = 0;
                for layout_name in layouts {
                    if count > 0 && count % page_size == 0 {
                        page += 1;
                    }
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: if page == 0 {
                            layout_name.clone()
                        } else {
                            format!("{} (p{})", layout_name, page + 1)
                        },
                        command: format!("action:loadlayout:{}", layout_name),
                        disabled: false,
                    });
                    count += 1;
                }
                if items.is_empty() {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: "No layouts found".to_string(),
                        command: String::new(),
                        disabled: true,
                    });
                }
            }
            Err(err) => {
                // If we can't load layouts, show a disabled message with reason
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: format!("No layouts: {}", err),
                    command: String::new(),
                    disabled: true,
                });
            }
        }

        // Add a close entry for accessibility
        items.push(crate::data::ui_state::PopupMenuItem {
            text: "Close menu".to_string(),
            command: String::new(),
            disabled: true,
        });

        items
    }

    /// Build submenu based on category name
    pub fn build_submenu(&self, category: &str) -> Vec<crate::data::ui_state::PopupMenuItem> {
        match category {
            "colors" => self.build_colors_submenu(),
            "highlights" => self.build_highlights_submenu(),
            "keybinds" => self.build_keybinds_submenu(),
            "layouts" => self.build_layouts_submenu(),
            "themes" => self.build_themes_submenu(),
            "windows" => self.build_windows_submenu(),
            "knownwindows" => self.build_known_windows_menu(),
            _ => Vec::new(),
        }
    }

    /// Handle menu response from server
    pub(super) fn handle_menu_response(
        &mut self,
        counter: &str,
        coords: &[(String, Option<String>)],
    ) {
        // Look up the pending request
        let pending = match self.pending_menu_requests.remove(counter) {
            Some(p) => p,
            None => {
                tracing::warn!("Received menu response for unknown counter: {}", counter);
                return;
            }
        };

        tracing::info!(
            "Menu response for exist_id {} (noun: {}): {} coords",
            pending.exist_id,
            pending.noun,
            coords.len()
        );

        // Check if cmdlist is loaded
        let cmdlist = match &self.cmdlist {
            Some(list) => list,
            None => {
                tracing::warn!("Context menu received but cmdlist not loaded");
                self.answer_remote_menu_empty(&pending);
                return;
            }
        };

        // Group menu items by category
        let mut categories: HashMap<String, Vec<crate::data::ui_state::PopupMenuItem>> =
            HashMap::new();

        for (coord, secondary_noun) in coords {
            if let Some(cmd) = coord.strip_prefix("__direct__:") {
                let menu_text = secondary_noun
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(cmd)
                    .to_string();
                categories.entry("0".to_string()).or_default().push(
                    crate::data::ui_state::PopupMenuItem {
                        text: menu_text,
                        command: cmd.to_string(),
                        disabled: false,
                    },
                );
                continue;
            }

            if let Some(entry) = cmdlist.get(coord) {
                // Skip _dialog commands
                if entry.command.starts_with("_dialog") {
                    continue;
                }

                // Build menu text (remove @ and # placeholders, substitute %)
                let menu_text = Self::format_menu_text(&entry.menu, secondary_noun.as_deref());

                // Build command with placeholders substituted
                let command = CmdList::substitute_command(
                    &entry.command,
                    &pending.noun,
                    &pending.exist_id,
                    secondary_noun.as_deref(),
                );

                let category = if entry.menu_cat.is_empty() {
                    "0".to_string()
                } else {
                    entry.menu_cat.clone()
                };

                categories.entry(category).or_default().push(
                    crate::data::ui_state::PopupMenuItem {
                        text: menu_text,
                        command,
                        disabled: false,
                    },
                );
            }
        }

        // Local codex injection: when the clicked object's noun is a known
        // creature (targets window, room creatures), the game's own context
        // menu gains a Bestiary item. `.bestiary <noun>` resolves unique
        // nouns straight to the entry and ambiguous ones to a match table.
        if !crate::core::bestiary::format::shared()
            .by_noun(&pending.noun)
            .is_empty()
        {
            categories.entry("0".to_string()).or_default().push(
                crate::data::ui_state::PopupMenuItem {
                    text: "Bestiary".to_string(),
                    command: format!(".bestiary {}", pending.noun),
                    disabled: false,
                },
            );
        }

        if categories.is_empty() {
            tracing::warn!("No menu items available for this object");
            self.answer_remote_menu_empty(&pending);
            return;
        }

        // Build final menu with categories
        let mut menu_items = Vec::new();
        let mut sorted_cats: Vec<_> = categories.keys().cloned().collect();

        // Sort categories, but keep "0" at the end
        sorted_cats.sort_by(|a, b| {
            if a == "0" {
                std::cmp::Ordering::Greater
            } else if b == "0" {
                std::cmp::Ordering::Less
            } else {
                a.cmp(b)
            }
        });

        // Route the response to its origin. A remote client gets a flat
        // list (submenu categories become disabled section headers, since
        // a phone bottom sheet has no nested menus); a pick comes back as
        // an ordinary cmd. The local popup path below stays unchanged.
        if let crate::core::remote::MenuOrigin::Remote {
            client_id,
            request_id,
        } = pending.origin
        {
            let mut items = Vec::new();
            for cat in &sorted_cats {
                let cat_items = categories.get(cat).unwrap();
                if cat.contains('_') && cat != "0" {
                    items.push(crate::core::remote::RemoteMenuItem {
                        text: Self::format_category_label(cat),
                        command: String::new(),
                        disabled: true,
                    });
                }
                items.extend(
                    cat_items
                        .iter()
                        .map(|item| crate::core::remote::RemoteMenuItem {
                            text: item.text.clone(),
                            command: item.command.clone(),
                            disabled: item.disabled,
                        }),
                );
            }
            if let Some(remote) = self.message_processor.remote.as_mut() {
                remote.push_menu(client_id, request_id, pending.noun.clone(), items);
            }
            return;
        }

        // Add items to menu
        for cat in &sorted_cats {
            let items = categories.get(cat).unwrap();

            // Categories with _ become submenus (except "0")
            if cat.contains('_') && cat != "0" {
                // Cache submenu items
                self.menu_categories.insert(cat.clone(), items.clone());

                // Add submenu entry to main menu
                let cat_name = Self::format_category_label(cat);
                menu_items.push(crate::data::ui_state::PopupMenuItem {
                    text: format!("{} >", cat_name),
                    command: format!("__SUBMENU__{}", cat),
                    disabled: false,
                });
            } else {
                // Add items directly to main menu
                menu_items.extend(items.clone());
            }
        }

        // Create popup menu at last click position (or centered)
        let position = self.last_link_click_pos.unwrap_or((40, 12));

        self.ui_state.popup_menu =
            Some(crate::data::ui_state::PopupMenu::new(menu_items, position));
        self.ui_state.input_mode = crate::data::ui_state::InputMode::Menu;

        tracing::info!(
            "Created context menu with {} items",
            self.ui_state.popup_menu.as_ref().unwrap().get_items().len()
        );
    }

    /// When a menu request from a remote client can't produce items, still
    /// answer with an empty menu — otherwise the client's sheet waits
    /// forever. Local origins need nothing (no popup was opened).
    pub(super) fn answer_remote_menu_empty(&mut self, pending: &PendingMenuRequest) {
        if let crate::core::remote::MenuOrigin::Remote {
            client_id,
            request_id,
        } = pending.origin
        {
            if let Some(remote) = self.message_processor.remote.as_mut() {
                remote.push_menu(client_id, request_id, pending.noun.clone(), Vec::new());
            }
        }
    }

    pub(super) fn format_category_label(cat: &str) -> String {
        let mut label = cat.split('_').nth(1).unwrap_or(cat).replace('-', " ");
        if label.is_empty() {
            label = cat.to_string();
        }

        if label.is_empty() {
            return "Other".to_string();
        }

        let mut chars = label.chars();
        let first = chars.next().unwrap();
        let mut output = String::new();
        for c in first.to_uppercase() {
            output.push(c);
        }
        output.push_str(chars.as_str());
        output
    }

    /// Format menu text by removing @ and # placeholders and substituting %
    pub(super) fn format_menu_text(menu: &str, secondary_noun: Option<&str>) -> String {
        let mut text = menu.to_string();

        // Substitute % with secondary noun
        if let Some(sec_noun) = secondary_noun {
            text = text.replace('%', sec_noun);
        }

        // Find first @ or #
        if let Some(pos) = text.find(['@', '#']) {
            let remaining = text[pos + 1..].trim();
            if remaining.is_empty() {
                // Placeholder at end - truncate
                text[..pos].trim_end().to_string()
            } else {
                // Placeholder in middle - remove it but keep rest
                let before = text[..pos].trim_end();
                let after = text[pos + 1..].trim_start();
                if before.is_empty() {
                    after.to_string()
                } else {
                    format!("{} {}", before, after)
                }
            }
        } else {
            text
        }
    }

    /// Request context menu for a link (local popup origin)
    /// Returns the _menu command to send to the server
    pub fn request_menu(
        &mut self,
        exist_id: String,
        noun: String,
        click_pos: (u16, u16),
    ) -> String {
        // Store click position for menu placement
        self.last_link_click_pos = Some(click_pos);
        self.request_menu_from(exist_id, noun, crate::core::remote::MenuOrigin::Local)
    }

    /// Resolve a link activation the way a local click does (mirrors the
    /// dispatch in frontend/tui/input.rs): `<d>` tags send their noun/text
    /// as a direct command, links with a coord resolve through cmdlist to
    /// a direct command (exits, default actions), and only plain links
    /// issue a `_menu` request (tagged with `origin` so the response
    /// routes back). Returns the command to send upstream, if any.
    pub fn resolve_link_activation(
        &mut self,
        link: &crate::data::LinkData,
        origin: crate::core::remote::MenuOrigin,
    ) -> Option<String> {
        if link.exist_id == crate::data::URL_LINK_SENTINEL {
            // Web link: frontends open the URL on their own side (browser on
            // desktop, window.open on the phone). Never a game command, and
            // never a _menu request for a fake exist id.
            tracing::debug!(
                "URL link activation reached core (frontend opens it): {}",
                link.noun
            );
            return None;
        }

        if link.exist_id == "_direct_" {
            // <d> tag: the noun (cmd attribute) or text IS the command
            let command = if !link.noun.is_empty() {
                &link.noun
            } else {
                &link.text
            };
            tracing::info!("Executing <d> direct command: {}", command);
            return Some(format!("{}\n", command));
        }

        if let Some(ref coord) = link.coord {
            // Coord link: look up the default action in cmdlist and send
            // it directly - no menu round-trip (e.g. exits move you)
            let Some(ref cmdlist) = self.cmdlist else {
                tracing::warn!("Cmdlist not loaded - cannot resolve coord {}", coord);
                return None;
            };
            let Some(entry) = cmdlist.get(coord) else {
                tracing::warn!("Coord {} not found in cmdlist for '{}'", coord, link.text);
                return None;
            };
            let command =
                CmdList::substitute_command(&entry.command, &link.noun, &link.exist_id, None);
            tracing::info!(
                "Executing cmdlist command for '{}' (coord: {}): {}",
                link.text,
                coord,
                command.trim()
            );
            return Some(format!("{}\n", command));
        }

        // Plain link: context menu round-trip
        Some(self.request_menu_from(link.exist_id.clone(), link.noun.clone(), origin))
    }

    /// Request context menu for a link on behalf of an origin (local UI or
    /// a remote web client). The `<menu>` response routes back to the
    /// origin in handle_menu_response.
    pub fn request_menu_from(
        &mut self,
        exist_id: String,
        noun: String,
        origin: crate::core::remote::MenuOrigin,
    ) -> String {
        // Parser links normally carry the bare object id, but target-list
        // creatures retain the protocol's leading `#`. Keep the menu request
        // and later cmdlist substitution on one canonical bare-id shape so a
        // combat target never becomes `##123` on the wire.
        let exist_id = exist_id.trim_start_matches('#').to_string();

        // Increment counter
        self.menu_request_counter += 1;
        let counter = self.menu_request_counter;

        // Store pending request
        self.pending_menu_requests.insert(
            counter.to_string(),
            PendingMenuRequest {
                exist_id: exist_id.clone(),
                noun,
                origin,
            },
        );

        // Return command to send to server
        format!("_menu #{} {}\n", exist_id, counter)
    }
    // ========== Menu Building Methods ==========

    /// Build the top-level "Add Window" menu showing widget categories
    pub fn build_add_window_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let categories_map =
            crate::core::local_catalog::addable_by_category(&self.layout, self.game_type());

        // Sort categories for consistent display
        let mut categories: Vec<_> = categories_map.into_iter().collect();
        categories.sort_by_key(|(cat, _)| cat.clone());

        categories
            .into_iter()
            .map(
                |(category, _templates)| crate::data::ui_state::PopupMenuItem {
                    text: category.display_name().to_string(),
                    command: format!("__SUBMENU_ADD__{:?}", category),
                    disabled: false,
                },
            )
            .collect()
    }

    /// Addable window templates grouped by category, as
    /// `(category display name, [(template name, display name)])`, for
    /// frontends that render native menus instead of the popup-menu stack.
    pub fn addable_window_templates(&self) -> Vec<(String, Vec<(String, String)>)> {
        let categories_map =
            crate::core::local_catalog::addable_by_category(&self.layout, self.game_type());
        let mut categories: Vec<_> = categories_map.into_iter().collect();
        categories.sort_by_key(|(category, _)| category.clone());
        categories
            .into_iter()
            .map(|(category, templates)| {
                let mut entries: Vec<(String, String)> = templates
                    .into_iter()
                    .filter(|name| {
                        self.layout
                            .get_window(name)
                            .map(|w| !w.base().visibility.is_shown())
                            .unwrap_or(true)
                    })
                    .map(|name| {
                        let display = self.get_window_display_name(&name);
                        (name, display)
                    })
                    .collect();
                entries.sort_by(|a, b| a.1.to_ascii_lowercase().cmp(&b.1.to_ascii_lowercase()));
                (category.display_name().to_string(), entries)
            })
            .filter(|(_, entries)| !entries.is_empty())
            .collect()
    }

    /// Build category submenu showing available windows of that type
    pub fn build_add_window_category_menu(
        &self,
        category: &crate::config::WidgetCategory,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let categories_map =
            crate::core::local_catalog::addable_by_category(&self.layout, self.game_type());

        if let Some(templates) = categories_map.get(category) {
            // Filter out templates already present in the layout (so they disappear once added)
            let available_templates: Vec<_> = templates
                .iter()
                .filter(|name| {
                    self.layout
                        .get_window(name)
                        .map(|w| !w.base().visibility.is_shown())
                        .unwrap_or(true)
                })
                .collect();

            // Special handling for Status: dashboard + Indicators submenu
            if matches!(category, crate::config::WidgetCategory::Status) {
                let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
                if available_templates.iter().any(|t| *t == "dashboard") {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: "Dashboard".to_string(),
                        command: "__ADD__dashboard".to_string(),
                        disabled: false,
                    });
                }
                // Indicators submenu (only if any indicator templates are available)
                let available_owned: Vec<String> =
                    available_templates.iter().map(|s| s.to_string()).collect();
                if !self.build_indicator_add_menu(&available_owned).is_empty() {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: "Indicators >".to_string(),
                        command: "__SUBMENU_INDICATORS".to_string(),
                        disabled: false,
                    });
                }
                return items;
            }

            let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();

            // Custom template entry (derive widget type from the first available template)
            // Skip for Hands to match the fixed submenu (left/right/spell only) and Other category per design.
            let allow_custom = !matches!(category, crate::config::WidgetCategory::Hand)
                && !matches!(category, crate::config::WidgetCategory::Other);
            let has_explicit_custom = available_templates
                .iter()
                .any(|name| name.ends_with("_custom"));
            if allow_custom && !has_explicit_custom {
                if let Some(first) = available_templates.first() {
                    if let Some(widget_type) =
                        crate::core::local_catalog::seed(first).map(|t| t.widget_type().to_string())
                    {
                        items.push(crate::data::ui_state::PopupMenuItem {
                            text: "Custom (blank)".to_string(),
                            command: format!("__ADD_CUSTOM__{}", widget_type),
                            disabled: false,
                        });
                    }
                }
            }

            items.extend(available_templates.into_iter().map(|name| {
                crate::data::ui_state::PopupMenuItem {
                    text: self.get_window_display_name(name),
                    command: format!("__ADD__{}", name),
                    disabled: false,
                }
            }));

            items
        } else {
            vec![]
        }
    }

    /// U3: the unified list of every window the client knows about, from
    /// the layout (persistent, possibly game-bound) plus session-only
    /// ephemeral windows (containers, dialog panels). This replaces the
    /// separate offer registry as the source for the Windows list.
    pub fn enumerate_known_windows(&self) -> Vec<crate::core::known_windows::KnownWindow> {
        use crate::config::WindowBinding;
        use crate::core::known_windows::{KnownWindow, KnownWindowKind};

        let mut out: Vec<KnownWindow> = Vec::new();

        // Persistent layout windows. Bound ones are game-discovered
        // dialogs/streams; unbound ones are template/custom widgets.
        // Nothing is unlisted: "main" is just the story window (hideable
        // while another window carries the main stream), and command_input
        // is hideable in the GUI (fallback bottom bar) while the TUI
        // force-shows it.
        for w in &self.layout.windows {
            let base = w.base();
            let name = base.name.clone();
            let kind = match &base.binding {
                Some(WindowBinding::Stream(_)) => KnownWindowKind::Stream,
                Some(WindowBinding::Dialog(_)) => KnownWindowKind::Dialog,
                Some(WindowBinding::Container(_)) => KnownWindowKind::Container,
                None => KnownWindowKind::Layout,
            };
            out.push(KnownWindow {
                name: name.clone(),
                title: base.title.clone().unwrap_or(name),
                kind,
                widget_type: w.widget_type().to_string(),
                shown: base.visibility.is_shown(),
                ephemeral: false,
            });
        }

        // Session-only ephemeral windows (containers, dialog panels) —
        // these live in ui_state, not the layout.
        for name in &self.ui_state.ephemeral_windows {
            let Some(win) = self.ui_state.windows.get(name) else {
                continue;
            };
            let (kind, wt) = match win.widget_type {
                crate::data::WidgetType::Container => (KnownWindowKind::Container, "container"),
                crate::data::WidgetType::DialogPanel => (KnownWindowKind::Dialog, "dialogpanel"),
                _ => (KnownWindowKind::Layout, "text"),
            };
            let title = match &win.content {
                crate::data::WindowContent::Container { container_title } => {
                    container_title.clone()
                }
                _ => name.clone(),
            };
            out.push(KnownWindow {
                name: name.clone(),
                title,
                kind,
                widget_type: wt.to_string(),
                shown: win.visible,
                ephemeral: true,
            });
        }

        // Sighted-but-not-open containers from the GameObjects registry, so
        // the user can opt one in the first time. The toggle key is the
        // ephemeral window name a container would get.
        for container in self.game_state.objects.containers() {
            if container.title.is_empty() {
                continue;
            }
            let win_name = container.title.replace(' ', "_").to_lowercase();
            if self.ui_state.windows.contains_key(&win_name) {
                continue; // already listed above as an open ephemeral window
            }
            out.push(KnownWindow {
                name: win_name,
                title: container.title.clone(),
                kind: KnownWindowKind::Container,
                widget_type: "container".to_string(),
                shown: false,
                ephemeral: true,
            });
        }

        // Full catalog: every template for this game type is a row even
        // before it exists in the layout — ticking one conjures it via
        // set_known_window_shown. Seed templates (`*_custom`) and spacers
        // are creation flows, not windows, so they stay out; command_input
        // has no template and is covered by the layout pass above.
        let existing: std::collections::HashSet<String> =
            out.iter().map(|k| k.name.to_ascii_lowercase()).collect();
        for template_name in crate::core::local_catalog::creatable_for_game(self.game_type()) {
            if template_name == "spacer" || template_name.ends_with("_custom") {
                continue;
            }
            if existing.contains(&template_name.to_ascii_lowercase()) {
                continue;
            }
            let Some(template) = crate::core::local_catalog::seed(&template_name) else {
                continue;
            };
            out.push(KnownWindow {
                title: template
                    .base()
                    .title
                    .clone()
                    .unwrap_or_else(|| template_name.clone()),
                name: template_name,
                kind: KnownWindowKind::Layout,
                widget_type: template.widget_type().to_string(),
                shown: false,
                ephemeral: false,
            });
        }

        // Discovery memory (redesign Phase 3): bindings this character has
        // seen in past sessions (or the well-known seeds) that no row
        // above covers — so "Bounty" is addable in a FRESH layout before
        // the game re-declares it. Strict union: dedicated-view ids stay
        // owned by the template rows above (including their game-type
        // gating), bound layout windows already listed, and name
        // collisions defer to the existing row. Ticking one conjures a
        // bound window exactly as a live discovery would.
        let existing: std::collections::HashSet<String> =
            out.iter().map(|k| k.name.to_ascii_lowercase()).collect();
        for entry in &self.window_registry.bindings {
            let (binding, kind, widget_type) = match entry.kind.as_str() {
                "stream" => (
                    crate::config::WindowBinding::Stream(entry.id.clone()),
                    KnownWindowKind::Stream,
                    "text",
                ),
                "dialog" => (
                    crate::config::WindowBinding::Dialog(entry.id.clone()),
                    KnownWindowKind::Dialog,
                    "dialogpanel",
                ),
                _ => continue,
            };
            if self.layout.has_window_bound_to(&entry.id) {
                continue;
            }
            if crate::core::view_resolver::resolve_view(&binding, None)
                .dedicated_key()
                .is_some()
            {
                continue;
            }
            if existing.contains(&entry.id.to_ascii_lowercase()) {
                continue;
            }
            out.push(KnownWindow {
                name: entry.id.clone(),
                title: if entry.title.is_empty() {
                    entry.id.clone()
                } else {
                    entry.title.clone()
                },
                kind,
                widget_type: widget_type.to_string(),
                shown: false,
                ephemeral: false,
            });
        }

        out
    }

    /// Build the unified Windows list menu: the FULL window catalog (every
    /// template + layout + ephemeral runtime), each row `[x]`/`[ ]` for its
    /// shown state, grouped under disabled category-header rows. Selecting
    /// a row emits `__TOGGLE_WINDOW__<name>` to flip it (ticking a
    /// never-added template conjures it). The GUI has its own Windows
    /// window; this menu is the TUI's view of the same catalog.
    pub fn build_known_windows_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        use crate::config::WidgetCategory;
        let known = self.enumerate_known_windows();
        if known.is_empty() {
            return vec![crate::data::ui_state::PopupMenuItem {
                text: "(no windows known yet)".to_string(),
                command: String::new(),
                disabled: true,
            }];
        }
        let mut items = Vec::new();
        for category in WidgetCategory::ALL {
            let mut group: Vec<_> = known
                .iter()
                .filter(|k| WidgetCategory::from_widget_type(&k.widget_type) == category)
                .collect();
            if group.is_empty() {
                continue;
            }
            group.sort_by(|a, b| {
                a.title
                    .to_ascii_lowercase()
                    .cmp(&b.title.to_ascii_lowercase())
            });
            items.push(crate::data::ui_state::PopupMenuItem {
                text: format!("── {} ──", category.display_name()),
                command: String::new(),
                disabled: true,
            });
            for k in group {
                let mark = if k.shown { "[x]" } else { "[ ]" };
                let session = if k.ephemeral { " (session)" } else { "" };
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: format!("{} {}{}", mark, k.title, session),
                    command: format!("__TOGGLE_WINDOW__{}", k.name),
                    disabled: false,
                });
            }
        }
        items
    }

    /// U3: toggle a known window's shown state by NAME (from the unified
    /// Windows list). Flips shown↔hidden via set_known_window_shown.
    pub fn toggle_known_window(&mut self, name: &str) {
        let currently_shown = self
            .enumerate_known_windows()
            .iter()
            .find(|k| k.name == name)
            .map(|k| k.shown)
            .unwrap_or(false);
        let (w, h) = (
            self.layout.terminal_width.unwrap_or(80),
            self.layout.terminal_height.unwrap_or(24),
        );
        self.set_known_window_shown(name, !currently_shown, w, h);
    }

    pub fn build_hide_window_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let categories_map = crate::core::local_catalog::visible_by_category(&self.layout, true);

        // Sort categories for consistent display
        let mut categories: Vec<_> = categories_map.into_iter().collect();
        categories.sort_by_key(|(cat, _)| cat.clone());

        categories
            .into_iter()
            .map(
                |(category, _templates)| crate::data::ui_state::PopupMenuItem {
                    text: category.display_name().to_string(),
                    command: format!("__SUBMENU_HIDE__{:?}", category),
                    disabled: false,
                },
            )
            .collect()
    }

    /// Build category submenu for hiding windows
    pub fn build_hide_window_category_menu(
        &self,
        category: &crate::config::WidgetCategory,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let categories_map = crate::core::local_catalog::visible_by_category(&self.layout, true);

        if let Some(templates) = categories_map.get(category) {
            // Special handling for Status: Dashboard item + Indicators submenu
            if matches!(category, crate::config::WidgetCategory::Status) {
                let dashboards: Vec<String> = templates
                    .iter()
                    .filter(|name| *name == "dashboard")
                    .cloned()
                    .collect();
                let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
                for name in dashboards {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: self.get_window_display_name(&name),
                        command: format!("__HIDE__{}", name),
                        disabled: false,
                    });
                }
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: "Indicators >".to_string(),
                    command: "__SUBMENU_HIDE_INDICATORS".to_string(),
                    disabled: false,
                });
                return items;
            }

            templates
                .iter()
                .map(|name| crate::data::ui_state::PopupMenuItem {
                    text: self.get_window_display_name(name),
                    command: format!("__HIDE__{}", name),
                    disabled: false,
                })
                .collect()
        } else {
            vec![]
        }
    }

    /// Build indicator submenu for Status -> Indicators
    pub fn build_indicator_add_menu(
        &self,
        available_templates: &[String],
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let available: std::collections::HashSet<String> = available_templates
            .iter()
            .map(|s| s.to_lowercase())
            .collect();

        let mut templates: Vec<_> = crate::config::Config::list_indicator_templates()
            .into_iter()
            .filter(|tpl| available.contains(&tpl.key().to_lowercase()))
            .collect();

        let desired_order = ["bleeding", "diseased", "poisoned", "stunned", "webbed"];
        let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();

        for desired in &desired_order {
            if let Some(idx) = templates.iter().position(|t| {
                t.key().eq_ignore_ascii_case(desired) || t.id.eq_ignore_ascii_case(desired)
            }) {
                let tpl = templates.remove(idx);
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: tpl.title_or_id(),
                    command: format!("__ADD__{}", tpl.key()),
                    disabled: false,
                });
            }
        }

        // Append remaining templates alphabetically
        templates.sort_by(|a, b| {
            a.title_or_id()
                .to_lowercase()
                .cmp(&b.title_or_id().to_lowercase())
        });
        for tpl in templates {
            items.push(crate::data::ui_state::PopupMenuItem {
                text: tpl.title_or_id(),
                command: format!("__ADD__{}", tpl.key()),
                disabled: false,
            });
        }

        // Always include the template editor entry at the bottom
        items.push(crate::data::ui_state::PopupMenuItem {
            text: "Editor".to_string(),
            command: "__INDICATOR_EDITOR".to_string(),
            disabled: false,
        });

        items
    }

    /// Indicator submenu for Hide
    pub fn build_indicator_hide_menu(
        &self,
        indicator_names: &[String],
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let desired_order = ["bleeding", "diseased", "poisoned", "stunned", "webbed"];
        let title_lookup: std::collections::HashMap<String, String> =
            crate::config::Config::list_indicator_templates()
                .into_iter()
                .map(|tpl| (tpl.key().to_lowercase(), tpl.title_or_id()))
                .collect();

        let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();

        for desired in &desired_order {
            for name in indicator_names {
                if name.eq_ignore_ascii_case(desired) {
                    let key = name.to_lowercase();
                    if used.insert(key.clone()) {
                        let text = title_lookup
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| self.get_window_display_name(name));
                        items.push(crate::data::ui_state::PopupMenuItem {
                            text,
                            command: format!("__HIDE__{}", name),
                            disabled: false,
                        });
                    }
                }
            }
        }
        // Append remaining indicators not in desired order
        let mut remaining: Vec<String> = indicator_names.iter().cloned().collect();
        remaining.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        for name in remaining {
            let key = name.to_lowercase();
            if used.insert(key.clone()) {
                let text = title_lookup
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| self.get_window_display_name(&name));
                items.push(crate::data::ui_state::PopupMenuItem {
                    text,
                    command: format!("__HIDE__{}", name),
                    disabled: false,
                });
            }
        }

        items
    }

    /// Indicator submenu for Edit
    pub fn build_indicator_edit_menu(
        &self,
        indicator_names: &[String],
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        let desired_order = ["bleeding", "diseased", "poisoned", "stunned", "webbed"];
        let title_lookup: std::collections::HashMap<String, String> =
            crate::config::Config::list_indicator_templates()
                .into_iter()
                .map(|tpl| (tpl.key().to_lowercase(), tpl.title_or_id()))
                .collect();

        let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();

        for desired in &desired_order {
            for name in indicator_names {
                if name.eq_ignore_ascii_case(desired) {
                    let key = name.to_lowercase();
                    if used.insert(key.clone()) {
                        let text = title_lookup
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| self.get_window_display_name(name));
                        items.push(crate::data::ui_state::PopupMenuItem {
                            text,
                            command: format!("__EDIT__{}", name),
                            disabled: false,
                        });
                    }
                }
            }
        }
        let mut remaining: Vec<String> = indicator_names.iter().cloned().collect();
        remaining.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        for name in remaining {
            let key = name.to_lowercase();
            if used.insert(key.clone()) {
                let text = title_lookup
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| self.get_window_display_name(&name));
                items.push(crate::data::ui_state::PopupMenuItem {
                    text,
                    command: format!("__EDIT__{}", name),
                    disabled: false,
                });
            }
        }

        // Append editor entry at the bottom
        items.push(crate::data::ui_state::PopupMenuItem {
            text: "Editor".to_string(),
            command: "__INDICATOR_EDITOR".to_string(),
            disabled: false,
        });

        items
    }

    /// Build "Edit Window" menu showing widget categories (only categories with visible windows)
    pub fn build_edit_window_menu(&self) -> Vec<crate::data::ui_state::PopupMenuItem> {
        // include_hidden: hidden windows stay editable from the picker.
        let categories_map =
            crate::core::local_catalog::layout_windows_by_category(&self.layout, false, true);

        // Sort categories for consistent display
        let mut categories: Vec<_> = categories_map.into_iter().collect();
        categories.sort_by_key(|(cat, _)| cat.clone());

        categories
            .into_iter()
            .map(
                |(category, _templates)| crate::data::ui_state::PopupMenuItem {
                    text: category.display_name().to_string(),
                    command: format!("__SUBMENU_EDIT__{:?}", category),
                    disabled: false,
                },
            )
            .collect()
    }

    /// Build category submenu for editing windows
    pub fn build_edit_window_category_menu(
        &self,
        category: &crate::config::WidgetCategory,
    ) -> Vec<crate::data::ui_state::PopupMenuItem> {
        // include_hidden: hidden windows stay editable from the picker.
        let categories_map =
            crate::core::local_catalog::layout_windows_by_category(&self.layout, false, true);

        if let Some(templates) = categories_map.get(category) {
            // Special handling for Status: Dashboard + Indicators submenu
            if matches!(category, crate::config::WidgetCategory::Status) {
                let dashboards: Vec<String> = templates
                    .iter()
                    .filter(|name| *name == "dashboard")
                    .cloned()
                    .collect();
                let mut items: Vec<crate::data::ui_state::PopupMenuItem> = Vec::new();
                for name in dashboards {
                    items.push(crate::data::ui_state::PopupMenuItem {
                        text: self.edit_menu_entry_text(&name),
                        command: format!("__EDIT__{}", name),
                        disabled: false,
                    });
                }
                items.push(crate::data::ui_state::PopupMenuItem {
                    text: "Indicators >".to_string(),
                    command: "__SUBMENU_EDIT_INDICATORS".to_string(),
                    disabled: false,
                });
                return items;
            }

            templates
                .iter()
                .map(|name| crate::data::ui_state::PopupMenuItem {
                    text: self.edit_menu_entry_text(name),
                    command: format!("__EDIT__{}", name),
                    disabled: false,
                })
                .collect()
        } else {
            vec![]
        }
    }

    /// Display text for an edit-menu entry; hidden windows are tagged so the
    /// picker makes their state obvious.
    pub(super) fn edit_menu_entry_text(&self, name: &str) -> String {
        let display = self.get_window_display_name(name);
        let hidden = self
            .layout
            .get_window(name)
            .is_some_and(|w| !w.base().visibility.is_shown());
        if hidden {
            format!("{} (hidden)", display)
        } else {
            display
        }
    }

    /// Get display name for a window (uses title from template, or falls back to name)
    pub fn get_window_display_name(&self, name: &str) -> String {
        crate::core::local_catalog::seed(name)
            .and_then(|t| t.base().title.clone())
            .unwrap_or_else(|| name.to_string())
    }
}
