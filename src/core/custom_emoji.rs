//! Custom (Discord-server-style) emoji registry.
//!
//! Users drop image files into `~/.vellum-fe/emoji/<name>.<ext>` and reference
//! them as `:name:` shortcodes, just like gemoji. Unlike gemoji, custom emoji
//! have no Unicode codepoint, so the shortcode resolver (see
//! [`crate::core::emoji::apply_to_segments`]) keeps the literal `:name:` in the
//! segment text and tags the segment with the custom name via
//! [`crate::data::TextSegment::custom_emoji`]. Frontends that can render images
//! (GUI from disk, web via `<img>`) swap the run for the picture; the TUI shows
//! the `:name:` text unchanged.
//!
//! The registry is a process-wide snapshot loaded at startup and refreshed on
//! `.reload`. It stores metadata only (name → path/format); the image bytes
//! are read lazily by the renderers that need them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Image formats a custom emoji file may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmojiFormat {
    Png,
    /// Animated PNG. Detected by an `acTL` chunk in a `.png`/`.apng` file.
    Apng,
    /// GIF (assumed possibly-animated; the web renders it natively, the GUI
    /// cycles frames).
    Gif,
    Webp,
}

impl EmojiFormat {
    /// May this format carry animation frames? GIF/APNG/WebP can all be
    /// animated (Discord serves animated custom emoji as WebP). Whether a given
    /// file actually animates is decided at decode time by the frame count; the
    /// GUI renderer doesn't gate on this, so it's an advisory/coverage hint.
    pub fn is_animated(self) -> bool {
        matches!(
            self,
            EmojiFormat::Apng | EmojiFormat::Gif | EmojiFormat::Webp
        )
    }

    /// MIME type for the web endpoint's Content-Type header.
    pub fn mime(self) -> &'static str {
        match self {
            EmojiFormat::Png | EmojiFormat::Apng => "image/png",
            EmojiFormat::Gif => "image/gif",
            EmojiFormat::Webp => "image/webp",
        }
    }
}

/// One installed custom emoji.
#[derive(Clone, Debug)]
pub struct CustomEmoji {
    /// Lowercased shortcode name (the key), e.g. "vibecat".
    pub name: String,
    /// Absolute path to the image file on disk.
    pub path: PathBuf,
    pub format: EmojiFormat,
}

/// A resolved snapshot of the custom emoji directory.
#[derive(Clone, Debug, Default)]
pub struct CustomEmojiRegistry {
    by_name: HashMap<String, CustomEmoji>,
}

impl CustomEmojiRegistry {
    /// Look up a custom emoji by shortcode name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&CustomEmoji> {
        if name.bytes().any(|b| b.is_ascii_uppercase()) {
            self.by_name.get(&name.to_ascii_lowercase())
        } else {
            self.by_name.get(name)
        }
    }

    /// True when a custom emoji with this name exists (case-insensitive).
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Number of installed custom emoji.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// All installed emoji, unordered.
    pub fn iter(&self) -> impl Iterator<Item = &CustomEmoji> {
        self.by_name.values()
    }

    /// Scan a directory for `<name>.<ext>` image files and build a registry.
    /// Unreadable dirs (e.g. the dir does not exist yet) yield an empty
    /// registry rather than an error — custom emoji are optional.
    pub fn scan_dir(dir: &Path) -> Self {
        let mut by_name = HashMap::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Self { by_name };
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // Only accept valid shortcode names so `:name:` can reference them
            // and so we never collide with the resolver's tokenizer.
            if stem.is_empty() || !stem.bytes().all(is_shortcode_byte) {
                continue;
            }
            let Some(format) = format_of(&path) else {
                continue;
            };
            let name = stem.to_ascii_lowercase();
            // First file wins on a name collision (stable across a rescan
            // because read_dir order is otherwise unspecified — but two files
            // sharing a stem with different extensions is a user mistake).
            by_name.entry(name.clone()).or_insert(CustomEmoji {
                name,
                path,
                format,
            });
        }
        Self { by_name }
    }
}

/// Shortcode name alphabet, matching the gemoji/resolver rule.
fn is_shortcode_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-'
}

/// Classify a file by extension, then (for PNG) sniff for the `acTL` chunk
/// that marks an animated PNG. Returns `None` for unsupported extensions.
fn format_of(path: &Path) -> Option<EmojiFormat> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some(if png_is_animated(path) {
            EmojiFormat::Apng
        } else {
            EmojiFormat::Png
        }),
        "apng" => Some(EmojiFormat::Apng),
        "gif" => Some(EmojiFormat::Gif),
        "webp" => Some(EmojiFormat::Webp),
        _ => None,
    }
}

