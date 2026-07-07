//! JNI surface for the Android shell (`dev.vellumfe.core.VellumCore`).
//!
//! Three functions, string-typed at the boundary:
//! - `startCore(dataDir) -> String` — start the headless runtime (idempotent)
//!   and return `{"port": N, "token": "..."}`; on failure `{"error": "..."}`.
//!   The WebView loads `http://127.0.0.1:<port>/play#token=<token>`.
//! - `stopCore()` — graceful shutdown via the runtime's watch channel.
//! - `coreStatus() -> String` — `{"running": bool, "port": N}` for the
//!   foreground-service notification. (Session state lands here in Phase C2
//!   when the notification consumes it.)
//!
//! The core logic is target-independent so `cargo check -p vellum-android`
//! validates it on any host; only the thin `jni_glue` layer is Android-only.

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
fn start_core(data_dir: &str) -> Result<(u16, String), String> {
    let mut core = lock_core();
    if let Some(running) = core.as_ref() {
        return Ok((running.port, running.token.clone()));
    }

    // Every config/profile/log path derives from this (config/paths.rs).
    // Safe on edition 2021; revisit if the crate moves to 2024 (set_var
    // becomes unsafe there because of concurrent readers).
    std::env::set_var("VELLUM_FE_DIR", data_dir);

    let config = vellum_fe::config::Config::load_with_options(None, None)
        .map_err(|e| format!("config load failed: {e:#}"))?;
    let token = vellum_fe::config::Config::load_or_create_web_token()
        .map_err(|e| format!("web token unavailable: {e:#}"))?;
    // The runtime forces web.enabled; the port comes from config ([web]
    // port, default 8040). Android is effectively single-instance on a
    // private loopback, so the configured port is the bound port; the
    // shell health-polls /play before showing the WebView regardless.
    let port = config.web.port;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let thread = std::thread::Builder::new()
        .name("vellum-core".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    log::error!("tokio runtime failed: {e}");
                    return;
                }
            };
            let result = runtime.block_on(vellum_fe::frontend::headless::async_run(
                config,
                None, // character: sessions start from the web login screen
                None, // no CLI direct credentials on Android
                None, // no Lich key
                shutdown_rx,
            ));
            if let Err(e) = result {
                log::error!("headless runtime exited with error: {e:#}");
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

/// Graceful shutdown; blocks until the runtime thread exits.
fn stop_core() {
    let Some(mut running) = lock_core().take() else {
        return;
    };
    let _ = running.shutdown_tx.send(true);
    if let Some(thread) = running.thread.take() {
        if thread.join().is_err() {
            log::error!("core thread panicked during shutdown");
        }
    }
}

fn status_json() -> String {
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

#[cfg(target_os = "android")]
mod jni_glue {
    use jni::objects::{JClass, JString};
    use jni::sys::jstring;
    use jni::JNIEnv;

    fn init_logging() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            android_logger::init_once(
                android_logger::Config::default()
                    .with_max_level(log::LevelFilter::Info)
                    .with_tag("VellumCore"),
            );
        });
    }

    fn to_jstring(env: &JNIEnv, s: &str) -> jstring {
        env.new_string(s)
            .map(|js| js.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_vellumfe_core_VellumCore_startCore(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
    ) -> jstring {
        init_logging();
        let data_dir: String = match env.get_string(&data_dir) {
            Ok(s) => s.into(),
            Err(e) => {
                return to_jstring(&env, &format!(r#"{{"error":"bad dataDir: {e}"}}"#));
            }
        };
        let reply = match super::start_core(&data_dir) {
            Ok((port, token)) => {
                serde_json::json!({ "port": port, "token": token }).to_string()
            }
            Err(message) => serde_json::json!({ "error": message }).to_string(),
        };
        to_jstring(&env, &reply)
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_vellumfe_core_VellumCore_stopCore(
        _env: JNIEnv,
        _class: JClass,
    ) {
        super::stop_core();
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_vellumfe_core_VellumCore_coreStatus(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        to_jstring(&env, &super::status_json())
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
