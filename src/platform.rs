//! Small platform-integration shims that differ between desktop and
//! headless/Android builds.

use anyhow::Result;

/// Open a URL or file path with the OS default handler (browser, explorer).
///
/// Without the `desktop` feature this is a logged no-op: on Android the
/// WebView shell routes external links to the system browser itself, so
/// nothing should reach this call.
#[cfg(feature = "desktop")]
pub fn open_url(target: &str) -> Result<()> {
    open::that(target)?;
    Ok(())
}

#[cfg(not(feature = "desktop"))]
pub fn open_url(target: &str) -> Result<()> {
    tracing::warn!("open_url unavailable without the `desktop` feature: {target}");
    anyhow::bail!("Opening URLs is unavailable in this build")
}
