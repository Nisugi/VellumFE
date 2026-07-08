//! GUI layout persistence for per-character window state.
//!
//! ## File Locations
//!
//! Per-character GUI state:
//! ```text
//! ~/.vellum-fe/gui/<profile>/<character>/layout_v1.json
//! ```
//!
//! Backup (created before save):
//! ```text
//! ~/.vellum-fe/gui/<profile>/<character>/layout_v1.bak.json
//! ```
//!
//! ## Schema Versioning
//!
//! Layout files are versioned via `schema_version` field. The migration system
//! allows loading older versions and upgrading to current.

use super::tab_id::TabKey;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Current schema version. Increment when making breaking changes.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Reference to a font by name or system default.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FontRef {
    /// Use system default font
    #[default]
    SystemDefault,
    /// Use a named font from the font configuration
    Named(String),
    /// Use a custom font file path
    Custom(String),
}

/// Text copy behavior options.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CopyBehavior {
    /// Copy plain text only
    #[default]
    PlainText,
    /// Copy with ANSI escape codes preserved
    AnsiCodes,
    /// Copy as HTML with styling
    Html,
}

/// Per-tab settings stored separately from dock layout.
///
/// Keyed by `TabKey` to survive tab renames.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabSettings {
    /// Primary font for regular text
    #[serde(default)]
    pub font_primary: FontRef,

    /// Secondary font for monospace content
    #[serde(default)]
    pub font_secondary: FontRef,

    /// Text size override for this window; None uses the global text size
    #[serde(default)]
    pub text_size: Option<f32>,

    /// Accent color for this window's border, as "#rrggbb"; None uses the
    /// theme's window border color
    #[serde(default)]
    pub accent_color: Option<String>,

    /// Whether to wrap text at window boundary
    #[serde(default = "default_wrap_text")]
    pub wrap_text: bool,

    /// How to copy text to clipboard
    #[serde(default)]
    pub copy_behavior: CopyBehavior,
}

fn default_wrap_text() -> bool {
    true
}

impl Default for TabSettings {
    fn default() -> Self {
        Self {
            font_primary: FontRef::SystemDefault,
            font_secondary: FontRef::SystemDefault,
            text_size: None,
            accent_color: None,
            wrap_text: true,
            copy_behavior: CopyBehavior::PlainText,
        }
    }
}

/// One of the bars the vitals window can display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VitalKind {
    Health,
    Mana,
    Stamina,
    Spirit,
    /// Mind state (GS4 experience absorption)
    Mind,
    Encumbrance,
    /// Progress toward next level (GS4)
    NextLevel,
    /// Blood points (GS4 Betrayer)
    Blood,
}

impl VitalKind {
    pub fn all() -> [VitalKind; 8] {
        [
            VitalKind::Health,
            VitalKind::Mana,
            VitalKind::Stamina,
            VitalKind::Spirit,
            VitalKind::Mind,
            VitalKind::Encumbrance,
            VitalKind::NextLevel,
            VitalKind::Blood,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            VitalKind::Health => "Health",
            VitalKind::Mana => "Mana",
            VitalKind::Stamina => "Stamina",
            VitalKind::Spirit => "Spirit",
            VitalKind::Mind => "Mind",
            VitalKind::Encumbrance => "Encumbrance",
            VitalKind::NextLevel => "Next Level",
            VitalKind::Blood => "Blood",
        }
    }
}

/// How the vitals window arranges its bars.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VitalsOrientation {
    /// All bars in one row (Wrayth-style)
    #[default]
    Horizontal,
    /// Bars stacked top to bottom
    Vertical,
}

/// Text drawn on each vitals bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VitalsTextFormat {
    /// "Health: 191/193"
    #[default]
    LabelValueMax,
    /// "Health: 99%"
    LabelPercent,
    /// "191/193"
    ValueMax,
    /// "99%"
    Percent,
    /// Bar only, no text
    None,
}

/// Vitals window configuration: which bars, their order, and how they render.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VitalsConfig {
    #[serde(default)]
    pub orientation: VitalsOrientation,

    /// Height of one vitals bar, in points
    #[serde(default = "default_vitals_bar_height")]
    pub bar_height: f32,

    #[serde(default)]
    pub text_format: VitalsTextFormat,

    /// Enabled bars, in display order
    #[serde(default = "default_vital_bars")]
    pub bars: Vec<VitalKind>,
}

