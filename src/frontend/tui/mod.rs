//! TUI Frontend - Terminal UI using ratatui
//!
//! This module implements the Frontend trait for terminal rendering.

mod active_effects;
pub mod crossterm_bridge;
pub mod textarea_bridge;
pub mod color_form;
pub mod color_palette_browser;
mod color_picker;
mod colors;
mod command_line;
mod input;
mod command_input;
mod compass;
mod countdown;
mod dashboard;
mod frontend_impl;
mod hand;
pub mod highlight_browser;
pub mod highlight_form;
mod highlight_utils;
pub mod indicator_template_editor;
mod indicator;
mod injury_doll;
mod inventory_window;
pub mod keybind_browser;
pub mod keybind_form;
pub mod menu_actions;
pub mod menu_builders;
mod perception;
mod performance_stats;
mod players;
mod popup_menu;
mod progress_bar;
mod room_window;
mod room_window_ops;
mod search;
mod sync;
mod scrollable_container;
pub mod settings_editor;
mod spacer;
pub mod spell_color_browser;
pub mod spell_color_form;
mod spells_window;
mod tabbed_text_window;
mod title_position;
mod targets;
mod dropdown_targets;
mod experience;
mod text_window;
mod container_window;
mod runtime;
pub mod theme_browser;
pub mod theme_editor;
mod theme_cache;
mod resize;
pub mod uicolors_browser;
pub mod window_editor;
mod widget_manager;
mod input_handlers;

pub use colors::resolve_window_colors;
pub use runtime::run;
pub mod widget_traits;
use theme_cache::ThemeCache;
use widget_manager::WidgetManager;
use resize::ResizeDebouncer;
use anyhow::Result;
use crossterm::{execute, terminal::{enable_raw_mode, EnterAlternateScreen}};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::HashMap;
use std::io;

pub struct TuiFrontend {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    /// Widget manager - handles all widget caches and synchronization
    widget_manager: WidgetManager,
    /// Active popup menu (if any)
    popup_menu: Option<popup_menu::PopupMenu>,
    /// Active submenu (if any)
    submenu: Option<popup_menu::PopupMenu>,
    /// Cached submenu items for the main menu
    menu_categories: HashMap<String, Vec<popup_menu::MenuItem>>,
    /// Active window editor (if any)
    pub window_editor: Option<window_editor::WindowEditor>,
    /// Indicator template editor (global templates)
    pub indicator_template_editor: Option<indicator_template_editor::IndicatorTemplateEditor>,
    /// Active highlight browser (if any)
    pub highlight_browser: Option<highlight_browser::HighlightBrowser>,
    /// Active highlight form (if any)
    pub highlight_form: Option<highlight_form::HighlightFormWidget>,
    /// Active keybind browser (if any)
    pub keybind_browser: Option<keybind_browser::KeybindBrowser>,
    /// Active keybind form (if any)
    pub keybind_form: Option<keybind_form::KeybindFormWidget>,
    /// Active color palette browser (if any)
    pub color_palette_browser: Option<color_palette_browser::ColorPaletteBrowser>,
    /// Active color form (if any)
    pub color_form: Option<color_form::ColorForm>,
    /// Active UI colors browser (if any)
    pub uicolors_browser: Option<uicolors_browser::UIColorsBrowser>,
    /// Active spell color browser (if any)
    pub spell_color_browser: Option<spell_color_browser::SpellColorBrowser>,
    /// Active spell color form (if any)
    pub spell_color_form: Option<spell_color_form::SpellColorFormWidget>,
    /// Active theme browser (if any)
    pub theme_browser: Option<theme_browser::ThemeBrowser>,
    /// Active theme editor (if any)
    pub theme_editor: Option<theme_editor::ThemeEditor>,
    /// Active settings editor (if any)
    pub settings_editor: Option<settings_editor::SettingsEditor>,
    /// Debouncer for terminal resize events (100ms debounce)
    resize_debouncer: ResizeDebouncer,
    /// Theme cache to avoid HashMap lookup + clone every render
    theme_cache: ThemeCache,
}

