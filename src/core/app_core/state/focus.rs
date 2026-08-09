//! Focused-window scrolling and focus cycling, including the focus
//! order derived from the layout.

use super::*;

impl AppCore {
    // ===========================================================================================
    // Window Scrolling Methods
    // ===========================================================================================

    /// Scroll the currently focused window up by one line
    pub fn scroll_current_window_up_one(&mut self) {
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    content.scroll_up(1);
                    self.needs_render = true;
                }
            }
        }
    }

    /// Scroll the currently focused window down by one line
    pub fn scroll_current_window_down_one(&mut self) {
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    content.scroll_down(1);
                    self.needs_render = true;
                }
            }
        }
    }

    /// Scroll the currently focused window up by one page
    pub fn scroll_current_window_up_page(&mut self) {
        tracing::debug!("scroll_current_window_up_page called, focused_window={:?}", self.ui_state.focused_window);
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                tracing::debug!("Found window '{}', widget_type={:?}", window_name, window.widget_type);
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    // Use a reasonable page size (20 lines)
                    let old_offset = content.scroll_offset;
                    content.scroll_up(20);
                    tracing::info!("Scrolled '{}' up: {} -> {}", window_name, old_offset, content.scroll_offset);
                    self.needs_render = true;
                } else {
                    tracing::debug!("Window '{}' content is not Text type", window_name);
                }
            } else {
                tracing::warn!("Focused window '{}' not found in windows map", window_name);
            }
        } else {
            tracing::warn!("No focused window set for scrolling");
        }
    }

    /// Scroll the currently focused window down by one page
    pub fn scroll_current_window_down_page(&mut self) {
        tracing::debug!("scroll_current_window_down_page called, focused_window={:?}", self.ui_state.focused_window);
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                tracing::debug!("Found window '{}', widget_type={:?}", window_name, window.widget_type);
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    // Use a reasonable page size (20 lines)
                    let old_offset = content.scroll_offset;
                    content.scroll_down(20);
                    tracing::info!("Scrolled '{}' down: {} -> {}", window_name, old_offset, content.scroll_offset);
                    self.needs_render = true;
                } else {
                    tracing::debug!("Window '{}' content is not Text type", window_name);
                }
            } else {
                tracing::warn!("Focused window '{}' not found in windows map", window_name);
            }
        } else {
            tracing::warn!("No focused window set for scrolling");
        }
    }

    /// Scroll the currently focused window to the top (oldest content)
    pub fn scroll_current_window_home(&mut self) {
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    content.scroll_to_top();
                    self.needs_render = true;
                }
            }
        }
    }

    /// Scroll the currently focused window to the bottom (newest content)
    pub fn scroll_current_window_end(&mut self) {
        if let Some(window_name) = &self.ui_state.focused_window.clone() {
            if let Some(window) = self.ui_state.windows.get_mut(window_name) {
                if let crate::data::WindowContent::Text(ref mut content) = window.content {
                    content.scroll_to_bottom();
                    self.needs_render = true;
                }
            }
        }
    }

    /// Cycle to the next scrollable text window
    /// Uses focus configuration (types + optional order) to choose focusable windows.
    pub fn cycle_focused_window(&mut self) {
        let focus_order = self.build_focus_order();
        if focus_order.is_empty() {
            return;
        }

        let current_idx = self
            .ui_state
            .focused_window
            .as_ref()
            .and_then(|name| focus_order.iter().position(|n| n == name))
            .unwrap_or(usize::MAX);

        let next_idx = if current_idx == usize::MAX {
            0
        } else {
            (current_idx + 1) % focus_order.len()
        };
        let next_name = focus_order[next_idx].clone();

        self.ui_state.set_focus(Some(next_name.clone()));
        self.add_system_message(&format!("Focused window: {}", next_name));
        self.needs_render = true;
        tracing::debug!("Cycled focused window to '{}'", next_name);
    }

    /// Cycle focus backwards through the focus order.
    pub fn cycle_focused_window_reverse(&mut self) {
        let focus_order = self.build_focus_order();
        if focus_order.is_empty() {
            return;
        }

        let current_idx = self
            .ui_state
            .focused_window
            .as_ref()
            .and_then(|name| focus_order.iter().position(|n| n == name))
            .unwrap_or(0);

        let prev_idx = if current_idx == 0 {
            focus_order.len() - 1
        } else {
            current_idx - 1
        };
        let prev_name = focus_order[prev_idx].clone();

        self.ui_state.set_focus(Some(prev_name.clone()));
        self.needs_render = true;
        tracing::debug!("Cycled focused window to '{}' (reverse)", prev_name);
    }

    pub(super) fn build_focus_order(&self) -> Vec<String> {
        let focus_config = &self.config.ui.focus;
        let mut focusable = std::collections::HashSet::new();
        if !focus_config.types.is_empty() {
            for entry in &focus_config.types {
                focusable.insert(entry.trim().to_lowercase());
            }
        }
        let mut excluded = std::collections::HashSet::new();
        for entry in &focus_config.exclude {
            let trimmed = entry.trim();
            if !trimmed.is_empty() {
                excluded.insert(trimmed.to_lowercase());
            }
        }

        let mut names = Vec::new();

        if !focus_config.order.is_empty() {
            for name in &focus_config.order {
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if excluded.contains(&trimmed.to_lowercase()) {
                    continue;
                }
                if let Some(window) = self.ui_state.windows.get(trimmed) {
                    if !window.visible {
                        continue;
                    }
                    if Self::is_focusable_widget(&window.widget_type, &focusable) {
                        names.push(trimmed.to_string());
                    }
                }
            }
        } else {
            for window_def in &self.layout.windows {
                if !window_def.base().visibility.is_shown() {
                    continue;
                }
                let name = window_def.name();
                if excluded.contains(&name.to_lowercase()) {
                    continue;
                }
                if let Some(window) = self.ui_state.windows.get(name) {
                    if Self::is_focusable_widget(&window.widget_type, &focusable) {
                        names.push(name.to_string());
                    }
                }
            }
        }

        for (name, window) in &self.ui_state.windows {
            if !window.visible {
                continue;
            }
            if excluded.contains(&name.to_lowercase()) {
                continue;
            }
            if names.contains(name) {
                continue;
            }
            if Self::is_focusable_widget(&window.widget_type, &focusable) {
                names.push(name.clone());
            }
        }

        names
    }

    pub(super) fn is_focusable_widget(
        widget_type: &crate::data::WidgetType,
        focusable: &std::collections::HashSet<String>,
    ) -> bool {
        if focusable.is_empty() {
            return !matches!(widget_type, crate::data::WidgetType::CommandInput);
        }
        let kind = match widget_type {
            crate::data::WidgetType::Text => "text",
            crate::data::WidgetType::TabbedText => "tabbedtext",
            crate::data::WidgetType::Progress => "progress",
            crate::data::WidgetType::Countdown => "countdown",
            crate::data::WidgetType::Compass => "compass",
            crate::data::WidgetType::Map => "map",
            crate::data::WidgetType::Indicator => "indicator",
            crate::data::WidgetType::Room => "room",
            crate::data::WidgetType::Inventory => "inventory",
            crate::data::WidgetType::Reserve => "reserve",
            crate::data::WidgetType::CommandInput => "command_input",
            crate::data::WidgetType::Dashboard => "dashboard",
            crate::data::WidgetType::InjuryDoll => "injury_doll",
            crate::data::WidgetType::Hand => "hand",
            crate::data::WidgetType::ActiveEffects => "active_effects",
            crate::data::WidgetType::Targets => "targets",
            crate::data::WidgetType::Players => "players",
            crate::data::WidgetType::Spells => "spells",
            crate::data::WidgetType::Spacer => "spacer",
            crate::data::WidgetType::Performance => "performance",
            crate::data::WidgetType::Perception => "perception",
            crate::data::WidgetType::Container => "container",
            crate::data::WidgetType::Experience => "experience",
            crate::data::WidgetType::GS4Experience => "gs4_experience",
            crate::data::WidgetType::Encumbrance => "encum",
            crate::data::WidgetType::Quickbar => "quickbar",
            crate::data::WidgetType::Hotkeybar => "hotkeybar",
            crate::data::WidgetType::MiniVitals => "minivitals",
            crate::data::WidgetType::Betrayer => "betrayer",
            crate::data::WidgetType::Items => "items",
            crate::data::WidgetType::WebUi => "webui",
            crate::data::WidgetType::DialogPanel => "dialogpanel",
        };
        focusable.contains(kind)
    }
}
