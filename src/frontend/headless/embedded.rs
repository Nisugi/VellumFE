//! Embeddable bootstrap for mobile shells (Android JNI, iOS C ABI).
//!
//! This is the canonical start/stop/status logic both platform shim crates
//! (`android/rust`, `ios/rust`) delegate to; the shims own only string
//! marshalling and platform logging. The embedding contract:
//!
//! - `data_dir`: the app's private storage directory. Becomes
//!   `VELLUM_FE_DIR`, from which every config/profile/log path derives
//!   (config/paths.rs).
//! - `VELLUM_PASSWORD_KEY` (optional): 64 lowercase hex chars (32 bytes),
//!   set by the shell *before* [`start`] so saved passwords are sealed with
//!   ChaCha20-Poly1305 (config/profiles.rs). Missing key = plaintext.
//! - [`start`] returns `(port, token)`; the shell health-polls
//!   `http://127.0.0.1:<port>/health` and then loads
//!   `http://127.0.0.1:<port>/play#token=<token>` in its WebView.

use std::sync::Mutex;

struct Core {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    thread: Option<std::thread::JoinHandle<()>>,
    port: u16,
    token: String,
}

static CORE: Mutex<Option<Core>> = Mutex::new(None);

fn lock_core() -> std::sync::MutexGuard<'static, Option<Core>> {
    match CORE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Start the headless runtime with all storage under `data_dir` (the app's
/// private files directory). Idempotent: a second call returns the running
/// instance's connection info.
pub fn start(data_dir: &str) -> Result<(u16, String), String> {
    let mut core = lock_core();
    if let Some(running) = core.as_ref() {
        return Ok((running.port, running.token.clone()));
    }

    // Every config/profile/log path derives from this (config/paths.rs).
    // Safe on edition 2021; revisit if the crate moves to 2024 (set_var
    // becomes unsafe there because of concurrent readers).
    std::env::set_var("VELLUM_FE_DIR", data_dir);

    let config = crate::config::Config::load_with_options(None, None)
        .map_err(|e| format!("config load failed: {e:#}"))?;
    let token = crate::config::Config::load_or_create_web_token()
        .map_err(|e| format!("web token unavailable: {e:#}"))?;
    // The runtime forces web.enabled; the port comes from config ([web]
    // port, default 8040). A mobile shell is effectively single-instance on
    // a private loopback, so the configured port is the bound port; the
    // shell health-polls /health before showing the WebView regardless.
    let port = config.web.port;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let thread = std::thread::Builder::new()
        .name("vellum-core".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("tokio runtime failed: {e}");
                    return;
                }
            };
            let result = runtime.block_on(super::async_run(
                config,
                None, // character: sessions start from the web login screen
                None, // no CLI direct credentials on mobile
                None, // no Lich key
                shutdown_rx,
            ));
            if let Err(e) = result {
                tracing::error!("headless runtime exited with error: {e:#}");
            }
        })
        .map_err(|e| format!("core thread spawn failed: {e}"))?;

    *core = Some(Core {
        shutdown_tx,
        thread: Some(thread),
        port,
        token: token.clone(),
    });
    Ok((port, token))
}

/// [`start`] with the reply pre-composed as the JSON envelope both shells
/// hand to their WebView boot code: `{"port": N, "token": "..."}` on
/// success, `{"error": "..."}` on failure.
pub fn start_json(data_dir: &str) -> String {
    match start(data_dir) {
        Ok((port, token)) => serde_json::json!({ "port": port, "token": token }).to_string(),
        Err(message) => serde_json::json!({ "error": message }).to_string(),
    }
}

/// Graceful shutdown; blocks until the runtime thread exits.
pub fn stop() {
    let Some(mut running) = lock_core().take() else {
        return;
    };
    let _ = running.shutdown_tx.send(true);
    if let Some(thread) = running.thread.take() {
        if thread.join().is_err() {
            tracing::error!("core thread panicked during shutdown");
        }
    }
}

/// `{"running": bool, "port": N}` for shell status surfaces.
pub fn status_json() -> String {
    let core = lock_core();
    match core.as_ref() {
        Some(running) => serde_json::json!({
            "running": true,
            "port": running.port,
        })
        .to_string(),
        None => serde_json::json!({ "running": false }).to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn status_reports_not_running() {
        let status: serde_json::Value = serde_json::from_str(&super::status_json()).unwrap();
        assert_eq!(status["running"], false);
    }
}
