//! GUI Frontend - Native GUI using egui
//!
//! This module provides an egui-based GUI frontend for VellumFE with
//! detachable windows, docking support, and per-character persistence.
//!
//! # Modules
//!
//! - `app` - Milestone 1 GUI runtime and egui application shell
//! - `tab_id` - Stable identity model for tabs (TabKey, TabId)
//! - `persistence` - Layout file schema and migration
//!
//! # Status
//!
//! - Milestone 0 (Contracts/Schema): Complete
//! - Milestone 1 (GUI skeleton): In progress

pub mod app;
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

/// GUI application launcher.
pub struct EguiApp {
    app_core: AppCore,
    login_key: Option<String>,
}

impl EguiApp {
    pub fn new(app_core: AppCore, login_key: Option<String>) -> Self {
        Self {
            app_core,
            login_key,
        }
    }

    pub fn run(self) -> Result<()> {
        app::run_native_gui(self.app_core, self.login_key)
    }
}