fn default_vitals_bar_height() -> f32 {
    18.0
}

fn default_vital_bars() -> Vec<VitalKind> {
    vec![
        VitalKind::Health,
        VitalKind::Mana,
        VitalKind::Stamina,
        VitalKind::Spirit,
    ]
}

impl Default for VitalsConfig {
    fn default() -> Self {
        Self {
            orientation: VitalsOrientation::default(),
            bar_height: default_vitals_bar_height(),
            text_format: VitalsTextFormat::default(),
            bars: default_vital_bars(),
        }
    }
}

/// A group of windows locked together and rendered as one window.
///
/// The first member is the leader: the group renders in the leader's slot
/// and zone. Members split the content area along `orientation`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabGroup {
    pub members: Vec<TabKey>,
    /// true = members side by side; false = stacked vertically
    #[serde(default)]
    pub horizontal: bool,
}

/// Application-wide GUI sizing/accessibility settings.
///
/// Defaults approximate Wrayth's compact look; every value is user-adjustable
/// (Settings → GUI) because players range from dense-layout veterans to
/// low-vision users who need everything larger.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuiUiSettings {
    /// Global UI zoom (egui zoom_factor). Also driven by Ctrl+= / Ctrl+- / Ctrl+0.
    #[serde(default = "default_zoom_factor")]
    pub zoom_factor: f32,

    /// Default text size for window content, in points.
    #[serde(default = "default_text_size")]
    pub text_size: f32,

    /// Title bar text size, in points; title bar height follows it.
    #[serde(default = "default_title_font_size")]
    pub title_font_size: f32,

    /// Height of one active-effect bar row, in points.
    #[serde(default = "default_effects_bar_height")]
    pub effects_bar_height: f32,

    /// Spacing/padding scale: 1.0 = egui defaults, lower = denser
    /// (Wrayth-like), higher = more comfortable.
    #[serde(default = "default_density")]
    pub density: f32,

    /// Corner radius for all progress bars (vitals, effects, experience,
    /// encumbrance, ...). 0 = square Wrayth-style corners.
    #[serde(default = "default_bar_corner_radius")]
    pub bar_corner_radius: f32,

    /// Automatically switch bar text between light and dark when the
    /// configured color would be unreadable against the bar fill.
    #[serde(default = "default_true")]
    pub auto_contrast_bar_text: bool,

    /// Vitals window layout and bar selection.
    #[serde(default)]
    pub vitals: VitalsConfig,
}

fn default_zoom_factor() -> f32 {
    1.0
}

fn default_text_size() -> f32 {
    14.0
}

fn default_title_font_size() -> f32 {
    13.0
}

fn default_effects_bar_height() -> f32 {
    18.0
}

fn default_density() -> f32 {
    0.8
}

fn default_bar_corner_radius() -> f32 {
    2.0
}

fn default_true() -> bool {
    true
}

impl Default for GuiUiSettings {
    fn default() -> Self {
        Self {
            zoom_factor: default_zoom_factor(),
            text_size: default_text_size(),
            title_font_size: default_title_font_size(),
            effects_bar_height: default_effects_bar_height(),
            density: default_density(),
            bar_corner_radius: default_bar_corner_radius(),
            auto_contrast_bar_text: default_true(),
            vitals: VitalsConfig::default(),
        }
    }
}

/// State of a detached (floating) viewport/window.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewportState {
    /// Which tab is in this viewport
    pub tab: TabKey,

    /// Outer window position in pixels [x, y]
    pub outer_pos_px: [f32; 2],

    /// Outer window size in pixels [width, height]
    pub outer_size_px: [f32; 2],

    /// Platform-dependent monitor identifier for restoration
    #[serde(default)]
    pub monitor_hint: Option<String>,

    /// DPI scale hint for this monitor
    #[serde(default)]
    pub scale_hint: Option<f32>,

    /// Whether window was maximized
    #[serde(default)]
    pub maximized: bool,
}

