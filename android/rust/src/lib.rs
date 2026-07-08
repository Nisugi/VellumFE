//! JNI surface for the Android shell (`dev.vellumfe.core.VellumCore`).
//!
//! Three functions, string-typed at the boundary:
//! - `startCore(dataDir) -> String` — start the headless runtime (idempotent)
//!   and return `{"port": N, "token": "..."}`; on failure `{"error": "..."}`.
//!   The WebView loads `http://127.0.0.1:<port>/play#token=<token>`.
//! - `stopCore()` — graceful shutdown via the runtime's watch channel.
//! - `coreStatus() -> String` — `{"running": bool, "port": N}` for the
//!   foreground-service notification.
//!
//! The bootstrap itself is the shared, target-independent
//! `vellum_fe::frontend::headless::embedded` (also used by `ios/rust`);
//! this crate owns only JNI marshalling and logcat wiring.

#[cfg(target_os = "android")]
mod jni_glue {
    use jni::objects::{JClass, JString};
    use jni::sys::jstring;
    use jni::JNIEnv;

    use vellum_fe::frontend::headless::embedded;

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
        to_jstring(&env, &embedded::start_json(&data_dir))
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_vellumfe_core_VellumCore_stopCore(
        _env: JNIEnv,
        _class: JClass,
    ) {
        embedded::stop();
    }

    #[no_mangle]
    pub extern "system" fn Java_dev_vellumfe_core_VellumCore_coreStatus(
        env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        to_jstring(&env, &embedded::status_json())
    }
}
