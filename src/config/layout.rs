//! Layout persistence and manipulation.
//!
//! `Layout` is the saved window arrangement (layout.toml); `LayoutMapping`
//! maps terminal size ranges to named layouts. Loading, saving, scaling,
//! and window add/hide/remove live here.

use super::*;

/// Terminal size range to layout mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutMapping {
    pub min_width: u16,
    pub min_height: u16,
    pub max_width: u16,
    pub max_height: u16,
    pub layout: String, // Layout name (e.g., "compact1", "half_screen")
}

impl LayoutMapping {
    /// Check if terminal size matches this mapping
    pub fn matches(&self, width: u16, height: u16) -> bool {
        width >= self.min_width
            && width <= self.max_width
            && height >= self.min_height
            && height <= self.max_height
    }
}

// CommandInputConfig removed - command_input is now a regular window in the windows array

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayoutConfig {
    // Layout is now entirely defined by window positions and sizes
    // No global grid needed
}

/// Represents a saved layout (windows only - command_input is just another window)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub windows: Vec<WindowDef>,
    #[serde(default)]
    pub terminal_width: Option<u16>, // Designed terminal width (for resize calculations)
    #[serde(default)]
    pub terminal_height: Option<u16>, // Designed terminal height (for resize calculations)
    #[serde(default)]
    pub base_layout: Option<String>, // Reference to base layout (for auto layouts)
    #[serde(default)]
    pub theme: Option<String>, // Theme applied when this layout was saved
}

/// Content alignment within widget area (used when borders are removed)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentAlign {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl ContentAlign {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "top-left" | "topleft" => ContentAlign::TopLeft,
            "top" | "top-center" | "topcenter" => ContentAlign::Top,
            "top-right" | "topright" => ContentAlign::TopRight,
            "left" | "center-left" | "centerleft" => ContentAlign::Left,
            "center" => ContentAlign::Center,
            "right" | "center-right" | "centerright" => ContentAlign::Right,
            "bottom-left" | "bottomleft" => ContentAlign::BottomLeft,
            "bottom" | "bottom-center" | "bottomcenter" => ContentAlign::Bottom,
            "bottom-right" | "bottomright" => ContentAlign::BottomRight,
            _ => ContentAlign::TopLeft, // Default
        }
    }

    /// Calculate offset for rendering content within a larger area
    /// Returns (row_offset, col_offset)
    pub fn calculate_offset(
        &self,
        content_width: u16,
        content_height: u16,
        area_width: u16,
        area_height: u16,
    ) -> (u16, u16) {
        let row_offset = match self {
            ContentAlign::TopLeft | ContentAlign::Top | ContentAlign::TopRight => 0,
            ContentAlign::Left | ContentAlign::Center | ContentAlign::Right => {
                (area_height.saturating_sub(content_height)) / 2
            }
            ContentAlign::BottomLeft | ContentAlign::Bottom | ContentAlign::BottomRight => {
                area_height.saturating_sub(content_height)
            }
        };

        let col_offset = match self {
            ContentAlign::TopLeft | ContentAlign::Left | ContentAlign::BottomLeft => 0,
            ContentAlign::Top | ContentAlign::Center | ContentAlign::Bottom => {
                (area_width.saturating_sub(content_width)) / 2
            }
            ContentAlign::TopRight | ContentAlign::Right | ContentAlign::BottomRight => {
                area_width.saturating_sub(content_width)
            }
        };

        (row_offset, col_offset)
    }
}

fn default_windows() -> Vec<WindowDef> {
    // Default layout: just main text window and command input
    // Users can add more windows via .addwindow command
    vec![
        Config::get_window_template("main").expect("main template should exist"),
        Config::get_window_template("command_input").expect("command_input template should exist"),
    ]
}

impl Layout {
    /// Load layout from file using new profile-based structure
    /// Priority: ~/.vellum-fe/{character}/layout.toml → ~/.vellum-fe/layouts/layout.toml → embedded
    pub fn load(character: Option<&str>) -> Result<Self> {
        let (layout, _base_name) = Self::load_with_terminal_size(character, None)?;
        Ok(layout)
    }