impl ViewportState {
    /// Create a new viewport state for a tab.
    pub fn new(tab: TabKey, pos: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            tab,
            outer_pos_px: pos,
            outer_size_px: size,
            monitor_hint: None,
            scale_hint: None,
            maximized: false,
        }
    }

    /// Clamp the viewport to be within visible bounds.
    ///
    /// If the window would be off-screen, move it to be visible with
    /// at least `min_visible_px` pixels showing on the target monitor.
    pub fn clamp_to_bounds(&mut self, monitor_rect: [f32; 4], min_visible_px: f32) {
        let [mx, my, mw, mh] = monitor_rect;
        let [mut x, mut y] = self.outer_pos_px;
        let [w, h] = self.outer_size_px;

        // Ensure at least min_visible_px of the window is visible
        x = x.max(mx - w + min_visible_px).min(mx + mw - min_visible_px);
        y = y.max(my - h + min_visible_px).min(my + mh - min_visible_px);

        self.outer_pos_px = [x, y];
    }
}

/// Saved geometry of the main OS window, in logical points.
///
/// Restored at launch so per-window rects (saved against this geometry)
/// are not clamped into a smaller default viewport on the first frames.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MainViewportState {
    /// Outer window position [x, y]; None lets the OS place the window
    #[serde(default)]
    pub outer_pos: Option<[f32; 2]>,

    /// Inner (client area) size [width, height]
    pub inner_size: [f32; 2],

    /// Whether the window was maximized
    #[serde(default)]
    pub maximized: bool,
}

/// Per-tab settings entry for serialization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabSettingsEntry {
    pub key: TabKey,
    pub settings: TabSettings,
}

/// Version 1 of the GUI layout file schema.
///
/// This is persisted per-character at:
/// `~/.vellum-fe/gui/<profile>/<character>/layout_v1.json`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuiLayoutFileV1 {
    /// Schema version (always 1 for this struct)
    pub schema_version: u32,

    /// Character identifier (for validation)
    pub character_id: String,

    /// Profile identifier (for validation)
    pub profile_id: String,

    /// When this layout was saved (RFC3339 format)
    pub saved_at_utc: String,

    /// Serialized `DockStateSnapshot` (visible tabs, window rects, zones,
    /// title-bar flags, shell layout) as a JSON value.
    pub dock_state_json: serde_json::Value,

    /// Tabs that are hidden (not displayed but not destroyed)
    #[serde(default)]
    pub hidden_tabs: Vec<TabKey>,

    /// Per-tab settings as a list (JSON doesn't support complex keys)
    #[serde(default)]
    pub tab_settings: Vec<TabSettingsEntry>,

    /// Application-wide UI font. `custom` takes a path to a .ttf/.otf file
    /// loaded at startup; `system_default` keeps egui's built-in fonts.
    #[serde(default)]
    pub ui_font: FontRef,

    /// Application-wide sizing/accessibility settings (zoom, text sizes).
    #[serde(default)]
    pub ui_settings: GuiUiSettings,

    /// Detached viewport state keyed by viewport ID string
    #[serde(default)]
    pub detached_viewports: HashMap<String, ViewportState>,

    /// Main OS window geometry, restored at launch
    #[serde(default)]
    pub main_viewport: Option<MainViewportState>,
}

impl GuiLayoutFileV1 {
    /// Create a new empty layout for a character.
    pub fn new(profile_id: impl Into<String>, character_id: impl Into<String>) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            character_id: character_id.into(),
            profile_id: profile_id.into(),
            saved_at_utc: chrono::Utc::now().to_rfc3339(),
            dock_state_json: serde_json::Value::Null,
            hidden_tabs: Vec::new(),
            tab_settings: Vec::new(),
            ui_font: FontRef::default(),
            ui_settings: GuiUiSettings::default(),
            detached_viewports: HashMap::new(),
            main_viewport: None,
        }
    }

    /// Update the saved timestamp to now.
    pub fn touch(&mut self) {
        self.saved_at_utc = chrono::Utc::now().to_rfc3339();
    }

    /// Validate that this layout matches the expected character/profile.
    pub fn validate(&self, profile_id: &str, character_id: &str) -> Result<()> {
        if self.profile_id != profile_id {
            anyhow::bail!(
                "Layout profile mismatch: expected '{}', got '{}'",
                profile_id,
                self.profile_id
            );
        }
        if self.character_id != character_id {
            anyhow::bail!(
                "Layout character mismatch: expected '{}', got '{}'",
                character_id,
                self.character_id
            );
        }
        Ok(())
    }

    /// Get settings for a tab.
    pub fn get_tab_settings(&self, key: &TabKey) -> Option<&TabSettings> {
        self.tab_settings
            .iter()
            .rev()
            .find(|e| &e.key == key)
            .map(|e| &e.settings)
    }

    /// Set settings for a tab.
    pub fn set_tab_settings(&mut self, key: TabKey, settings: TabSettings) {
        // Remove existing entry if present
        self.tab_settings.retain(|e| e.key != key);
        self.tab_settings.push(TabSettingsEntry { key, settings });
    }

    /// Convert tab_settings to a HashMap for easier runtime access.
    pub fn tab_settings_map(&self) -> HashMap<TabKey, TabSettings> {
        self.tab_settings
            .iter()
            .map(|e| (e.key.clone(), e.settings.clone()))
            .collect()
    }
}

