//! GUI Frontend - Native GUI using egui
//!
//! This module provides an egui-based GUI frontend for VellumFE with
//! detachable windows, docking support, and per-character persistence.
//!
//! # Modules
//!
//! - `tab_id` - Stable identity model for tabs (TabKey, TabId)
//! - `persistence` - Layout file schema and migration
//!
//! # Status
//!
//! - Milestone 0 (Contracts/Schema): In progress
//! - Milestone 1+ (Runtime): Not yet implemented
//!
//! For now, use `--frontend tui` (default) or omit the flag entirely.

pub mod persistence;
pub mod tab_id;

// Re-exports for convenience
pub use persistence::{
    CopyBehavior, FontRef, GuiLayoutFileV1, LayoutError, TabSettings, ViewportState,
    CURRENT_SCHEMA_VERSION,
};
pub use tab_id::{TabId, TabKey};

use crate::core::AppCore;
use anyhow::Result;

/// Placeholder GUI application struct.
///
/// This will be replaced with the full VellumGuiApp in Milestone 1.
pub struct EguiApp {
    _app_core: AppCore,
}

impl EguiApp {
    pub fn new(app_core: AppCore) -> Self {
        Self {
            _app_core: app_core,
        }
    }

    pub fn run(self) -> Result<()> {
        eprintln!("\n⚠️  GUI Frontend Not Yet Complete");
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!("The egui GUI frontend is under development.");
        eprintln!("Milestone 0 (Contracts) is complete - types and schemas defined.");
        eprintln!("Please use the TUI frontend for now:\n");
        eprintln!("  vellum-fe --frontend tui [options]");
        eprintln!("  or simply:");
        eprintln!("  vellum-fe [options]\n");
        Ok(())
    }
}
