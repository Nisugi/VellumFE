//! Clipboard integration for copy/paste/cut operations
//!
//! Uses arboard for cross-platform clipboard access

use anyhow::Result;
use arboard::Clipboard;

/// Copy text to system clipboard
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
pub fn paste() -> Result<String> {
    let mut clipboard = Clipboard::new()?;
    let text = clipboard.get_text()?;
    tracing::debug!("Pasted {} bytes from clipboard", text.len());
    Ok(text)
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

    #[test]
    fn test_empty_copy() {
        // Should not fail on empty string
        assert!(copy("").is_ok());
    }
}