/// Detect an animated PNG by scanning the first chunks for `acTL`, which by
/// spec appears before the first `IDAT`. We read a bounded prefix so a large
/// static PNG does not cost a full read just to classify it.
fn png_is_animated(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    // acTL must precede IDAT; the PNG header + a few chunk headers fit well
    // under 64 KiB. If acTL is farther in, the file is unusual — treat as
    // static (the GUI simply paints the first frame).
    let mut buf = [0u8; 64 * 1024];
    let n = f.read(&mut buf).unwrap_or(0);
    let head = &buf[..n];
    // Find "acTL" but stop at the first "IDAT" (per spec acTL is earlier).
    let idat = find_bytes(head, b"IDAT");
    let actl = find_bytes(head, b"acTL");
    match (actl, idat) {
        (Some(a), Some(i)) => a < i,
        (Some(_), None) => true,
        _ => false,
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

/// Process-wide registry snapshot. Empty until [`reload`] runs at startup.
static REGISTRY: RwLock<Option<CustomEmojiRegistry>> = RwLock::new(None);

/// The custom emoji directory: `~/.vellum-fe/emoji/` (respects `VELLUM_FE_DIR`).
pub fn emoji_dir() -> Option<PathBuf> {
    crate::config::Config::base_dir().ok().map(|d| d.join("emoji"))
}

/// (Re)scan the emoji directory and publish the new snapshot. Call at startup
/// and on `.reload`. Returns the number of emoji found.
pub fn reload() -> usize {
    let registry = match emoji_dir() {
        Some(dir) => CustomEmojiRegistry::scan_dir(&dir),
        None => CustomEmojiRegistry::default(),
    };
    let count = registry.len();
    *REGISTRY.write().expect("custom emoji registry poisoned") = Some(registry);
    count
}

/// Is `name` (case-insensitive) an installed custom emoji? Cheap enough to
/// call from the shortcode resolver's hot path (a read lock + hash lookup).
pub fn contains(name: &str) -> bool {
    REGISTRY
        .read()
        .expect("custom emoji registry poisoned")
        .as_ref()
        .is_some_and(|r| r.contains(name))
}

/// Resolve `name` to its metadata, cloned out of the registry.
pub fn get(name: &str) -> Option<CustomEmoji> {
    REGISTRY
        .read()
        .expect("custom emoji registry poisoned")
        .as_ref()
        .and_then(|r| r.get(name).cloned())
}

/// Install a registry snapshot directly (tests only).
#[cfg(test)]
pub fn set_for_test(registry: CustomEmojiRegistry) {
    *REGISTRY.write().expect("custom emoji registry poisoned") = Some(registry);
}

/// Build a registry from `(name, format)` pairs without touching disk (tests).
#[cfg(test)]
pub fn registry_from_names(entries: &[(&str, EmojiFormat)]) -> CustomEmojiRegistry {
    let mut by_name = HashMap::new();
    for (name, format) in entries {
        let name = name.to_ascii_lowercase();
        by_name.insert(
            name.clone(),
            CustomEmoji {
                name: name.clone(),
                path: PathBuf::from(format!("{name}.png")),
                format: *format,
            },
        );
    }
    CustomEmojiRegistry { by_name }
}

/// Snapshot every installed emoji (for editors / the web endpoint listing).
pub fn all() -> Vec<CustomEmoji> {
    REGISTRY
        .read()
        .expect("custom emoji registry poisoned")
        .as_ref()
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) {
        let mut f = std::fs::File::create(dir.join(name)).unwrap();
        f.write_all(bytes).unwrap();
    }

    // Minimal valid-enough PNG header for format sniffing (signature + a fake
    // IHDR/IDAT layout is unnecessary; scan_dir only reads extension + acTL).
    const PNG_SIG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    #[test]
    fn scan_picks_up_named_images_case_insensitively() {
        let tmp = std::env::temp_dir().join(format!(
            "vellum_emoji_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        write_file(&tmp, "VibeCat.png", &[PNG_SIG, b"IDATxx"].concat());
        write_file(&tmp, "dance.gif", b"GIF89a....");
        write_file(&tmp, "notes.txt", b"ignore me"); // unsupported ext

        let reg = CustomEmojiRegistry::scan_dir(&tmp);
        assert_eq!(reg.len(), 2, "png + gif, txt ignored");
        assert!(reg.contains("vibecat"), "lookup is case-insensitive");
        assert!(reg.contains("VibeCat"));
        assert!(reg.contains("dance"));
        assert!(!reg.contains("notes"));
        assert_eq!(reg.get("dance").unwrap().format, EmojiFormat::Gif);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn animated_png_detected_by_actl_before_idat() {
        let tmp = std::env::temp_dir().join(format!(
            "vellum_emoji_apng_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        // acTL before IDAT => animated.
        write_file(&tmp, "wave.png", &[PNG_SIG, b"....acTL....IDAT...."].concat());
        // IDAT with no acTL => static.
        write_file(&tmp, "still.png", &[PNG_SIG, b"....IDAT...."].concat());

        let reg = CustomEmojiRegistry::scan_dir(&tmp);
        assert_eq!(reg.get("wave").unwrap().format, EmojiFormat::Apng);
        assert!(reg.get("wave").unwrap().format.is_animated());
        assert_eq!(reg.get("still").unwrap().format, EmojiFormat::Png);
        assert!(!reg.get("still").unwrap().format.is_animated());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_names_that_are_not_valid_shortcodes() {
        let tmp = std::env::temp_dir().join(format!(
            "vellum_emoji_badnames_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        write_file(&tmp, "has space.png", PNG_SIG);
        write_file(&tmp, "ok_name-1.png", PNG_SIG);

        let reg = CustomEmojiRegistry::scan_dir(&tmp);
        assert!(reg.contains("ok_name-1"));
        assert!(!reg.contains("has space"), "spaces can't appear in :name:");
        assert_eq!(reg.len(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn missing_dir_yields_empty_registry() {
        let reg = CustomEmojiRegistry::scan_dir(Path::new(
            "/definitely/not/a/real/emoji/dir/xyzzy",
        ));
        assert!(reg.is_empty());
    }
}
