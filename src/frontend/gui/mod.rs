//! GUI Frontend - Native GUI using egui
//!
//! This module provides an egui-based GUI frontend for VellumFE with
//! detachable windows, docking support, and per-character persistence.
//!
//! # Modules
//!
//! - `app` - GUI runtime: shell zones, docking, detached viewports, popup
//!   menus, link dispatch, keybind handling, widget rendering, layout save
//! - `tab_id` - Stable identity model for tabs (TabKey, TabId)
//! - `persistence` - Layout file schema and migration
//!
//! # Architecture
//!
//! The GUI is a native `eframe::App` driven by the egui event loop; it
//! deliberately does not implement the `Frontend` trait (that trait models a
//! frontend polled/rendered by an app-owned loop, which eframe inverts). The
//! shared contract with the TUI is `AppCore` + `UiState` + the config layer.
//!
//! See docs/GUI_AUDIT.md for the feature-parity roadmap.

pub mod app;
pub mod launcher;
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
    direct: Option<crate::network::DirectConnectConfig>,
    login_key: Option<String>,
}

impl EguiApp {
    pub fn new(
        app_core: AppCore,
        direct: Option<crate::network::DirectConnectConfig>,
        login_key: Option<String>,
    ) -> Self {
        Self {
            app_core,
            direct,
            login_key,
        }
    }

    pub fn run(self) -> Result<()> {
        app::run_native_gui(self.app_core, self.direct, self.login_key)
    }
}