    /// Load layout with terminal size for auto-selection
    /// Returns (layout, base_layout_name) where base_layout_name is the source layout file name (without .toml)
    ///
    /// New structure:
    /// 1. ~/.vellum-fe/{character}/layout.toml (auto-save from exit)
    /// 2. ~/.vellum-fe/default/layouts/default.toml (shared default)
    /// 3. Embedded default
    pub fn load_with_terminal_size(
        character: Option<&str>,
        terminal_size: Option<(u16, u16)>,
    ) -> Result<(Self, Option<String>)> {
        let profile_dir = Config::profile_dir(character)?;
        let default_profile_dir = Config::profile_dir(None)?; // ~/.vellum-fe/default/
        let _shared_layouts_dir = Config::layouts_dir()?; // ~/.vellum-fe/layouts/ (templates only)

        // 1. Try character auto-save layout: ~/.vellum-fe/{character}/layout.toml
        let auto_layout_path = profile_dir.join("layout.toml");
        if auto_layout_path.exists() {
            tracing::info!("Loading auto-save layout from {:?}", auto_layout_path);
            let mut layout = Self::load_from_file(&auto_layout_path)?;
            let base_name = layout
                .base_layout
                .clone()
                .unwrap_or_else(|| "default".to_string());

            // Check if we need to scale from base layout
            if let Some((curr_width, curr_height)) = terminal_size {
                if let (Some(layout_width), Some(layout_height)) =
                    (layout.terminal_width, layout.terminal_height)
                {
                    if curr_width != layout_width || curr_height != layout_height {
                        tracing::info!(
                            "Terminal size changed from {}x{} to {}x{}, scaling current layout (preserving user customizations like spacers)",
                            layout_width,
                            layout_height,
                            curr_width,
                            curr_height
                        );

                        // DO NOT load base layout - it would overwrite user customizations!
                        // The current layout (with spacers and other customizations) is the correct baseline
                        // Scale the CURRENT layout to the new terminal size
                        layout.scale_to_terminal_size(curr_width, curr_height);
                    }
                }
            }

            return Ok((layout, Some(base_name)));
        }

        // 2. Try default profile auto-save layout: ~/.vellum-fe/default/layout.toml
        let default_path = default_profile_dir.join("layout.toml");
        if default_path.exists() {
            tracing::info!(
                "Loading default profile auto-save layout from {:?}",
                default_path
            );
            let layout = Self::load_from_file(&default_path)?;
            return Ok((layout, Some("layout".to_string())));
        }

        // 3. Fall back to embedded default (should have been extracted by extract_defaults())
        tracing::warn!(
            "No layout found, using embedded default (this should have been extracted!)"
        );
        let layout: Layout =
            toml::from_str(LAYOUT_DEFAULT).context("Failed to parse embedded default layout")?;

        Ok((layout, Some("layout".to_string())))
    }

    /// Scale all windows proportionally to fit new terminal size
    pub fn scale_to_terminal_size(&mut self, new_width: u16, new_height: u16) {
        let base_width = self.terminal_width.unwrap_or(new_width);
        let base_height = self.terminal_height.unwrap_or(new_height);

        if base_width == 0 || base_height == 0 {
            tracing::warn!(
                "Invalid base terminal size ({}x{}), skipping scale",
                base_width,
                base_height
            );
            return;
        }

        let scale_x = new_width as f32 / base_width as f32;
        let scale_y = new_height as f32 / base_height as f32;

        tracing::info!(
            "Scaling layout from {}x{} to {}x{} (scale: {:.2}x, {:.2}y)",
            base_width,
            base_height,
            new_width,
            new_height,
            scale_x,
            scale_y
        );

        for window in &mut self.windows {
            // Capture name and type before mutable borrow
            let window_name = window.name().to_string();
            let window_type = window.widget_type().to_string();

            let base = window.base_mut();
            let old_col = base.col;
            let old_row = base.row;
            let old_cols = base.cols;
            let old_rows = base.rows;

            base.col = (base.col as f32 * scale_x).round() as u16;
            base.row = (base.row as f32 * scale_y).round() as u16;
            base.cols = (base.cols as f32 * scale_x).round() as u16;
            base.rows = (base.rows as f32 * scale_y).round() as u16;

            // Ensure minimum sizes
            if base.cols < 1 {
                base.cols = 1;
            }
            if base.rows < 1 {
                base.rows = 1;
            }

            // Respect min/max constraints if set
            if let Some(min_cols) = base.min_cols {
                if base.cols < min_cols {
                    base.cols = min_cols;
                }
            }
            if let Some(max_cols) = base.max_cols {
                if base.cols > max_cols {
                    base.cols = max_cols;
                }
            }
            if let Some(min_rows) = base.min_rows {
                if base.rows < min_rows {
                    base.rows = min_rows;
                }
            }
            if let Some(max_rows) = base.max_rows {
                if base.rows > max_rows {
                    base.rows = max_rows;
                }
            }

            tracing::debug!(
                "  {} [{}]: pos {}x{} -> {}x{}, size {}x{} -> {}x{}",
                window_name,
                window_type,
                old_col,
                old_row,
                base.col,
                base.row,
                old_cols,
                old_rows,
                base.cols,
                base.rows
            );
        }

        // Update terminal size to new size
        self.terminal_width = Some(new_width);
        self.terminal_height = Some(new_height);
    }

