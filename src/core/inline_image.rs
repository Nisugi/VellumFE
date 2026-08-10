//! Inline image registry — art a Lich script can float into a text window.
//!
//! Users drop image files into `~/.vellum-fe/images/inline/<name>.<ext>` and a
//! script references them by NAME with
//! `<vellumImg src='name' rows='4' align='left'/>`. The parser validates the
//! name against the shortcode alphabet before it ever reaches this registry,
//! and this registry only ever hands back paths it discovered itself by
//! scanning that one directory — so a feed can name art but can never read an
//! arbitrary file.
//!
//! Structurally this mirrors [`crate::core::custom_emoji`]: a process-wide
//! snapshot loaded at startup and refreshed on `.reload`, storing metadata
//! only (name → path/format) while renderers read the bytes lazily. The
//! scanning and format-sniffing logic is shared with that module rather than
//! duplicated, so APNG/GIF/WebP animation support comes along for free.

use std::path::PathBuf;

use super::custom_emoji::{CustomEmoji, CustomEmojiRegistry, EmojiFormat};
use std::sync::RwLock;

/// One installed inline image. Same shape as an installed custom emoji —
/// name, path, format — so the two share a scanner.
pub type InlineImageArt = CustomEmoji;

/// Format of an inline image file. Alias of the emoji format enum: the
/// supported set (PNG/APNG/GIF/WebP) and the `mime()`/`is_animated()`
/// behavior are identical.
pub type InlineImageFormat = EmojiFormat;

/// Process-wide registry snapshot. Empty until [`reload`] runs at startup.
static REGISTRY: RwLock<Option<CustomEmojiRegistry>> = RwLock::new(None);

/// Pool category holding inline image art, relative to `global/images/`.
pub const POOL_CATEGORY: &str = "inline";

/// The inline image directory: `~/.vellum-fe/global/images/inline/`
/// (respects `VELLUM_FE_DIR`). Lives in the shared image pool rather than a
/// private folder so the existing pickers and `jinx` installs can see it.
pub fn image_dir() -> Option<PathBuf> {
    crate::config::Config::global_image_category_dir(POOL_CATEGORY).ok()
}

/// (Re)scan the inline image directory and publish the new snapshot. Call at
/// startup and on `.reload`. Returns the number of images found.
pub fn reload() -> usize {
    let registry = match image_dir() {
        Some(dir) => CustomEmojiRegistry::scan_dir(&dir),
        None => CustomEmojiRegistry::default(),
    };
    let count = registry.len();
    *REGISTRY.write().expect("inline image registry poisoned") = Some(registry);
    count
}

/// Is `name` (case-insensitive) an installed inline image?
pub fn contains(name: &str) -> bool {
    REGISTRY
        .read()
        .expect("inline image registry poisoned")
        .as_ref()
        .is_some_and(|r| r.contains(name))
}

/// Resolve `name` to its metadata, cloned out of the registry. `None` for an
/// unknown name — callers fall back to the segment's `[img:name]` text.
pub fn get(name: &str) -> Option<InlineImageArt> {
    REGISTRY
        .read()
        .expect("inline image registry poisoned")
        .as_ref()
        .and_then(|r| r.get(name).cloned())
}

/// All installed inline images, sorted by name (for pickers and the web
/// listing endpoint).
pub fn all() -> Vec<InlineImageArt> {
    let mut images: Vec<InlineImageArt> = REGISTRY
        .read()
        .expect("inline image registry poisoned")
        .as_ref()
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default();
    images.sort_by(|a, b| a.name.cmp(&b.name));
    images
}

/// Install a registry snapshot directly (tests only).
#[cfg(test)]
pub fn set_for_test(registry: CustomEmojiRegistry) {
    *REGISTRY.write().expect("inline image registry poisoned") = Some(registry);
}

/// Serializes tests that install a snapshot: [`set_for_test`] writes
/// process-wide state, so two tests running in parallel otherwise clobber
/// each other's art and fail intermittently. Hold this for the whole test.
#[cfg(test)]
pub static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    /// An unknown name must resolve to None rather than panicking or
    /// synthesizing a path — the renderer's fallback depends on it.
    #[test]
    fn unknown_name_resolves_to_none() {
        set_for_test(CustomEmojiRegistry::default());
        assert!(get("nope").is_none());
        assert!(!contains("nope"));
    }

    /// Lookup is case-insensitive, so `src='Banner'` and `src='banner'` name
    /// the same art.
    #[test]
    fn lookup_is_case_insensitive() {
        let mut registry = CustomEmojiRegistry::default();
        registry.insert_for_test(InlineImageArt {
            name: "banner".to_string(),
            path: PathBuf::from("banner.png"),
            format: InlineImageFormat::Png,
        });
        set_for_test(registry);
        assert!(contains("banner"));
        assert!(contains("Banner"));
        assert_eq!(get("BANNER").map(|i| i.name), Some("banner".to_string()));
    }
}
