//! Tab lifecycle and window-group management for the GUI shell:
//! discovering available tabs from core windows, hide/forget plumbing,
//! tab groups (lock-together stacks), and group/window content rendering.

use super::*;

impl VellumGuiApp {
    pub(super) fn collect_available_tabs(app_core: &AppCore) -> HashMap<TabKey, GuiTab> {
        let mut keys: Vec<String> = app_core.ui_state.windows.keys().cloned().collect();
        // Canonical windows first so they claim singleton keys (Targets,
        // Room, …) ahead of user-added "custom-*" duplicates, which fall
        // back to name-keyed tabs below instead of hijacking the slot.
        keys.sort_by_key(|name| (name.starts_with("custom-"), name.clone()));

        let mut tabs = HashMap::new();
        for name in keys {
            let Some(window) = app_core.ui_state.windows.get(&name) else {
                continue;
            };

            let Some(mut tab_key) = Self::tab_key_for_window(&name, window) else {
                continue;
            };
            // A second window of a singleton type would silently lose the
            // entry race and never get a tab (invisible, unlisted). Key
            // extras by window name so every window stays reachable.
            if tabs.contains_key(&tab_key) {
                tab_key = TabKey::WindowByName { id: name.clone() };
            }

            // The main story window keeps its canonical title regardless of the
            // layout's window name (legacy layouts call it "main"/"primary").
            // Every other window shows its configured title (base.title, set by
            // .rename / the window editor) — falling back to the window name
            // only when no title is set. Previously this always used the name,
            // so a renamed custom window's title bar kept the internal id.
            let title = if tab_key == TabKey::TextMain {
                tab_key.default_title()
            } else {
                app_core
                    .layout
                    .windows
                    .iter()
                    .find(|w| w.name() == name)
                    .and_then(|w| w.base().title.clone())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| window.name.clone())
            };
            tabs.entry(tab_key.clone()).or_insert_with(|| GuiTab {
                id: TabId::with_title(tab_key, title),
                window_name: name.clone(),
            });
        }