    pub fn load_from_file(path: &std::path::Path) -> Result<Self> {
        let contents =
            fs::read_to_string(path).context(format!("Failed to read layout file: {:?}", path))?;
        let mut layout: Layout = toml::from_str(&contents)
            .context(format!("Failed to parse layout file: {:?}", path))?;

        // Debug: Log what terminal size was loaded
        tracing::debug!(
            "Loaded layout from {:?}: terminal_width={:?}, terminal_height={:?}",
            path,
            layout.terminal_width,
            layout.terminal_height
        );

        // Migration: Ensure command_input exists in windows array with valid values
        if let Some(idx) = layout
            .windows
            .iter()
            .position(|w| w.widget_type() == "command_input")
        {
            // Command input exists but might have invalid values (cols=0, rows=0, etc)
            let cmd_input_base = layout.windows[idx].base_mut();
            if cmd_input_base.cols == 0 || cmd_input_base.rows == 0 {
                tracing::warn!(
                    "Command input has invalid size ({}x{}), fixing with defaults",
                    cmd_input_base.rows,
                    cmd_input_base.cols
                );
                // Get defaults from default_windows()
                if let Some(default_cmd) = default_windows()
                    .into_iter()
                    .find(|w| w.widget_type() == "command_input")
                {
                    let default_base = default_cmd.base();
                    cmd_input_base.row = default_base.row;
                    cmd_input_base.col = default_base.col;
                    cmd_input_base.rows = default_base.rows;
                    cmd_input_base.cols = default_base.cols;
                }
            }
        } else {
            // Command input doesn't exist - add it
            if let Some(cmd_input) = default_windows()
                .into_iter()
                .find(|w| w.widget_type() == "command_input")
            {
                tracing::info!("Migrating command_input to windows array");
                layout.windows.push(cmd_input);
            }
        }

        for window in &mut layout.windows {
            if window.widget_type() == "targets" {
                let base = window.base_mut();
                if base.name == "dd_targets" {
                    tracing::info!("Renaming legacy targets window 'dd_targets' -> 'targets'");
                    base.name = "targets".to_string();
                }
            }
        }

        Ok(layout)
    }

    /// Save layout to file
    /// If force_terminal_size is true, always update terminal_width/height to terminal_size
    /// Save layout to shared layouts directory (.savelayout command)
    /// Saves to: ~/.vellum-fe/default/layouts/{name}.toml
    /// Normalize windows before saving - convert None colors back to "-" to preserve transparency
    fn normalize_windows_for_save(&mut self) {
        // Sort windows: spacers last, others maintain relative order
        // This prevents spacers from appearing first in TOML and overlapping during resize
        self.windows.sort_by_key(|w| {
            if w.widget_type() == "spacer" {
                1 // Spacers go last
            } else {
                0 // All other windows maintain order
            }
        });

        for window in &mut self.windows {
            // Convert None to Some("-") for color fields to preserve transparency setting
            let normalize = |field: &mut Option<String>| {
                if field.is_none() {
                    *field = Some("-".to_string());
                }
            };

            let base = window.base_mut();
            normalize(&mut base.background_color);
            normalize(&mut base.border_color);
            normalize(&mut base.text_color);
        }
    }

