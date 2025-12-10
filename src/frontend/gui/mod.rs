//! GUI Frontend - Native GUI using egui
//!
//! This module will implement the Frontend trait for native GUI rendering.
//! Currently a stub - will be implemented after TUI frontend is stable.

use crate::core::AppCore;
use anyhow::Result;

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
        eprintln!("GUI frontend not yet implemented. Use --frontend tui for now.");
        Ok(())
    }
}
