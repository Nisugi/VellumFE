//! Containers window (TUI): the managed-inventory tree (extended feed
//! snapshot, `.invsync`) rendered as an indented, always-expanded list -
//! containers with capacity and open/closed state, items nested under
//! their real parents. Data comes from `GameState.managed_inventory`;
//! this widget only displays it.

use crate::core::state::{ManagedInventoryItem, ManagedInventoryState};
use ratatui::{buffer::Buffer, layout::Rect};

pub struct ContainersWindow {
    widget: super::list_widget::ListWidget,
    base_title: String,
    /// (token, generation) of the rendered snapshot, for change detection.
    cache: Option<(String, u64)>,
}

const CONTAINER_COLOR: &str = "#87afd7";
const CLOSED_COLOR: &str = "#d78700";
const GROUP_COLOR: &str = "#666666";

impl ContainersWindow {
    pub fn new(title: &str) -> Self {
        Self {
            widget: super::list_widget::ListWidget::new(title),
            base_title: title.to_string(),
            cache: None,
        }
    }

    /// Rebuild from the snapshot. Returns true when the display changed.
    pub fn update_from_state(&mut self, snapshot: Option<&ManagedInventoryState>) -> bool {
        let key = snapshot.map(|s| (s.token.clone(), s.generation));
        if self.cache == key {
            return false;
        }
        self.cache = key;
        self.widget.clear();

        let Some(snap) = snapshot else {
            self.widget.add_simple_line(
                ".invsync to load".to_string(),
                Some(GROUP_COLOR.to_string()),
                None,
            );
            self.widget.set_title(self.base_title.clone());
            return true;
        };

        let mut children: std::collections::HashMap<&str, Vec<&ManagedInventoryItem>> =
            std::collections::HashMap::new();
        for item in &snap.items {
            children.entry(item.parent.as_str()).or_default().push(item);
        }
        let group_of = |relation: &str| -> usize {
            match relation {
                "righthand" | "lefthand" => 0,
                "worn" => 1,
                "atfeet" | "reserved" => 2,
                _ => 3,
            }
        };
        const GROUPS: [&str; 4] = ["hands", "worn", "at feet", "room"];
        let mut roots: [Vec<&ManagedInventoryItem>; 4] = Default::default();
        for item in &snap.items {
            if item.parent == "player" || item.parent == "room" {
                roots[group_of(&item.relation)].push(item);
            }
        }
        for (gi, group) in roots.iter().enumerate() {
            if group.is_empty() {
                continue;
            }
            self.widget.add_simple_line(
                format!("- {} -", GROUPS[gi]),
                Some(GROUP_COLOR.to_string()),
                None,
            );
            for item in group {
                self.add_node(item, &children, 0);
            }
        }
        let title = if snap.complete {
            format!("{} ({})", self.base_title, snap.items.len())
        } else {
            format!("{} ({}, incomplete)", self.base_title, snap.items.len())
        };
        self.widget.set_title(title);
        true
    }

    fn add_node(
        &mut self,
        item: &ManagedInventoryItem,
        children: &std::collections::HashMap<&str, Vec<&ManagedInventoryItem>>,
        depth: usize,
    ) {
        if depth > 16 {
            return; // malformed parent cycle guard
        }
        let indent = " ".repeat(depth * 2);
        let kids = children.get(item.id.as_str());
        let is_container = item.is_container() || kids.is_some_and(|k| !k.is_empty());
        if is_container {
            let state = if item.is_locked() {
                " [locked]"
            } else if item.is_closed() {
                " [closed]"
            } else {
                ""
            };
            let count = kids.map(|k| k.len()).unwrap_or(0);
            let capacity = match (item.in_encum, item.in_capacity()) {
                (Some(cur), Some(cap)) => format!(" {cur}/{} lbs", cap.pounds),
                _ => String::new(),
            };
            let color = if item.is_closed() || item.is_locked() {
                CLOSED_COLOR
            } else {
                CONTAINER_COLOR
            };
            self.widget.add_simple_line(
                format!("{indent}{}{state} ({count}){capacity}", item.name),
                Some(color.to_string()),
                None,
            );
            if let Some(kids) = kids {
                for kid in kids {
                    self.add_node(kid, children, depth + 1);
                }
            }
        } else {
            self.widget
                .add_simple_line(format!("{indent}{}", item.name), None, None);
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.widget.render(area, buf);
    }
}