    pub fn save(
        &mut self,
        name: &str,
        terminal_size: Option<(u16, u16)>,
        force_terminal_size: bool,
    ) -> Result<()> {
        // Capture terminal size for layout baseline
        if force_terminal_size {
            // Force update terminal size (used by .resize to match resized widgets)
            if let Some((width, height)) = terminal_size {
                tracing::info!(
                    "Forcing layout terminal size to {}x{} (was {:?}x{:?})",
                    width,
                    height,
                    self.terminal_width,
                    self.terminal_height
                );
                self.terminal_width = Some(width);
                self.terminal_height = Some(height);
            }
        } else if self.terminal_width.is_none() || self.terminal_height.is_none() {
            // Only set if not already set
            if let Some((width, height)) = terminal_size {
                self.terminal_width = Some(width);
                self.terminal_height = Some(height);
                tracing::info!(
                    "Set layout terminal size to {}x{} (was not previously set)",
                    width,
                    height
                );
            }
        } else {
            tracing::debug!(
                "Preserving existing layout terminal size: {}x{} (not overwriting with current terminal size)",
                self.terminal_width.unwrap(), self.terminal_height.unwrap()
            );
        }

        // Normalize windows before saving (convert None colors to "-")
        self.normalize_windows_for_save();

        // Save to shared layouts directory: ~/.vellum-fe/default/layouts/{name}.toml
        let layouts_dir = Config::layouts_dir()?;
        fs::create_dir_all(&layouts_dir)?;

        let layout_path = layouts_dir.join(format!("{}.toml", name));
        let toml_string = toml::to_string_pretty(&self).context("Failed to serialize layout")?;
        fs::write(&layout_path, toml_string).context("Failed to write layout file")?;

        tracing::info!("Saved layout '{}' to {:?}", name, layout_path);
        Ok(())
    }

    /// Save as character auto-save layout (on exit/resize)
    /// Saves to: ~/.vellum-fe/{character}/layout.toml
    pub fn save_auto(
        &mut self,
        character: &str,
        base_layout_name: &str,
        terminal_size: Option<(u16, u16)>,
    ) -> Result<()> {
        // Set base_layout reference
        self.base_layout = Some(base_layout_name.to_string());

        // Always update terminal size for auto layouts
        if let Some((width, height)) = terminal_size {
            self.terminal_width = Some(width);
            self.terminal_height = Some(height);
        }

        // Normalize windows before saving (convert None colors to "-")
        self.normalize_windows_for_save();

        // Save to character profile: ~/.vellum-fe/{character}/layout.toml
        let profile_dir = Config::profile_dir(Some(character))?;
        fs::create_dir_all(&profile_dir)?;

        let layout_path = profile_dir.join("layout.toml");
        let toml_string =
            toml::to_string_pretty(&self).context("Failed to serialize auto layout")?;
        fs::write(&layout_path, toml_string).context("Failed to write auto layout file")?;

        tracing::info!(
            "Saved auto layout for {} to {:?} (base: {}, terminal: {:?}x{:?})",
            character,
            layout_path,
            base_layout_name,
            self.terminal_width,
            self.terminal_height
        );

        Ok(())
    }

    /// Validate layout and print results to stdout
    /// Returns Ok(()) if valid (with warnings OK), Err if fatal errors found
    pub fn validate_and_print(&self) -> Result<()> {
        println!("✓ Layout loaded successfully");
        println!("  {} windows defined", self.windows.len());

        // Basic validation checks
        let mut errors = 0;
        let mut warnings = 0;

        for window in &self.windows {
            let name = window.name();
            let base = window.base();

            // Check for zero dimensions
            if base.rows == 0 {
                eprintln!("✗ Error: Window '{}' has zero height", name);
                errors += 1;
            }
            if base.cols == 0 {
                eprintln!("✗ Error: Window '{}' has zero width", name);
                errors += 1;
            }

            // Check for empty names
            if name.is_empty() {
                eprintln!("✗ Error: Window has empty name");
                errors += 1;
            }

            // Warn about very small windows
            if base.rows == 1 && base.cols < 10 {
                eprintln!(
                    "⚠ Warning: Window '{}' is very small ({}x{})",
                    name, base.cols, base.rows
                );
                warnings += 1;
            }
        }

        // Summary
        if errors == 0 && warnings == 0 {
            println!("✓ Layout is valid with no issues");
        } else {
            if errors > 0 {
                eprintln!("\n✗ Found {} error(s)", errors);
            }
            if warnings > 0 {
                println!("⚠ Found {} warning(s)", warnings);
            }
        }

        if errors > 0 {
            anyhow::bail!("Layout validation failed with {} error(s)", errors);
        }

        Ok(())
    }

    /// Get a window from the layout by name
    pub fn get_window(&self, name: &str) -> Option<&WindowDef> {
        self.windows.iter().find(|w| w.name() == name)
    }

