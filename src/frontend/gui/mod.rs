//! GUI Frontend - Native GUI using egui
//!
//! # ⚠️ WORK IN PROGRESS - NOT IMPLEMENTED
//!
//! This module is a stub for future egui-based GUI implementation.
//! The TUI frontend (ratatui) is the current stable interface.
//!
//! ## Roadmap
//! - [ ] Implement Frontend trait for egui
//! - [ ] Port widget renderers from TUI
//! - [ ] Add window management
//! - [ ] Implement input handling
//!
//! For now, use `--frontend tui` (default) or omit the flag entirely.

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
        eprintln!("\n⚠️  GUI Frontend Not Implemented");
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!("The egui GUI frontend is a work-in-progress stub.");
        eprintln!("Please use the TUI frontend instead (default):\n");
        eprintln!("  vellum-fe --frontend tui [options]");
        eprintln!("  or simply:");
        eprintln!("  vellum-fe [options]\n");
        Ok(())
    }
}
