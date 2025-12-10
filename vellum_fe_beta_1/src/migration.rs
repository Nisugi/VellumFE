use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use chrono::Local;

/// Migration system for transitioning from old config structure to new organized structure
///
/// Old structure:
/// ~/.vellum-fe/
/// ├── cmdlist1.xml
/// ├── common/highlights.toml
/// ├── sounds/
/// ├── <character>/...
/// ├── layouts/
/// ├── highlights/
/// └── keybinds/
///
/// New structure:
/// ~/.vellum-fe/
/// ├── global/
/// │   ├── cmdlist1.xml
/// │   ├── highlights.toml
/// │   └── sounds/
/// ├── profiles/<character>/...
/// ├── layouts/
/// └── presets/
///     ├── highlights/
///     └── keybinds/
pub struct Migration {
    config_dir: PathBuf,
    backup_dir: PathBuf,
    log: Vec<String>,
}

impl Migration {
    /// Check if migration is needed
    pub fn needs_migration(config_dir: &Path) -> Result<bool> {
        let migrated_marker = config_dir.join(".migrated");

        if migrated_marker.exists() {
            return Ok(false); // Already migrated
        }

        // Check for indicators of old structure
        let has_cmdlist_at_root = config_dir.join("cmdlist1.xml").exists();
        let has_old_common = config_dir.join("common").exists();
        let has_sounds_at_root = config_dir.join("sounds").exists();

        // Check if new structure already exists (partial manual migration or conflict)
        let has_global = config_dir.join("global").exists();
        let has_profiles = config_dir.join("profiles").exists();

        // If both old and new exist, it's a conflict - skip migration
        if (has_cmdlist_at_root || has_old_common || has_sounds_at_root) && (has_global || has_profiles) {
            println!("⚠ Warning: Both old and new config structures detected!");
            println!("  Skipping automatic migration to avoid conflicts.");
            println!("  Please manually organize your ~/.vellum-fe/ directory.");
            return Ok(false);
        }

        Ok(has_cmdlist_at_root || has_old_common || has_sounds_at_root)
    }

    /// Run the migration process
    pub fn run(config_dir: PathBuf) -> Result<()> {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let backup_dir = config_dir.parent()
            .context("Could not get parent directory")?
            .join(format!(".vellum-fe.backup-{}", timestamp));

        let mut migration = Migration {
            config_dir: config_dir.clone(),
            backup_dir,
            log: Vec::new(),
        };

        migration.log("Starting VellumFE config migration...".to_string());

        // Step 1: Create backup
        migration.create_backup()?;

        // Step 2: Migrate in order (continue on errors, log them)
        let mut errors = Vec::new();

        if let Err(e) = migration.migrate_global() {
            errors.push(format!("Global migration: {}", e));
        }

        if let Err(e) = migration.migrate_profiles() {
            errors.push(format!("Profiles migration: {}", e));
        }

        if let Err(e) = migration.migrate_presets() {
            errors.push(format!("Presets migration: {}", e));
        }

        // Step 3: Write log and mark as migrated (even if partial)
        migration.write_log()?;
        migration.mark_migrated()?;

        // Step 4: Report results
        println!("\n✓ Migration completed!");
        println!("  Backup created at: {}", migration.backup_dir.display());
        println!("  Migration log: {}/.migration_log", config_dir.display());

        if !errors.is_empty() {
            println!("\n⚠ Partial migration - some steps failed:");
            for error in &errors {
                println!("  - {}", error);
            }
            println!("\n  Your system should still work with the migrated files.");
            println!("  Check the migration log for details.");
            println!("  You can restore from backup if needed: {}", migration.backup_dir.display());
        } else {
            println!("  All settings migrated successfully!");
            println!("\n  Old files are preserved in the backup.");
            println!("  You can delete the backup folder once you've verified everything works:");
            println!("  {}", migration.backup_dir.display());
        }

        Ok(())
    }

    fn log(&mut self, message: String) {
        println!("  {}", message);
        self.log.push(format!("[{}] {}", Local::now().format("%H:%M:%S"), message));
    }

    fn create_backup(&mut self) -> Result<()> {
        self.log("Creating backup...".to_string());

        // Copy entire config directory to backup location
        self.copy_dir_recursive(&self.config_dir, &self.backup_dir)?;

        self.log(format!("Backup created: {}", self.backup_dir.display()));
        Ok(())
    }

