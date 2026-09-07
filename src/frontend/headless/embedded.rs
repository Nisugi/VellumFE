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
//!   ChaCha20-Poly1305 (config/profiles.rs). Missing key = persistent secret
//!   saving is DISABLED (session-only login still works; never plaintext).
//!   Desktop headless testing may opt into plaintext explicitly with
//!   `VELLUM_ALLOW_PLAINTEXT_SECRETS=1`.
//! - [`start`] blocks until the web server is actually serving, then
//!   returns `(port, token)` — the listener's real bound port (an unpinned
//!   instance may walk past an occupied base port) and the exact pairing
//!   token that listener authenticates. The shell health-polls
//!   `http://127.0.0.1:<port>/health` as a final reachability check and
//!   then loads `http://127.0.0.1:<port>/play#token=<token>` in its
//!   WebView.

use std::sync::Mutex;
use std::time::Duration;

struct Core {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    thread: Option<std::thread::JoinHandle<()>>,
    port: u16,
    token: String,
}

static CORE: Mutex<Option<Core>> = Mutex::new(None);

/// Upper bound on how long [`start`] waits for the web server to come up.
/// Startup is local work (config load, bind, token file) and normally
/// completes in well under a second; the bound only guards against a wedged
/// runtime so a shell never blocks its boot forever.
const STARTUP_WAIT: Duration = Duration::from_secs(30);

fn lock_core() -> std::sync::MutexGuard<'static, Option<Core>> {
    match CORE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Start the headless runtime with all storage under `data_dir` (the app's
/// private files directory) and wait (bounded) until its web server is
/// ready. Idempotent: a second call returns the running instance's
/// connection info. The instance lock is held for the whole startup wait,
/// so concurrent callers (Android activity + foreground service) serialize
/// and all receive the same successful instance — and a retry can never
/// overlap the cleanup of a failed startup.
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

    // The runtime reports the server's actual endpoint (bound port + the
    // token that listener installed) or the startup failure. The token is
    // deliberately NOT loaded here: the server is the sole token authority,
    // and a second load could race first-run creation.
    let (startup_tx, startup_rx) = std::sync::mpsc::channel();
    let thread_startup_tx = startup_tx.clone();

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let thread = std::thread::Builder::new()
        .name("vellum-core".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("tokio runtime failed: {e}");
                    let _ = thread_startup_tx.send(Err(format!("tokio runtime failed: {e}")));
                    return;
                }
            };
            let result = runtime.block_on(super::async_run_embedded(
                config,
                None, // character: sessions start from the web login screen
                None, // no CLI direct credentials on mobile
                None, // no Lich key
                shutdown_rx,
                thread_startup_tx.clone(),
            ));
            if let Err(e) = result {
                tracing::error!("headless runtime exited with error: {e:#}");
                // Harmless duplicate if the runtime already reported; the
                // waiter only ever consumes the first report.
                let _ = thread_startup_tx.send(Err(format!("core runtime failed: {e:#}")));
            }
        })
        .map_err(|e| format!("core thread spawn failed: {e}"))?;
    drop(startup_tx);

    match startup_rx.recv_timeout(STARTUP_WAIT) {
        Ok(Ok(endpoint)) => {
            let port = endpoint.bound_port();
            let token = endpoint.token().to_string();
            *core = Some(Core {
                shutdown_tx,
                thread: Some(thread),
                port,
                token: token.clone(),
            });
            Ok((port, token))
        }
        Ok(Err(message)) => {
            // Failed startup: request cleanup and wait for the runtime
            // thread so nothing of the failed instance survives into a
            // retry. Never cache it as running.
            let _ = shutdown_tx.send(true);
            if thread.join().is_err() {
                tracing::error!("core thread panicked during failed startup");
            }
            Err(message)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            // Request cleanup, but do not block the shell on joining a
            // wedged runtime; the thread is detached and winds down on its
            // own. Nothing is cached, so a retry starts fresh.
            let _ = shutdown_tx.send(true);
            drop(thread);
            Err(format!(
                "timed out after {}s waiting for the web server to start",
                STARTUP_WAIT.as_secs()
            ))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            // The runtime thread ended without reporting (shutdown raced
            // startup, or an unreported early exit).
            let _ = shutdown_tx.send(true);
            if thread.join().is_err() {
                tracing::error!("core thread panicked before startup completed");
            }
            Err("core runtime exited before the web server started".to_string())
        }
    }
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
