//! Missing-spells watchlist widget: watched spells (`.spellwatch`) that are
//! NOT currently active in the ActiveSpells/Buffs feeds. The list itself is
//! computed in `core::missing_spells`; this widget only displays it.

use ratatui::{buffer::Buffer, layout::Rect};

pub struct MissingSpells {
    widget: super::list_widget::ListWidget,
    base_title: String,
    /// Cache of the rendered list for change detection ("num:name" joined).
    cache: String,
}

impl MissingSpells {
    pub fn new(title: &str) -> Self {
        Self {
            widget: super::list_widget::ListWidget::new(title),
            base_title: title.to_string(),
            cache: String::new(),
        }
    }

    /// Rebuild from the computed missing list. Returns true when the
    /// display changed.
    pub fn update_from_state(
        &mut self,
        missing: &[crate::core::missing_spells::MissingSpell],
        watched_count: usize,
    ) -> bool {
        let new_cache: String = missing
            .iter()
            .map(|spell| format!("{}:{}", spell.number, spell.name))
            .collect::<Vec<_>>()
            .join(",")
            + &format!("|{watched_count}");
        if self.cache == new_cache {
            return false;
        }
        self.cache = new_cache;
        self.widget.clear();

        if watched_count == 0 {
            self.widget.add_simple_line(
                ".spellwatch add <n> to watch".to_string(),
                Some("#666666".to_string()),
                None,
            );
        } else if missing.is_empty() {
            self.widget.add_simple_line(
                "All spells up".to_string(),
                Some("#5f875f".to_string()),
                None,
            );
        } else {
            for spell in missing {
                self.widget.add_simple_line(
                    format!("{} {}", spell.number, spell.name),
                    Some("#d78700".to_string()),
                    None,
                );
            }
        }
        // Title carries the count so a glance tells the story even scrolled.
        let title = if missing.is_empty() {
            self.base_title.clone()
        } else {
            format!("{} ({})", self.base_title, missing.len())
        };
        self.widget.set_title(title);
        true
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.widget.render(area, buf);
    }
}