/// Envelope for loading layout files with unknown versions.
///
/// First deserialize to this to check schema_version, then migrate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutEnvelope {
    /// Schema version (determines which struct to deserialize as)
    pub schema_version: u32,

    /// All other fields as raw JSON for migration
    #[serde(flatten)]
    pub data: serde_json::Value,
}

/// Error type for layout loading/migration.
#[derive(Debug)]
pub enum LayoutError {
    UnknownVersion(u32),
    FutureVersion(u32),
    ParseError(serde_json::Error),
    IoError(std::io::Error),
    MigrationFailed { from: u32, to: u32, reason: String },
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::UnknownVersion(v) => {
                write!(
                    f,
                    "Unknown schema version {} (current is {})",
                    v, CURRENT_SCHEMA_VERSION
                )
            }
            LayoutError::FutureVersion(v) => {
                write!(
                    f,
                    "Future schema version {} (current is {}) - please upgrade VellumFE",
                    v, CURRENT_SCHEMA_VERSION
                )
            }
            LayoutError::ParseError(e) => write!(f, "Failed to parse layout file: {}", e),
            LayoutError::IoError(e) => write!(f, "IO error: {}", e),
            LayoutError::MigrationFailed { from, to, reason } => {
                write!(
                    f,
                    "Migration failed from version {} to {}: {}",
                    from, to, reason
                )
            }
        }
    }
}

impl std::error::Error for LayoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LayoutError::ParseError(e) => Some(e),
            LayoutError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for LayoutError {
    fn from(e: serde_json::Error) -> Self {
        LayoutError::ParseError(e)
    }
}

impl From<std::io::Error> for LayoutError {
    fn from(e: std::io::Error) -> Self {
        LayoutError::IoError(e)
    }
}

/// Migrate a layout from any known version to current.
///
/// Returns the migrated layout or an error if migration is not possible.
pub fn migrate_layout(envelope: LayoutEnvelope) -> Result<GuiLayoutFileV1, LayoutError> {
    match envelope.schema_version {
        1 => {
            // Current version - reconstruct full JSON with schema_version included
            // (serde flatten extracts schema_version separately from data)
            let mut full_data = envelope.data;
            if let serde_json::Value::Object(ref mut map) = full_data {
                map.insert(
                    "schema_version".to_string(),
                    serde_json::Value::Number(envelope.schema_version.into()),
                );
            }
            let layout: GuiLayoutFileV1 = serde_json::from_value(full_data)?;
            Ok(layout)
        }
        v if v > CURRENT_SCHEMA_VERSION => Err(LayoutError::FutureVersion(v)),
        v => Err(LayoutError::UnknownVersion(v)),
    }
}

/// Get the path to the GUI layout directory for a character.
pub fn layout_dir(profile: &str, character: &str) -> Result<PathBuf> {
    let base = crate::config::Config::base_dir()?;
    Ok(base.join("gui").join(profile).join(character))
}

/// Get the path to the layout file for a character.
pub fn layout_path(profile: &str, character: &str) -> Result<PathBuf> {
    Ok(layout_dir(profile, character)?.join("layout_v1.json"))
}

/// Get the path to the backup layout file for a character.
pub fn backup_path(profile: &str, character: &str) -> Result<PathBuf> {
    Ok(layout_dir(profile, character)?.join("layout_v1.bak.json"))
}