    /// Add a window to the layout (from template or make visible if exists)
    /// Generate a unique spacer widget name based on existing spacers in layout
    /// Uses max number + 1 algorithm, checking ALL widgets including hidden ones
    /// Pattern: spacer_1, spacer_2, spacer_3, etc.
    pub fn generate_spacer_name(&self) -> String {
        let max_number = self
            .windows
            .iter()
            .filter_map(|w| {
                // Only consider spacer widgets
                match w {
                    WindowDef::Spacer { base, .. } => {
                        // Extract number from name like "spacer_5"
                        if let Some(num_str) = base.name.strip_prefix("spacer_") {
                            num_str.parse::<u32>().ok()
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .max()
            .unwrap_or(0);

        format!("spacer_{}", max_number + 1)
    }

    /// Generate a unique widget name for any widget type
    /// Uses max number + 1 algorithm, checking ALL widgets with matching prefix
    /// Pattern: custom-{widgettype}-1, custom-{widgettype}-2, etc.
    /// Example: custom-tabbedtext-1, custom-text-2, custom-progress-1
    pub fn generate_widget_name(&self, widget_type: &str) -> String {
        // Normalize widget type: lowercase and strip _custom suffix
        // This ensures "tabbedtext_custom" → "custom-tabbedtext-1" (not "custom-tabbedtext_custom-1")
        let lowercase = widget_type.to_lowercase();
        let normalized_type = lowercase
            .strip_suffix("_custom")
            .unwrap_or(&lowercase);
        let prefix = format!("custom-{}-", normalized_type);

        let max_number = self
            .windows
            .iter()
            .filter_map(|w| {
                let name = w.name();
                // Extract number from name like "custom-text-5"
                if let Some(num_str) = name.strip_prefix(&prefix) {
                    num_str.parse::<u32>().ok()
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);

        format!("custom-{}-{}", normalized_type, max_number + 1)
    }

    pub fn add_window(&mut self, name: &str) -> Result<()> {
        // Check if window already exists in layout
        if let Some(existing) = self.windows.iter_mut().find(|w| w.name() == name) {
            // Just make it visible
            existing.base_mut().visible = true;
            tracing::info!("Window '{}' already exists, setting visible=true", name);
            return Ok(());
        }

        // Get template
        let mut window_def = Config::get_window_template(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown window template: {}", name))?;

        // Auto-generate unique name for templates with empty names
        // This includes spacers and custom widgets (tabbedtext_custom, text_custom, etc.)
        if window_def.base().name.is_empty() {
            let auto_name = if name == "spacer" {
                self.generate_spacer_name()
            } else {
                self.generate_widget_name(name)
            };
            window_def.base_mut().name = auto_name.clone();
            tracing::info!("Auto-generated window name: {} for template '{}'", auto_name, name);
        }

        // Set visible
        window_def.base_mut().visible = true;

        // Add to layout
        self.windows.push(window_def);
        tracing::info!("Added window '{}' from template", name);
        Ok(())
    }

    /// Hide a window (set visible = false)
    pub fn hide_window(&mut self, name: &str) -> Result<()> {
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.name() == name)
            .ok_or_else(|| anyhow::anyhow!("Window not found: {}", name))?;

        window.base_mut().visible = false;
        tracing::info!("Window '{}' hidden (visible=false)", name);
        Ok(())
    }

    /// Remove window from layout if it matches the default template
    /// (keeps layout file minimal by not saving unmodified windows)
    pub fn remove_window_if_default(&mut self, name: &str) {
        if let Some(template) = Config::get_window_template(name) {
            self.windows.retain(|w| {
                if w.name() == name {
                    // Compare window to template - if identical, remove (return false to filter out)
                    // If different, keep (return true)
                    w != &template
                } else {
                    true
                }
            });
        }
    }
}

impl Config {
    /// Find the appropriate layout for a given terminal size
    /// Returns the layout name if a matching mapping is found
    pub fn find_layout_for_size(&self, width: u16, height: u16) -> Option<String> {
        for mapping in &self.layout_mappings {
            if mapping.matches(width, height) {
                tracing::info!(
                    "Found layout mapping for {}x{}: '{}' (range: {}x{} to {}x{})",
                    width,
                    height,
                    mapping.layout,
                    mapping.min_width,
                    mapping.min_height,
                    mapping.max_width,
                    mapping.max_height
                );
                return Some(mapping.layout.clone());
            }
        }
        tracing::debug!(
            "No layout mapping found for terminal size {}x{}",
            width,
            height
        );
        None
    }
}