        tabs
    }

    pub(super) fn tab_key_for_window(name: &str, window: &WindowState) -> Option<TabKey> {
        let key = match window.widget_type {
            WidgetType::Spacer => return None,
            WidgetType::CommandInput => TabKey::CommandInput,
            WidgetType::Text | WidgetType::TabbedText => {
                if Self::is_main_stream_window(name, window) {
                    TabKey::TextMain
                } else {
                    TabKey::TextByName {
                        id: name.to_string(),
                    }
                }
            }
            WidgetType::Inventory => TabKey::Inventory {
                id: name.to_string(),
            },
            WidgetType::ActiveEffects => TabKey::ActiveEffects {
                id: name.to_string(),
            },
            WidgetType::Quickbar => TabKey::Quickbar {
                id: name.to_string(),
            },
            WidgetType::MiniVitals => TabKey::Vitals,
            WidgetType::Progress => {
                // Legacy layouts use a Progress-typed window named "vitals"
                // for the multi-bar cluster; standalone bars (stance, single
                // health/mana bars) each get their own tab.
                if name.eq_ignore_ascii_case("vitals") || name.eq_ignore_ascii_case("minivitals") {
                    TabKey::Vitals
                } else {
                    TabKey::ProgressBar {
                        id: name.to_string(),
                    }
                }
            }
            WidgetType::Countdown => TabKey::Countdown {
                id: name.to_string(),
            },
            WidgetType::Compass => TabKey::Compass,
            WidgetType::Map => TabKey::Map,
            WidgetType::Indicator => TabKey::Indicators,
            WidgetType::Targets => TabKey::Targets,
            WidgetType::Players => TabKey::Players,
            WidgetType::Room => TabKey::Room,
            WidgetType::Experience | WidgetType::GS4Experience => TabKey::Experience,
            WidgetType::InjuryDoll => TabKey::InjuryDoll,
            WidgetType::Dashboard => TabKey::Dashboard,
            WidgetType::Encumbrance => TabKey::Encumbrance,
            WidgetType::Perception => TabKey::Perception,
            WidgetType::Hand => {
                let lower = name.to_ascii_lowercase();
                if lower.contains("left") {
                    TabKey::LeftHand
                } else if lower.contains("right") {
                    TabKey::RightHand
                } else {
                    TabKey::SpellHand
                }
            }
            WidgetType::WebUi => {
                // Key on the bound page id, not the window name, so a WebUI
                // panel never shares TabByName's keyspace with text windows.
                let page = match &window.content {
                    WindowContent::WebUi(content) => content.page_id.clone(),
                    _ => name.to_string(),
                };
                TabKey::WebUi { page }
            }
            _ => TabKey::TextByName {
                id: name.to_string(),
            },
        };

        Some(key)
    }

    pub(super) fn is_main_stream_window(name: &str, window: &WindowState) -> bool {
        if name.eq_ignore_ascii_case("main") {
            return true;
        }

        match &window.content {
            WindowContent::Text(content)
            | WindowContent::Inventory(content)
            | WindowContent::Reserve(content)
            | WindowContent::Spells(content) => content
                .streams
                .iter()
                .any(|stream| stream.eq_ignore_ascii_case("main")),
            WindowContent::TabbedText(tabbed) => Self::find_main_tab(tabbed).is_some(),
            _ => false,
        }
    }

    pub(super) fn find_main_tab(tabbed: &TabbedTextContent) -> Option<&crate::data::TabState> {
        tabbed.tabs.iter().find(|tab| {
            tab.definition
                .streams
                .iter()
                .any(|stream| stream.eq_ignore_ascii_case("main"))
        })
    }

    /// Order-independent hash of everything tab identity derives from:
    /// window key, display title, widget type, and main-stream status.
    /// Allocation-free, so the per-frame no-change path stays cheap.
    pub(super) fn available_tabs_fingerprint(app_core: &AppCore) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut acc = 0u64;
        for (name, window) in &app_core.ui_state.windows {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            name.hash(&mut hasher);
            window.name.hash(&mut hasher);
            std::mem::discriminant(&window.widget_type).hash(&mut hasher);
            Self::is_main_stream_window(name, window).hash(&mut hasher);
            // The configured title feeds the tab's display title, so a rename
            // must change the fingerprint — otherwise collect_available_tabs is
            // skipped and the title bar keeps the old (internal-id) title.
            app_core
                .layout
                .windows
                .iter()
                .find(|w| w.name() == name)
                .and_then(|w| w.base().title.as_deref())
                .hash(&mut hasher);
            acc = acc.wrapping_add(hasher.finish());
        }
        acc
    }

    pub(super) fn refresh_available_tabs_if_needed(&mut self) {
        let fingerprint = Self::available_tabs_fingerprint(&self.app_core);
        if self.available_tabs_fingerprint == Some(fingerprint) {
            return;
        }
        self.available_tabs_fingerprint = Some(fingerprint);

        let refreshed = Self::collect_available_tabs(&self.app_core);
        if refreshed.len() == self.available_tabs.len()
            && refreshed.iter().all(|(key, refreshed_tab)| {
                self.available_tabs
                    .get(key)
                    .map(|tab| {
                        tab.window_name == refreshed_tab.window_name
                            && tab.id.title == refreshed_tab.id.title
                    })
                    .unwrap_or(false)
            })
        {
            return;
        }

        // A window can leave available_tabs two ways: DELETED (gone from the
        // layout defs) or merely HIDDEN (Windows-menu untick / .hide — core
        // removes it from ui_state but keeps its layout def). Purging the
        // stored rect for a hide dropped it to the top-left default when the
        // user re-showed it. Keep the rect (and its paired sidebar gap) for
        // a tab whose window is still in the layout defs; deletes already
        // purge their rect via forget_tab_state before removing the def, so
        // this only spares the hidden case. Mirrors
        // restore_keeps_rect_for_hidden_tab on the load path. The old tab list
        // (still holding window_name for each key) resolves a leaving key back
        // to its window name.
        let previous_tabs = std::mem::replace(&mut self.available_tabs, refreshed);
        // Keys whose rect survives this refresh: still a live tab, OR hidden
        // (not deleted) — its window name resolved from the OLD tab list is
        // still present in the layout defs.
        let rect_survivors = Self::rect_survivor_keys(
            &previous_tabs,
            &self.available_tabs,
            &self.app_core.layout.windows,
        );
        self.hidden_tabs
            .retain(|key| self.available_tabs.contains_key(key));
        self.main_window_rects
            .retain(|key, _| rect_survivors.contains(key));
        self.last_center_window_rects
            .retain(|key, _| self.available_tabs.contains_key(key));
        self.sidebar_gap_above
            .retain(|key, _| rect_survivors.contains(key));
        self.tab_zones
            .retain(|key, _| self.available_tabs.contains_key(key));
        self.no_title_tabs
            .retain(|key| self.available_tabs.contains_key(key));
        // Any window that vanished (delete, layout swap) must also leave its
        // groups, or surviving members render as followers of a ghost leader
        // and can't be re-added individually. sanitize_tab_groups drops
        // absent members and dissolves groups left with fewer than two.
        self.tab_groups =
            Self::sanitize_tab_groups(std::mem::take(&mut self.tab_groups), &self.available_tabs);
        for (key, tab) in &self.available_tabs {
            if !self.tab_zones.contains_key(key) {
                // A pending zone pref (Windows-window dropdown set while
                // the window was hidden) beats the widget default.
                let zone = self
                    .pending_zones
                    .get(&tab.window_name)
                    .copied()
                    .unwrap_or_else(|| Self::default_zone_for_tab_key(key));
                self.tab_zones.insert(key.clone(), zone);
            }
        }
        self.prune_detached_tabs();
        self.layout_dirty = true;
    }
    /// Will the command-input tab render somewhere this frame — a zone
    /// surface or a detached viewport? When not, the shell shows the fixed
    /// bottom panel instead so typing is always possible.
    pub(super) fn command_input_tab_rendered(&self) -> bool {
        let key = TabKey::CommandInput;
        if !self.available_tabs.contains_key(&key) || self.hidden_tabs.contains(&key) {
            return false;
        }
        if self.detached_tabs.contains_key(&key) {
            return true;
        }
        match self.zone_for_tab(&key) {
            GuiShellZone::Header => self.shell_layout.header_visible,
            GuiShellZone::Footer => self.shell_layout.footer_visible,
            GuiShellZone::LeftSidebar => !self.shell_layout.left_sidebar_collapsed,
            GuiShellZone::RightSidebar => !self.shell_layout.right_sidebar_collapsed,
            _ => true,
        }
    }

    /// Hide a window the authoritative way: flip its core WindowVisibility —
    /// the same layer the Windows-window checkbox drives. Right-click Hide,
    /// detached-viewport close, `.hidewindow`, and load-time extras all route
    /// here, so "hide" and "uncheck" are one mechanism (auto-spawn suppressed,
    /// checkbox reflects reality). The old `hidden_tabs` overlay survives only
    /// to honor legacy layout files at restore time; any overlay mark for this
    /// window is cleared so the two layers can never disagree. The stored rect
    /// survives the hide (rect_survivor_keys) so a re-show lands in place.
    pub(super) fn core_hide_tab(&mut self, key: &TabKey) {
        let Some(name) = self
            .available_tabs
            .get(key)
            .map(|tab| tab.window_name.clone())
        else {
            return;
        };
        self.core_hide_window_by_name(&name);
    }

    /// Name-keyed twin of [`Self::core_hide_tab`] for callers that start from
    /// a window name (`.hidewindow <name>`).
    pub(super) fn core_hide_window_by_name(&mut self, name: &str) {
        if let Some(key) = self.find_tab_key_by_name(name) {
            self.hidden_tabs.remove(&key);
        }
        let (w, h) = self.core_layout_size;
        self.app_core.set_known_window_shown(name, false, w, h);
        self.layout_dirty = true;
    }

    /// Find the live tab whose window matches `window_name` (bridges the
    /// core known-windows list, keyed by name, to the GUI zone system,
    /// keyed by TabKey). None for windows that aren't currently a live tab.
    pub(super) fn find_tab_key_by_name(&self, window_name: &str) -> Option<TabKey> {
        self.available_tabs
            .iter()
            .find(|(_, tab)| tab.window_name == window_name)
            .map(|(key, _)| key.clone())
    }


    /// Drop group members that no longer exist, groups that shrink below
    /// two members, and duplicate memberships (first group wins).
    pub(super) fn sanitize_tab_groups(
        groups: Vec<TabGroup>,
        available_tabs: &HashMap<TabKey, GuiTab>,
    ) -> Vec<TabGroup> {
        let mut seen: HashSet<TabKey> = HashSet::new();
        groups
            .into_iter()
            .filter_map(|mut group| {
                group
                    .members
                    .retain(|key| available_tabs.contains_key(key) && seen.insert(key.clone()));
                (group.members.len() >= 2).then_some(group)
            })
            .collect()
    }

    /// The group a tab belongs to, if any.
    pub(super) fn group_for_tab(&self, key: &TabKey) -> Option<&TabGroup> {
        self.tab_groups
            .iter()
            .find(|group| group.members.contains(key))
    }

    /// True when this tab is in a group but is not its leader (first member):
    /// such tabs render inside the leader's window, never on their own.
    pub(super) fn is_grouped_follower(&self, key: &TabKey) -> bool {
        self.group_for_tab(key)
            .is_some_and(|group| group.members.first() != Some(key))
    }

    /// Remove a tab from its group, dissolving groups left with one member.
    pub(super) fn ungroup_tab(&mut self, key: &TabKey) {
        if self.group_for_tab(key).is_none() {
            return;
        }
        Self::drop_tab_from_groups(&mut self.tab_groups, key);
        self.layout_dirty = true;
    }

    /// Dissolve the entire group a tab belongs to (every member becomes a
    /// standalone window again). Used by the Windows manager's "Ungroup"
    /// control. No-op when the tab isn't grouped.
    pub(in crate::frontend::gui) fn dissolve_group_of(&mut self, key: &TabKey) {
        let members: Vec<TabKey> = match self.group_for_tab(key) {
            Some(group) => group.members.clone(),
            None => return,
        };
        self.tab_groups.retain(|group| !group.members.contains(key));
        // Followers rendered inside the old leader; give each its own zone
        // entry again if it somehow lost one (defensive — normally intact).
        for member in &members {
            self.tab_zones
                .entry(member.clone())
                .or_insert_with(|| Self::default_zone_for_tab_key(member));
        }
        self.layout_dirty = true;
    }

    /// Strip a tab from every group's member/merged/end_anchored lists and
    /// drop any group left with fewer than two members. Pure so both
    /// `ungroup_tab` and the delete path can share it (bug: deleting a
    /// grouped window left the group intact, so surviving members stayed
    /// grouped followers — checked and un-re-addable in the Windows menu).
    pub(super) fn drop_tab_from_groups(groups: &mut Vec<TabGroup>, key: &TabKey) {
        for group in groups.iter_mut() {
            group.members.retain(|member| member != key);
            group.merged.retain(|member| member != key);
            group.end_anchored.retain(|member| member != key);
            group.weights.retain(|(member, _)| member != key);
        }
        groups.retain(|group| group.members.len() >= 2);
    }

    /// Forget every scrap of per-tab GUI state for a window that no longer
    /// exists (deleted from the layout). Dissolves its group so surviving
    /// members are freed, and purges the position/zone/visibility maps so a
    /// re-added window of the same key starts clean instead of inheriting a
    /// stale rect or a phantom "hidden" mark. `pending_zones` is keyed by
    /// window name, so the caller passes it separately.
    pub(super) fn forget_tab_state(&mut self, key: &TabKey, window_name: &str) {
        Self::drop_tab_from_groups(&mut self.tab_groups, key);
        // BEFORE dropping this window's own state: strip sibling anchors
        // referencing it from other windows (their resolved rects commit
        // first, using this window's still-present rect — nothing
        // teleports). A hidden window keeps its anchors; only true
        // deletion lands here.
        self.prune_sibling_refs_to(key);
        self.hidden_tabs.remove(key);
        self.main_window_rects.remove(key);
        self.window_anchors.remove(key);
        self.last_center_window_rects.remove(key);
        self.sidebar_gap_above.remove(key);
        self.tab_zones.remove(key);
        self.no_title_tabs.remove(key);
        self.tab_settings.remove(key);
        self.detached_tabs.remove(key);
        self.pending_zones.remove(window_name);
        self.layout_dirty = true;
    }

    /// Add `other` to `leader`'s group (creating one if needed) and move it
    /// into the leader's zone so the group renders on one surface.
    pub(super) fn group_tabs(&mut self, leader: &TabKey, other: TabKey) {
        if leader == &other {
            return;
        }
        // Invariant (see detach_tab): a detached tab is never grouped. A
        // detached leader would leave the follower unrendered — grouped
        // followers are skipped by every zone surface, and the detached
        // render path has no group awareness.
        if self.detached_tabs.contains_key(leader) || self.detached_tabs.contains_key(&other) {
            self.app_core
                .add_system_message("Can't group a detached window. Reattach it first.");
            return;
        }
        self.ungroup_tab(&other);
        let leader_zone = self.zone_for_tab(leader);
        if let Some(index) = self
            .tab_groups
            .iter()
            .position(|group| group.members.contains(leader))
        {
            self.tab_groups[index].members.push(other.clone());
        } else {
            self.tab_groups.push(TabGroup {
                members: vec![leader.clone(), other.clone()],
                horizontal: false,
                merged: Vec::new(),
                end_anchored: Vec::new(),
                weights: Vec::new(),
            });
        }
        self.tab_zones.insert(other, leader_zone);
        self.layout_dirty = true;
    }

    /// Display title for a docked window: grouped leaders show all member
    /// titles joined; everything else shows its own title.
    pub(super) fn window_display_title(&self, tab: &GuiTab) -> String {
        // A per-window custom title (Edit Window dialog) wins over both the
        // stream title and the grouped-member join — the escape hatch for
        // groups whose auto-title runs long. Empty/whitespace falls through.
        if let Some(custom) = self
            .tab_settings
            .get(&tab.id.key)
            .and_then(|settings| settings.custom_title.as_deref())
            .map(str::trim)
            .filter(|title| !title.is_empty())
        {
            return custom.to_string();
        }
        match self.group_for_tab(&tab.id.key) {
            Some(group) if group.members.first() == Some(&tab.id.key) => group
                .members
                .iter()
                .filter_map(|key| self.available_tabs.get(key))
                .map(|member| member.id.title.as_str())
                .collect::<Vec<_>>()
                .join(" + "),
            _ => tab.id.title.clone(),
        }
    }

    /// Render a window's content, or — when the window leads a group — all
    /// member contents split along the group's orientation.
    pub(super) fn render_window_or_group_content(
        &self,
        ui: &mut egui::Ui,
        tab: &GuiTab,
    ) -> Option<GuiLinkClick> {
        // Game content is NOT UI chrome: a skin's [ui] palette colors menus and
        // editors (its `text` can be an accent like stealth's orange), but story
        // / spells / room / inventory text must stay in the theme's own text
        // color, never the chrome accent. Pin the game text color here so every
        // window's content is immune to the global chrome `override_text_color`;
        // explicit per-span colors (highlights, links, stream colors) still win.
        let game_text = theme::color32(self.current_theme.text_primary);
        ui.visuals_mut().override_text_color = Some(game_text);
        ui.visuals_mut().widgets.noninteractive.fg_stroke.color = game_text;
        let members: Vec<GuiTab> = match self.group_for_tab(&tab.id.key) {
            Some(group) => group
                .members
                .iter()
                .filter(|key| !self.hidden_tabs.contains(*key))
                .filter(|key| !self.detached_tabs.contains_key(*key))
                .filter_map(|key| self.available_tabs.get(key).cloned())
                .collect(),
            None => Vec::new(),
        };
        if members.len() < 2 {
            return Self::render_window_content(
                &self.app_core,
                ui,
                tab,
                self.widget_render_settings(&tab.id.key),
            );
        }
        let (horizontal, merged, end_anchored) = self
            .group_for_tab(&tab.id.key)
            .map(|group| {
                (
                    group.horizontal,
                    group.merged.clone(),
                    group.end_anchored.clone(),
                )
            })
            .unwrap_or_default();

        // Partition members into slots along the group axis: a merged
        // member joins its predecessor's slot and stacks along the
        // perpendicular axis (a column of a side-by-side group holds a
        // vertical stack; a row of a stacked group holds a side-by-side
        // run). The first member always opens a slot.
        let mut slots: Vec<Vec<GuiTab>> = Vec::new();
        for member in members {
            if !slots.is_empty() && merged.contains(&member.id.key) {
                slots.last_mut().expect("slots checked non-empty").push(member);
            } else {
                slots.push(vec![member]);
            }
        }

        // One shared background across the whole group rect (the leader's
        // resolved background), painted once BEFORE members render — members
        // suppress their own in render_group_stack, so a grouped mesh reads as
        // a single surface instead of one strip per member.
        if let Some(background) = self.widget_render_settings(&tab.id.key).background {
            // Confine the mesh to the window's REAL content bounds. For a
            // compact/small window (e.g. the compass, whose content has a min
            // size) `available_rect_before_wrap` extends to the whole layout
            // region and even `max_rect` can exceed a window shrunk below the
            // content minimum — so the mesh tiled/clipped past the frame and
            // spilled onto the neighbor. Take the TIGHTEST of the three rects
            // (available, the ui's assigned max_rect, and egui's paint clip) so
            // the mesh never exceeds the smallest, i.e. the actual window.
            let rect = ui
                .available_rect_before_wrap()
                .intersect(ui.max_rect())
                .intersect(ui.clip_rect());
            let shapes = crate::frontend::gui::skin::background_shapes(
                rect,
                &background,
                ui.visuals().window_fill(),
            );
            ui.painter()
                .with_clip_rect(rect)
                .add(egui::Shape::Vec(shapes));
        }

        let mut clicked = None;
        // Each member's screen rect, recorded so window-level drag-and-drop
        // can resolve drops to the member under the pointer instead of the
        // whole group window (e.g. left vs right hand in a hand group).
        let mut member_rects: Vec<(String, Rect)> = Vec::new();
        if horizontal {
            ui.columns(slots.len(), |columns| {
                for (column, slot) in columns.iter_mut().zip(slots.iter()) {
                    let anchored = end_anchored.contains(&slot[0].id.key);
                    self.render_group_stack(
                        column,
                        &tab.id.key,
                        slot,
                        anchored,
                        &mut member_rects,
                        &mut clicked,
                    );
                }
            });
        } else {
            let gap = ui.spacing().item_spacing.y;
            let bar_height = ui.spacing().interact_size.y.max(16.0);
            // A row slot is as tall as its tallest fixed member; any
            // flexible member makes the whole row flexible.
            let slot_heights: Vec<Option<f32>> = slots
                .iter()
                .map(|slot| {
                    slot.iter()
                        .map(|member| self.member_natural_height(gap, bar_height, member))
                        .try_fold(0.0f32, |tallest, natural| {
                            natural.map(|height| tallest.max(height))
                        })
                })
                .collect();
            // A slot's weight is its first member's weight (the slot key).
            // Weighted leftover split: buffs=2 / cooldowns=1 gives buffs twice
            // the height instead of a forced 50/50.
            let slot_weights: Vec<f32> = slots
                .iter()
                .map(|slot| self.member_weight(&tab.id.key, &slot[0].id.key))
                .collect();
            let resolved_slot_heights = Self::distribute_group_heights(
                ui.available_height(),
                gap,
                &slot_heights,
                &slot_weights,
            );
            let width = ui.available_width().max(1.0);
            for (slot, resolved) in slots.iter().zip(&resolved_slot_heights) {
                let each_height = *resolved;
                if let [member] = slot.as_slice() {
                    let block = ui.push_id(&member.id.key, |ui| {
                        ui.allocate_ui(Vec2::new(width, each_height), |ui| {
                            ui.set_min_size(Vec2::new(width, each_height));
                            ui.set_max_height(each_height);
                            if let Some(click) = Self::render_window_content(
                                &self.app_core,
                                ui,
                                member,
                                self.widget_render_settings(&member.id.key),
                            ) {
                                clicked = Some(click);
                            }
                        })
                    });
                    member_rects.push((member.window_name.clone(), block.inner.response.rect));
                } else {
                    ui.allocate_ui(Vec2::new(width, each_height), |ui| {
                        ui.set_min_size(Vec2::new(width, each_height));
                        ui.set_max_height(each_height);
                        ui.columns(slot.len(), |columns| {
                            for (column, member) in columns.iter_mut().zip(slot.iter()) {
                                member_rects
                                    .push((member.window_name.clone(), column.max_rect()));
                                column.push_id(&member.id.key, |ui| {
                                    if let Some(click) = Self::render_window_content(
                                        &self.app_core,
                                        ui,
                                        member,
                                        self.widget_render_settings(&member.id.key),
                                    ) {
                                        clicked = Some(click);
                                    }
                                });
                            }
                        });
                    });
                }
            }
        }
        ui.ctx().data_mut(|data| {
            data.insert_temp(Self::group_member_rects_id(&tab.id.key), member_rects);
        });
        clicked
    }

    /// egui temp-data key for a group leader's per-member screen rects,
    /// refreshed every frame the group renders.
    pub(super) fn group_member_rects_id(leader: &TabKey) -> egui::Id {
        egui::Id::new("gui_group_member_rects").with(leader)
    }

    /// Natural (fixed) height of a group member, when it has one. Compact
    /// widgets (bars, timers, hands) only ever draw one row, so they get
    /// exactly that; the leftover splits among flexible members (doll,
    /// text, ...) instead of equal N-way shares that leave dead space
    /// under each bar. None = flexible.
    pub(super) fn member_natural_height(&self, gap: f32, bar_height: f32, member: &GuiTab) -> Option<f32> {
        match self
            .app_core
            .ui_state
            .windows
            .get(&member.window_name)
            .map(|window| &window.content)
        {
            // Progress/countdown bars are genuinely one row tall. Hands are
            // NOT fixed: their icon scales to fill the height they're given
            // (render_hand_content reads ui.available_height), matching the
            // standalone `compact_height_cap` intent (Hand => no cap). Pinning
            // a grouped hand to one bar row froze its icon at ~16px no matter
            // how the group window was resized — so hands stay flexible (fall
            // through to None) and share the group's growable height.
            Some(WindowContent::Progress(_) | WindowContent::Countdown(_)) => {
                Some(bar_height)
            }
            Some(WindowContent::Betrayer)
                if self.app_core.game_state.betrayer.items.is_empty() =>
            {
                Some(bar_height)
            }
            Some(WindowContent::Encumbrance) => {
                let (show_bar, show_label) =
                    Self::encumbrance_flags(&self.app_core, &member.window_name);
                let rows = (show_bar as u32 + show_label as u32).max(1) as f32;
                Some(bar_height * rows + gap * (rows - 1.0))
            }
            Some(WindowContent::GS4Experience) => {
                let (level, mind, exp_bar, total, ascension) =
                    Self::gs4_experience_flags(&self.app_core, &member.window_name);
                let rows = ([level, mind, exp_bar, total, ascension]
                    .into_iter()
                    .filter(|on| *on)
                    .count()
                    .max(1)) as f32;
                Some(bar_height * rows + gap * (rows - 1.0))
            }
            Some(WindowContent::MiniVitals) => {
                use crate::frontend::gui::persistence::VitalsOrientation;
                let vitals = &self.ui_settings.vitals;
                let row = vitals.bar_height.clamp(8.0, 60.0);
                match vitals.orientation {
                    VitalsOrientation::Horizontal => Some(row),
                    VitalsOrientation::Vertical => {
                        let count = vitals.bars.len().max(1) as f32;
                        Some(row * count + gap * (count - 1.0))
                    }
                }
            }
            // A dashboard is fixed to its row count so a grouped dashboard
            // hugs its grid instead of absorbing the group's leftover space
            // (which left an empty band below a 2-row grid). Rows come from the
            // config + layout, same as the standalone height cap; flow wraps
            // by width, so it stays flexible.
            Some(WindowContent::Dashboard { .. }) => {
                use crate::config::DashboardLayout;
                let data = self
                    .app_core
                    .layout
                    .windows
                    .iter()
                    .find(|def| def.name() == member.window_name)
                    .and_then(|def| match def {
                        crate::config::WindowDef::Dashboard { data, .. } => Some(data),
                        _ => None,
                    })?;
                let count = data.cell_count().max(1);
                let rows = match DashboardLayout::from_str(&data.layout) {
                    DashboardLayout::Flow => return None,
                    DashboardLayout::Horizontal => 1,
                    DashboardLayout::Vertical => count,
                    DashboardLayout::Grid { cols, .. } => count.div_ceil(cols.max(1)),
                };
                Some(bar_height * rows as f32 + gap * (rows.saturating_sub(1) as f32))
            }
            _ => None,
        }
    }

    /// Resolve each member's height along the stack axis. Fixed members
    /// (`Some(natural)`) keep their natural height; the leftover — total
    /// available minus gaps minus the fixed members' heights — splits among
    /// the flexible members (`None`) in proportion to their `weights`.
    ///
    /// A weight <= 0 is treated as 1.0 (the neutral default), so an empty or
    /// all-default weight list reproduces the historical equal split. Each
    /// flexible share is floored at `MIN_FLEX_HEIGHT` so a tiny weight can't
    /// collapse a member to nothing. `weights` is parallel to `natural`
    /// (same length, same order).
    pub(super) fn distribute_group_heights(
        available: f32,
        gap: f32,
        natural: &[Option<f32>],
        weights: &[f32],
    ) -> Vec<f32> {
        const MIN_FLEX_HEIGHT: f32 = 24.0;
        let fixed_total: f32 = natural.iter().flatten().sum();
        let total_gap = gap * (natural.len().saturating_sub(1) as f32);
        let leftover = (available - total_gap - fixed_total).max(0.0);
        // Sum of weights over the flexible members only; each non-positive
        // weight contributes the neutral 1.0.
        let flex_weight_total: f32 = natural
            .iter()
            .zip(weights)
            .filter(|(n, _)| n.is_none())
            .map(|(_, w)| if *w > 0.0 { *w } else { 1.0 })
            .sum();
        natural
            .iter()
            .zip(weights)
            .map(|(n, w)| match n {
                Some(h) => *h,
                None => {
                    if flex_weight_total > 0.0 {
                        let w = if *w > 0.0 { *w } else { 1.0 };
                        (leftover * w / flex_weight_total).max(MIN_FLEX_HEIGHT)
                    } else {
                        MIN_FLEX_HEIGHT
                    }
                }
            })
            .collect()
    }

    /// The weight for a member within its group, defaulting to 1.0 when the
    /// group has no explicit weight for it (or the group isn't found).
    pub(super) fn member_weight(&self, leader: &TabKey, member: &TabKey) -> f32 {
        self.group_for_tab(leader)
            .and_then(|group| {
                group
                    .weights
                    .iter()
                    .find(|(key, _)| key == member)
                    .map(|(_, w)| *w)
            })
            .filter(|w| *w > 0.0)
            .unwrap_or(1.0)
    }

    /// Render one group slot's members stacked vertically (a column of a
    /// side-by-side group, or the whole body of a stacked group's slot).
    /// Fixed members get their natural height and the leftover splits
    /// among flexible ones; when every member is fixed, the leftover pads
    /// the bottom — or the top when the slot is end-anchored, so a bar
    /// stack can hug the column's bottom edge.
    pub(super) fn render_group_stack(
        &self,
        ui: &mut egui::Ui,
        leader: &TabKey,
        members: &[GuiTab],
        end_anchored: bool,
        member_rects: &mut Vec<(String, Rect)>,
        clicked: &mut Option<GuiLinkClick>,
    ) {
        let gap = ui.spacing().item_spacing.y;
        let bar_height = ui.spacing().interact_size.y.max(16.0);
        let natural_heights: Vec<Option<f32>> = members
            .iter()
            .map(|member| self.member_natural_height(gap, bar_height, member))
            .collect();
        let weights: Vec<f32> = members
            .iter()
            .map(|member| self.member_weight(leader, &member.id.key))
            .collect();
        let fixed_total: f32 = natural_heights.iter().flatten().sum();
        let flexible_count = natural_heights.iter().filter(|h| h.is_none()).count() as f32;
        let total_gap = gap * (members.len() as f32 - 1.0);
        // Weighted per-member heights: fixed members keep their natural
        // height, the leftover splits among flexible members by weight.
        let resolved_heights =
            Self::distribute_group_heights(ui.available_height(), gap, &natural_heights, &weights);
        if end_anchored && flexible_count == 0.0 {
            let leftover = ui.available_height() - total_gap - fixed_total;
            if leftover > 0.0 {
                ui.add_space(leftover);
            }
        }
        let width = ui.available_width().max(1.0);
        for (member, each_height) in members.iter().zip(&resolved_heights) {
            let each_height = *each_height;
            let block = ui.push_id(&member.id.key, |ui| {
                ui.allocate_ui(Vec2::new(width, each_height), |ui| {
                    ui.set_min_size(Vec2::new(width, each_height));
                    ui.set_max_height(each_height);
                    // Members in a group don't paint their own background —
                    // the group leader paints ONE shared background across the
                    // whole group rect (see render_window_or_group_content), so
                    // the mesh reads as one surface instead of per-member strips.
                    let mut settings = self.widget_render_settings(&member.id.key);
                    settings.background = None;
                    if let Some(click) =
                        Self::render_window_content(&self.app_core, ui, member, settings)
                    {
                        *clicked = Some(click);
                    }
                })
            });
            member_rects.push((member.window_name.clone(), block.inner.response.rect));
        }
    }

}