    fn migrate_global(&mut self) -> Result<()> {
        self.log("Migrating global files...".to_string());

        let global_dir = self.config_dir.join("global");
        fs::create_dir_all(&global_dir)?;

        // Move cmdlist1.xml
        let old_cmdlist = self.config_dir.join("cmdlist1.xml");
        if old_cmdlist.exists() {
            let new_cmdlist = global_dir.join("cmdlist1.xml");
            fs::copy(&old_cmdlist, &new_cmdlist)?;
            self.log("  cmdlist1.xml → global/cmdlist1.xml".to_string());
        }

        // Move common/highlights.toml → global/highlights.toml
        let old_common_hl = self.config_dir.join("common").join("highlights.toml");
        if old_common_hl.exists() {
            let new_global_hl = global_dir.join("highlights.toml");
            fs::copy(&old_common_hl, &new_global_hl)?;
            self.log("  common/highlights.toml → global/highlights.toml".to_string());
        }

        // Move sounds/ → global/sounds/
        let old_sounds = self.config_dir.join("sounds");
        if old_sounds.exists() {
            let new_sounds = global_dir.join("sounds");
            self.copy_dir_recursive(&old_sounds, &new_sounds)?;
            self.log("  sounds/ → global/sounds/".to_string());
        }

        Ok(())
    }

    fn migrate_profiles(&mut self) -> Result<()> {
        self.log("Migrating character profiles...".to_string());

        let profiles_dir = self.config_dir.join("profiles");
        fs::create_dir_all(&profiles_dir)?;

        // Find all character folders (folders containing config.toml)
        for entry in fs::read_dir(&self.config_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let dir_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Skip known non-character directories
            if matches!(dir_name, "global" | "profiles" | "layouts" | "presets" | "common" | "highlights" | "keybinds" | "sounds") {
                continue;
            }

            // Check if it's a character folder (has config.toml or layout.toml)
            let has_config = path.join("config.toml").exists();
            let has_layout = path.join("layout.toml").exists();

            if has_config || has_layout {
                let new_profile_dir = profiles_dir.join(dir_name);
                self.copy_dir_recursive(&path, &new_profile_dir)?;
                self.log(format!("  {}/ → profiles/{}/", dir_name, dir_name));
            }
        }

        Ok(())
    }

    fn migrate_presets(&mut self) -> Result<()> {
        self.log("Migrating preset collections...".to_string());

        let presets_dir = self.config_dir.join("presets");
        fs::create_dir_all(&presets_dir)?;

        // Move highlights/ → presets/highlights/
        let old_highlights_dir = self.config_dir.join("highlights");
        if old_highlights_dir.exists() {
            let new_highlights_dir = presets_dir.join("highlights");
            self.copy_dir_recursive(&old_highlights_dir, &new_highlights_dir)?;
            self.log("  highlights/ → presets/highlights/".to_string());
        }

        // Move keybinds/ → presets/keybinds/
        let old_keybinds_dir = self.config_dir.join("keybinds");
        if old_keybinds_dir.exists() {
            let new_keybinds_dir = presets_dir.join("keybinds");
            self.copy_dir_recursive(&old_keybinds_dir, &new_keybinds_dir)?;
            self.log("  keybinds/ → presets/keybinds/".to_string());
        }

        // layouts/ stays in place (no migration needed)

        Ok(())
    }

    fn mark_migrated(&self) -> Result<()> {
        let marker = self.config_dir.join(".migrated");
        fs::write(marker, format!("Migrated on {}\n", Local::now().format("%Y-%m-%d %H:%M:%S")))?;
        Ok(())
    }

    fn write_log(&self) -> Result<()> {
        let log_path = self.config_dir.join(".migration_log");
        let log_content = self.log.join("\n");
        fs::write(log_path, log_content)?;
        Ok(())
    }

    /// Recursively copy a directory
    fn copy_dir_recursive(&self, src: &Path, dst: &Path) -> Result<()> {
        if !dst.exists() {
            fs::create_dir_all(dst)?;
        }

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                self.copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)?;
            }
        }

        Ok(())
    }
}