impl TuiFrontend {
    pub fn new() -> Result<Self> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        )?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            widget_manager: WidgetManager::new(),
            popup_menu: None,
            submenu: None,
            menu_categories: HashMap::new(),
            window_editor: None,
            indicator_template_editor: None,
            highlight_browser: None,
            highlight_form: None,
            keybind_browser: None,
            keybind_form: None,
            color_palette_browser: None,
            color_form: None,
            uicolors_browser: None,
            spell_color_browser: None,
            spell_color_form: None,
            theme_browser: None,
            theme_editor: None,
            settings_editor: None,
            resize_debouncer: ResizeDebouncer::new(300), // 300ms debounce
            theme_cache: ThemeCache::new(),
        })
    }

    /// Update cached theme (call this when theme changes via command/browser)
    pub fn update_theme_cache(&mut self, theme_id: String, theme: crate::theme::AppTheme) {
        self.theme_cache.update(theme_id, theme);
    }

    /// Get the terminal size (width, height)
    pub fn size(&self) -> (u16, u16) {
        let size = self.terminal.size().unwrap_or_default();
        (size.width, size.height)
    }

    /// Navigate to next tab in all tabbed windows
    pub fn next_tab_all(&mut self) {
        for widget in self.widget_manager.tabbed_text_windows.values_mut() {
            widget.next_tab();
        }
    }

    /// Navigate to previous tab in all tabbed windows
    pub fn prev_tab_all(&mut self) {
        for widget in self.widget_manager.tabbed_text_windows.values_mut() {
            widget.prev_tab();
        }
    }

    /// Navigate to next tab with unread messages (searches all tabbed windows)
    /// Returns true if found, false if no unread tabs
    pub fn go_to_next_unread_tab(&mut self) -> bool {
        for widget in self.widget_manager.tabbed_text_windows.values_mut() {
            if widget.next_tab_with_unread() {
                return true; // Found and switched
            }
        }
        false
    }

    /// Propagate the active tab index from tabbed widgets back into ui_state so sync doesn't reset it.
    pub fn sync_tabbed_active_state(&mut self, app_core: &mut crate::core::AppCore) {
        for (name, widget) in &self.widget_manager.tabbed_text_windows {
            if let Some(window_state) = app_core.ui_state.get_window_mut(name) {
                if let crate::data::WindowContent::TabbedText(tabbed) = &mut window_state.content {
                    let active = widget.get_active_tab_index();
                    if active < tabbed.tabs.len() {
                        tabbed.active_tab_index = active;
                    }
                }
            }
        }
    }

    /// Scroll a window by a number of lines across supported widget types
    pub fn scroll_window(&mut self, window_name: &str, lines: i32) {
        // Try text window first
        if let Some(text_window) = self.widget_manager.text_windows.get_mut(window_name) {
            if lines > 0 {
                text_window.scroll_up(lines as usize);
            } else if lines < 0 {
                text_window.scroll_down((-lines) as usize);
            }
            return;
        }

        // Try room window
        if let Some(room_window) = self.widget_manager.room_windows.get_mut(window_name) {
            if lines > 0 {
                room_window.scroll_up(lines as usize);
            } else if lines < 0 {
                room_window.scroll_down((-lines) as usize);
            }
            return;
        }

        // Try inventory window
        if let Some(inventory_window) = self.widget_manager.inventory_windows.get_mut(window_name) {
            if lines > 0 {
                inventory_window.scroll_up(lines as usize);
            } else if lines < 0 {
                inventory_window.scroll_down((-lines) as usize);
            }
            return;
        }

        // Try spells window
        if let Some(spells_window) = self.widget_manager.spells_windows.get_mut(window_name) {
            if lines > 0 {
                spells_window.scroll_up(lines as usize);
            } else if lines < 0 {
                spells_window.scroll_down((-lines) as usize);
            }
            return;
        }

        // Try active_effects widget
        if let Some(active_effects) = self.widget_manager.active_effects_windows.get_mut(window_name) {
            if lines > 0 {
                active_effects.scroll_up(lines as usize);
            } else if lines < 0 {
                active_effects.scroll_down((-lines) as usize);
            }
            return;
        }

        // Try targets widget
        if let Some(targets) = self.widget_manager.targets_widgets.get_mut(window_name) {
            if lines > 0 {
                targets.scroll_up(lines as usize);
            } else if lines < 0 {
                targets.scroll_down((-lines) as usize);
            }
            return;
        }

        // Try players widget
        if let Some(players) = self.widget_manager.players_widgets.get_mut(window_name) {
            if lines > 0 {
                players.scroll_up(lines as usize);
            } else if lines < 0 {
                players.scroll_down((-lines) as usize);
            }
            return;
        }

        // Try tabbed text window
        if let Some(tabbed_window) = self.widget_manager.tabbed_text_windows.get_mut(window_name) {
            if lines > 0 {
                tabbed_window.scroll_up(lines as usize);
            } else if lines < 0 {
                tabbed_window.scroll_down((-lines) as usize);
            }
        }
    }

    // Note: refresh_highlights removed - highlights now applied in core (MessageProcessor)

    /// Load color_palette entries into terminal palette slots using OSC 4
    ///
    /// Reads colors from config.colors.color_palette and writes each color
    /// that has a slot assignment to the terminal using OSC 4 escape sequences.
    pub fn execute_setpalette(&mut self, app_core: &crate::core::AppCore) -> anyhow::Result<()> {
        use std::io::Write;

        let palette = &app_core.config.colors.color_palette;
        let backend = self.terminal.backend_mut();
        let mut count = 0;

        for palette_color in palette {
            if let Some(slot) = palette_color.slot {
                // Parse hex color to RGB directly - bypass mode-aware conversion
                // (parse_hex_color returns Indexed in Slot mode, breaking the Rgb match)
                let hex = palette_color.color.trim().trim_start_matches('#');
                if hex.len() == 6 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        u8::from_str_radix(&hex[0..2], 16),
                        u8::from_str_radix(&hex[2..4], 16),
                        u8::from_str_radix(&hex[4..6], 16),
                    ) {
                        // OSC 4 format: ESC]4;<slot>;rgb:<rr>/<gg>/<bb>BEL
                        let seq = format!(
                            "\x1b]4;{};rgb:{:02x}/{:02x}/{:02x}\x07",
                            slot, r, g, b
                        );
                        backend.write_all(seq.as_bytes())?;
                        count += 1;
                    }
                }
            }
        }
        backend.flush()?;
        tracing::info!("Loaded {} colors into terminal palette", count);
        Ok(())
    }

    /// Reset terminal palette to defaults using OSC 104
    pub fn execute_resetpalette(&mut self) -> anyhow::Result<()> {
        use std::io::Write;

        let backend = self.terminal.backend_mut();
        backend.write_all(b"\x1b]104\x07")?;
        backend.flush()?;
        tracing::info!("Reset terminal palette to defaults");
        Ok(())
    }
}