/// Load a layout file for a character.
///
/// Strategy:
/// 1. Try to load the main file
/// 2. If that fails, try the backup
/// 3. Migrate if needed
/// 4. Validate character/profile match
pub fn load_layout(profile: &str, character: &str) -> Result<GuiLayoutFileV1> {
    let path = layout_path(profile, character)?;
    let backup = backup_path(profile, character)?;

    // Try main file first
    let result = load_from_path(&path);

    let layout = match result {
        Ok(layout) => layout,
        Err(e) => {
            // Log warning and try backup
            tracing::warn!("Failed to load layout from {:?}: {}", path, e);

            if backup.exists() {
                tracing::info!("Trying backup layout file");
                load_from_path(&backup).context("Failed to load backup layout")?
            } else {
                return Err(e);
            }
        }
    };

    // Validate matches expected character/profile
    layout.validate(profile, character)?;

    Ok(layout)
}

/// Load and migrate a layout from a specific path.
fn load_from_path(path: &PathBuf) -> Result<GuiLayoutFileV1> {
    let content = std::fs::read_to_string(path).context("Failed to read layout file")?;

    let envelope: LayoutEnvelope =
        serde_json::from_str(&content).context("Failed to parse layout envelope")?;

    let layout = migrate_layout(envelope).context("Failed to migrate layout")?;

    Ok(layout)
}

/// Save a layout file for a character.
///
/// Strategy:
/// 1. Create backup of existing file
/// 2. Write to temp file
/// 3. Atomic rename to final path
pub fn save_layout(layout: &GuiLayoutFileV1, profile: &str, character: &str) -> Result<()> {
    let path = layout_path(profile, character)?;
    let backup = backup_path(profile, character)?;
    let dir = layout_dir(profile, character)?;

    // Ensure directory exists
    std::fs::create_dir_all(&dir).context("Failed to create layout directory")?;

    // Create backup of existing file
    if path.exists() {
        std::fs::copy(&path, &backup).context("Failed to create backup")?;
    }

    write_layout_atomically(layout, &dir, "layout_v1.tmp.json", &path)?;

    tracing::debug!("Saved layout to {:?}", path);
    Ok(())
}

/// Serialize and write via temp file + rename (atomic on most filesystems).
fn write_layout_atomically(
    layout: &GuiLayoutFileV1,
    dir: &std::path::Path,
    temp_name: &str,
    path: &PathBuf,
) -> Result<()> {
    let content = serde_json::to_string_pretty(layout).context("Failed to serialize layout")?;
    let temp_path = dir.join(temp_name);
    std::fs::write(&temp_path, &content).context("Failed to write temp layout file")?;
    if let Err(rename_err) = std::fs::rename(&temp_path, path) {
        // Windows does not allow renaming over an existing file.
        // If replacement is needed, remove existing destination and retry.
        if path.exists() {
            std::fs::remove_file(path)
                .context("Failed to remove existing layout file before rename")?;
            std::fs::rename(&temp_path, path)
                .context("Failed to rename temp to final after replacing existing file")?;
        } else {
            return Err(rename_err).context("Failed to rename temp to final");
        }
    }
    Ok(())
}

// ---- Named layout checkpoints ----------------------------------------------
//
// `.savelayout <name>` / `.loadlayout <name>` in the GUI. These are explicit
// checkpoints, deliberately separate from the auto-saved live slot
// (`layout_v1.json`): loading one replaces the live arrangement, and the
// autosave keeps writing the live slot afterward — fiddling never rewrites
// a checkpoint.

/// Directory holding a character's named layout checkpoints.
pub fn named_layouts_dir(profile: &str, character: &str) -> Result<PathBuf> {
    Ok(layout_dir(profile, character)?.join("layouts"))
}

/// True when a checkpoint name is safe to use as a file stem (also blocks
/// path traversal, since names become `<name>.json`).
pub fn is_valid_layout_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Save a snapshot as a named checkpoint.
pub fn save_named_layout(
    layout: &GuiLayoutFileV1,
    profile: &str,
    character: &str,
    name: &str,
) -> Result<()> {
    if !is_valid_layout_name(name) {
        anyhow::bail!("Layout names use letters, digits, '-' and '_' only");
    }
    let dir = named_layouts_dir(profile, character)?;
    std::fs::create_dir_all(&dir).context("Failed to create named layouts directory")?;
    let path = dir.join(format!("{name}.json"));
    write_layout_atomically(layout, &dir, &format!("{name}.tmp.json"), &path)?;
    tracing::info!("Saved named GUI layout to {:?}", path);
    Ok(())
}

