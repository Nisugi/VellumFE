//! C ABI surface for the iOS shell (Swift `CoreBridge`).
//!
//! Four functions, C-string-typed at the boundary; declarations live in
//! `include/vellum_core.h` (hand-written — keep the two in sync):
//! - `vellum_start_core(dataDir) -> char*` — start the headless runtime
//!   (idempotent) and return `{"port": N, "token": "..."}`; on failure
//!   `{"error": "..."}`. The WKWebView loads
//!   `http://127.0.0.1:<port>/play#token=<token>`.
//! - `vellum_stop_core()` — graceful shutdown via the runtime's watch channel.
//! - `vellum_core_status() -> char*` — `{"running": bool, "port": N}`.
//! - `vellum_string_free(char*)` — return a string from the calls above.
//!
//! Every returned string is a Rust-owned `CString`; Swift copies it into a
//! `String` and must call `vellum_string_free` exactly once. The bootstrap
//! itself is the shared `vellum_fe::frontend::headless::embedded` (also
//! used by `android/rust`); this crate owns only C marshalling and os_log
//! wiring.

use std::ffi::{c_char, CStr, CString};

use vellum_fe::frontend::headless::embedded;

fn init_logging() {
    #[cfg(target_vendor = "apple")]
    {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            // subsystem shows up as the filterable source in Console.app.
            if let Err(e) = oslog::OsLogger::new("dev.vellumfe.core")
                .level_filter(log::LevelFilter::Info)
                .init()
            {
                eprintln!("os_log init failed: {e}");
            }
        });
    }
}

/// Copy a Rust string across the boundary; the caller owns the result and
/// frees it with `vellum_string_free`. Interior NULs cannot occur in the
/// JSON we produce; guard anyway rather than panic across the FFI edge.
fn to_c_string(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `data_dir` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn vellum_start_core(data_dir: *const c_char) -> *mut c_char {
    init_logging();
    if data_dir.is_null() {
        return to_c_string(r#"{"error":"dataDir is null"}"#.to_string());
    }
    let data_dir = match CStr::from_ptr(data_dir).to_str() {
        Ok(s) => s,
        Err(e) => {
            return to_c_string(format!(r#"{{"error":"bad dataDir: {e}"}}"#));
        }
    };
    to_c_string(embedded::start_json(data_dir))
}

#[no_mangle]
pub extern "C" fn vellum_stop_core() {
    embedded::stop();
}

#[no_mangle]
pub extern "C" fn vellum_core_status() -> *mut c_char {
    to_c_string(embedded::status_json())
}

/// # Safety
/// `s` must be a pointer previously returned by a `vellum_*` function (or
/// NULL, which is a no-op), and must not be used after this call.
#[no_mangle]
pub unsafe extern "C" fn vellum_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip through the real C ABI: status before any start must be
    /// `running: false`, and the returned pointer must free cleanly.
    #[test]
    fn status_round_trips_through_c_strings() {
        let ptr = vellum_core_status();
        assert!(!ptr.is_null());
        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_owned();
        unsafe { vellum_string_free(ptr) };
        let status: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(status["running"], false);
    }

    #[test]
    fn null_data_dir_reports_error_not_crash() {
        let ptr = unsafe { vellum_start_core(std::ptr::null()) };
        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_owned();
        unsafe { vellum_string_free(ptr) };
        assert!(json.contains("error"));
    }
}
