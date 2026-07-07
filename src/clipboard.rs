//! Clipboard integration for copy/paste/cut operations
//!
//! Uses arboard for cross-platform clipboard access on desktop builds.
//! Without the `desktop` feature (Android headless builds) these are
//! error-returning stubs: the web frontend copies via the browser/WebView
//! clipboard instead, so nothing routes through here.

use anyhow::Result;
#[cfg(feature = "desktop")]
use arboard::Clipboard;

/// Copy text to system clipboard
#[cfg(feature = "desktop")]
pub fn copy(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(()); // Nothing to copy
    }

    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text.to_string())?;
    tracing::debug!("Copied {} bytes to clipboard", text.len());
    Ok(())
}

/// Paste text from system clipboard
#[cfg(feature = "desktop")]
pub fn paste() -> Result<String> {
    let mut clipboard = Clipboard::new()?;
    let text = clipboard.get_text()?;
    tracing::debug!("Pasted {} bytes from clipboard", text.len());
    Ok(text)
}

#[cfg(not(feature = "desktop"))]
pub fn copy(_text: &str) -> Result<()> {
    anyhow::bail!("Clipboard unavailable without the `desktop` feature")
}

#[cfg(not(feature = "desktop"))]
pub fn paste() -> Result<String> {
    anyhow::bail!("Clipboard unavailable without the `desktop` feature")
}

/// Cut text to clipboard (copy + return empty string as replacement)
/// Caller is responsible for actually removing the text from the source
pub fn cut(text: &str) -> Result<()> {
    copy(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires clipboard access, may fail in CI
    fn test_copy_paste() {
        let test_text = "Hello, clipboard!";

        // Copy
        copy(test_text).expect("Copy failed");

        // Paste
        let result = paste().expect("Paste failed");
        assert_eq!(result, test_text);
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn test_empty_copy() {
        // Should not fail on empty string
        assert!(copy("").is_ok());
    }
}