/// Load a named checkpoint (with schema migration).
///
/// Unlike the live slot, the profile/character stamp is not validated:
/// a checkpoint copied from another character loads fine — tabs that don't
/// exist in this session are dropped during reconciliation.
pub fn load_named_layout(profile: &str, character: &str, name: &str) -> Result<GuiLayoutFileV1> {
    if !is_valid_layout_name(name) {
        anyhow::bail!("Layout names use letters, digits, '-' and '_' only");
    }
    let path = named_layouts_dir(profile, character)?.join(format!("{name}.json"));
    if !path.exists() {
        anyhow::bail!("No saved layout named '{name}'");
    }
    load_from_path(&path)
}

/// List a character's named checkpoints, sorted.
pub fn list_named_layouts(profile: &str, character: &str) -> Vec<String> {
    let Ok(dir) = named_layouts_dir(profile, character) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "json" {
                return None;
            }
            let stem = path.file_stem()?.to_str()?;
            is_valid_layout_name(stem).then(|| stem.to_string())
        })
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_name_validation() {
        assert!(is_valid_layout_name("combat"));
        assert!(is_valid_layout_name("town-square_2"));
        assert!(!is_valid_layout_name(""));
        assert!(!is_valid_layout_name("my layout"));
        assert!(!is_valid_layout_name("../escape"));
        assert!(!is_valid_layout_name("a".repeat(65).as_str()));
    }

    #[test]
    fn test_font_ref_serialization() {
        let default = FontRef::SystemDefault;
        let json = serde_json::to_string(&default).unwrap();
        assert_eq!(json, r#""system_default""#);

        let named = FontRef::Named("Consolas".to_string());
        let json = serde_json::to_string(&named).unwrap();
        assert!(json.contains("named"));
        assert!(json.contains("Consolas"));

        // Round-trip
        let parsed: FontRef = serde_json::from_str(&json).unwrap();
        match parsed {
            FontRef::Named(name) => assert_eq!(name, "Consolas"),
            _ => panic!("Expected Named variant"),
        }
    }

    #[test]
    fn test_copy_behavior_serialization() {
        let behaviors = vec![
            (CopyBehavior::PlainText, "plain_text"),
            (CopyBehavior::AnsiCodes, "ansi_codes"),
            (CopyBehavior::Html, "html"),
        ];

        for (behavior, expected) in behaviors {
            let json = serde_json::to_string(&behavior).unwrap();
            assert!(json.contains(expected), "Expected {} in {}", expected, json);

            let parsed: CopyBehavior = serde_json::from_str(&json).unwrap();
            assert_eq!(
                std::mem::discriminant(&behavior),
                std::mem::discriminant(&parsed)
            );
        }
    }

    #[test]
    fn test_tab_settings_default() {
        let settings = TabSettings::default();
        assert!(settings.wrap_text);
        assert!(matches!(settings.font_primary, FontRef::SystemDefault));
        assert!(matches!(settings.copy_behavior, CopyBehavior::PlainText));
    }

    #[test]
    fn test_tab_settings_serialization() {
        let settings = TabSettings {
            font_primary: FontRef::Named("JetBrains Mono".to_string()),
            font_secondary: FontRef::SystemDefault,
            text_size: None,
            accent_color: None,
            wrap_text: false,
            copy_behavior: CopyBehavior::Html,
        };

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: TabSettings = serde_json::from_str(&json).unwrap();

        assert!(!parsed.wrap_text);
        match parsed.font_primary {
            FontRef::Named(name) => assert_eq!(name, "JetBrains Mono"),
            _ => panic!("Expected Named font"),
        }
    }

    #[test]
    fn test_viewport_state_serialization() {
        let state = ViewportState::new(TabKey::Vitals, [100.0, 200.0], [400.0, 300.0]);

        let json = serde_json::to_string(&state).unwrap();
        let parsed: ViewportState = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tab, TabKey::Vitals);
        assert_eq!(parsed.outer_pos_px, [100.0, 200.0]);
        assert_eq!(parsed.outer_size_px, [400.0, 300.0]);
        assert!(!parsed.maximized);
    }

    #[test]
    fn test_viewport_clamp_to_bounds() {
        let mut state = ViewportState::new(TabKey::Vitals, [-100.0, -100.0], [200.0, 150.0]);

        // Monitor at [0, 0] with size [1920, 1080]
        state.clamp_to_bounds([0.0, 0.0, 1920.0, 1080.0], 50.0);

        // Should be clamped to show at least 50px on screen
        assert!(state.outer_pos_px[0] >= -150.0); // width - min_visible
        assert!(state.outer_pos_px[1] >= -100.0); // height - min_visible
    }

    #[test]
    fn test_gui_layout_file_v1_new() {
        let layout = GuiLayoutFileV1::new("default", "Testchar");

        assert_eq!(layout.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(layout.profile_id, "default");
        assert_eq!(layout.character_id, "Testchar");
        assert!(layout.hidden_tabs.is_empty());
        assert!(layout.tab_settings.is_empty());
        assert!(layout.detached_viewports.is_empty());
    }

    #[test]
    fn test_gui_layout_file_v1_round_trip() {
        let mut layout = GuiLayoutFileV1::new("prime", "Guildenstern");
        layout.hidden_tabs.push(TabKey::Compass);
        layout.set_tab_settings(
            TabKey::Vitals,
            TabSettings {
                wrap_text: false,
                ..Default::default()
            },
        );
        layout.detached_viewports.insert(
            "vp_1".to_string(),
            ViewportState::new(TabKey::Room, [500.0, 100.0], [300.0, 200.0]),
        );

        // Serialize
        let json = serde_json::to_string_pretty(&layout).unwrap();

        // Deserialize
        let parsed: GuiLayoutFileV1 = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.profile_id, "prime");
        assert_eq!(parsed.character_id, "Guildenstern");
        assert_eq!(parsed.hidden_tabs.len(), 1);
        assert_eq!(parsed.hidden_tabs[0], TabKey::Compass);
        assert!(parsed.get_tab_settings(&TabKey::Vitals).is_some());
        assert!(parsed.detached_viewports.contains_key("vp_1"));
    }

    #[test]
    fn test_get_tab_settings_prefers_latest_duplicate() {
        let mut layout = GuiLayoutFileV1::new("prime", "Guildenstern");
        layout.tab_settings.push(TabSettingsEntry {
            key: TabKey::Vitals,
            settings: TabSettings {
                wrap_text: true,
                ..Default::default()
            },
        });
        layout.tab_settings.push(TabSettingsEntry {
            key: TabKey::Vitals,
            settings: TabSettings {
                wrap_text: false,
                ..Default::default()
            },
        });

        // Latest duplicate should win.
        let settings = layout
            .get_tab_settings(&TabKey::Vitals)
            .expect("vitals settings should exist");
        assert!(!settings.wrap_text);

        // HashMap conversion should match get_tab_settings semantics.
        let map = layout.tab_settings_map();
        let mapped = map
            .get(&TabKey::Vitals)
            .expect("vitals map entry should exist");
        assert!(!mapped.wrap_text);
    }

    #[test]
    fn test_gui_layout_file_v1_validate() {
        let layout = GuiLayoutFileV1::new("prime", "Guildenstern");

        // Should pass with matching IDs
        assert!(layout.validate("prime", "Guildenstern").is_ok());

        // Should fail with wrong profile
        assert!(layout.validate("test", "Guildenstern").is_err());

        // Should fail with wrong character
        assert!(layout.validate("prime", "OtherChar").is_err());
    }

    #[test]
    fn test_layout_envelope_parse() {
        let json = r#"{
            "schema_version": 1,
            "character_id": "Test",
            "profile_id": "default",
            "saved_at_utc": "2024-01-01T00:00:00Z",
            "dock_state_json": null,
            "hidden_tabs": [],
            "tab_settings": [],
            "detached_viewports": {}
        }"#;

        let envelope: LayoutEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.schema_version, 1);

        let layout = migrate_layout(envelope).unwrap();
        assert_eq!(layout.character_id, "Test");
    }

    #[test]
    fn test_migrate_layout_current_version() {
        let json = r#"{
            "schema_version": 1,
            "character_id": "Test",
            "profile_id": "default",
            "saved_at_utc": "2024-01-01T00:00:00Z",
            "dock_state_json": null
        }"#;

        let envelope: LayoutEnvelope = serde_json::from_str(json).unwrap();
        let result = migrate_layout(envelope);
        assert!(result.is_ok());
    }

    #[test]
    fn test_migrate_layout_future_version() {
        let json = r#"{
            "schema_version": 999,
            "character_id": "Test",
            "profile_id": "default",
            "saved_at_utc": "2024-01-01T00:00:00Z",
            "dock_state_json": null
        }"#;

        let envelope: LayoutEnvelope = serde_json::from_str(json).unwrap();
        let result = migrate_layout(envelope);

        match result {
            Err(LayoutError::FutureVersion(v)) => assert_eq!(v, 999),
            _ => panic!("Expected FutureVersion error"),
        }
    }

    #[test]
    fn test_migrate_layout_unknown_version() {
        let envelope = LayoutEnvelope {
            schema_version: 0,
            data: serde_json::json!({}),
        };

        let result = migrate_layout(envelope);
        match result {
            Err(LayoutError::UnknownVersion(v)) => assert_eq!(v, 0),
            _ => panic!("Expected UnknownVersion error"),
        }
    }

    #[test]
    fn test_complex_layout_round_trip() {
        // Create a complex layout with all fields populated
        let mut layout = GuiLayoutFileV1::new("prime", "ComplexChar");

        // Add hidden tabs
        layout.hidden_tabs = vec![
            TabKey::Compass,
            TabKey::Perception,
            TabKey::TextByName {
                id: "combat".to_string(),
            },
        ];

        // Add tab settings for multiple tabs
        layout.set_tab_settings(
            TabKey::TextMain,
            TabSettings {
                font_primary: FontRef::Named("Fira Code".to_string()),
                font_secondary: FontRef::Named("Consolas".to_string()),
                text_size: Some(16.0),
                accent_color: Some("#4784d9".to_string()),
                wrap_text: true,
                copy_behavior: CopyBehavior::AnsiCodes,
            },
        );
        layout.set_tab_settings(
            TabKey::Quickbar {
                id: "1".to_string(),
            },
            TabSettings::default(),
        );

        // Add detached viewports
        layout.detached_viewports.insert(
            "viewport_1".to_string(),
            ViewportState {
                tab: TabKey::Vitals,
                outer_pos_px: [1920.0, 100.0],
                outer_size_px: [400.0, 300.0],
                monitor_hint: Some("\\\\?\\DISPLAY#DELL#1".to_string()),
                scale_hint: Some(1.25),
                maximized: false,
            },
        );
        layout.detached_viewports.insert(
            "viewport_2".to_string(),
            ViewportState {
                tab: TabKey::Room,
                outer_pos_px: [0.0, 0.0],
                outer_size_px: [800.0, 600.0],
                monitor_hint: None,
                scale_hint: None,
                maximized: true,
            },
        );

        // Add dock state (opaque JSON)
        layout.dock_state_json = serde_json::json!({
            "tree": {
                "root": { "tabs": ["main", "vitals"] }
            }
        });

        // Serialize and deserialize
        let json = serde_json::to_string_pretty(&layout).unwrap();
        let parsed: GuiLayoutFileV1 = serde_json::from_str(&json).unwrap();

        // Verify all fields
        assert_eq!(parsed.hidden_tabs.len(), 3);
        assert_eq!(parsed.tab_settings.len(), 2);
        assert_eq!(parsed.detached_viewports.len(), 2);
        assert!(!parsed.dock_state_json.is_null());

        // Verify specific values
        let vitals_viewport = parsed.detached_viewports.get("viewport_1").unwrap();
        assert_eq!(vitals_viewport.tab, TabKey::Vitals);
        assert_eq!(vitals_viewport.scale_hint, Some(1.25));

        let main_settings = parsed.get_tab_settings(&TabKey::TextMain).unwrap();
        match &main_settings.font_primary {
            FontRef::Named(name) => assert_eq!(name, "Fira Code"),
            _ => panic!("Expected Named font"),
        }
    }
}
